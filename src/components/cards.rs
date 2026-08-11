use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use color_eyre::Result;
use ratatui::prelude::*;
use ratatui::widgets::{
    Block, BorderType, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    Wrap,
};

use super::Component;
use crate::action::Action;
use crate::components::Tabs;

const CARD_CHROME: u16 = 3;

#[derive(Debug, Clone)]
pub struct Card<'a> {
    pub title: &'a str,
    pub description: &'a str,
}

/// A scrollable and selectable grid of project cards.
#[derive(Default)]
pub struct CardGrid<'a> {
    cards: Vec<Card<'a>>,
    selected_tab: Arc<AtomicUsize>,
    selected: usize,
    offset: usize,
}

impl<'a> CardGrid<'a> {
    pub fn new(cards: Vec<Card<'a>>, selected_tab: Arc<AtomicUsize>) -> Self {
        Self { cards, selected_tab, ..Default::default() }
    }

    fn is_active(&self) -> bool {
        self.selected_tab.load(Ordering::Relaxed) == Tabs::PROJECTS
    }

    fn select_next(&mut self) {
        if self.selected + 1 < self.cards.len() {
            self.selected += 1;
        }
    }

    fn select_previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn row_height(&self, row: usize, cols: usize, col_width: u16) -> u16 {
        let inner = col_width.saturating_sub(6);
        let start = row * cols;
        let end = (start + cols).min(self.cards.len());

        self.cards
            .get(start..end)
            .unwrap_or_default()
            .iter()
            .map(|card| wrapped_lines(card.description, inner))
            .max()
            .unwrap_or(1)
            .saturating_add(CARD_CHROME)
    }

    fn rows_fitting(&self, offset: usize, cols: usize, col_width: u16, height: u16) -> usize {
        let total_rows = self.cards.len().div_ceil(cols);
        let mut used = 0u16;
        let mut count = 0usize;

        for row in offset..total_rows {
            let h = self.row_height(row, cols, col_width);
            if used + h > height {
                break;
            }

            used += h;
            count += 1;
        }

        count.max(1)
    }

    fn scroll_into_view(&mut self, cols: usize, col_width: u16, height: u16) {
        if cols == 0 {
            return;
        }

        let row = self.selected / cols;
        if row < self.offset {
            self.offset = row;
            return;
        }

        // Try offsets until the selected row is visible
        while row >= self.offset + self.rows_fitting(self.offset, cols, col_width, height) {
            self.offset += 1;
        }
    }

    fn draw_card(&self, frame: &mut Frame, card: &Card<'a>, area: Rect, selected: bool) {
        let (border_style, title_style) = if selected {
            (
                Style::default().fg(Color::Magenta),
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            )
        } else {
            (
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::DIM),
                Style::default().fg(Color::Blue).add_modifier(Modifier::UNDERLINED),
            )
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // An indicator along with the color change for the selected card
        let marker = if selected { "▸ " } else { "  " };
        let body = Paragraph::new(vec![
            Line::from(vec![
                Span::styled(marker, Style::default().fg(Color::Magenta)),
                Span::styled(card.title, title_style),
            ]),
            Line::from(vec![
                Span::from("  "),
                Span::styled(card.description, Style::default().fg(Color::White)),
            ]),
        ])
        .wrap(Wrap { trim: true });

        frame.render_widget(
            body,
            Rect {
                x: inner.x + 1,
                y: inner.y,
                width: inner.width.saturating_sub(2),
                height: inner.height,
            },
        );
    }
}

impl Component for CardGrid<'_> {
    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Tick => {}
            Action::Render => {}
            Action::SelectNext if self.is_active() => self.select_next(),
            Action::SelectPrev if self.is_active() => self.select_previous(),
            _ => {}
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        if self.cards.is_empty() || area.width < 24 || area.height < 4 {
            return Ok(());
        }

        let grid = Rect { width: area.width.saturating_sub(2), ..area }; // scrollbar reserved
        let cols = match grid.width {
            w if w < 80 => 1,
            w if w < 130 => 2,
            _ => 3,
        };

        let col_width = grid.width / cols as u16;
        let total_rows = self.cards.len().div_ceil(cols);

        self.scroll_into_view(cols, col_width, grid.height);
        let visible_rows = self.rows_fitting(self.offset, cols, col_width, grid.height);
        let max_offset = total_rows.saturating_sub(visible_rows);
        self.offset = self.offset.min(max_offset);

        let mut y = grid.y;
        for row in self.offset..total_rows {
            let row_height = self.row_height(row, cols, col_width);
            if y + row_height > grid.bottom() {
                break;
            }

            for col in 0..cols {
                let index = row * cols + col;
                let Some(card) = self.cards.get(index) else {
                    break;
                };

                let cell = Rect {
                    x: grid.x + col as u16 * col_width,
                    y,
                    width: col_width,
                    height: row_height,
                };

                self.draw_card(frame, card, cell, self.is_active() && index == self.selected);
            }

            y += row_height;
        }

        if total_rows > visible_rows {
            let mut state = ScrollbarState::new(max_offset).position(self.offset);
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(None)
                    .end_symbol(None)
                    .style(Style::default().fg(Color::DarkGray)),
                area,
                &mut state,
            );
        }

        Ok(())
    }
}

/// Estimates the number of lines `text` will take after wrapped by `width`.
fn wrapped_lines(text: &str, width: u16) -> u16 {
    if width == 0 {
        return 1;
    }

    let mut lines = 1u16;
    let mut used = 0usize;
    for word in text.split_whitespace() {
        let word_len = word.chars().count();
        let needed = if used == 0 { word_len } else { used + 1 + word_len };
        if needed > width as usize {
            lines = lines.saturating_add(1);
            used = word_len;
        } else {
            used = needed;
        }
    }

    lines
}
