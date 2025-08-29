use color_eyre::Result;
use ratatui::prelude::*;
use ratatui::widgets::*;

use super::Component;
use crate::action::Action;

#[derive(Debug, Clone)]
pub struct Card<'a> {
    pub title: &'a str,
    pub description: &'a str,
}

// FIXME: Redundant border drawing logic, see `Tabs` component
fn draw_custom_border(buf: &mut Buffer, rect: Rect, style: Style, clip: Rect) {
    let area = rect.intersection(clip);
    if area.is_empty() {
        return;
    }

    if area.width < 2 || area.height < 2 {
        return;
    }

    let x0 = area.x;
    let x1 = area.x + area.width - 1;
    let y0 = area.y;
    let y1 = area.y + area.height - 1;

    // Corners
    buf[(x0, y0)].set_char('╭').set_style(style);
    buf[(x1, y0)].set_char('╮').set_style(style);
    buf[(x0, y1)].set_char('╰').set_style(style);
    buf[(x1, y1)].set_char('╯').set_style(style);

    // Horizontal edges
    for x in (x0 + 1)..x1 {
        buf[(x, y0)].set_char('─').set_style(style);
        buf[(x, y1)].set_char('─').set_style(style);
    }

    // Vertical edges
    for y in (y0 + 1)..y1 {
        buf[(x0, y)].set_char('│').set_style(style);
        buf[(x1, y)].set_char('│').set_style(style);
    }
}

impl<'a> Component for Vec<Card<'a>> {
    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Tick => {}
            Action::Render => {}
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let num_rows = (self.len() as f32 / 3.0).ceil() as usize;
        let row_constraints = vec![Constraint::Length(6); num_rows];

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(row_constraints)
            .split(area);

        for (i, row) in rows.iter().enumerate() {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(33),
                    Constraint::Percentage(33),
                    Constraint::Percentage(34),
                ])
                .split(*row);

            for (j, col) in cols.iter().enumerate() {
                let index = i * 3 + j;
                if let Some(card) = self.get(index) {
                    let border_style = Style::default().add_modifier(Modifier::DIM);
                    let buf = frame.buffer_mut();

                    draw_custom_border(buf, *col, border_style, area);

                    let paragraph = Paragraph::new(vec![
                        Line::styled(
                            card.title,
                            Style::default()
                                .fg(Color::Blue)
                                .add_modifier(Modifier::UNDERLINED),
                        ),
                        Line::raw(card.description),
                    ])
                    .style(Style::default())
                    .wrap(Wrap { trim: true });

                    frame.render_widget(
                        paragraph,
                        Rect {
                            x: col.x + 2,
                            y: col.y + 1,
                            width: col.width.saturating_sub(4),
                            height: col.height.saturating_sub(2),
                        },
                    );
                }
            }
        }

        Ok(())
    }
}
