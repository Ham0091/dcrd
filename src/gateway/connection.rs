use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tokio::time::{interval, sleep, Duration, Instant};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

use crate::rest::api::RestClient;
use crate::state::AppState;
use crate::Command;

const GATEWAY_URL: &str = "wss://gateway.discord.gg/?v=10&encoding=json";
const MAX_RECONNECT_ATTEMPTS: u32 = 3;

/// Main gateway entry point — reconnects on failure until shutdown.
/// Stops immediately on authentication failures (close code 4004).
pub async fn run(
    state: Arc<AppState>,
    mut cmd_rx: mpsc::Receiver<Command>,
    rest: Arc<RestClient>,
    mut shutdown: broadcast::Receiver<()>,
) -> anyhow::Result<()> {
    let mut attempts: u32 = 0;
    loop {
        tokio::select! {
            result = connect_and_run(&state, &mut cmd_rx, &rest) => {
                match result {
                    Ok(()) => {
                        info!("Gateway connection closed cleanly");
                        return Ok(());
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        // Authentication failures are permanent — don't retry
                        if msg.contains("4004") {
                            error!("Authentication failed (4004). Check your DCRD_TOKEN. Aborting.");
                            return Err(e);
                        }
                        attempts += 1;
                        if attempts >= MAX_RECONNECT_ATTEMPTS {
                            error!("Failed to connect after {} attempts: {}", attempts, e);
                            return Err(e);
                        }
                        let delay = 5 * attempts; // exponential-ish backoff
                        warn!("Gateway error: {} — reconnecting in {}s (attempt {}/{})...", e, delay, attempts, MAX_RECONNECT_ATTEMPTS);
                        sleep(Duration::from_secs(delay as u64)).await;
                    }
                }
            }
            _ = shutdown.recv() => {
                info!("Gateway shutdown signal received");
                return Ok(());
            }
        }
    }
}

