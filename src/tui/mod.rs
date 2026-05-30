pub mod app;
pub mod chat_pane;
pub mod input;
pub mod render;
pub mod voice_pane;

use std::sync::Arc;
use std::time::Duration;

use crossterm::event::EventStream;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures_util::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info};

use crate::rest::api::RestClient;
use crate::state::AppState;
use crate::Command;

use app::TuiApp;

/// Run the TUI event loop on the main thread.
///
/// This function:
/// 1. Sets up the terminal (raw mode, alternate screen)
/// 2. Creates the ratatui Terminal
/// 3. Enters the main event loop (keyboard input + rendering)
/// 4. Cleans up the terminal on exit
pub async fn run(
    state: Arc<AppState>,
    rest: Arc<RestClient>,
    cmd_tx: mpsc::Sender<Command>,
    shutdown_tx: broadcast::Sender<()>,
) -> anyhow::Result<()> {
    // ── Terminal setup ───────────────────────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let result = run_inner(&mut terminal, &state, &rest, &cmd_tx, &shutdown_tx).await;

    // ── Terminal cleanup ─────────────────────────────────────────────────
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run_inner(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    state: &Arc<AppState>,
    _rest: &Arc<RestClient>,
    cmd_tx: &mpsc::Sender<Command>,
    shutdown_tx: &broadcast::Sender<()>,
) -> anyhow::Result<()> {
    let mut app = TuiApp::new();
    let mut event_stream = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(33)); // ~30 FPS

    // Fetch initial messages if we have a channel selected
    {
        let channel_id = *state.current_channel_id.read().await;
        if let Some(cid) = channel_id {
            let _ = cmd_tx.send(Command::FetchMessages { channel_id: cid }).await;
        }
    }

    info!("TUI started");

    loop {
        // ── Render ─────────────────────────────────────────────────────
        terminal.draw(|frame| {
            render::render(frame, state, &app);
        })?;

        // ── Event handling ─────────────────────────────────────────────
        tokio::select! {
            // Keyboard / terminal events
            maybe_event = event_stream.next() => {
                match maybe_event {
                    Some(Ok(event)) => {
                        input::handle_input(event, &mut app, state, cmd_tx).await;
                    }
                    Some(Err(e)) => {
                        error!("Terminal event error: {}", e);
                    }
                    None => {
                        info!("Event stream ended");
                        break;
                    }
                }
            }

            // Tick (for periodic re-render even without events)
            _ = tick.tick() => {
                // Nothing special — just triggers a re-render
            }
        }

        // ── Quit check ─────────────────────────────────────────────────
        if app.should_quit {
            let _ = shutdown_tx.send(());
            break;
        }
    }

    info!("TUI exiting");
    Ok(())
}
