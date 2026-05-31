use std::env;

/// Application configuration loaded from environment variables.
#[allow(dead_code)]
pub struct Config {
    /// Discord user account token
    pub token: String,
    /// Optional default guild ID to select on startup
    pub default_guild_id: Option<u64>,
    /// Optional default channel ID to select on startup
    pub default_channel_id: Option<u64>,
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// Required:
    ///   - `DCRD_TOKEN` — Discord user account token (extracted from Discord client)
    ///
    /// Optional:
    ///   - `DCRD_GUILD_ID` — default guild to select
    ///   - `DCRD_CHANNEL_ID` — default channel to select
    pub fn load() -> anyhow::Result<Self> {
        let token = env::var("DCRD_TOKEN")
            .map(|t| t.trim().to_string())
            .map_err(|_| {
                anyhow::anyhow!(
                    "DCRD_TOKEN environment variable not set.\n\
                     Set it with: set DCRD_TOKEN=your_user_token_here"
                )
            })?;

        if token.is_empty() {
            return Err(anyhow::anyhow!("DCRD_TOKEN is empty"));
        }

        tracing::info!("DCRD_TOKEN loaded (len={}, starts_with_MTU={})", token.len(), token.starts_with("MTU"));

        let default_guild_id = env::var("DCRD_GUILD_ID")
            .ok()
            .and_then(|s| s.parse::<u64>().ok());

        let default_channel_id = env::var("DCRD_CHANNEL_ID")
            .ok()
            .and_then(|s| s.parse::<u64>().ok());

        Ok(Config {
            token,
            default_guild_id,
            default_channel_id,
        })
    }
}
