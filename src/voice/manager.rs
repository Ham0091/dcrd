use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

use crate::state::AppState;

use super::encryption::VoiceCipher;
use super::opus_codec::{OpusDecoder, OpusEncoder, FRAME_SIZE};
use super::udp::VoiceUdp;

/// Connect to a Discord voice channel.
///
/// This is the main voice connection lifecycle:
/// 1. Connect to Voice Gateway WebSocket
/// 2. Voice HELLO → IDENTIFY
/// 3. Receive READY (IP, port, SSRC, modes)
/// 4. UDP IP discovery
/// 5. SELECT_PROTOCOL
/// 6. Receive SESSION_DESCRIPTION (encryption key)
/// 7. Start audio streaming loop
pub async fn connect_voice(
    state: Arc<AppState>,
    guild_id: u64,
    endpoint: String,
    token: String,
    session_id: String,
    user_id: u64,
) -> anyhow::Result<()> {
    // Strip port suffix if present (endpoint might be "host:port")
    let host = endpoint.split(':').next().unwrap_or(&endpoint);
    let voice_url = format!("wss://{}/?v=8", host);

    info!("Connecting to Voice Gateway: {}", voice_url);

    let (ws_stream, _) = connect_async(&voice_url).await?;
    let (mut ws_write, mut ws_read) = ws_stream.split();

    // ── Voice HELLO (op 8) ──────────────────────────────────────────────
    let hello_text = next_text(&mut ws_read).await?;
    let hello: Value = serde_json::from_str(&hello_text)?;
    let op = hello.get("op").and_then(|o| o.as_u64()).unwrap_or(0);
    if op != 8 {
        return Err(anyhow::anyhow!("Expected voice HELLO (op 8), got op {}", op));
    }
    let hb_interval = hello
        .get("d")
        .and_then(|d| d.get("heartbeat_interval"))
        .and_then(|h| h.as_u64())
        .unwrap_or(41250);
    info!("Voice HELLO: heartbeat_interval={}ms", hb_interval);

    // ── Voice IDENTIFY (op 0) ───────────────────────────────────────────
    let identify = json!({
        "op": 0,
        "d": {
            "server_id": guild_id.to_string(),
            "user_id": user_id.to_string(),
            "session_id": session_id,
            "token": token
        }
    });
    ws_write.send(Message::Text(identify.to_string())).await?;
    info!("Voice IDENTIFY sent");

    // ── Wait for READY (op 2) ───────────────────────────────────────────
    let ready_text = next_text(&mut ws_read).await?;
    let ready: Value = serde_json::from_str(&ready_text)?;
    let ready_op = ready.get("op").and_then(|o| o.as_u64()).unwrap_or(0);
    if ready_op != 2 {
        return Err(anyhow::anyhow!(
            "Expected voice READY (op 2), got op {}",
            ready_op
        ));
    }

    let ssrc = ready
        .get("d")
        .and_then(|d| d.get("ssrc"))
        .and_then(|s| s.as_u64())
        .unwrap_or(0) as u32;
    let modes = ready
        .get("d")
        .and_then(|d| d.get("modes"))
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();
    let ip = ready
        .get("d")
        .and_then(|d| d.get("ip"))
        .and_then(|i| i.as_str())
        .unwrap_or("0.0.0.0");
    let port = ready
        .get("d")
        .and_then(|d| d.get("port"))
        .and_then(|p| p.as_u64())
        .unwrap_or(0) as u16;

    info!(
        "Voice READY: ssrc={}, ip={}, port={}, modes={:?}",
        ssrc, ip, port, modes
    );

    // Check if xsalsa20_poly1305 is supported
    let use_mode = if modes.iter().any(|m| m.as_str() == Some("xsalsa20_poly1305")) {
        "xsalsa20_poly1305"
    } else if modes.iter().any(|m| m.as_str() == Some("aead_xchacha20_poly1305_rtpsize")) {
        "aead_xchacha20_poly1305_rtpsize"
    } else {
        return Err(anyhow::anyhow!(
            "No supported encryption mode found. Available: {:?}",
            modes
        ));
    };

    // ── UDP IP Discovery ────────────────────────────────────────────────
    let remote_addr: SocketAddr = format!("{}:{}", ip, port).parse()?;
    let udp = VoiceUdp::new(remote_addr, ssrc).await?;
    let discovery = udp.ip_discovery().await?;
    info!(
        "UDP IP discovery: external={}:{}",
        discovery.ip, discovery.port
    );

    // ── SELECT_PROTOCOL (op 1) ──────────────────────────────────────────
    let select_protocol = json!({
        "op": 1,
        "d": {
            "protocol": "udp",
            "data": {
                "address": discovery.ip,
                "port": discovery.port,
                "mode": use_mode
            }
        }
    });
    ws_write
        .send(Message::Text(select_protocol.to_string()))
        .await?;
    info!("SELECT_PROTOCOL sent (mode: {})", use_mode);

    // ── Wait for SESSION_DESCRIPTION (op 4) ─────────────────────────────
    let desc_text = next_text(&mut ws_read).await?;
    let desc: Value = serde_json::from_str(&desc_text)?;
    let desc_op = desc.get("op").and_then(|o| o.as_u64()).unwrap_or(0);
    if desc_op != 4 {
        return Err(anyhow::anyhow!(
            "Expected SESSION_DESCRIPTION (op 4), got op {}",
            desc_op
        ));
    }

    let key_array = desc
        .get("d")
        .and_then(|d| d.get("secret_key"))
        .and_then(|k| k.as_array())
        .ok_or_else(|| anyhow::anyhow!("Missing secret_key in SESSION_DESCRIPTION"))?;

    let mut key_bytes = [0u8; 32];
    for (i, b) in key_array.iter().enumerate() {
        if i >= 32 {
            break;
        }
        key_bytes[i] = b.as_u64().unwrap_or(0) as u8;
    }

    let cipher = VoiceCipher::new(&key_bytes);
    info!("SESSION_DESCRIPTION received, encryption key set");

    // Update voice state with connected status
    {
        let mut voice = state.voice_state.write().await;
        voice.connected = true;
        voice.guild_id = Some(guild_id);
    }

    // ── Voice Gateway Heartbeat Task ────────────────────────────────────
    // We need to send heartbeats to the voice gateway
    let (hb_stop_tx, mut hb_stop_rx) = tokio::sync::oneshot::channel::<()>();
    let hb_interval_dur = Duration::from_millis(hb_interval * 3 / 4); // slightly faster than required
    tokio::spawn(async move {
        let mut ticker = interval(hb_interval_dur);
        ticker.tick().await; // skip first
        let mut seq: u64 = 0;
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    let _hb = json!({"op": 3, "d": seq});
                    // We can't easily share ws_write, so we'll handle heartbeats differently
                    // For now, the main loop will handle heartbeats
                    seq += 1;
                }
                _ = &mut hb_stop_rx => {
                    break;
                }
            }
        }
    });

    // ── Audio Streaming Loop ────────────────────────────────────────────
    let result = audio_loop(&state, ws_write, ws_read, udp, cipher, ssrc, hb_interval).await;

    let _ = hb_stop_tx.send(());

    // Update voice state on disconnect
    {
        let mut voice = state.voice_state.write().await;
        voice.connected = false;
        voice.channel_id = None;
        voice.channel_name = None;
        voice.session_id = None;
        voice.endpoint = None;
        voice.token = None;
    }

    info!("Voice connection ended");
    result
}

