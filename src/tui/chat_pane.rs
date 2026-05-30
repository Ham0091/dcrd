use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::state::message::Message;

/// Render the chat message list in the given area.
///
/// Messages are displayed with:
/// - Timestamps in dim gray
/// - Usernames in hash-based colors (consistent per user)
/// - Message text in white
/// - Scrollable with scroll_offset (0 = show newest at bottom)
pub fn render_chat(
    area: Rect,
    messages: &[Message],
    scroll_offset: usize,
    frame: &mut ratatui::Frame,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Chat ")
        .title_style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if messages.is_empty() {
        let empty = Paragraph::new("No messages yet. Type a message below to get started.")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty, inner);
        return;
    }

    // Calculate visible messages based on scroll offset
    let visible_height = inner.height as usize;
    let total = messages.len();

    // end_idx = index of the last message to show (exclusive upper bound)
    let end = total.saturating_sub(scroll_offset);
    let start = end.saturating_sub(visible_height);

    let visible = &messages[start..end];

    let lines: Vec<Line> = visible
        .iter()
        .map(|msg| {
            let time = msg.display_time();
            let color = username_color(&msg.author_name);

            Line::from(vec![
                Span::styled(
                    format!("[{}] ", time),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("{}: ", msg.author_name),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(&msg.content),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

/// Generate a consistent color for a username using a hash.
fn username_color(name: &str) -> Color {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    let hash = hasher.finish();

    // Map to a set of distinguishable terminal colors
    const COLORS: &[Color] = &[
        Color::Rgb(255, 128, 128), // salmon
        Color::Rgb(128, 255, 128), // light green
        Color::Rgb(128, 128, 255), // light blue
        Color::Rgb(255, 255, 128), // yellow
        Color::Rgb(255, 128, 255), // magenta
        Color::Rgb(128, 255, 255), // cyan
        Color::Rgb(255, 192, 128), // orange
        Color::Rgb(192, 128, 255), // purple
        Color::Rgb(128, 255, 192), // mint
        Color::Rgb(255, 128, 192), // pink
    ];

    COLORS[(hash as usize) % COLORS.len()]
}
