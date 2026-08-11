use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use color_eyre::Result;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::Component;
use crate::action::Action;

/// Keybinding hints shown on the left of the footer, indexed by tab
#[rustfmt::skip]
const HINTS: [&[(&str, &str)]; 3] = [
    &[("<->", "tabs"),  ("?", "help"),   ("q", "quit")],
    &[("<->", "tabs"),  ("↑↓", "select"), ("?", "help"), ("q", "quit")],
    &[("↑↓", "posts"), ("⏎", "read"),   ("esc", "back"), ("<->", "tabs"), ("?", "help")],
];

#[derive(Default)]
pub struct Footer {
    selected_tab: Arc<AtomicUsize>,
}

impl Footer {
    pub fn new(selected_tab: Arc<AtomicUsize>) -> Self {
        Self { selected_tab }
    }

    fn hints(&self) -> Line<'static> {
        let key_style = Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD);
        let label_style = Style::default().fg(Color::DarkGray);

        let tab = self.selected_tab.load(Ordering::Relaxed).min(HINTS.len() - 1);
        let mut spans = vec![Span::from(" ")];

        for (key, label) in HINTS[tab] {
            spans.push(Span::styled(*key, key_style));
            spans.push(Span::from(" "));
            spans.push(Span::styled(*label, label_style));
            spans.push(Span::from("   "));
        }

        Line::from(spans)
    }

    #[rustfmt::skip]
    fn identity(&self) -> Line<'static> {
        let dim = Style::default().fg(Color::DarkGray);
        Line::from(vec![
            Span::styled(env!("CARGO_PKG_NAME"), Style::default().fg(Color::Indexed(183))),
            Span::styled(format!(" · {}@", env!("VERGEN_GIT_BRANCH")), dim),
            Span::styled(env!("PKG_FULL_VERSION"), Style::default().fg(Color::Green).dim()),
            Span::from(" "),
        ])
    }
}

impl Component for Footer {
    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Tick => {}
            Action::Render => {}
            _ => {}
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        frame.render_widget(Paragraph::new(self.hints()), area);

        // The identity is only worth showing when it will not collide with the hints
        if area.width > 90 {
            frame.render_widget(
                Paragraph::new(self.identity()).alignment(Alignment::Right),
                area,
            );
        }

        Ok(())
    }
}
