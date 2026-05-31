use serde_json::json;
use tokio_tungstenite::tungstenite::Message;
use tracing::debug;

/// Build the IDENTIFY payload (op 2) with minimal intents.
///
/// Intents 641 = GUILDS (1 << 0) + GUILD_VOICE_STATES (1 << 7) + GUILD_MESSAGES (1 << 9).
/// We request ONLY the intents we need to minimize gateway traffic.
pub fn build_identify(token: &str) -> Message {
    let masked = if token.len() > 10 {
        format!("{}...{}", &token[..6], &token[token.len()-4..])
    } else {
        "***".to_string()
    };
    debug!("Building IDENTIFY with token prefix: {}", masked);
    let payload = json!({
        "op": 2,
        "d": {
            "token": token,
            "intents": 641,
            "properties": {
                "os": "windows",
                "browser": "dcrd",
                "device": "dcrd"
            },
            "compress": false
        }
    });
    Message::Text(payload.to_string())
}

/// Build a RESUME payload (op 6) for reconnecting.
#[allow(dead_code)]
pub fn build_resume(token: &str, session_id: &str, seq: u64) -> Message {
    let payload = json!({
        "op": 6,
        "d": {
            "token": token,
            "session_id": session_id,
            "seq": seq
        }
    });
    Message::Text(payload.to_string())
}

/// Build a VOICE_STATE_UPDATE payload (op 4) to join/leave a voice channel.
pub fn build_voice_state_update(
    guild_id: u64,
    channel_id: Option<u64>,
    self_mute: bool,
    self_deaf: bool,
) -> Message {
    let payload = json!({
        "op": 4,
        "d": {
            "guild_id": guild_id.to_string(),
            "channel_id": channel_id.map(|c| c.to_string()),
            "self_mute": self_mute,
            "self_deaf": self_deaf
        }
    });
    Message::Text(payload.to_string())
}
