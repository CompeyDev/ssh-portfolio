use std::default::Default;

use color_eyre::eyre::Result;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use tokio::sync::mpsc::UnboundedSender;

use crate::action::Action;
use crate::components::Component;
use crate::config::Config;
use crate::cli::VERSION;

#[derive(Debug, Default)]
pub struct VersionInfo {
    config: Config,
    action_tx: Option<UnboundedSender<Action>>,
}

impl VersionInfo {
    pub fn new() -> Self {
        Self { ..Default::default() }
    }

    #[rustfmt::skip]
    pub fn status_content(&self) -> Line<'static> {
        let shell_style = Style::new().fg(Color::Indexed(183));
        Line::from(vec![
            Span::styled("󰇁 ", shell_style.dim()),
            Span::styled(env!("CARGO_PKG_NAME"), shell_style.bold()),
            Span::styled(format!(" ({}@{})", env!("VERGEN_GIT_BRANCH"), *VERSION), Style::new().fg(Color::Green).italic()),
            Span::styled("█", shell_style.add_modifier(Modifier::BOLD | Modifier::RAPID_BLINK)),
        ])
    }
}

impl Component for VersionInfo {
    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        self.config = config;
        Ok(())
    }

    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Tick => {}
            Action::Render => {}
            _ => {}
        };

        Ok(None)
    }

    fn draw(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> Result<()> {
        Paragraph::new(self.status_content())
            .alignment(Alignment::Right)
            .render(area, frame.buffer_mut());

        Ok(())
    }
}
