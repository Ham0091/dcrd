use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use std::sync::Arc;

use crate::state::AppState;

use super::app::{InputMode, TuiApp};
use super::chat_pane;
use super::voice_pane;

/// Render the entire TUI frame.
///
/// Layout:
/// ┌──────────────────────────────────┐
/// │ Title bar (server/channel info)  │
/// ├──────────────────────────────────┤
/// │                                  │
/// │ Chat pane (scrollable messages)  │ ~75% height
/// │                                  │
/// ├──────────────────────────────────┤
/// │ > Input area                     │ ~3 lines
/// ├──────────────────────────────────┤
/// │ Voice status bar                 │ ~3 lines
/// └──────────────────────────────────┘
pub fn render(
    frame: &mut ratatui::Frame,
    state: &Arc<AppState>,
    app: &TuiApp,
) {
    let size = frame.area();

    // Main vertical layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // Title bar
            Constraint::Min(5),    // Chat pane
            Constraint::Length(3), // Input area
            Constraint::Length(3), // Voice status bar
        ])
        .split(size);

    // ── Title bar ────────────────────────────────────────────────────────
    render_title_bar(chunks[0], state, frame);

    // ── Chat pane ────────────────────────────────────────────────────────
    let channel_id = state.current_channel_id.try_read().ok();
    let cid = channel_id.as_ref().and_then(|c| **c);
    let messages = cid
        .map(|id| state.get_messages(id))
        .unwrap_or_default();

    chat_pane::render_chat(chunks[1], &messages, app.scroll_offset, frame);

    // ── Input area ───────────────────────────────────────────────────────
    render_input_area(chunks[2], app, frame);

    // ── Voice status bar ─────────────────────────────────────────────────
    voice_pane::render_voice_bar(chunks[3], state, frame);
}

/// Render the top title bar showing current server and channel.
fn render_title_bar(area: Rect, state: &Arc<AppState>, frame: &mut ratatui::Frame) {
    let guild_id = state.current_guild_id.try_read().ok();
    let channel_id = state.current_channel_id.try_read().ok();

    let gid = guild_id.as_ref().and_then(|g| **g);
    let cid = channel_id.as_ref().and_then(|c| **c);

    let guild_name = gid
        .and_then(|id| state.guilds.get(&id))
        .map(|g| g.name.clone())
        .unwrap_or_else(|| "No Server".to_string());

    let channel_name = cid
        .and_then(|id| state.channels.get(&id))
        .map(|c| format!("#{}", c.name))
        .unwrap_or_else(|| "#none".to_string());

    let user_name = state
        .user
        .try_read()
        .ok()
        .and_then(|u| u.as_ref().map(|u| u.username.clone()))
        .unwrap_or_default();

    let title_line = Line::from(vec![
        Span::styled(
            " dcrd ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{} ", channel_name),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::styled(guild_name, Style::default().fg(Color::White)),
        Span::styled(
            format!(" │ {}", user_name),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let block = Block::default()
        .style(Style::default().bg(Color::Rgb(30, 30, 40)));

    let paragraph = Paragraph::new(title_line).block(block);
    frame.render_widget(paragraph, area);
}

/// Render the text input area.
fn render_input_area(area: Rect, app: &TuiApp, frame: &mut ratatui::Frame) {
    let mode_indicator = match app.mode {
        InputMode::Normal => Span::styled(
            " [NORMAL] ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        InputMode::Insert => Span::styled(
            " [INSERT] ",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Input ")
        .title_style(Style::default().fg(Color::White));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let input_text = if app.input.is_empty() && app.mode == InputMode::Normal {
        "Press any key or Enter to start typing...".to_string()
    } else {
        format!("> {}", app.input)
    };

    let style = if app.input.is_empty() && app.mode == InputMode::Normal {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };

    let line = Line::from(vec![mode_indicator, Span::styled(input_text, style)]);
    let paragraph = Paragraph::new(line);

    frame.render_widget(paragraph, inner);

    // Show cursor in insert mode
    if app.mode == InputMode::Insert {
        let cursor_x = inner.x + 2 + 10 + app.cursor as u16; // "> " + "[INSERT]" + cursor pos
        let cursor_x = cursor_x.min(inner.x + inner.width.saturating_sub(1));
        frame.set_cursor_position((cursor_x, inner.y));
    }
}
