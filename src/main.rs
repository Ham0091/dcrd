mod audio;
mod config;
mod gateway;
mod rest;
mod state;
mod tui;
mod voice;

use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};

/// Commands sent from the TUI to the gateway/REST handler.
pub enum Command {
    /// Send a text message to a channel.
    SendMessage {
        channel_id: u64,
        content: String,
    },
    /// Fetch recent messages for a channel (via REST).
    FetchMessages {
        channel_id: u64,
    },
    /// Update voice state (join/leave voice channel).
    VoiceStateUpdate {
        guild_id: u64,
        channel_id: Option<u64>,
        self_mute: bool,
        self_deaf: bool,
    },
    /// Toggle self-mute in voice.
    ToggleMute,
    /// Toggle self-deafen in voice.
    ToggleDeafen,
    /// Graceful shutdown.
    Quit,
}

/// Entry point — single-threaded tokio runtime.
///
/// Architecture:
/// - Gateway task: WebSocket events, heartbeats, REST calls, voice state
/// - TUI task (main): keyboard input, rendering, command dispatch
/// - Voice task: spawned on demand when joining a voice channel
/// - Audio threads: capture/playback via cpal on OS threads
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    // Initialize logging — redirect to file so it doesn't corrupt the TUI.
    // Log file lives next to the executable.
    let log_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("dcrd.log")))
        .unwrap_or_else(|| "dcrd.log".into());
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_thread_ids(false)
        .with_writer(std::sync::Mutex::new(log_file))
        .init();

    tracing::info!("dcrd — Ultra-low-RAM Discord client starting...");

    // Load configuration
    let cfg = config::Config::load()?;
    tracing::info!("Configuration loaded (token: {}...)", &cfg.token[..8.min(cfg.token.len())]);

    // Create shared application state
    let state = Arc::new(state::AppState::new(cfg.token.clone()));

    // Create REST client
    let rest = Arc::new(rest::api::RestClient::new(&cfg.token)?);

    // Create communication channels
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>(32);
    let (shutdown_tx, _) = broadcast::channel::<()>(1);

    // ── Spawn gateway task ───────────────────────────────────────────────
    let gw_state = state.clone();
    let gw_rest = rest.clone();
    let gw_shutdown = shutdown_tx.subscribe();
    tokio::spawn(async move {
        if let Err(e) = gateway::connection::run(gw_state, cmd_rx, gw_rest, gw_shutdown).await {
            tracing::error!("Gateway task error: {}", e);
        }
        tracing::info!("Gateway task exited");
    });

    // ── Run TUI on main task ─────────────────────────────────────────────
    let tui_result = tui::run(state, rest, cmd_tx, shutdown_tx).await;

    if let Err(ref e) = tui_result {
        tracing::error!("TUI error: {}", e);
    }

    tracing::info!("dcrd shutting down");
    tui_result
}
