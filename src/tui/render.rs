use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use std::sync::Arc;

use crate::state::channel::ChannelType;
use crate::state::AppState;

use super::app::{InputMode, TuiApp};
use super::chat_pane;
use super::voice_pane;

// ─── Discord-inspired dark theme palette ─────────────────────────────────────

const BG_DARK: Color = Color::Rgb(32, 34, 37);       // #202225
const BG_SIDEBAR: Color = Color::Rgb(26, 27, 30);    // #1a1b1e
const BG_INPUT: Color = Color::Rgb(54, 57, 63);      // #36393f
const TEXT_NORMAL: Color = Color::Rgb(219, 222, 225); // #dbdee1
const TEXT_MUTED: Color = Color::Rgb(148, 155, 164);  // #949ba4
const ACCENT_BLUE: Color = Color::Rgb(88, 101, 242);  // #5865f2
const ACCENT_GREEN: Color = Color::Rgb(87, 242, 135); // #57f287
const DIVIDER: Color = Color::Rgb(66, 69, 74);        // #42454a
const SELECTED_BG: Color = Color::Rgb(54, 57, 63);    // #36393f

/// Render the entire TUI frame.
///
/// Layout:
/// ┌──────────────────────────────────────────────────────────┐
/// │ Title bar (dcrd · channel · server · user · mode)        │ 1 line
/// ├─────────┬────────────────────────────────────────────────┤
/// │ SERVERS │                                                │
/// │ ► srv1  │                                                │
/// │   srv2  │         Chat messages (scrollable)             │ Min 5
/// │─────────│                                                │
/// │ CHANNELS│                                                │
/// │ ► gen   │                                                │
/// │   voice │                                                │
/// ├─────────┴────────────────────────────────────────────────┤
/// │ > Input area                                             │ 3 lines
/// ├──────────────────────────────────────────────────────────┤
/// │ Voice/status bar + keybind hints                         │ 3 lines
/// └──────────────────────────────────────────────────────────┘
pub fn render(
    frame: &mut ratatui::Frame,
    state: &Arc<AppState>,
    app: &TuiApp,
) {
    let size = frame.area();
    let show_sidebar = size.width >= 72;

    // Main vertical layout
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // Title bar
            Constraint::Min(5),    // Main content (sidebar + chat)
            Constraint::Length(3), // Input area
            Constraint::Length(3), // Voice/status + keybinds
        ])
        .split(size);

    // ── Title bar (full width) ──
    render_title_bar(main_chunks[0], state, app, frame);

    // ── Main content: sidebar + chat ──
    if show_sidebar {
        let content_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(22), // Sidebar
                Constraint::Min(10),   // Chat pane
            ])
            .split(main_chunks[1]);

        render_sidebar(content_chunks[0], state, frame);
        render_chat_section(content_chunks[1], state, app, frame);
    } else {
        render_chat_section(main_chunks[1], state, app, frame);
    }

    // ── Input area (full width) ──
    render_input_area(main_chunks[2], app, frame);

    // ── Voice/status bar (full width) ──
    voice_pane::render_voice_bar(main_chunks[3], state, frame, &app.status_message);
}

// ─── Chat Section ────────────────────────────────────────────────────────────

fn render_chat_section(
    area: Rect,
    state: &Arc<AppState>,
    app: &TuiApp,
    frame: &mut ratatui::Frame,
) {
    let cid = state
        .current_channel_id
        .try_read()
        .ok()
        .and_then(|c| *c);
    let messages = cid
        .map(|id| state.get_messages(id))
        .unwrap_or_default();

    chat_pane::render_chat(area, &messages, app.scroll_offset, frame);
}

// ─── Title Bar ───────────────────────────────────────────────────────────────

