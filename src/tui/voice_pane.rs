use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::state::AppState;

/// Render the voice status bar at the bottom of the screen.
///
/// Shows:
/// - Voice channel name (or "Not connected")
/// - Connected users
/// - Mute/Deafen status
/// - Help hint
pub fn render_voice_bar(area: Rect, state: &AppState, frame: &mut ratatui::Frame) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Voice ")
        .title_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // We need to read voice state synchronously for rendering.
    // Use try_read to avoid blocking the render thread.
    let voice_state = state.voice_state.try_read();

    let line = if let Ok(ref voice) = voice_state {
        if voice.connected {
            let channel_name = voice.channel_name.as_deref().unwrap_or("unknown");
            let muted = if voice.muted { "🔇" } else { "🔊" };
            let deafened = if voice.deafened { "🔕" } else { "🔔" };

            // Get voice users for the current guild only
            let guild_id = voice.guild_id.unwrap_or(0);
            let users = state.get_voice_user_names_for_guild(guild_id);
            let user_list = if users.is_empty() {
                String::from("none")
            } else {
                users.join(", ")
            };

            Line::from(vec![
                Span::styled(
                    format!("🔊 {} ", channel_name),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("│ Users: {} ", user_list),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    format!("│ Mute: {} ", muted),
                    Style::default().fg(if voice.muted {
                        Color::Red
                    } else {
                        Color::Green
                    }),
                ),
                Span::styled(
                    format!("│ Deaf: {} ", deafened),
                    Style::default().fg(if voice.deafened {
                        Color::Red
                    } else {
                        Color::Green
                    }),
                ),
                Span::styled(
                    "│ :help for commands",
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        } else {
            Line::from(vec![
                Span::styled(
                    "Not connected to voice",
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    " │ :vc join | :help for commands",
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        }
    } else {
        Line::from(Span::styled(
            "Voice state loading...",
            Style::default().fg(Color::DarkGray),
        ))
    };

    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, inner);
}