/// Main audio streaming loop — handles sending and receiving voice data.
async fn audio_loop<SRead>(
    state: &Arc<AppState>,
    mut ws_write: futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    mut ws_read: SRead,
    mut udp: VoiceUdp,
    cipher: VoiceCipher,
    _ssrc: u32,
    hb_interval_ms: u64,
) -> anyhow::Result<()>
where
    SRead: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    // Create Opus encoder/decoder
    let mut encoder = match OpusEncoder::new() {
        Ok(e) => e,
        Err(e) => {
            warn!("Failed to create Opus encoder: {} — voice send disabled", e);
            // We'll still receive, just won't send
            return recv_only_loop(state, ws_write, ws_read, udp, cipher, hb_interval_ms).await;
        }
    };
    let mut decoder = match OpusDecoder::new() {
        Ok(d) => d,
        Err(e) => {
            warn!("Failed to create Opus decoder: {} — voice receive disabled", e);
            return send_only_loop(state, ws_write, ws_read, udp, cipher, &mut encoder, hb_interval_ms).await;
        }
    };

    // Audio capture/playback setup (deferred to audio module)
    // For now, we'll use a simple approach: capture from mic, encode, send
    // and receive, decode, play

    let mic_buffer = Arc::new(crossbeam_queue::ArrayQueue::<i16>::new(FRAME_SIZE * 4));
    let spk_buffer = Arc::new(crossbeam_queue::ArrayQueue::<i16>::new(FRAME_SIZE * 4));

    // Spawn audio capture thread
    let mic_buf = mic_buffer.clone();
    let capture_handle = std::thread::spawn(move || {
        if let Err(e) = crate::audio::capture::run_capture(mic_buf) {
            error!("Audio capture error: {}", e);
        }
    });

    // Spawn audio playback thread
    let spk_buf = spk_buffer.clone();
    let playback_handle = std::thread::spawn(move || {
        if let Err(e) = crate::audio::playback::run_playback(spk_buf) {
            error!("Audio playback error: {}", e);
        }
    });

    let mut send_buf = vec![0i16; FRAME_SIZE];
    let mut send_buf_pos = 0;

    let mut hb_interval = interval(Duration::from_millis(hb_interval_ms * 3 / 4));
    hb_interval.tick().await;
    let mut hb_seq: u64 = 0;

    // Silence frame for muted state
    let silence = vec![0i16; FRAME_SIZE];

    loop {
        tokio::select! {
            // ── Voice Gateway WebSocket events ──
            ws_msg = ws_read.next() => {
                match ws_msg {
                    Some(Ok(Message::Text(text))) => {
                        let v: Value = serde_json::from_str(&text)?;
                        let op = v.get("op").and_then(|o| o.as_u64()).unwrap_or(0);
                        match op {
                            3 => {
                                // HEARTBEAT_ACK from voice gateway
                                debug!("Voice heartbeat ACK");
                            }
                            5 => {
                                // Speaking notification — ignore for now
                            }
                            13 => {
                                // Client disconnect — someone left
                                debug!("Voice client disconnect event");
                            }
                            _ => {
                                debug!("Voice WS op: {}", op);
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!("Voice WebSocket closed");
                        break;
                    }
                    Some(Err(e)) => {
                        error!("Voice WebSocket error: {}", e);
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }

            // ── Voice Gateway heartbeat ──
            _ = hb_interval.tick() => {
                let hb = json!({"op": 3, "d": hb_seq});
                ws_write.send(Message::Text(hb.to_string())).await.ok();
                hb_seq += 1;
            }

            // ── Capture audio from microphone ──
            _ = tokio::time::sleep(Duration::from_millis(20)) => {
                let voice = state.voice_state.read().await;
                let is_muted = voice.muted || voice.deafened;
                drop(voice);

                // Fill send buffer from mic
                while let Some(sample) = mic_buffer.pop() {
                    if send_buf_pos < FRAME_SIZE {
                        send_buf[send_buf_pos] = sample;
                        send_buf_pos += 1;
                    }
                }

                // When we have a full frame, encode and send
                if send_buf_pos >= FRAME_SIZE {
                    let frame = if is_muted {
                        &silence
                    } else {
                        &send_buf[..FRAME_SIZE]
                    };

                    match encoder.encode(frame) {
                        Ok(opus_packet) => {
                            // Encrypt
                            let _header = [0u8; 12]; // Will be set by UDP
                            match cipher.encrypt(&[0x80, 0x78, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], opus_packet) {
                                Ok(encrypted) => {
                                    if let Err(e) = udp.send_voice_packet(&encrypted).await {
                                        debug!("Voice send error: {}", e);
                                    }
                                }
                                Err(e) => debug!("Encrypt error: {}", e),
                            }
                        }
                        Err(e) => debug!("Encode error: {}", e),
                    }

                    // Shift remaining samples
                    send_buf.copy_within(FRAME_SIZE..send_buf_pos, 0);
                    send_buf_pos -= FRAME_SIZE;
                }

                // Try to receive UDP packets
                match tokio::time::timeout(Duration::from_millis(1), udp.recv_voice_packet()).await {
                    Ok(Ok((header, payload))) => {
                        match cipher.decrypt(&header, &payload) {
                            Ok(decrypted) => {
                                match decoder.decode(&decrypted) {
                                    Ok(pcm) => {
                                        for &sample in pcm {
                                            spk_buffer.push(sample).ok();
                                        }
                                    }
                                    Err(e) => debug!("Decode error: {}", e),
                                }
                            }
                            Err(e) => debug!("Decrypt error: {}", e),
                        }
                    }
                    Ok(Err(e)) => debug!("UDP recv error: {}", e),
                    Err(_) => {} // Timeout — no packet available
                }
            }
        }
    }

    // Cleanup
    capture_handle.join().ok();
    playback_handle.join().ok();

    Ok(())
}

/// Fallback loop when encoder is not available — receive only.
async fn recv_only_loop<SRead>(
    _state: &Arc<AppState>,
    mut ws_write: futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    mut ws_read: SRead,
    udp: VoiceUdp,
    cipher: VoiceCipher,
    hb_interval_ms: u64,
) -> anyhow::Result<()>
where
    SRead: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let mut decoder = OpusDecoder::new()?;
    let spk_buffer = Arc::new(crossbeam_queue::ArrayQueue::<i16>::new(FRAME_SIZE * 4));

    let spk_buf = spk_buffer.clone();
    let playback_handle = std::thread::spawn(move || {
        if let Err(e) = crate::audio::playback::run_playback(spk_buf) {
            error!("Audio playback error: {}", e);
        }
    });

    let mut hb_interval = interval(Duration::from_millis(hb_interval_ms * 3 / 4));
    hb_interval.tick().await;
    let mut hb_seq: u64 = 0;

    loop {
        tokio::select! {
            ws_msg = ws_read.next() => {
                match ws_msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
            _ = hb_interval.tick() => {
                let hb = json!({"op": 3, "d": hb_seq});
                ws_write.send(Message::Text(hb.to_string())).await.ok();
                hb_seq += 1;
            }
            _ = tokio::time::sleep(Duration::from_millis(20)) => {
                match tokio::time::timeout(Duration::from_millis(1), udp.recv_voice_packet()).await {
                    Ok(Ok((header, payload))) => {
                        if let Ok(decrypted) = cipher.decrypt(&header, &payload) {
                            if let Ok(pcm) = decoder.decode(&decrypted) {
                                for &sample in pcm {
                                    spk_buffer.push(sample).ok();
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    playback_handle.join().ok();
    Ok(())
}

/// Fallback loop when decoder is not available — send only.
async fn send_only_loop<SRead>(
    _state: &Arc<AppState>,
    mut ws_write: futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    mut ws_read: SRead,
    mut udp: VoiceUdp,
    cipher: VoiceCipher,
    encoder: &mut OpusEncoder,
    hb_interval_ms: u64,
) -> anyhow::Result<()>
where
    SRead: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let mic_buffer = Arc::new(crossbeam_queue::ArrayQueue::<i16>::new(FRAME_SIZE * 4));
    let mic_buf = mic_buffer.clone();
    let capture_handle = std::thread::spawn(move || {
        if let Err(e) = crate::audio::capture::run_capture(mic_buf) {
            error!("Audio capture error: {}", e);
        }
    });

    let mut send_buf = vec![0i16; FRAME_SIZE];
    let mut send_buf_pos = 0;

    let mut hb_interval = interval(Duration::from_millis(hb_interval_ms * 3 / 4));
    hb_interval.tick().await;
    let mut hb_seq: u64 = 0;

    loop {
        tokio::select! {
            ws_msg = ws_read.next() => {
                match ws_msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
            _ = hb_interval.tick() => {
                let hb = json!({"op": 3, "d": hb_seq});
                ws_write.send(Message::Text(hb.to_string())).await.ok();
                hb_seq += 1;
            }
            _ = tokio::time::sleep(Duration::from_millis(20)) => {
                while let Some(sample) = mic_buffer.pop() {
                    if send_buf_pos < FRAME_SIZE {
                        send_buf[send_buf_pos] = sample;
                        send_buf_pos += 1;
                    }
                }
                if send_buf_pos >= FRAME_SIZE {
                    if let Ok(opus_packet) = encoder.encode(&send_buf[..FRAME_SIZE]) {
                        let header = [0x80, 0x78, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
                        if let Ok(encrypted) = cipher.encrypt(&header, opus_packet) {
                            udp.send_voice_packet(&encrypted).await.ok();
                        }
                    }
                    send_buf.copy_within(FRAME_SIZE..send_buf_pos, 0);
                    send_buf_pos -= FRAME_SIZE;
                }
            }
        }
    }

    capture_handle.join().ok();
    Ok(())
}

/// Read the next Text message from a voice WebSocket stream.
async fn next_text<S>(read: &mut S) -> anyhow::Result<String>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => return Ok(text),
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => continue,
            Ok(Message::Close(_)) => {
                return Err(anyhow::anyhow!("Voice WS closed before text message"))
            }
            Ok(_) => continue,
            Err(e) => return Err(anyhow::anyhow!("Voice WS error: {}", e)),
        }
    }
    Err(anyhow::anyhow!("Voice WS stream ended"))
}
