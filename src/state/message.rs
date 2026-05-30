use std::collections::VecDeque;

/// Maximum messages stored per channel (ring buffer).
pub const MAX_MESSAGES: usize = 200;

/// A single chat message — minimal fields to save RAM.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Message {
    pub id: u64,
    pub channel_id: u64,
    pub author_name: String,
    pub author_id: u64,
    pub content: String,
    /// ISO-8601 timestamp from Discord, or empty string
    pub timestamp: String,
}

impl Message {
    /// Parse a message from a MESSAGE_CREATE or REST API JSON payload.
    pub fn from_json(data: &serde_json::Value) -> Option<Self> {
        let id = data.get("id")?.as_str()?.parse::<u64>().ok()?;
        let channel_id = data.get("channel_id")?.as_str()?.parse::<u64>().ok()?;
        let content = data
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        let author = data.get("author")?;
        let author_name = author
            .get("username")
            .and_then(|u| u.as_str())
            .unwrap_or("unknown")
            .to_string();
        let author_id = author
            .get("id")
            .and_then(|i| i.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        let timestamp = data
            .get("timestamp")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        Some(Message {
            id,
            channel_id,
            author_name,
            author_id,
            content,
            timestamp,
        })
    }

    /// Format the timestamp for display — extracts HH:MM from ISO-8601.
    pub fn display_time(&self) -> &str {
        // Discord timestamps look like: "2024-01-15T12:30:00.000000+00:00"
        // We want "12:30"
        if let Some(t_pos) = self.timestamp.find('T') {
            let time_part = &self.timestamp[t_pos + 1..];
            if time_part.len() >= 5 {
                return &time_part[..5];
            }
        }
        "??:??"
    }
}

/// Type alias for the per-channel message ring buffer.
pub type MessageBuffer = VecDeque<Message>;

/// Push a message into a ring buffer, evicting the oldest if at capacity.
pub fn push_message(buffer: &mut MessageBuffer, msg: Message) {
    if buffer.len() >= MAX_MESSAGES {
        buffer.pop_front();
    }
    buffer.push_back(msg);
}
