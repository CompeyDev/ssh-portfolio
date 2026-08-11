use std::collections::BTreeMap;

use color_eyre::Result;
use ratatui::layout::Flex;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::Component;
use crate::action::Action;
use crate::app::Mode;
use crate::config::{key_event_to_string, Config};

#[derive(Default)]
pub struct Help {
    config: Config,
    visible: bool,
}

impl Help {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    fn entries(&self) -> Vec<(String, String)> {
        let Some(keymap) = self.config.keybindings.get(&Mode::Home) else {
            return Vec::new();
        };

        let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (sequence, action) in keymap.iter() {
            let keys = sequence.iter().map(key_event_to_string).collect::<Vec<_>>().join(" ");
            grouped.entry(action.to_string().to_lowercase()).or_default().push(keys);
        }

        grouped
            .into_iter()
            .map(|(action, mut keys)| {
                keys.sort();
                (action, keys.join(", "))
            })
            .collect()
    }

    fn lines(&self) -> Vec<Line<'static>> {
        let key_style = Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD);
        let action_style = Style::default().fg(Color::White);

        let mut lines = vec![Line::default()];
        for (action, keys) in self.entries() {
            lines.push(Line::from(vec![
                Span::styled(format!("  {keys:>18}  "), key_style),
                Span::styled(action, action_style),
            ]));
        }

        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "  press ? or esc to close",
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
        )));

        lines
    }
}

impl Component for Help {
    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        self.config = config;
        Ok(())
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Tick => {}
            Action::Render => {}
            Action::Help => self.visible = !self.visible,
            Action::Quit => self.visible = false,
            _ => {}
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        if !self.visible {
            return Ok(());
        }

        let lines = self.lines();
        let height = (lines.len() as u16).saturating_add(2).min(area.height);
        let width = 46.min(area.width);

        let [area] =
            Layout::horizontal([Constraint::Length(width)]).flex(Flex::Center).areas(area);
        let [area] =
            Layout::vertical([Constraint::Length(height)]).flex(Flex::Center).areas(area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Magenta))
            .title(Span::styled(
                " keys ",
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(block, area);
        frame.render_widget(Paragraph::new(lines), inner);

        Ok(())
    }
}
