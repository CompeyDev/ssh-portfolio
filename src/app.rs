use std::io::{Error, ErrorKind};
use std::net::IpAddr;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::time::Instant;

use color_eyre::{eyre, Result};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch, Mutex, RwLock};
use tokio::task::block_in_place;
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::action::Action;
use crate::components::*;
use crate::config::Config;
use crate::keycode::KeyCodeExt;
use crate::tui::terminal::{TerminalGeometry, TerminalInfo};
use crate::tui::{Event, Terminal, Tui};
use crate::CONFIG;

pub struct App {
    config: Config,
    tick_rate: f64,
    frame_rate: f64,
    terminal_info: Arc<RwLock<TerminalInfo>>,

    should_quit: bool,
    should_suspend: bool,
    needs_resize: bool,

    mode: Mode,
    last_tick_key_events: Vec<KeyEvent>,
    action_tx: mpsc::UnboundedSender<Action>,
    action_rx: mpsc::UnboundedReceiver<Action>,

    ssh_keystroke_rx: mpsc::Receiver<Vec<u8>>,
    geometry_rx: watch::Receiver<TerminalGeometry>,

    boot: Boot,
    /// When the connection intro animation started, or `None` if it has finished
    boot_started: Option<Instant>,

    // TODO: Refactor into its own `Components` struct
    tabs: Arc<Mutex<Tabs>>,
    content: Arc<Mutex<Content>>,
    cat: Arc<Mutex<Debbie>>,
    #[cfg(feature = "blog")]
    blog_posts: Arc<Mutex<BlogPosts>>,
    footer: Arc<Mutex<Footer>>,
    client_info: Arc<Mutex<ClientInfo>>,
    help: Arc<Mutex<Help>>,
    credits: Arc<Mutex<Credits>>,
}

pub const TABS: [&str; 3] = ["about", "projects", "blog"];

const PROSE_SIZE: u16 = 76; // banner + intro paragraph without wrapping
const RAIL_GUTTER: u16 = 2; // gap between prose and rail

const CAT_RESERVE: u16 = Debbie::HEIGHT - 1 + Debbie::Z_ROWS;

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Home,
}

/// Differentiates a real draw error from the component being busy. Useful to prevent
/// tearing, as `try_draw` does not flush or swap the buffer in this case.
const FRAME_BUSY: ErrorKind = ErrorKind::WouldBlock;

fn try_component<T>(
    component: &Arc<Mutex<T>>,
    f: impl FnOnce(&mut T) -> Result<Option<Action>>,
) -> Result<Option<Action>> {
    // Not a blocking lock, in order to prevent a potential deadlock
    match component.try_lock() {
        Ok(mut guard) => f(&mut guard),
        Err(_) => Ok(None),
    }
}

impl App {
    pub const MIN_TUI_DIMS: (u16, u16) = (105, 25);

    pub fn new(
        terminal_info: Arc<RwLock<TerminalInfo>>,
        username: Arc<RwLock<Option<String>>>,
        client_ip: Option<IpAddr>,
        tick_rate: f64,
        frame_rate: f64,
        keystroke_rx: mpsc::Receiver<Vec<u8>>,
        geometry_rx: watch::Receiver<TerminalGeometry>,
    ) -> Result<Self> {
        let (action_tx, action_rx) = mpsc::unbounded_channel();
        let boot = Boot::new(Arc::clone(&terminal_info), username, client_ip);

        // Initialize components
        let active_tab = Arc::new(AtomicUsize::new(0));
        let tabs = Arc::new(Mutex::new(Tabs::new(TABS.to_vec(), Arc::clone(&active_tab))));
        let content = Arc::new(Mutex::new(Content::new(Arc::clone(&active_tab))));

        let cat = Arc::new(Mutex::new(Debbie::new()));

        #[cfg(feature = "blog")]
        let rt = tokio::runtime::Handle::current();
        #[cfg(feature = "blog")]
        let blog_posts = Arc::new(Mutex::new(BlogPosts::new(
            rt.block_on(content.try_lock()?.blog_content())?,
        )));

        let footer = Arc::new(Mutex::new(Footer::new(active_tab)));
        let client_info = Arc::new(Mutex::new(ClientInfo::new()));
        let help = Arc::new(Mutex::new(Help::new()));
        let credits = Arc::new(Mutex::new(Credits::new()));

        Ok(Self {
            terminal_info,
            tick_rate,
            frame_rate,
            should_quit: false,
            should_suspend: false,
            needs_resize: false,

            config: CONFIG.clone(),
            mode: Mode::Home,
            last_tick_key_events: Vec::new(),
            action_tx,
            action_rx,

            ssh_keystroke_rx: keystroke_rx,
            geometry_rx,

            boot,
            boot_started: None,

            tabs,
            content,
            cat,
            #[cfg(feature = "blog")]
            blog_posts,
            footer,
            client_info,
            help,
            credits,
        })
    }

