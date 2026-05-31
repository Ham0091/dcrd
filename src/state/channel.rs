/// Channel type discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelType {
    Text = 0,
    Dm = 1,
    Voice = 2,
    GroupDm = 3,
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

        let raw_type = data.get("type").and_then(|t| t.as_u64());
        let channel_type = match raw_type {
            Some(0) => ChannelType::Text,
            Some(1) => ChannelType::Dm,
            Some(2) => ChannelType::Voice,
            Some(3) => ChannelType::GroupDm,
            Some(5) => ChannelType::Text,   // Announcement
            Some(13) => ChannelType::Voice,  // Stage
            Some(15) => ChannelType::Text,   // Forum
            _ => ChannelType::Text,
        };

        // DM channels have `recipients` instead of `name`
        let name = match channel_type {
            ChannelType::Dm => {
                // Single DM — use the recipient's username
                data.get("recipients")
                    .and_then(|r| r.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|u| u.get("username"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("DM")
                    .to_string()
            }
            ChannelType::GroupDm => {
                // Group DM — use the group name or list of recipients
                data.get("name")
                    .and_then(|n| n.as_str())
                    .map(|s| s.to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| {
                        data.get("recipients")
                            .and_then(|r| r.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|u| u.get("username").and_then(|n| n.as_str()))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            })
                            .unwrap_or_else(|| "Group DM".to_string())
                    })
            }
            _ => {
                data.get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown")
                    .to_lowercase()
                    .replace(' ', "-")
            }
        };

        let position = data
            .get("position")
            .and_then(|p| p.as_i64())
            .unwrap_or(0);

        Some(Channel {
            id,
            guild_id,
            name,
            channel_type,
            position,
        })
    }
}
