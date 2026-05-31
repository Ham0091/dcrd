use reqwest::Client;
use serde_json::{json, Value};
use tracing::debug;

const API_BASE: &str = "https://discord.com/api/v10";

/// Minimal REST client for Discord API calls.
///
/// Only implements the endpoints we actually need:
/// - POST /channels/{id}/messages  — send a text message
/// - GET  /channels/{id}/messages  — fetch recent messages
pub struct RestClient {
    client: Client,
    token: String,
}

impl RestClient {
    pub fn new(token: &str) -> anyhow::Result<Self> {
        let client = Client::builder()
            .user_agent("dcrd/0.1.0")
            .build()?;
        Ok(RestClient {
            client,
            token: token.to_string(),
        })
    }

    /// Send a text message to a channel.
    ///
    /// Returns the created message JSON on success.
    pub async fn send_message(&self, channel_id: u64, content: &str) -> anyhow::Result<Value> {
        let url = format!("{}/channels/{}/messages", API_BASE, channel_id);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", &self.token)
            .json(&json!({ "content": content }))
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Send message failed ({}): {}",
                status,
                body
            ));
        }

        let body: Value = resp.json().await?;
        debug!("Message sent to channel {}", channel_id);
        Ok(body)
    }

    /// Fetch basic info for a single guild (name, etc.).
    ///
    /// Returns the guild JSON on success.
    pub async fn fetch_guild_info(&self, guild_id: u64) -> anyhow::Result<Value> {
        let url = format!("{}/guilds/{}", API_BASE, guild_id);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", &self.token)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Fetch guild info failed ({}): {}",
                status,
                body
            ));
        }

        let body: Value = resp.json().await?;
        debug!("Fetched guild info for {}", guild_id);
        Ok(body)
    }

    /// Fetch the user's guilds (GET /users/@me/guilds).
    pub async fn fetch_my_guilds(&self) -> anyhow::Result<Vec<Value>> {
        let url = format!("{}/users/@me/guilds", API_BASE);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", &self.token)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Fetch my guilds failed ({}): {}",
                status,
                body
            ));
        }

        let body: Vec<Value> = resp.json().await?;
        debug!("Fetched {} guilds from REST", body.len());
        Ok(body)
    }

    /// Fetch the most recent messages for a channel.
    ///
    /// Returns messages in reverse-chronological order (newest first).
    pub async fn fetch_messages(
        &self,
        channel_id: u64,
        limit: u64,
    ) -> anyhow::Result<Vec<Value>> {
        let url = format!(
            "{}/channels/{}/messages?limit={}",
            API_BASE, channel_id, limit
        );
        let resp = self
            .client
            .get(&url)
            .header("Authorization", &self.token)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Fetch messages failed ({}): {}",
                status,
                body
            ));
        }

        let body: Vec<Value> = resp.json().await?;
        debug!(
            "Fetched {} messages for channel {}",
            body.len(),
            channel_id
        );
        Ok(body)
    }
}