    #[optimize(speed)]
    pub async fn run(
        &mut self,
        term: Arc<Mutex<Terminal>>,
        tui: Arc<RwLock<Option<Tui>>>,
    ) -> Result<()> {
        let mut tui = tui.write().await;
        let tui = tui.get_or_insert(
            Tui::new(term)?.tick_rate(self.tick_rate).frame_rate(self.frame_rate),
        );

        // Force the dimensions to be validated before rendering anything by sending a `Resize` event
        let term_size = tui.terminal.try_lock()?.size()?;
        tui.event_tx.send(Event::Resize(term_size.width, term_size.height)).await?;

        // Cue the initial connection loading animation
        self.boot_started = Some(Instant::now());

        // Blocking initialization logic for tui and components
        block_in_place(|| {
            tui.enter()?;

            // Register action handlers
            self.tabs.try_lock()?.register_action_handler(self.action_tx.clone())?;
            self.content.try_lock()?.register_action_handler(self.action_tx.clone())?;
            self.cat.try_lock()?.register_action_handler(self.action_tx.clone())?;
            #[cfg(feature = "blog")]
            self.blog_posts.try_lock()?.register_action_handler(self.action_tx.clone())?;

            // Register config handlers
            self.tabs.try_lock()?.register_config_handler(self.config.clone())?;
            self.content.try_lock()?.register_config_handler(self.config.clone())?;
            self.cat.try_lock()?.register_config_handler(self.config.clone())?;
            #[cfg(feature = "blog")]
            self.blog_posts.try_lock()?.register_config_handler(self.config.clone())?;
            self.help.try_lock()?.register_config_handler(self.config.clone())?;

            // Initialize components
            let size = tui.terminal.try_lock()?.size()?;
            self.tabs.try_lock()?.init(self.terminal_info.clone(), size)?;
            self.content.try_lock()?.init(self.terminal_info.clone(), size)?;
            self.cat.try_lock()?.init(self.terminal_info.clone(), size)?;
            #[cfg(feature = "blog")]
            self.blog_posts.try_lock()?.init(self.terminal_info.clone(), size)?;
            self.footer.try_lock()?.init(self.terminal_info.clone(), size)?;
            self.client_info.try_lock()?.init(self.terminal_info.clone(), size)?;
            self.help.try_lock()?.init(self.terminal_info.clone(), size)?;

            Ok::<_, eyre::Error>(())
        })?;

        let action_tx = self.action_tx.clone();
        let mut resume_tx: Option<Arc<CancellationToken>> = None;
        loop {
            self.handle_events(tui).await?;
            block_in_place(|| self.handle_actions(tui))?;
            if self.should_suspend {
                if let Some(ref tx) = resume_tx {
                    tx.cancel();
                    resume_tx = None;
                } else {
                    resume_tx = Some(tui.suspend().await?);
                    continue;
                }

                action_tx.send(Action::Resume)?;
                action_tx.send(Action::ClearScreen)?;
                block_in_place(|| tui.enter())?;
            } else if self.should_quit {
                tui.stop().await?;
                break;
            }
        }

        tui.exit().await
    }

