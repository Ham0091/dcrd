pub mod channel;
pub mod message;
pub mod server;
pub mod user;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64};

use dashmap::DashMap;
use tokio::sync::RwLock;

use channel::{Channel, ChannelType};
use message::{push_message, Message, MessageBuffer};
use server::Guild;
use user::User;

/// Voice connection state.
#[derive(Debug, Clone)]
pub struct VoiceState {
    pub connected: bool,
    pub guild_id: Option<u64>,
    pub channel_id: Option<u64>,
    pub channel_name: Option<String>,
    pub muted: bool,
    pub deafened: bool,
    /// Usernames of people currently in the voice channel.
    pub users: Vec<String>,
    /// Session ID from VOICE_STATE_UPDATE.
    pub session_id: Option<String>,
    /// Endpoint from VOICE_SERVER_UPDATE.
    pub endpoint: Option<String>,
    /// Token from VOICE_SERVER_UPDATE.
    pub token: Option<String>,
}

impl Default for VoiceState {
    fn default() -> Self {
        VoiceState {
            connected: false,
            guild_id: None,
            channel_id: None,
            channel_name: None,
            muted: false,
            deafened: false,
            users: Vec::new(),
            session_id: None,
            endpoint: None,
            token: None,
        }
    }
}

/// Shared application state — accessed concurrently by gateway and TUI tasks.
///
/// Uses `DashMap` for lock-free concurrent map access and `RwLock` for
/// infrequently-changing scalar values.
pub struct AppState {
    /// Discord bot token
    pub token: String,
    /// Current user info (from READY)
    pub user: RwLock<Option<User>>,
    /// Guilds (servers) — keyed by guild ID
    pub guilds: DashMap<u64, Guild>,
    /// All known channels — keyed by channel ID
    pub channels: DashMap<u64, Channel>,
    /// Per-channel message ring buffers — keyed by channel ID
    pub messages: DashMap<u64, MessageBuffer>,
    /// Currently selected guild ID
    pub current_guild_id: RwLock<Option<u64>>,
    /// Currently selected channel ID
    pub current_channel_id: RwLock<Option<u64>>,
    /// Voice connection state
    pub voice_state: RwLock<VoiceState>,
    /// Gateway sequence number (for heartbeats and resumes)
    pub gateway_seq: AtomicU64,
    /// Gateway session ID (for resumes)
    pub session_id: RwLock<Option<String>>,
    /// Whether the last heartbeat ACK was received
    pub heartbeat_ack_received: AtomicBool,
    /// Users in voice channels — keyed by (guild_id, user_id)
    pub voice_users: DashMap<(u64, u64), String>,
}

impl AppState {
    pub fn new(token: String) -> Self {
        AppState {
            token,
            user: RwLock::new(None),
            guilds: DashMap::new(),
            channels: DashMap::new(),
            messages: DashMap::new(),
            current_guild_id: RwLock::new(None),
            current_channel_id: RwLock::new(None),
            voice_state: RwLock::new(VoiceState::default()),
            gateway_seq: AtomicU64::new(0),
            session_id: RwLock::new(None),
            heartbeat_ack_received: AtomicBool::new(true),
            voice_users: DashMap::new(),
        }
    }

    /// Add a message to the appropriate channel's ring buffer.
    pub fn add_message(&self, msg: Message) {
        let channel_id = msg.channel_id;
        let mut entry = self
            .messages
            .entry(channel_id)
            .or_insert_with(VecDeque::new);
        push_message(&mut entry, msg);
    }

    /// Get a cloned snapshot of messages for a channel (oldest first).
    pub fn get_messages(&self, channel_id: u64) -> Vec<Message> {
        self.messages
            .get(&channel_id)
            .map(|buf| buf.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get text channels for a guild, sorted by position.
    pub fn get_text_channels(&self, guild_id: u64) -> Vec<Channel> {
        let mut chs: Vec<Channel> = self
            .channels
            .iter()
            .filter(|e| e.value().guild_id == guild_id && e.value().channel_type == ChannelType::Text)
            .map(|e| e.value().clone())
            .collect();
        chs.sort_by_key(|c| c.position);
        chs
    }

    /// Get voice channels for a guild, sorted by position.
    pub fn get_voice_channels(&self, guild_id: u64) -> Vec<Channel> {
        let mut chs: Vec<Channel> = self
            .channels
            .iter()
            .filter(|e| e.value().guild_id == guild_id && e.value().channel_type == ChannelType::Voice)
            .map(|e| e.value().clone())
            .collect();
        chs.sort_by_key(|c| c.position);
        chs
    }

    /// Find a text channel by name within a guild.
    pub fn find_text_channel_by_name(&self, guild_id: u64, name: &str) -> Option<Channel> {
        self.channels.iter().find_map(|entry| {
            let ch = entry.value();
            if ch.guild_id == guild_id
                && ch.channel_type == ChannelType::Text
                && ch.name == name
            {
                Some(ch.clone())
            } else {
                None
            }
        })
    }

    /// Find a voice channel by name within a guild.
    pub fn find_voice_channel_by_name(&self, guild_id: u64, name: &str) -> Option<Channel> {
        self.channels.iter().find_map(|entry| {
            let ch = entry.value();
            if ch.guild_id == guild_id
                && ch.channel_type == ChannelType::Voice
                && ch.name == name
            {
                Some(ch.clone())
            } else {
                None
            }
        })
    }

    /// Get voice channel users filtered by guild_id as a sorted list of usernames.
    pub fn get_voice_user_names_for_guild(&self, guild_id: u64) -> Vec<String> {
        let mut names: Vec<String> = self
            .voice_users
            .iter()
            .filter(|e| e.key().0 == guild_id)
            .map(|e| e.value().clone())
            .collect();
        names.sort();
        names.dedup();
        names
    }
}
