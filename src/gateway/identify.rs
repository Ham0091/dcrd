use serde_json::json;
use tokio_tungstenite::tungstenite::Message;
use tracing::debug;

/// Build the IDENTIFY payload (op 2) for a user account (selfbot).
///
/// User accounts use a different IDENTIFY structure than bots:
/// - No `intents` field (user accounts receive all events)
/// - `properties` mimics the Discord desktop client
/// - `presence` sets initial online status
/// - `capabilities` is a bitmask (4093 = standard Discord desktop capabilities)
pub fn build_identify(token: &str) -> Message {
    let masked = if token.len() > 10 {
        format!("{}...{}", &token[..6], &token[token.len()-4..])
    } else {
        "***".to_string()
    };
    debug!("Building IDENTIFY (user account) with token prefix: {}", masked);
    let payload = json!({
        "op": 2,
        "d": {
            "token": token,
            "capabilities": 4093,
            "properties": {
                "os": "Windows",
                "browser": "Chrome",
                "device": "",
                "system_locale": "en-US",
                "browser_user_agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/137.0.0.0 Safari/537.36",
                "browser_version": "137.0.0.0",
                "os_version": "10",
                "referrer": "",
                "referring_domain": "",
                "referrer_current": "",
                "referring_domain_current": "",
                "release_channel": "stable",
                "client_build_number": 411000,
                "client_event_source": null
            },
            "presence": {
                "status": "online",
                "since": 0,
                "activities": [],
                "afk": false
            },
            "compress": false,
            "client_state": {
                "guild_versions": {}
            }
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
