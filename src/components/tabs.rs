use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use color_eyre::Result;
use ratatui::prelude::*;
use ratatui::widgets::*;
use tokio::sync::mpsc::UnboundedSender;

use super::Component;
use crate::action::Action;
use crate::config::Config;

// TODO: Clean the border drawing logic up into its own component

#[derive(Default)]
pub struct Tabs {
    command_tx: Option<UnboundedSender<Action>>,
    config: Config,
    tabs: Vec<&'static str>,
    selected_tab: Arc<AtomicUsize>,
}

impl Tabs {
    pub fn new(tabs: Vec<&'static str>, selected_tab: Arc<AtomicUsize>) -> Self {
        Self { tabs, selected_tab, ..Default::default() }
    }

    pub fn next(&mut self) {
        if self.selected_tab.load(Ordering::Relaxed) < self.tabs.len() - 1 {
            self.selected_tab.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn previous(&mut self) {
        if self.selected_tab.load(Ordering::Relaxed) > 0 {
            self.selected_tab.fetch_sub(1, Ordering::Relaxed);
        }
    }

    pub fn current_tab(&self) -> usize {
        self.selected_tab.load(Ordering::Relaxed)
    }
}

impl Component for Tabs {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.command_tx = Some(tx);
        Ok(())
    }

    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        self.config = config;
        Ok(())
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Tick => {}
            Action::Render => {}
            Action::NextTab => self.next(),
            Action::PrevTab => self.previous(),
            _ => {}
        };

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let mut tab_lines = vec![Line::default(), Line::default()];

        for (i, &tab) in self.tabs.iter().enumerate() {
            let style = if self.selected_tab.load(Ordering::Relaxed) == i {
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            tab_lines[0].spans.push(Span::styled(
                format!("╭{}╮", "─".repeat(tab.len() + 2)),
                Style::default().fg(Color::DarkGray),
            ));

            tab_lines[1].spans.push(Span::styled("│", Style::default().fg(Color::DarkGray)));
            tab_lines[1].spans.push(Span::styled(format!(" {tab} "), style));
            tab_lines[1].spans.push(Span::styled("│", Style::default().fg(Color::DarkGray)));
        }

        let tabs_widget =
            Paragraph::new(tab_lines).block(Block::default().borders(Borders::NONE));

        frame.render_widget(
            tabs_widget,
            Rect { x: area.x, y: area.y, width: area.width, height: 2 },
        );

        Ok(())
    }
}