    async fn handle_events(&mut self, tui: &mut Tui) -> Result<()> {
        tokio::select! {
            Some(event) = tui.next_event() => {
                let action_tx = self.action_tx.clone();
                match event {
                    Event::Quit => action_tx.send(Action::Quit)?,
                    Event::Tick => action_tx.send(Action::Tick)?,
                    Event::Render => action_tx.send(Action::Render)?,
                    Event::Resize(x, y) => action_tx.send(Action::Resize(x, y))?,
                    Event::Key(key) => block_in_place(|| self.handle_key_event(key))?,
                    _ => {}
                };

                // Components which we cannot lock are busy. Skip them for now, rather than erroring
                // at the session level
                if let Some(action) = try_component(&self.tabs, |c| c.handle_events(Some(event.clone())))? {
                    action_tx.send(action)?;
                }

                if let Some(action) = try_component(&self.content, |c| c.handle_events(Some(event.clone())))? {
                    action_tx.send(action)?;
                }

                if let Some(action) = try_component(&self.cat, |c| c.handle_events(Some(event.clone())))? {
                    action_tx.send(action)?;
                }
            }

            Some(keystroke_data) = self.ssh_keystroke_rx.recv() => {
                if self.boot_started.take().is_some() {
                    // Connection animation playing, any key press is treated as a skip
                    self.action_tx.send(Action::ClearScreen)?;
                } else {
                    let key_event = KeyCode::from_xterm_seq(&keystroke_data[..]).into_key_event();
                    block_in_place(|| self.handle_key_event(key_event))?;
                }
            }

            Ok(()) = self.geometry_rx.changed() => {
                let geometry = *self.geometry_rx.borrow_and_update();
                self.action_tx.send(Action::Resize(geometry.cols, geometry.rows))?;
            }
        }

        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<()> {
        if let Ok(mut credits) = self.credits.try_lock() {
            // If the credits view is active, it should capture all regular navigation keys
            if credits.is_visible() && credits.handle_key(key) {
                return Ok(());
            }
        }

        let action_tx = self.action_tx.clone();
        let Some(keymap) = self.config.keybindings.get(&self.mode) else {
            return Ok(());
        };

        match keymap.get(&vec![key]) {
            Some(action) => {
                debug!("Got action: {action:?}");
                action_tx.send(action.clone())?;
            }
            _ => {
                self.last_tick_key_events.push(key);
                if let Some(action) = keymap.get(&self.last_tick_key_events) {
                    debug!("Got action: {action:?}");
                    action_tx.send(action.clone())?;
                }
            }
        }

        Ok(())
    }

    fn handle_actions(&mut self, tui: &mut Tui) -> Result<()> {
        while let Ok(action) = self.action_rx.try_recv() {
            if action != Action::Tick && action != Action::Render {
                debug!("{action:?}");
            }

            match action {
                Action::Tick => {
                    self.last_tick_key_events.drain(..);
                }

                // Esc and q back out of whatever is on top before they quit. The components
                // clear their own state when they see `Quit` below, so this must read the state
                // *before* the broadcast. Let's wait for the busy components to finish before
                // actually propagating the signal to quit the session
                Action::Quit => {
                    #[cfg(feature = "blog")]
                    let nested = self.blog_posts.try_lock().map_or(true, |c| c.is_in_post());
                    #[cfg(not(feature = "blog"))]
                    let nested = false;

                    let help_visible = self.help.try_lock().map_or(true, |c| c.is_visible());
                    let credits_visible =
                        self.credits.try_lock().map_or(true, |c| c.is_visible());
                    self.should_quit = !nested && !help_visible && !credits_visible;
                }
                Action::Suspend => self.should_suspend = true,
                Action::Resume => self.should_suspend = false,
                Action::ClearScreen => {
                    if let Ok(mut term) = tui.terminal.try_lock() {
                        term.clear()?;
                    }
                }
                Action::Resize(w, h) => {
                    self.needs_resize = w < Self::MIN_TUI_DIMS.0 || h < Self::MIN_TUI_DIMS.1;
                    self.resize(tui, w, h)?;
                }
                Action::Render => self.render(tui)?,
                _ => {}
            }

            // Update each component, skipping any we cannot lock this round
            if let Some(action) = try_component(&self.tabs, |c| c.update(action.clone()))? {
                self.action_tx.send(action)?;
            }

            if let Some(action) = try_component(&self.content, |c| c.update(action.clone()))? {
                self.action_tx.send(action)?;
            }

            if let Some(action) = try_component(&self.cat, |c| c.update(action.clone()))? {
                self.action_tx.send(action)?;
            }

            #[cfg(feature = "blog")]
            if let Some(action) =
                try_component(&self.blog_posts, |c| c.update(action.clone()))?
            {
                self.action_tx.send(action)?;
            }

            if let Some(action) = try_component(&self.footer, |c| c.update(action.clone()))? {
                self.action_tx.send(action)?;
            }

            if let Some(action) =
                try_component(&self.client_info, |c| c.update(action.clone()))?
            {
                self.action_tx.send(action)?;
            }

            if let Some(action) = try_component(&self.help, |c| c.update(action.clone()))? {
                self.action_tx.send(action)?;
            }

            if let Some(action) = try_component(&self.credits, |c| c.update(action.clone()))? {
                self.action_tx.send(action)?;
            }
        }

        Ok(())
    }

    pub fn resize(&mut self, tui: &mut Tui, w: u16, h: u16) -> Result<()> {
        let geometry = *self.geometry_rx.borrow();
        let Ok(mut term) = tui.terminal.try_lock() else {
            // Terminal is busy, skip the resize until it is ready
            return Ok(());
        };

        term.backend_mut().dims = (w, h);
        term.backend_mut().pixel = (geometry.pixel_width, geometry.pixel_height);
        term.resize(Rect::new(0, 0, w, h))?;
        drop(term);

        self.render(tui)
    }

    fn render(&mut self, tui: &mut Tui) -> Result<()> {
        let Ok(mut term) = tui.terminal.try_lock() else {
            // Skip this frame if the terminal is busy
            return Ok(());
        };

        if term.backend().needs_redraw() {
            // Frame got dropped because the client could not keep up, force a clear
            // so ratatui has to start from scratch
            term.clear()?;
        }

        if self.needs_resize {
            term.draw(|frame| {
                let size = frame.area();
                let error_message = format!(
                    "window size must be at least {}x{}, currently {}x{}",
                    Self::MIN_TUI_DIMS.0,
                    Self::MIN_TUI_DIMS.1,
                    size.width,
                    size.height
                );

                let error_width = error_message.chars().count().try_into().unwrap_or(55);
                let error_height = 5;

                #[rustfmt::skip]
                let area = Block::default()
                    .borders(Borders::all())
                    .style(Style::new().fg(Color::White))
                    .inner(Rect::new(
                        size.width
                            .checked_sub(error_width)
                            .and_then(|n| n.checked_div(2))
                            .unwrap_or_default(),
                        size.height
                            .checked_sub(error_height)
                            .and_then(|n| n.checked_div(2))
                            .unwrap_or_default(),
                        if error_width > size.width { u16::MIN } else { error_width },
                        if size.height > error_height { error_height } else { size.height },
                    ));

                frame.render_widget(Clear, area);
                frame.render_widget(
                    Paragraph::new(
                        Line::from(error_message.clone()).style(
                            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                        ),
                    )
                    .alignment(Alignment::Center)
                    .wrap(Wrap { trim: false }),
                    area,
                );
            })?;

            return Ok(());
        }

        let boot_elapsed = self.boot_started.map(|t| t.elapsed().as_secs_f32());
        let booting = boot_elapsed.is_some_and(|e| e < BOOT_TOTAL);

        let result = term.try_draw(|frame| {
            let full = frame.area();
            let [header, body, footer_area] = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .areas(full);

            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "devcomp.xyz ",
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ))),
                Rect { x: header.x + 2, y: header.y + 2, width: 14, height: 1 },
            );

