/// Channel type discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelType {
    Text = 0,
    Voice = 2,
}

/// Minimal channel model.
#[derive(Debug, Clone)]
pub struct Channel {
    pub id: u64,
    pub guild_id: u64,
    pub name: String,
    pub channel_type: ChannelType,
    pub position: i64,
}

impl Channel {
    /// Parse a channel from a CHANNEL_CREATE or GUILD_CREATE.channels JSON object.
    pub fn from_json(data: &serde_json::Value, guild_id: u64) -> Option<Self> {
        let id = data.get("id")?.as_str()?.parse::<u64>().ok()?;
        let name = data
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("unknown")
            .to_lowercase()
            .replace(' ', "-");
        let position = data
            .get("position")
            .and_then(|p| p.as_i64())
            .unwrap_or(0);

        let channel_type = match data.get("type").and_then(|t| t.as_u64()) {
            Some(0) => ChannelType::Text,
            Some(2) => ChannelType::Voice,
            Some(5) => ChannelType::Text,   // Announcement
            Some(13) => ChannelType::Voice,  // Stage
            Some(15) => ChannelType::Text,   // Forum
            _ => ChannelType::Text,
        };

        Some(Channel {
            id,
            guild_id,
            name,
            channel_type,
            position,
        })
    }
}
