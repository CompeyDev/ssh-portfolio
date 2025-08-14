use color_eyre::eyre::Result;
use ratatui::style::{Color, Style};
use ratatui::widgets::{List, ListState};
use tokio::sync::mpsc::UnboundedSender;

use crate::action::Action;
use crate::components::Component;
use crate::config::Config;

#[derive(Default)]
pub struct SelectionList {
    command_tx: Option<UnboundedSender<Action>>,
    config: Config,
    options: List<'static>,
    list_state: ListState,
}

impl SelectionList {
    pub fn new(options: Vec<String>) -> Self {
        let mut list_state = ListState::default();
        list_state.select_first();

        Self {
            options: List::new(options)
                .highlight_style(Style::default().fg(Color::Yellow)),
            list_state,
            ..Default::default()
        }
    }
}

impl Component for SelectionList {
    fn register_action_handler(
        &mut self,
        tx: UnboundedSender<Action>,
    ) -> Result<()> {
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
            Action::SelectNext => self.list_state.select_next(),
            Action::SelectPrev => self.list_state.select_previous(),
            _ => {}
        };

        Ok(None)
    }

    fn draw(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> Result<()> {
        frame.render_stateful_widget(
            self.options.clone(),
            area,
            &mut self.list_state,
        );
        Ok(())
    }
}