            if booting {
                // During the boot, we still draw the chrome and the domain name, just without
                // the interactable tabs
                let inner = content_frame(frame, body, &[], 0);
                let content_area = inner.inner(Margin { horizontal: 2, vertical: 1 });
                self.boot.draw(frame, content_area, boot_elapsed.unwrap_or(0.0));
                return Ok(());
            }

            // Render the tabs
            let mut tabs = self.tabs.try_lock().map_err(|_| Error::from(FRAME_BUSY))?;
            let current_tab = tabs.current_tab();
            tabs.draw(
                frame,
                Rect {
                    x: header.x + TAB_BAR_X,
                    y: header.y + 1,
                    width: header.width.saturating_sub(TAB_BAR_X),
                    height: header.height,
                },
            )
            .map_err(Error::other)?;
            drop(tabs);

            // Render the frame
            let inner = content_frame(frame, body, &TABS, current_tab);
            let mut content_area = inner.inner(Margin { horizontal: 2, vertical: 1 });

            if current_tab != Tabs::ABOUT {
                // About tab is short, so the cat can render however, but for other tabs
                // we need to reserve space for the cat so it does not overlap with the
                // tab's contents
                content_area.height = content_area.height.saturating_sub(CAT_RESERVE);
            }

            // The about page gets a right-hand rail, but only where there is space
            if current_tab == Tabs::ABOUT
                && content_area.width >= PROSE_SIZE + PANEL_WIDTH + RAIL_GUTTER
            {
                let [prose, rail] =
                    Layout::horizontal([Constraint::Min(0), Constraint::Length(PANEL_WIDTH)])
                        .areas(content_area);

                self.client_info
                    .try_lock()
                    .map_err(|_| Error::from(FRAME_BUSY))?
                    .draw(frame, Rect { height: 6.min(rail.height), ..rail })
                    .map_err(Error::other)?;

                content_area =
                    Rect { width: prose.width.saturating_sub(RAIL_GUTTER), ..prose };
            }

