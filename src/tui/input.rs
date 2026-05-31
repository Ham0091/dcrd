use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::sync::Arc;
use tokio::sync::mpsc;
use crate::state::AppState;
use crate::Command;

use super::app::{InputMode, TuiApp};

/// Process a single crossterm event, updating TUI state and sending commands.
pub async fn handle_input(
    event: Event,
    app: &mut TuiApp,
    state: &Arc<AppState>,
    cmd_tx: &mpsc::Sender<Command>,
) {
    match event {
        // On Windows, crossterm 0.28+ generates Press, Release, and Repeat events.
        // Only handle Press to avoid double-character input on each keystroke.
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            handle_key(key, app, state, cmd_tx).await;
        }
        _ => {}
    }
}

async fn handle_key(
    key: KeyEvent,
    app: &mut TuiApp,
    state: &Arc<AppState>,
    cmd_tx: &mpsc::Sender<Command>,
) {
    // Global keybindings (work in any mode)
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('c') => {
                app.should_quit = true;
                let _ = cmd_tx.send(Command::Quit).await;
                return;
            }
            KeyCode::Char('m') => {
                let _ = cmd_tx.send(Command::ToggleMute).await;
                return;
            }
            KeyCode::Char('d') => {
                let _ = cmd_tx.send(Command::ToggleDeafen).await;
                return;
            }
            KeyCode::Up => {
                // Switch to previous channel
                switch_channel_by_offset(state, cmd_tx, -1).await;
                app.scroll_to_bottom();
                return;
            }
            KeyCode::Down => {
                // Switch to next channel
                switch_channel_by_offset(state, cmd_tx, 1).await;
                app.scroll_to_bottom();
                return;
            }
            _ => {}
        }
    }

    match app.mode {
        InputMode::Normal => handle_normal_mode(key, app, state, cmd_tx).await,
        InputMode::Insert => handle_insert_mode(key, app, state, cmd_tx).await,
    }
}

async fn handle_normal_mode(
    key: KeyEvent,
    app: &mut TuiApp,
    _state: &Arc<AppState>,
    _cmd_tx: &mpsc::Sender<Command>,
) {
    match key.code {
        // Enter Insert mode by typing any printable character
        KeyCode::Char(c) => {
            app.enter_insert();
            app.insert_char(c);
        }
        // Enter Insert mode explicitly
        KeyCode::Enter => {
            app.enter_insert();
        }
        // Scroll
        KeyCode::Up => app.scroll_up(),
        KeyCode::Down => app.scroll_down(),
        _ => {}
    }
}

async fn handle_insert_mode(
    key: KeyEvent,
    app: &mut TuiApp,
    state: &Arc<AppState>,
    cmd_tx: &mpsc::Sender<Command>,
) {
    match key.code {
        // Exit insert mode
        KeyCode::Esc => {
            app.enter_normal();
        }
        // Send message or execute command
        KeyCode::Enter => {
            let input = app.take_input();
            let trimmed = input.trim();
            if trimmed.is_empty() {
                return;
            }

            if trimmed.starts_with(':') {
                execute_command(trimmed, app, state, cmd_tx).await;
            } else {
                // Send as message
                let channel_id = *state.current_channel_id.read().await;
                if let Some(cid) = channel_id {
                    // Local echo — add message to state immediately so it appears in chat
                    let user_name = state
                        .user
                        .try_read()
                        .ok()
                        .and_then(|u| u.as_ref().map(|u| u.username.clone()))
                        .unwrap_or_else(|| "you".to_string());
                    let user_id = state
                        .user
                        .try_read()
                        .ok()
                        .and_then(|u| u.as_ref().map(|u| u.id))
                        .unwrap_or(0);
                    let echo_msg = crate::state::message::Message {
                        id: 0, // Will be replaced when Discord echoes it back
                        channel_id: cid,
                        author_name: user_name,
                        author_id: user_id,
                        content: trimmed.to_string(),
                        timestamp: String::new(),
                    };
                    state.add_message(echo_msg);

                    let _ = cmd_tx
                        .send(Command::SendMessage {
                            channel_id: cid,
                            content: trimmed.to_string(),
                        })
                        .await;
                    app.set_status(format!("✓ Sent: {}", trimmed.chars().take(60).collect::<String>()));
                    app.scroll_to_bottom();
                } else {
                    app.set_status("No channel selected — use :ch to switch".to_string());
                }
            }
        }
        // Character input — clear status message on typing
        KeyCode::Char(c) => {
            if !app.status_message.is_empty() {
                app.status_message.clear();
            }
            app.insert_char(c);
        }
        // Backspace
        KeyCode::Backspace => {
            app.delete_char();
        }
        // Delete
        KeyCode::Delete => {
            if app.cursor < app.input.len() {
                let next = app.input[app.cursor..]
                    .char_indices()
                    .nth(1)
                    .map(|(i, _)| app.cursor + i)
                    .unwrap_or(app.input.len());
                app.input.drain(app.cursor..next);
            }
        }
        // Cursor movement
        KeyCode::Left => app.cursor_left(),
        KeyCode::Right => app.cursor_right(),
        // Home/End
        KeyCode::Home => app.cursor = 0,
        KeyCode::End => app.cursor = app.input.len(),
        _ => {}
    }
}

