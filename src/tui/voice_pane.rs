use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::state::AppState;

// Theme colors (matching render.rs)
const BG_INPUT: Color = Color::Rgb(54, 57, 63);      // #36393f
const TEXT_NORMAL: Color = Color::Rgb(219, 222, 225); // #dbdee1
const TEXT_MUTED: Color = Color::Rgb(148, 155, 164);  // #949ba4
const ACCENT_GREEN: Color = Color::Rgb(87, 242, 135); // #57f287
const ACCENT_RED: Color = Color::Rgb(237, 66, 69);    // #ed4245
const DIVIDER: Color = Color::Rgb(66, 69, 74);        // #42454a

/// Render the voice/status bar at the bottom of the screen.
///
/// Priority order:
/// 1. Command status message (from :vc, :ch, etc.)
/// 2. Connected voice state with mute/deafen + keybinds
/// 3. Voice task status (connecting, failed)
/// 4. Default "not in voice" with keybinds
pub fn render_voice_bar(
    area: Rect,
    state: &AppState,
    frame: &mut ratatui::Frame,
    status_message: &str,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(DIVIDER))
        .style(Style::default().fg(TEXT_NORMAL).bg(BG_INPUT));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // ── Priority 1: Status message from commands ──
    if !status_message.is_empty() {
        let style = if status_message.starts_with('✓')
            || status_message.starts_with("Switched")
            || status_message.starts_with("Joining")
            || status_message.starts_with("Voice connected")
            || status_message.starts_with("Joined")
        {
            Style::default()
                .fg(ACCENT_GREEN)
                .add_modifier(Modifier::BOLD)
        } else if status_message.starts_with("Voice failed")
            || status_message.starts_with("No ")
            || status_message.starts_with("Unknown")
            || status_message.starts_with("Channel")
        {
            Style::default()
                .fg(ACCENT_RED)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT_NORMAL)
        };

        let line = Line::from(Span::styled(format!(" {}", status_message), style));
        frame.render_widget(Paragraph::new(line), inner);
        return;
    }

    // ── Priority 2: Connected voice state ──
    let voice_state = state.voice_state.try_read();
    if let Ok(ref voice) = voice_state {
        if voice.connected {
            let channel_name = voice.channel_name.as_deref().unwrap_or("unknown");
            let muted_icon = if voice.muted { "🔇" } else { "🔊" };
            let deaf_icon = if voice.deafened { "🔕" } else { "🔔" };

            let guild_id = voice.guild_id.unwrap_or(0);
            let user_count = state.get_voice_user_names_for_guild(guild_id).len();
            let users_text = if user_count > 0 {
                format!(" ({} users)", user_count)
            } else {
                String::new()
            };

            let line = Line::from(vec![
                Span::styled(
                    format!(" 🔊 {}{} ", channel_name, users_text),
                    Style::default()
                        .fg(ACCENT_GREEN)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{} {} ", muted_icon, deaf_icon),
                    Style::default().fg(TEXT_NORMAL),
                ),
                Span::styled(
                    "│ Ctrl+M Mute │ Ctrl+D Deafen │ :vc leave",
                    Style::default().fg(TEXT_MUTED),
                ),
            ]);
            frame.render_widget(Paragraph::new(line), inner);
            return;
        }
    }

    // ── Priority 3: Voice task status (connecting, failed, etc.) ──
    let voice_status = state
        .voice_status
        .try_read()
        .ok()
        .map(|s| s.clone())
        .unwrap_or_default();
    if !voice_status.is_empty() {
        let style = if voice_status.contains("failed") {
            Style::default()
                .fg(ACCENT_RED)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(ACCENT_GREEN)
                .add_modifier(Modifier::BOLD)
        };

        let line = Line::from(vec![
            Span::styled(format!(" ⏳ {}", voice_status), style),
            Span::styled(
                " │ Ctrl+M Mute │ Ctrl+D Deafen │ :vc leave",
                Style::default().fg(TEXT_MUTED),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), inner);
        return;
    }

    // ── Priority 4: Not connected — show keybinds ──
    let line = Line::from(vec![
        Span::styled(" Not in voice", Style::default().fg(TEXT_MUTED)),
        Span::styled(
            " │ :vc join │ Ctrl+M Mute │ Ctrl+D Deafen │ :help",
            Style::default().fg(TEXT_MUTED),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), inner);
}
