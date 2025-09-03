use std::io::{BufReader, Cursor};
use std::sync::Arc;

use color_eyre::eyre::eyre;
use color_eyre::Result;
use image::{ImageReader, Rgba};
use ratatui::layout::{Constraint, Flex, Layout, Rect, Size};
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{Resize, StatefulImage};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::RwLock;

use crate::action::Action;
use crate::com;
use crate::com::whtwnd::blog::defs::Ogp;
use crate::components::{Component, SelectionList};
use crate::tui::terminal::{TerminalInfo, TerminalKind, UnsupportedReason, DEFAULT_FONT_SIZE};

pub type Post = Arc<com::whtwnd::blog::entry::Record>;
pub struct BlogPosts {
    list: SelectionList<Post>,
    posts: Vec<Post>,
    image_renderer: Option<Picker>,
    in_post: (Option<StatefulProtocol>, Option<usize>),
}

impl BlogPosts {
    pub fn new(posts: Vec<Post>) -> Self {
        let posts_ref = posts.to_vec();
        Self {
            list: SelectionList::new(posts),
            image_renderer: Some(Picker {
                font_size: DEFAULT_FONT_SIZE,
                protocol_type: ProtocolType::Halfblocks,
                background_color: Rgba([0, 0, 0, 0]),
                // NOTE: Multiplexers such as tmux are currently unsupported, we ensure that we have an
                // xterm based terminal emulator in ssh.rs, if not, we reject the conection to begin with
                is_tmux: false,
                capabilities: vec![],
            }),
            posts: posts_ref,
            in_post: (None, None),
        }
    }

    pub fn is_in_post(&self) -> bool {
        self.in_post.1.is_some()
    }

    async fn header_image(&self, img: Ogp) -> Result<StatefulProtocol> {
        if let Some(picker) = &self.image_renderer {
            let img_blob = reqwest::get(img.url.clone())
                .await?
                .bytes()
                .await?
                .iter()
                .cloned()
                .collect::<Vec<u8>>();

            let dyn_img = ImageReader::new(BufReader::new(Cursor::new(img_blob)))
                .with_guessed_format()?
                .decode()?;
            let sized_img = picker.new_resize_protocol(dyn_img);

            return Ok(sized_img);
        }

        Err(eyre!("No image supported renderer initialized"))
    }
}

impl Component for BlogPosts {
    fn init(&mut self, term_info: Arc<RwLock<TerminalInfo>>, _: Size) -> Result<()> {
        let locked_info = term_info.blocking_read().clone();

        if matches!(locked_info.kind(), TerminalKind::Unsupported(UnsupportedReason::Unsized))
        {
            self.image_renderer = None;
        }

        if let Some(picker) = &mut self.image_renderer {
            picker.capabilities = locked_info.kind().capabilities();
            picker.protocol_type = locked_info.kind().as_protocol();
            picker.font_size = locked_info.font_size();

            tracing::info!(
                "Using {:?} rendering protocol for blog image renderer, font size: {:?}",
                picker.protocol_type(),
                picker.font_size(),
            );
        }

        Ok(())
    }

    fn register_config_handler(&mut self, config: crate::config::Config) -> Result<()> {
        self.list.register_config_handler(config)
    }

    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.list.register_action_handler(tx)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match self.list.update(action.clone())?.unwrap() {
            // safe to unwrap, guaranteed to not be `None`
            Action::Tick => {}
            Action::Render => {}
            Action::Quit | Action::PrevTab | Action::NextTab => self.in_post = (None, None),

            // FIXME: This makes it possible to scroll through the list with arrow keys even
            // when it is not rendered, which is not ideal; should probably fix later, minor bug
            Action::Continue(post_id) => self.in_post.1 = post_id,
            _ => {}
        };

        Ok(None)
    }

    fn draw(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> Result<()> {
        if let Some(post_id_inner) = self.in_post.1 {
            let post = self
                .posts
                .get(post_id_inner)
                .ok_or(eyre!("Current post apparently doesn't exist"))?;

            let post_body = post.title.clone().map_or(post.content.clone(), |title| {
                format!("# {}\n\n{}", title, post.content)
            });

            let post_body_widget =
                Paragraph::new(tui_markdown::from_str(&post_body)).wrap(Wrap { trim: true });

            // FIXME: content in the body often overlaps with the `Cat` component and gets
            // formatted weirdly. maybe deal with that at some point? real solution is probably a
            // refactor to use `Layout`s instead of rolling our own layout logic
            if let Some(img) = self.in_post.0.as_mut() {
                // Render prefetched image on current draw call
                let [image_area, text_area] =
                    Layout::vertical([Constraint::Percentage(40), Constraint::Percentage(60)])
                        .flex(Flex::SpaceBetween)
                        .vertical_margin(2)
                        .areas(area);

                let resized_img = img.size_for(Resize::Fit(None), image_area);
                let [image_area] = Layout::horizontal([Constraint::Length(resized_img.width)])
                    .flex(Flex::Center)
                    .areas(image_area);

                frame.render_stateful_widget(StatefulImage::default(), image_area, img);
                frame.render_widget(post_body_widget, text_area);
            } else if self.image_renderer.is_some() {
                // Image not cached, load image and skip rendering for current draw call
                if let Some(ref post_ogp) = post.ogp {
                    let rt = tokio::runtime::Handle::current();
                    let img =
                        rt.block_on(async { self.header_image(post_ogp.clone()).await })?;
                    self.in_post.0 = Some(img);
                } else {
                    frame.render_widget(
                        post_body_widget,
                        Rect::new(area.x + 1, area.y + 1, area.width, area.height),
                    );
                }
            } else if let Some(ref post_ogp) = post.ogp {
                // No image rendering capabilities, only display text
                let img_url = super::truncate(&post_ogp.url, area.width as usize / 3);
                let url_widget = Line::from(img_url).centered().style(
                    Style::default()
                        .add_modifier(Modifier::BOLD | Modifier::ITALIC)
                        .fg(Color::Yellow),
                );

                frame.render_widget(
                    url_widget,
                    Rect::new(area.x + 1, area.y + 1, area.width, area.height),
                );

                frame.render_widget(
                    post_body_widget,
                    Rect::new(area.x + 3, area.y + 3, area.width, area.height),
                );
            }
        } else {
            self.list.draw(frame, area)?;
        }

        Ok(())
    }
}