/// Execute a colon-command (e.g. `:vc join`, `:quit`).
async fn execute_command(
    input: &str,
    app: &mut TuiApp,
    state: &Arc<AppState>,
    cmd_tx: &mpsc::Sender<Command>,
) {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.is_empty() {
        return;
    }

    match parts[0] {
        ":quit" | ":q" => {
            app.should_quit = true;
            let _ = cmd_tx.send(Command::Quit).await;
        }

        ":vc" => {
            handle_vc_command(&parts[1..], app, state, cmd_tx).await;
        }

        ":ch" => {
            handle_ch_command(&parts[1..], app, state, cmd_tx).await;
        }

        ":srv" => {
            handle_srv_command(&parts[1..], app, state, cmd_tx).await;
        }

        ":dm" => {
            handle_dm_command(&parts[1..], app, state, cmd_tx).await;
        }

        ":help" | ":h" => {
            app.set_status(
                "Commands: :vc join/leave | :ch #name | :srv name | :dm list/name | :quit | Ctrl+M mute | Ctrl+D deafen".to_string(),
            );
        }

        _ => {
            app.set_status(format!("Unknown command: {}", parts[0]));
        }
    }
}

async fn handle_vc_command(
    args: &[&str],
    app: &mut TuiApp,
    state: &Arc<AppState>,
    cmd_tx: &mpsc::Sender<Command>,
) {
    if args.is_empty() {
        app.set_status("Usage: :vc join [channel-name] | :vc leave".to_string());
        return;
    }

    match args[0] {
        "join" => {
            let guild_id = *state.current_guild_id.read().await;
            if let Some(gid) = guild_id {
                // Determine which voice channel to join
                let channel_id = if args.len() > 1 {
                    // Named channel
                    let name = args[1].trim_start_matches('#');
                    state
                        .find_voice_channel_by_name(gid, name)
                        .map(|ch| ch.id)
                } else {
                    // First voice channel in the guild
                    state
                        .get_voice_channels(gid)
                        .first()
                        .map(|ch| ch.id)
                };

                if let Some(cid) = channel_id {
                    let _ = cmd_tx
                        .send(Command::VoiceStateUpdate {
                            guild_id: gid,
                            channel_id: Some(cid),
                            self_mute: false,
                            self_deaf: false,
                        })
                        .await;
                    app.set_status(format!("Joining voice channel..."));
                } else {
                    app.set_status("No voice channel found".to_string());
                }
            } else {
                app.set_status("No guild selected".to_string());
            }
        }
        "leave" => {
            let guild_id = *state.current_guild_id.read().await;
            if let Some(gid) = guild_id {
                let _ = cmd_tx
                    .send(Command::VoiceStateUpdate {
                        guild_id: gid,
                        channel_id: None,
                        self_mute: false,
                        self_deaf: false,
                    })
                    .await;
                app.set_status("Leaving voice channel...".to_string());
            } else {
                app.set_status("No guild selected".to_string());
            }
        }
        _ => {
            app.set_status("Usage: :vc join [channel-name] | :vc leave".to_string());
        }
    }
}

async fn handle_ch_command(
    args: &[&str],
    app: &mut TuiApp,
    state: &Arc<AppState>,
    cmd_tx: &mpsc::Sender<Command>,
) {
    if args.is_empty() {
        // List channels
        let guild_id = *state.current_guild_id.read().await;
        if let Some(gid) = guild_id {
            let channels = state.get_text_channels(gid);
            let names: Vec<&str> = channels.iter().map(|c| c.name.as_str()).collect();
            app.set_status(format!("Channels: {}", names.join(", ")));
        } else {
            app.set_status("No guild selected".to_string());
        }
        return;
    }

    let name = args[0].trim_start_matches('#').to_lowercase();
    let guild_id = *state.current_guild_id.read().await;

    if let Some(gid) = guild_id {
        if let Some(ch) = state.find_text_channel_by_name(gid, &name) {
            *state.current_channel_id.write().await = Some(ch.id);
            app.scroll_to_bottom();
            app.set_status(format!("Switched to #{}", ch.name));

            // Fetch recent messages for the new channel
            let _ = cmd_tx
                .send(Command::FetchMessages {
                    channel_id: ch.id,
                })
                .await;
        } else {
            app.set_status(format!("Channel #{} not found", name));
        }
    } else {
        app.set_status("No guild selected".to_string());
    }
}