            if current_tab == Tabs::BLOG {
                #[cfg(feature = "blog")]
                {
                    // Render the post selection list if the blog tab is selected
                    self.blog_posts
                        .try_lock()
                        .map_err(|_| Error::from(FRAME_BUSY))?
                        .draw(frame, content_area)
                        .map_err(Error::other)?;
                }

                #[cfg(not(feature = "blog"))]
                {
                    let placeholder = Paragraph::new(
                        "blog feature is disabled. enable the `blog` feature to view this \
                         tab.",
                    )
                    .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));

                    frame.render_widget(placeholder, Rect { height: 1, ..content_area });
                }
            } else {
                self.content
                    .try_lock()
                    .map_err(|_| Error::from(FRAME_BUSY))?
                    .draw(frame, content_area)
                    .map_err(Error::other)?;
            }

            // Render the eepy cat :3
            self.cat
                .try_lock()
                .map_err(|_| Error::from(FRAME_BUSY))?
                .draw(frame, body)
                .map_err(Error::other)?;

            // Render the footer, minus the cat
            self.footer
                .try_lock()
                .map_err(|_| Error::from(FRAME_BUSY))?
                .draw(frame, footer_area)
                .map_err(Error::other)?;

            // The help overlay sits above everything else
            self.help
                .try_lock()
                .map_err(|_| Error::from(FRAME_BUSY))?
                .draw(frame, full)
                .map_err(Error::other)?;

            // The credits overlay sits above even the help overlay
            self.credits
                .try_lock()
                .map_err(|_| Error::from(FRAME_BUSY))?
                .draw(frame, full)
                .map_err(Error::other)?;

            Ok::<_, Error>(())
        });

        if boot_elapsed.is_some_and(|e| e >= BOOT_TOTAL) {
            // Boot finished and has faded, render TUI on next cycle
            self.boot_started = None;
        }

        match result {
            Ok(_) => Ok(()),
            // A component was busy, let's skip this frame and let the next tick redraw
            Err(err) if err.kind() == FRAME_BUSY => Ok(()),
            Err(err) => Err(err.into()),
        }
    }
}
