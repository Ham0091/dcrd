/// Minimal user model — only stores self (no full Discord cache).
#[derive(Debug, Clone)]
pub struct User {
    pub id: u64,
    pub username: String,
}

impl User {
    /// Parse user from the READY event's `user` field.
    pub fn from_ready_user(data: &serde_json::Value) -> Option<Self> {
        let id = data.get("id")?.as_str()?.parse::<u64>().ok()?;
        let username = data
            .get("username")
            .and_then(|u| u.as_str())
            .unwrap_or("unknown")
            .to_string();
        Some(User { id, username })
    }
}
