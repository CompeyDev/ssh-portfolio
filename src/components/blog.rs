use std::sync::Arc;

use color_eyre::Result;
use ratatui::widgets::Widget;
use tokio::sync::mpsc::UnboundedSender;

use crate::action::Action;
use crate::com;
use crate::components::{Component, SelectionList};

pub type Post = Arc<com::whtwnd::blog::entry::Record>;
#[derive(Debug)]
pub struct BlogPosts {
    list: SelectionList<Post>,
    posts: Vec<Post>,
    in_post: Option<usize>,
}

impl BlogPosts {
    pub fn new(posts: Vec<Post>) -> Self {
        let posts_ref = posts.to_vec();
        Self {
            list: SelectionList::new(posts),
            posts: posts_ref,
            in_post: None,
        }
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
        self.list.register_config_handler(config)
    }

    fn register_action_handler(
        &mut self,
        tx: UnboundedSender<Action>,
    ) -> Result<()> {
        self.list.register_action_handler(tx)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match self.list.update(action.clone())?.unwrap() {
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
                .posts
                .get(post_id_inner)
                .map_or(String::from("404 - Blog not found!"), |post| {
                    post.content.clone()
                });

            let post_widget = tui_markdown::from_str(&post_body);
            post_widget.render(area, frame.buffer_mut());
        } else {
            self.list.draw(frame, area)?;
        }

        Ok(())
    }
}
