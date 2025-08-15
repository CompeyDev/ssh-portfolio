use chrono::DateTime;
use color_eyre::eyre::Result;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use tokio::sync::mpsc::UnboundedSender;

use crate::action::Action;
use crate::components::{Component, Post};
use crate::config::Config;

fn truncate(s: &str, max: usize) -> String {
    s.char_indices()
        .find(|(idx, ch)| idx + ch.len_utf8() > max)
        .map_or(s.to_string(), |(idx, _)| s[..idx].to_string() + "...")
}

#[derive(Debug)]
pub struct SelectionList<T> {
    config: Config,
    pub(super) options: Vec<T>,
    pub(super) list_state: ListState,
    action_tx: Option<UnboundedSender<Action>>,
}

impl<T> SelectionList<T> {
    pub fn new(options: Vec<T>) -> Self {
        let mut list_state = ListState::default();
        list_state.select_first();

        Self { config: Config::default(), options, list_state, action_tx: None }
    }
}

impl Component for SelectionList<Post> {
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
            Action::Continue(None) => {
                if let Some(tx) = &self.action_tx {
                    tx.send(Action::Continue(self.list_state.selected()))?;
                }
            }
            Action::SelectNext => self.list_state.select_next(),
            Action::SelectPrev => self.list_state.select_previous(),
            _ => {}
        };

        Ok(Some(action))
    }

    fn draw(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> Result<()> {
        let items = self.options.iter().enumerate().map(|(i, post)| {
            let bold_style = Style::default().add_modifier(Modifier::BOLD);
            let accent_style = bold_style.fg(Color::LightMagenta);

            let post_creation_date = post
                .created_at
                .as_ref()
                .map(|dt| DateTime::parse_from_rfc3339(dt.as_str()))
                .and_then(Result::ok)
                .map_or(DateTime::UNIX_EPOCH.date_naive().to_string(), |dt| {
                    dt.date_naive().to_string()
                });

            let arrow_or_pad =
                if self.list_state.selected().is_some_and(|selection| selection == i) {
                    "▶ ".to_string()
                } else {
                    format!("{:>2}", " ")
                };

            let padded_date = format!("{:>10}", post_creation_date);

            let title_spans = vec![
                Span::styled(arrow_or_pad, accent_style),
                Span::raw(" "),
                Span::styled(padded_date, bold_style),
                Span::styled(" • ", accent_style),
                Span::styled(
                    post.title.clone().unwrap_or("[object Object]".to_string()), // LMAOOO
                    accent_style,
                ),
            ];

            let mut list_content = vec![Line::from(title_spans)];

            let line_format = [
                Span::raw(format!("{:>14}", " ")),
                Span::styled("┊", Style::default().add_modifier(Modifier::DIM)),
            ];

            let subtitle_span = Span::raw(
                [" ", post.subtitle.as_ref().unwrap_or(&truncate(post.content.as_ref(), 40))]
                    .concat(),
            );

            list_content.push(Line::from([line_format.as_slice(), &[subtitle_span]].concat()));
            list_content.push(Line::from([line_format.as_slice(), &[Span::raw("")]].concat()));

            ListItem::new(list_content)
        });

        frame.render_stateful_widget(
            List::new(items)
                .block(Block::default().borders(Borders::NONE))
                .highlight_style(Style::default())
                .highlight_symbol(""),
            area,
            &mut self.list_state,
        );
        Ok(())
    }
}
