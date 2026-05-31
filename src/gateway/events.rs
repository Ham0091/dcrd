use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::state::channel::Channel;
use crate::state::message::Message as StateMessage;
use crate::state::server::Guild;
use crate::state::AppState;

/// Handle a parsed gateway event by opcode.
///
/// Returns `Err("RECONNECT")` or `Err("INVALID_SESSION")` to signal the
/// connection loop to reconnect.
pub async fn handle_event(
    op: u64,
    data: Option<Value>,
    event_name: Option<&str>,
    seq: Option<u64>,
    state: &Arc<AppState>,
) -> anyhow::Result<()> {
    // Update sequence number for heartbeats/resumes
    if let Some(s) = seq {
        state
            .gateway_seq
            .store(s, std::sync::atomic::Ordering::Relaxed);
    }

    match op {
        0 => handle_dispatch(data, event_name, state).await,
        7 => {
            info!("Gateway RECONNECT requested");
            Err(anyhow::anyhow!("RECONNECT"))
        }
        9 => {
            warn!("Gateway INVALID_SESSION — must re-identify");
            Err(anyhow::anyhow!("INVALID_SESSION"))
        }
        10 => {
            // HELLO — handled in connection.rs, ignore here
            Ok(())
        }
        11 => {
            debug!("Heartbeat ACK received");
            state
                .heartbeat_ack_received
                .store(true, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        }
        _ => {
            debug!("Unhandled gateway op: {}", op);
            Ok(())
        }
    }
}

/// Dispatch op-0 (dispatch) events by event name.
async fn handle_dispatch(
    data: Option<Value>,
    event_name: Option<&str>,
    state: &Arc<AppState>,
) -> anyhow::Result<()> {
    let data = match data {
        Some(d) => d,
        None => return Ok(()),
    };

    match event_name {
        Some("READY") => handle_ready(&data, state).await,
        Some("GUILD_CREATE") => handle_guild_create(&data, state).await,
        Some("CHANNEL_CREATE") => handle_channel_create(&data, state).await,
        Some("MESSAGE_CREATE") => handle_message_create(&data, state).await,
        Some("MESSAGE_UPDATE") => handle_message_update(&data, state).await,
        Some("MESSAGE_DELETE") => handle_message_delete(&data, state).await,
        Some("VOICE_STATE_UPDATE") => handle_voice_state_update(&data, state).await,
        Some("VOICE_SERVER_UPDATE") => handle_voice_server_update(&data, state).await,
        Some(other) => {
            debug!("Unhandled dispatch event: {}", other);
            Ok(())
        }
        None => Ok(()),
    }
}

// ─── READY ───────────────────────────────────────────────────────────────────

async fn handle_ready(data: &Value, state: &Arc<AppState>) -> anyhow::Result<()> {
    info!("Gateway READY received");

    // Session ID
    if let Some(sid) = data.get("session_id").and_then(|s| s.as_str()) {
        *state.session_id.write().await = Some(sid.to_string());
        info!("  Session ID: {}", sid);
    }

    // Self user
    if let Some(user_data) = data.get("user") {
        if let Some(user) = crate::state::user::User::from_ready_user(user_data) {
            info!("  Logged in as: {} (ID: {})", user.username, user.id);
            *state.user.write().await = Some(user);
        }
    }

    // Private channels (DMs) — only present for user accounts, not bots
    if let Some(private_channels) = data.get("private_channels").and_then(|c| c.as_array()) {
        for cd in private_channels {
            if let Some(ch) = Channel::from_json(cd, 0) {
                info!("  DM: {} (ID: {}, type: {:?})", ch.name, ch.id, ch.channel_type);
                state.channels.insert(ch.id, ch);
            }
        }
    }

    // Guilds (servers)
    if let Some(guilds) = data.get("guilds").and_then(|g| g.as_array()) {
        for gd in guilds {
            if let Some(guild) = Guild::from_json(gd) {
                info!("  Guild: {} (ID: {})", guild.name, guild.id);
                state.guilds.insert(guild.id, guild);

                // Channels inside the guild
                if let Some(channels) = gd.get("channels").and_then(|c| c.as_array()) {
                    for cd in channels {
                        if let Some(ch) = Channel::from_json(cd, gd["id"].as_str().unwrap_or("0").parse().unwrap_or(0)) {
                            state.channels.insert(ch.id, ch);
                        }
                    }
                }
            }
        }
    }

    // Auto-select first guild and first text channel
    let guild_ids: Vec<u64> = state.guilds.iter().map(|e| *e.key()).collect();
    if let Some(&first_guild) = guild_ids.first() {
        *state.current_guild_id.write().await = Some(first_guild);

        let mut text_channels = state.get_text_channels(first_guild);
        text_channels.sort_by_key(|c| c.position);
        if let Some(first_ch) = text_channels.first() {
            *state.current_channel_id.write().await = Some(first_ch.id);
            info!("  Default channel: #{} (ID: {})", first_ch.name, first_ch.id);
        }
    }

    Ok(())
}

// ─── GUILD_CREATE ────────────────────────────────────────────────────────────

async fn handle_guild_create(data: &Value, state: &Arc<AppState>) -> anyhow::Result<()> {
    let guild_id = data
        .get("id")
        .and_then(|i| i.as_str())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    if let Some(guild) = Guild::from_json(data) {
        info!("Guild discovered: {} (ID: {})", guild.name, guild.id);
        state.guilds.insert(guild_id, guild);
    }

    // Channels
    if let Some(channels) = data.get("channels").and_then(|c| c.as_array()) {
        for cd in channels {
            if let Some(ch) = Channel::from_json(cd, guild_id) {
                state.channels.insert(ch.id, ch);
            }
        }
    }

    // Voice states — track who is already in voice channels
    if let Some(voice_states) = data.get("voice_states").and_then(|v| v.as_array()) {
        for vs in voice_states {
            let user_id = vs
                .get("user_id")
                .and_then(|u| u.as_str())
                .and_then(|s| s.parse::<u64>().ok());
            let channel_id = vs
                .get("channel_id")
                .and_then(|c| c.as_str())
                .and_then(|s| s.parse::<u64>().ok());

            if let (Some(uid), Some(_cid)) = (user_id, channel_id) {
                // We'll resolve the username later if needed
                state
                    .voice_users
                    .insert((guild_id, uid), format!("User#{}", uid));
            }
        }
    }

    Ok(())
}

// ─── CHANNEL_CREATE ──────────────────────────────────────────────────────────

async fn handle_channel_create(data: &Value, state: &Arc<AppState>) -> anyhow::Result<()> {
    let guild_id = data
        .get("guild_id")
        .and_then(|g| g.as_str())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    if let Some(ch) = Channel::from_json(data, guild_id) {
        info!("Channel created: #{} (ID: {})", ch.name, ch.id);
        state.channels.insert(ch.id, ch);
    }
    Ok(())
}

// ─── MESSAGE_CREATE ──────────────────────────────────────────────────────────

async fn handle_message_create(data: &Value, state: &Arc<AppState>) -> anyhow::Result<()> {
    if let Some(msg) = StateMessage::from_json(data) {
        debug!(
            "Message #{}: <{}> {}",
            msg.channel_id, msg.author_name, msg.content
        );
        state.add_message(msg);
    }
    Ok(())
}

// ─── MESSAGE_UPDATE ──────────────────────────────────────────────────────────

async fn handle_message_update(data: &Value, state: &Arc<AppState>) -> anyhow::Result<()> {
    let channel_id = data
        .get("channel_id")
        .and_then(|c| c.as_str())
        .and_then(|s| s.parse::<u64>().ok());
    let message_id = data
        .get("id")
        .and_then(|i| i.as_str())
        .and_then(|s| s.parse::<u64>().ok());

    if let (Some(cid), Some(mid)) = (channel_id, message_id) {
        if let Some(mut messages) = state.messages.get_mut(&cid) {
            if let Some(msg) = messages.iter_mut().find(|m| m.id == mid) {
                if let Some(content) = data.get("content").and_then(|c| c.as_str()) {
                    msg.content = content.to_string();
                }
            }
        }
    }
    Ok(())
}

// ─── MESSAGE_DELETE ──────────────────────────────────────────────────────────

async fn handle_message_delete(data: &Value, state: &Arc<AppState>) -> anyhow::Result<()> {
    let channel_id = data
        .get("channel_id")
        .and_then(|c| c.as_str())
        .and_then(|s| s.parse::<u64>().ok());
    let message_id = data
        .get("id")
        .and_then(|i| i.as_str())
        .and_then(|s| s.parse::<u64>().ok());

    if let (Some(cid), Some(mid)) = (channel_id, message_id) {
        if let Some(mut messages) = state.messages.get_mut(&cid) {
            messages.retain(|m| m.id != mid);
        }
    }
    Ok(())
}

// ─── VOICE_STATE_UPDATE ─────────────────────────────────────────────────────

async fn handle_voice_state_update(data: &Value, state: &Arc<AppState>) -> anyhow::Result<()> {
    let self_user_id = state.user.read().await.as_ref().map(|u| u.id);
    let user_id = data
        .get("user_id")
        .and_then(|u| u.as_str())
        .and_then(|s| s.parse::<u64>().ok());

    let guild_id = data
        .get("guild_id")
        .and_then(|g| g.as_str())
        .and_then(|s| s.parse::<u64>().ok());

    // Track voice users
    if let (Some(uid), Some(gid)) = (user_id, guild_id) {
        let channel_id = data
            .get("channel_id")
            .and_then(|c| c.as_str())
            .and_then(|s| s.parse::<u64>().ok());

        if channel_id.is_some() {
            // User joined a voice channel
            let username = data
                .get("member")
                .and_then(|m| m.get("user"))
                .and_then(|u| u.get("username"))
                .and_then(|u| u.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("User#{}", uid));
            state.voice_users.insert((gid, uid), username);
        } else {
            // User left voice channel (channel_id is null)
            state.voice_users.remove(&(gid, uid));
        }
    }

    // Update our own voice state
    if user_id == self_user_id {
        let mut voice = state.voice_state.write().await;

        if let Some(channel_id) = data
            .get("channel_id")
            .and_then(|c| c.as_str())
            .and_then(|s| s.parse::<u64>().ok())
        {
            voice.connected = true;
            voice.channel_id = Some(channel_id);
            voice.guild_id = guild_id;
            voice.muted = data
                .get("self_mute")
                .and_then(|m| m.as_bool())
                .unwrap_or(false);
            voice.deafened = data
                .get("self_deaf")
                .and_then(|d| d.as_bool())
                .unwrap_or(false);

            if let Some(sid) = data.get("session_id").and_then(|s| s.as_str()) {
                voice.session_id = Some(sid.to_string());
            }

            // Resolve channel name
            if let Some(ch) = state.channels.get(&channel_id) {
                voice.channel_name = Some(ch.name.clone());
            }

            info!(
                "Voice: joined channel {}, muted={}, deafened={}",
                channel_id, voice.muted, voice.deafened
            );
        } else {
            // channel_id is null → disconnected from voice
            voice.connected = false;
            voice.channel_id = None;
            voice.channel_name = None;
            voice.session_id = None;
            voice.endpoint = None;
            voice.token = None;
            voice.users.clear();
            info!("Voice: disconnected");
        }

        // Update cross-task voice status for the TUI
        let status = if voice.connected {
            "Joined voice channel".to_string()
        } else {
            String::new()
        };
        drop(voice);
        *state.voice_status.write().await = status;
    }

    Ok(())
}

// ─── VOICE_SERVER_UPDATE ────────────────────────────────────────────────────

async fn handle_voice_server_update(data: &Value, state: &Arc<AppState>) -> anyhow::Result<()> {
    let endpoint = data
        .get("endpoint")
        .and_then(|e| e.as_str())
        .map(|s| s.to_string());
    let token = data
        .get("token")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());
    let guild_id = data
        .get("guild_id")
        .and_then(|g| g.as_str())
        .and_then(|s| s.parse::<u64>().ok());

    info!(
        "Voice server: endpoint={:?}, guild={:?}",
        endpoint, guild_id
    );

    {
        let mut voice = state.voice_state.write().await;
        voice.endpoint = endpoint.clone();
        voice.token = token.clone();
        voice.guild_id = guild_id;
    }

    // If we have session_id + endpoint + token, we can connect to voice gateway
    let voice = state.voice_state.read().await;
    if voice.connected
        && voice.session_id.is_some()
        && voice.endpoint.is_some()
        && voice.token.is_some()
    {
        let st = state.clone();
        let ep = voice.endpoint.clone().unwrap();
        let tk = voice.token.clone().unwrap();
        let sid = voice.session_id.clone().unwrap();
        let gid = voice.guild_id.unwrap();
        let uid = state
            .user
            .read()
            .await
            .as_ref()
            .map(|u| u.id)
            .unwrap_or(0);

        tokio::spawn(async move {
            let result = crate::voice::manager::connect_voice(st.clone(), gid, ep, tk, sid, uid).await;
            if let Err(e) = result {
                error!("Voice connection error: {}", e);
                // Reset voice state so TUI doesn't show stale "connected"
                {
                    let mut voice = st.voice_state.write().await;
                    voice.connected = false;
                    voice.channel_id = None;
                    voice.channel_name = None;
                    voice.session_id = None;
                    voice.endpoint = None;
                    voice.token = None;
                }
                *st.voice_status.write().await = format!("Voice failed: {}", e);
            }
        });
    }

    Ok(())
}
