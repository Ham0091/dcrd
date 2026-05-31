/// Minimal guild (server) model.
#[derive(Debug, Clone)]
pub struct Guild {
    pub id: u64,
    pub name: String,
    pub icon: Option<String>,
}

impl Guild {
    /// Parse a guild from a GUILD_CREATE or READY.guilds[] JSON object.
    pub fn from_json(data: &serde_json::Value) -> Option<Self> {
        let id = data.get("id")?.as_str()?.parse::<u64>().ok()?;
        let name = data
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("Unknown Server")
            .to_string();
        let icon = data
            .get("icon")
            .and_then(|i| i.as_str())
            .map(|s| s.to_string());
        Some(Guild { id, name, icon })
    }
}
