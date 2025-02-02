use color_eyre::Result;
use indoc::indoc;
use ratatui::{prelude::*, widgets::*};
use tokio::sync::mpsc::UnboundedSender;

use super::Component;
use crate::{action::Action, config::Config};

const CAT_ASCII_ART: &str = indoc! {r#"
      |\__/,|   (`\
      |_ _  |.--.) )
      ( T   )     /
     (((^_(((/(((_>
"#};

#[derive(Default)]
pub struct Cat {
    command_tx: Option<UnboundedSender<Action>>,
    config: Config,
}

impl Cat {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Component for Cat {
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
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        frame.render_widget(
            Paragraph::new(CAT_ASCII_ART).style(
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::SLOW_BLINK | Modifier::BOLD),
            ),
            Rect {
                x: area.width - 17,
                y: area.height - 4,
                width: 16,
                height: 6,
            },
        );

        Ok(())
    }
}
