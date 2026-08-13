use std::io::{BufReader, Cursor};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use color_eyre::eyre::eyre;
use color_eyre::Result;
use image::{ImageReader, Rgba};
use ratatui::layout::{Constraint, Flex, Layout, Rect, Size};
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui_image::picker::ProtocolType;
use ratatui_image::protocol::halfblocks::Halfblocks;
use ratatui_image::protocol::iterm2::Iterm2;
use ratatui_image::protocol::kitty::StatefulKitty;
use ratatui_image::protocol::sixel::Sixel;
use ratatui_image::protocol::{ImageSource, StatefulProtocol, StatefulProtocolType};
use ratatui_image::{Resize, StatefulImage};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::RwLock;

use crate::action::Action;
use crate::com;
use crate::com::whtwnd::blog::defs::Ogp;
use crate::components::{Component, SelectionList};
use crate::tui::terminal::TerminalInfo;

pub type Post = Arc<com::whtwnd::blog::entry::Record>;

/// Kitty images are tagged by IDs and must be unique for a terminal session
///
/// The ID is incremented, as opposed to ratatui-image's internal RNG based
/// system, since this is cheaper and truly has no likelihood of collision
static NEXT_IMAGE_ID: AtomicU32 = AtomicU32::new(1);

pub struct BlogPosts {
    list: SelectionList<Post>,
    posts: Vec<Post>,
    in_post: (Option<StatefulProtocol>, Option<usize>),
    terminal_info: Option<Arc<RwLock<TerminalInfo>>>,
}

impl BlogPosts {
    pub fn new(posts: Vec<Post>) -> Self {
        let posts_ref = posts.to_vec();
        Self {
            list: SelectionList::new(posts),
            posts: posts_ref,
            in_post: (None, None),
            terminal_info: None,
        }
    }

    pub fn is_in_post(&self) -> bool {
        self.in_post.1.is_some()
    }

    fn images_available(&self) -> bool {
        self.terminal_info
            .as_ref()
            .and_then(|info| info.try_read().ok())
            .is_some_and(|info| info.supports_images())
    }

    fn draw_url_fallback(frame: &mut Frame, area: Rect, ogp: &Ogp, body: Paragraph<'_>) {
        let img_url = super::truncate(&ogp.url, area.width as usize / 3);
        let url_widget = Line::from(img_url).centered().style(
            Style::default().add_modifier(Modifier::BOLD | Modifier::ITALIC).fg(Color::Yellow),
        );

        frame.render_widget(
            url_widget,
            Rect::new(area.x + 1, area.y + 1, area.width, area.height),
        );

        frame.render_widget(body, Rect::new(area.x + 3, area.y + 3, area.width, area.height));
    }

    fn stateful_image_type(protocol: ProtocolType) -> StatefulProtocolType {
        match protocol {
            ProtocolType::Halfblocks => {
                StatefulProtocolType::Halfblocks(Halfblocks::default())
            }
            ProtocolType::Sixel => StatefulProtocolType::Sixel(Sixel::default()),
            ProtocolType::Iterm2 => StatefulProtocolType::ITerm2(Iterm2::default()),
            ProtocolType::Kitty => StatefulProtocolType::Kitty(StatefulKitty::new(
                NEXT_IMAGE_ID.fetch_add(1, Ordering::Relaxed),
                false,
            )),
        }
    }

    async fn header_image(&mut self, img: Ogp) -> Result<StatefulProtocol> {
        // The probe is shared and asynchronous, read its state at the present moment
        let Some(shared) = self.terminal_info.clone() else {
            return Err(eyre!("No terminal info available to render an image against"));
        };

        let (protocol, font_size) = {
            let info = shared.read().await;
            (info.protocol(), info.font_size())
        };

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

        // A fully transparent background skips the constructor's underlay, which would
        // otherwise allocate a second full-size image and composite onto it
        let source = ImageSource::new(dyn_img, font_size, Rgba([0, 0, 0, 0]));

        Ok(StatefulProtocol::new(source, font_size, Self::stateful_image_type(protocol)))
    }
}

impl Component for BlogPosts {
    fn init(&mut self, term_info: Arc<RwLock<TerminalInfo>>, _: Size) -> Result<()> {
        // Holding a shared reference to the handle since the probing is asynchronous and
        // the state at this point of initialization might not be the actual value yet
        self.terminal_info = Some(term_info);
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

            let post_ogp = post.ogp.clone();
            let post_body_widget =
                Paragraph::new(tui_markdown::from_str(&post_body)).wrap(Wrap { trim: true });

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
            } else if self.images_available() {
                // Image not cached, load image and skip rendering for current draw call
                if let Some(post_ogp) = post_ogp.clone() {
                    let rt = tokio::runtime::Handle::current();
                    match rt.block_on(async { self.header_image(post_ogp.clone()).await }) {
                        Ok(img) => self.in_post.0 = Some(img),
                        Err(err) => {
                            // Image fetch failed
                            tracing::warn!("Header image unavailable, showing url: {err}");
                            Self::draw_url_fallback(frame, area, &post_ogp, post_body_widget);
                        }
                    }
                } else {
                    frame.render_widget(
                        post_body_widget,
                        Rect::new(area.x + 1, area.y + 1, area.width, area.height),
                    );
                }
            } else if let Some(ref post_ogp) = post.ogp {
                // No image support at all
                Self::draw_url_fallback(frame, area, post_ogp, post_body_widget);
            }
        } else {
            self.list.draw(frame, area)?;
        }

        Ok(())
    }
}