fn render_title_bar(
    area: Rect,
    state: &Arc<AppState>,
    app: &TuiApp,
    frame: &mut ratatui::Frame,
) {
    let gid = state
        .current_guild_id
        .try_read()
        .ok()
        .and_then(|g| *g);
    let cid = state
        .current_channel_id
        .try_read()
        .ok()
        .and_then(|c| *c);

    let is_dm = gid == Some(0);
    let guild_name = if is_dm {
        "Direct Messages".to_string()
    } else {
        gid.and_then(|id| state.guilds.get(&id))
            .map(|g| g.name.clone())
            .unwrap_or_else(|| "No Server".to_string())
    };

    let channel_name = cid
        .and_then(|id| state.channels.get(&id))
        .map(|c| {
            if c.channel_type == ChannelType::Dm || c.channel_type == ChannelType::GroupDm {
                format!("💬 {}", c.name)
            } else {
                format!("#{}", c.name)
            }
        })
        .unwrap_or_else(|| "none".to_string());

    let user_name = state
        .user
        .try_read()
        .ok()
        .and_then(|u| u.as_ref().map(|u| u.username.clone()))
        .unwrap_or_default();

    let (mode_text, mode_color) = match app.mode {
        InputMode::Normal => ("NORMAL", ACCENT_BLUE),
        InputMode::Insert => ("INSERT", ACCENT_GREEN),
    };

    let title_line = Line::from(vec![
        Span::styled(
            " dcrd ",
            Style::default()
                .fg(Color::White)
                .bg(ACCENT_BLUE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {} ", channel_name),
            Style::default()
                .fg(TEXT_NORMAL)
                .bg(BG_DARK)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · ", Style::default().fg(DIVIDER).bg(BG_DARK)),
        Span::styled(
            format!("{} ", guild_name),
            Style::default().fg(TEXT_MUTED).bg(BG_DARK),
        ),
        Span::styled(" · ", Style::default().fg(DIVIDER).bg(BG_DARK)),
        Span::styled(
            format!("{} ", user_name),
            Style::default().fg(TEXT_MUTED).bg(BG_DARK),
        ),
        Span::styled(
            format!(" [{}]", mode_text),
            Style::default()
                .fg(mode_color)
                .bg(BG_DARK)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let block = Block::default().style(Style::default().bg(BG_DARK));
    frame.render_widget(Paragraph::new(title_line).block(block), area);
}

// ─── Sidebar ─────────────────────────────────────────────────────────────────

fn render_sidebar(area: Rect, state: &Arc<AppState>, frame: &mut ratatui::Frame) {
    let block = Block::default().style(Style::default().bg(BG_SIDEBAR));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let current_guild_id = state
        .current_guild_id
        .try_read()
        .ok()
        .and_then(|g| *g);
    let current_channel_id = state
        .current_channel_id
        .try_read()
        .ok()
        .and_then(|c| *c);
    let is_dm_mode = current_guild_id == Some(0);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // "SERVERS" header
            Constraint::Min(2),   // Server list
            Constraint::Length(1), // Divider
            Constraint::Length(1), // "CHANNELS" header
            Constraint::Min(2),   // Channel list
        ])
        .split(inner);

    // ── Servers header ──
    frame.render_widget(
        Paragraph::new(Span::styled(
            " SERVERS",
            Style::default()
                .fg(TEXT_MUTED)
                .add_modifier(Modifier::BOLD),
        )),
        chunks[0],
    );

    // ── Server list ──
    let mut guilds: Vec<(u64, String)> = state
        .guilds
        .iter()
        .map(|e| (*e.key(), e.value().name.clone()))
        .collect();
    guilds.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));

    let mut server_lines: Vec<Line> = Vec::new();

    // DMs entry
    let dm_style = if is_dm_mode {
        Style::default()
            .fg(TEXT_NORMAL)
            .bg(SELECTED_BG)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_MUTED)
    };
    server_lines.push(Line::from(vec![
        Span::styled(if is_dm_mode { " ▸ " } else { "   " }, dm_style),
        Span::styled("💬 Direct Messages", dm_style),
    ]));

    for (gid, gname) in &guilds {
        let is_selected = Some(*gid) == current_guild_id;
        let style = if is_selected {
            Style::default()
                .fg(TEXT_NORMAL)
                .bg(SELECTED_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT_MUTED)
        };
        let indicator = if is_selected { " ▸ " } else { "   " };
        let display_name = truncate_str(gname, 17);
        server_lines.push(Line::from(vec![
            Span::styled(indicator, style),
            Span::styled(display_name, style),
        ]));
    }

    frame.render_widget(Paragraph::new(server_lines), chunks[1]);

    // ── Divider ──
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " ────────────────────",
            Style::default().fg(DIVIDER),
        ))),
        chunks[2],
    );

    // ── Channels header ──
    frame.render_widget(
        Paragraph::new(Span::styled(
            " CHANNELS",
            Style::default()
                .fg(TEXT_MUTED)
                .add_modifier(Modifier::BOLD),
        )),
        chunks[3],
    );

    // ── Channel list ──
    let mut channel_lines: Vec<Line> = Vec::new();

    if is_dm_mode {
        // Show DM channels
        let dms = state.get_dm_channels();
        for ch in &dms {
            let is_selected = Some(ch.id) == current_channel_id;
            let style = if is_selected {
                Style::default()
                    .fg(TEXT_NORMAL)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT_MUTED)
            };
            let indicator = if is_selected { "▸ " } else { "  " };
            let name = truncate_str(&ch.name, 17);
            channel_lines.push(Line::from(Span::styled(
                format!(" {}{}", indicator, name),
                style,
            )));
        }
    } else if let Some(gid) = current_guild_id {
        // Text channels
        let text_channels = state.get_text_channels(gid);
        for ch in &text_channels {
            let is_selected = Some(ch.id) == current_channel_id;
            let style = if is_selected {
                Style::default()
                    .fg(TEXT_NORMAL)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT_MUTED)
            };
            let indicator = if is_selected { "▸" } else { " " };
            let name = truncate_str(&ch.name, 16);
            channel_lines.push(Line::from(Span::styled(
                format!(" {} #{}", indicator, name),
                style,
            )));
        }

        // Voice channels
        let voice_channels = state.get_voice_channels(gid);
        if !voice_channels.is_empty() {
            channel_lines.push(Line::from(Span::styled(
                " ── Voice ──",
                Style::default().fg(DIVIDER),
            )));
            for ch in &voice_channels {
                let name = truncate_str(&ch.name, 15);
                channel_lines.push(Line::from(Span::styled(
                    format!("   🔊 {}", name),
                    Style::default().fg(ACCENT_GREEN),
                )));
            }
        }
    }

    if channel_lines.is_empty() {
        channel_lines.push(Line::from(Span::styled(
            "  No channels",
            Style::default().fg(TEXT_MUTED),
        )));
    }

    frame.render_widget(Paragraph::new(channel_lines), chunks[4]);
}

// ─── Input Area ──────────────────────────────────────────────────────────────

fn render_input_area(area: Rect, app: &TuiApp, frame: &mut ratatui::Frame) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(DIVIDER))
        .style(Style::default().bg(BG_INPUT));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let is_empty_normal = app.input.is_empty() && app.mode == InputMode::Normal;

    let input_text = if is_empty_normal {
        "Type a message or :help for commands…"
    } else {
        app.input.as_str()
    };

    let text_style = if is_empty_normal {
        Style::default().fg(TEXT_MUTED)
    } else {
        Style::default().fg(TEXT_NORMAL)
    };

    let line = Line::from(vec![
        Span::styled(
            "> ",
            Style::default()
                .fg(ACCENT_BLUE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(input_text, text_style),
    ]);

    frame.render_widget(Paragraph::new(line), inner);

    // Show cursor in insert mode
    if app.mode == InputMode::Insert {
        let cursor_x = inner.x + 2 + app.cursor as u16; // "> " = 2 chars
        let cursor_x = cursor_x.min(inner.x + inner.width.saturating_sub(1));
        frame.set_cursor_position((cursor_x, inner.y));
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len - 1])
    }
}