/// Single gateway connection lifecycle: HELLO → IDENTIFY → event loop.
async fn connect_and_run(
    state: &Arc<AppState>,
    cmd_rx: &mut mpsc::Receiver<Command>,
    rest: &Arc<RestClient>,
) -> anyhow::Result<()> {
    info!("Connecting to Discord Gateway...");
    let (ws_stream, _) = connect_async(GATEWAY_URL).await?;
    let (mut ws_write, mut ws_read) = ws_stream.split();

    // ── HELLO (op 10) ────────────────────────────────────────────────────
    let hello_text = next_text_message(&mut ws_read).await?;
    let hello: Value = serde_json::from_str(&hello_text)?;
    let op = hello.get("op").and_then(|o| o.as_u64()).unwrap_or(0);
    if op != 10 {
        return Err(anyhow::anyhow!("Expected HELLO (op 10), got op {}", op));
    }
    let heartbeat_interval_ms = hello
        .get("d")
        .and_then(|d| d.get("heartbeat_interval"))
        .and_then(|h| h.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing heartbeat_interval in HELLO"))?;
    info!("HELLO: heartbeat_interval = {}ms", heartbeat_interval_ms);

    // ── IDENTIFY (op 2) ──────────────────────────────────────────────────
    let identify = super::identify::build_identify(&state.token);
    ws_write.send(identify).await?;
    info!("IDENTIFY sent");

    // ── Main event loop ──────────────────────────────────────────────────
    let mut heartbeat_tick = interval(Duration::from_millis(heartbeat_interval_ms));
    // Skip the first immediate tick
    heartbeat_tick.tick().await;
    // Jitter: wait a random fraction of the interval before first heartbeat
    let jitter = super::heartbeat::jitter_ms(heartbeat_interval_ms);
    let jitter_deadline = Instant::now() + Duration::from_millis(jitter);

    loop {
        tokio::select! {
            // ── Incoming WebSocket messages ──
            ws_msg = ws_read.next() => {
                match ws_msg {
                    Some(Ok(Message::Text(text))) => {
                        let v: Value = serde_json::from_str(&text)?;
                        let op = v.get("op").and_then(|o| o.as_u64()).unwrap_or(0);
                        let d = v.get("d").cloned();
                        let t = v.get("t").and_then(|t| t.as_str()).map(|s| s.to_string());
                        let s = v.get("s").and_then(|s| s.as_u64());

                        if let Err(e) = super::events::handle_event(
                            op, d, t.as_deref(), s, state,
                        ).await {
                            let msg = e.to_string();
                            if msg == "RECONNECT" || msg == "INVALID_SESSION" {
                                return Err(e);
                            }
                            error!("Event error: {}", e);
                        }
                    }
                    Some(Ok(Message::Close(close_frame))) => {
                        let details = close_frame
                            .as_ref()
                            .map(|cf| format!("code={}, reason={}", cf.code, cf.reason))
                            .unwrap_or_else(|| "no details".to_string());
                        return Err(anyhow::anyhow!("WebSocket closed by server: {}", details));
                    }
                    Some(Err(e)) => {
                        return Err(anyhow::anyhow!("WebSocket error: {}", e));
                    }
                    None => {
                        return Err(anyhow::anyhow!("WebSocket stream ended"));
                    }
                    _ => { /* Ping/Pong handled by tungstenite */ }
                }
            }

            // ── Heartbeat tick ──
            _ = heartbeat_tick.tick(), if Instant::now() >= jitter_deadline => {
                if !state.heartbeat_ack_received.load(Ordering::Relaxed) {
                    warn!("Heartbeat ACK missed — reconnecting");
                    return Err(anyhow::anyhow!("Heartbeat timeout"));
                }
                state.heartbeat_ack_received.store(false, Ordering::Relaxed);

                let seq = state.gateway_seq.load(Ordering::Relaxed);
                let hb = super::heartbeat::build_heartbeat(seq);
                ws_write.send(Message::Text(hb.to_string())).await?;
                debug!("Heartbeat sent (seq={})", seq);
            }

            // ── Commands from TUI ──
            Some(cmd) = cmd_rx.recv() => {
                match cmd {
                    Command::SendMessage { channel_id, content } => {
                        // Send via REST API
                        match rest.send_message(channel_id, &content).await {
                            Ok(_) => debug!("Message sent to channel {}", channel_id),
                            Err(e) => error!("Failed to send message: {}", e),
                        }
                    }
                    Command::FetchMessages { channel_id } => {
                        // Fetch recent messages via REST
                        match rest.fetch_messages(channel_id, 50).await {
                            Ok(msgs) => {
                                for msg_data in msgs.iter().rev() {
                                    if let Some(msg) = crate::state::message::Message::from_json(msg_data) {
                                        state.add_message(msg);
                                    }
                                }
                                debug!("Fetched messages for channel {}", channel_id);
                            }
                            Err(e) => error!("Failed to fetch messages: {}", e),
                        }
                    }
                    Command::VoiceStateUpdate { guild_id, channel_id, self_mute, self_deaf } => {
                        let payload = super::identify::build_voice_state_update(
                            guild_id, channel_id, self_mute, self_deaf,
                        );
                        ws_write.send(payload).await?;
                        info!("Voice state update sent: guild={}, channel={:?}, mute={}, deaf={}",
                            guild_id, channel_id, self_mute, self_deaf);
                    }
                    Command::ToggleMute => {
                        let mut voice = state.voice_state.write().await;
                        if voice.connected {
                            voice.muted = !voice.muted;
                            let mute = voice.muted;
                            let guild_id = voice.guild_id.unwrap_or(0);
                            let channel_id = voice.channel_id;
                            drop(voice);
                            let payload = super::identify::build_voice_state_update(
                                guild_id, channel_id, mute, false,
                            );
                            ws_write.send(payload).await?;
                            info!("Mute toggled: {}", mute);
                        }
                    }
                    Command::ToggleDeafen => {
                        let mut voice = state.voice_state.write().await;
                        if voice.connected {
                            voice.deafened = !voice.deafened;
                            // Discord requires self_mute=true when self_deaf=true
                            if voice.deafened {
                                voice.muted = true;
                            }
                            let deaf = voice.deafened;
                            let muted = voice.muted;
                            let guild_id = voice.guild_id.unwrap_or(0);
                            let channel_id = voice.channel_id;
                            drop(voice);
                            let payload = super::identify::build_voice_state_update(
                                guild_id, channel_id, muted, deaf,
                            );
                            ws_write.send(payload).await?;
                            info!("Deafen toggled: {}", deaf);
                        }
                    }
                    Command::Quit => {
                        info!("Quit command received");
                        ws_write.send(Message::Close(None)).await.ok();
                        return Ok(());
                    }
                }
            }
        }
    }
}

/// Read the next Text message from a WebSocket stream, skipping non-text frames.
async fn next_text_message<S>(read: &mut S) -> anyhow::Result<String>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => return Ok(text),
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => continue,
            Ok(Message::Close(_)) => {
                return Err(anyhow::anyhow!("WebSocket closed before text message"))
            }
            Ok(_) => continue,
            Err(e) => return Err(anyhow::anyhow!("WebSocket error: {}", e)),
        }
    }
    Err(anyhow::anyhow!("WebSocket stream ended"))
}
