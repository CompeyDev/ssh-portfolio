use color_eyre::Result;
use ratatui::widgets::Widget;
use tokio::sync::mpsc::UnboundedSender;

use crate::action::Action;
use crate::components::{Component, SelectionList};

#[derive(Debug)]
pub struct BlogPosts {
    titles: SelectionList,
    contents: Vec<String>,
    in_post: Option<usize>,
}

impl BlogPosts {
    pub fn new(posts: Vec<(String, String)>) -> Self {
        let (titles, contents): (Vec<String>, Vec<String>) =
            posts.iter().cloned().unzip();

        Self { titles: SelectionList::new(titles), contents, in_post: None }
    }

    pub fn is_in_post(&self) -> bool {
        self.in_post.is_some()
    }
}

impl Component for BlogPosts {
    fn register_config_handler(
        &mut self,
        config: crate::config::Config,
    ) -> Result<()> {
        self.titles.register_config_handler(config)
    }
    
    fn register_action_handler(
        &mut self,
        tx: UnboundedSender<Action>,
    ) -> Result<()> {
        self.titles.register_action_handler(tx)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match self.titles.update(action.clone())?.unwrap() {
            // safe to unwrap, guaranteed to not be `None`
            Action::Tick => {}
            Action::Render => {}

            Action::Quit | Action::PrevTab | Action::NextTab => {
                self.in_post = None
            }
            Action::Continue(post_id) => self.in_post = post_id,
            _ => {}
        };

        Ok(None)
    }

    fn draw(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> Result<()> {
        if let Some(post_id_inner) = self.in_post {
            let post_body = self
                .contents
                .get(post_id_inner)
                .cloned()
                .unwrap_or("404 - Blog not found!".to_string());

            let post_widget = tui_markdown::from_str(&post_body);
            post_widget.render(area, frame.buffer_mut()); 
        } else {
            self.titles.draw(frame, area)?;
        }

        Ok(())
    }
}
