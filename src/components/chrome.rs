use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

/// Column at which the tab bar starts, measured from the left edge of the frame
pub const TAB_BAR_X: u16 = 14;

/// Draws the rounded content frame and notch its top border underneath the tab chips,
/// so the selected chip reads as continuous with the panel below it
///
/// ## Arguments
/// * `area` - The full content area, border included
/// * `tabs` - Tab labels, in the order they are drawn
/// * `selected` - Index of the active tab
///
/// ## Returns
/// * `Rect` - The area inside the border, for callers to draw into
pub fn content_frame(frame: &mut Frame, area: Rect, tabs: &[&str], selected: usize) -> Rect {
    let dim = Style::default().fg(Color::DarkGray);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(dim);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Overwrite the top border below the chips
    let mut spans = Vec::with_capacity(tabs.len() * 5);
    for (i, tab) in tabs.iter().enumerate() {
        let (fill, style) = if i == selected {
            ("━", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD))
        } else {
            ("─", dim)
        };

        spans.push(Span::styled("┴", dim));
        spans.push(Span::styled("─", dim));
        spans.push(Span::styled(fill.repeat(tab.chars().count()), style));
        spans.push(Span::styled("─", dim));
        spans.push(Span::styled("┴", dim));
    }

    let notch_width: u16 = tabs.iter().map(|tab| tab.chars().count() as u16 + 4).sum();
    let notch_x = area.x + TAB_BAR_X;
    if notch_x + notch_width < area.right() {
        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect { x: notch_x, y: area.y, width: notch_width, height: 1 },
        );
    }

    inner
}