async fn handle_srv_command(
    args: &[&str],
    app: &mut TuiApp,
    state: &Arc<AppState>,
    cmd_tx: &mpsc::Sender<Command>,
) {
    if args.is_empty() {
        // List guilds
        let guilds: Vec<String> = state.guilds.iter().map(|e| e.value().name.clone()).collect();
        app.set_status(format!("Servers: {}", guilds.join(", ")));
        return;
    }

    let name = args.join(" ").to_lowercase();

    // Find guild by name (case-insensitive partial match)
    let found = state.guilds.iter().find_map(|entry| {
        let guild = entry.value();
        if guild.name.to_lowercase().contains(&name) {
            Some((guild.id, guild.name.clone()))
        } else {
            None
        }
    });

    if let Some((gid, gname)) = found {
        *state.current_guild_id.write().await = Some(gid);

        // Switch to first text channel in the new guild
        let channels = state.get_text_channels(gid);
        if let Some(first_ch) = channels.first() {
            *state.current_channel_id.write().await = Some(first_ch.id);
            let _ = cmd_tx
                .send(Command::FetchMessages {
                    channel_id: first_ch.id,
                })
                .await;
            app.scroll_to_bottom();
            app.set_status(format!("Switched to server: {}", gname));
        } else {
            // No channels loaded yet — fetch them via REST
            *state.current_channel_id.write().await = None;
            let _ = cmd_tx
                .send(Command::FetchGuildChannels { guild_id: gid })
                .await;
            app.set_status(format!("Loading channels for {}...", gname));
        }
    } else {
        app.set_status(format!("Server '{}' not found", name));
    }
}

/// Switch to the next/previous text channel in the current guild.
async fn switch_channel_by_offset(
    state: &Arc<AppState>,
    cmd_tx: &mpsc::Sender<Command>,
    offset: i32,
) {
    let guild_id = *state.current_guild_id.read().await;
    let current_channel_id = *state.current_channel_id.read().await;

    if let (Some(gid), Some(ccid)) = (guild_id, current_channel_id) {
        let channels = state.get_text_channels(gid);
        if channels.is_empty() {
            return;
        }

        let current_idx = channels
            .iter()
            .position(|c| c.id == ccid)
            .unwrap_or(0);

        let len = channels.len();
        let new_idx = ((current_idx as isize + offset as isize).rem_euclid(len as isize)) as usize;

        let new_channel = &channels[new_idx];
        *state.current_channel_id.write().await = Some(new_channel.id);

        let _ = cmd_tx
            .send(Command::FetchMessages {
                channel_id: new_channel.id,
            })
            .await;
    }
}

async fn handle_dm_command(
    args: &[&str],
    app: &mut TuiApp,
    state: &Arc<AppState>,
    cmd_tx: &mpsc::Sender<Command>,
) {
    if args.is_empty() || args[0] == "list" {
        // List DM channels
        let dms = state.get_dm_channels();
        if dms.is_empty() {
            app.set_status("No DM channels found".to_string());
        } else {
            let names: Vec<String> = dms.iter().map(|c| c.name.clone()).collect();
            app.set_status(format!("DMs: {}", names.join(", ")));
        }
        return;
    }

    let name = args.join(" ").to_lowercase();

    // Find DM by recipient name (case-insensitive partial match)
    let dms = state.get_dm_channels();
    let found = dms.iter().find(|ch| ch.name.to_lowercase().contains(&name));

    if let Some(ch) = found {
        // Set current_guild_id to 0 to indicate DM mode
        *state.current_guild_id.write().await = Some(0);
        *state.current_channel_id.write().await = Some(ch.id);
        app.scroll_to_bottom();
        app.set_status(format!("Switched to DM with {}", ch.name));

        // Fetch recent messages
        let _ = cmd_tx
            .send(Command::FetchMessages {
                channel_id: ch.id,
            })
            .await;
    } else {
        app.set_status(format!("DM with '{}' not found. Use :dm list to see available DMs.", name));
    }
}
