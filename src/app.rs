use std::sync::Arc;

use color_eyre::{eyre, Result};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::Rect;
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{mpsc, Mutex, RwLock},
    task::block_in_place,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use crate::{
    action::Action,
    components::{fps::FpsCounter, home::Home, Component},
    config::Config,
    keycode::KeyCodeExt,
    tui::{Event, Terminal, Tui},
};

pub struct App {
    config: Config,
    tick_rate: f64,
    frame_rate: f64,
    components: Vec<Box<Arc<Mutex<dyn Component>>>>,
    should_quit: bool,
    should_suspend: bool,
    mode: Mode,
    last_tick_key_events: Vec<KeyEvent>,
    action_tx: mpsc::UnboundedSender<Action>,
    action_rx: mpsc::UnboundedReceiver<Action>,
    ssh_rx: mpsc::UnboundedReceiver<Vec<u8>>,
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Home,
}

impl App {
    pub fn new(
        tick_rate: f64,
        frame_rate: f64,
        ssh_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    ) -> Result<Self> {
        let (action_tx, action_rx) = mpsc::unbounded_channel();
        Ok(Self {
            tick_rate,
            frame_rate,
            components: vec![
                Box::new(Arc::new(Mutex::new(Home::new()))),
                Box::new(Arc::new(Mutex::new(FpsCounter::default()))),
            ],
            should_quit: false,
            should_suspend: false,
            config: Config::new()?,
            mode: Mode::Home,
            last_tick_key_events: Vec::new(),
            action_tx,
            action_rx,
            ssh_rx,
        })
    }

    pub async fn run(
        &mut self,
        term: Arc<Mutex<Terminal>>,
        tui: Arc<RwLock<Option<Tui>>>,
    ) -> Result<()> {
        let mut tui = tui.write().await;
        let mut tui = tui.get_or_insert(
            Tui::new(term)?
                // .mouse(true) // uncomment this line to enable mouse support
                .tick_rate(self.tick_rate)
                .frame_rate(self.frame_rate),
        );

        // Blocking initialization logic for tui and components
        block_in_place(|| {
            tui.enter()?;

            for component in self.components.iter_mut() {
                component
                    .try_lock()?
                    .register_action_handler(self.action_tx.clone())?;
            }

            for component in self.components.iter_mut() {
                component
                    .try_lock()?
                    .register_config_handler(self.config.clone())?;
            }

            for component in self.components.iter_mut() {
                component
                    .try_lock()?
                    .init(tui.terminal.try_lock()?.size()?)?;
            }

            Ok::<_, eyre::Error>(())
        })?;

        let action_tx = self.action_tx.clone();
        let mut resume_tx: Option<Arc<CancellationToken>> = None;
        loop {
            self.handle_events(&mut tui).await?;
            // self.handle_actions(&mut tui)?;
            block_in_place(|| self.handle_actions(&mut tui))?;
            if self.should_suspend {
                if let Some(ref tx) = resume_tx {
                    tx.cancel(); 
                    resume_tx = None;
                } else {
                    resume_tx = Some(tui.suspend().await?);
                    continue
                }
                action_tx.send(Action::Resume)?;
                action_tx.send(Action::ClearScreen)?;
                // tui.mouse(true);
                block_in_place(|| tui.enter())?;
            } else if self.should_quit {
                block_in_place(|| tui.stop())?;
                break;
            }
        }

        block_in_place(|| tui.exit())
    }

    async fn handle_events(&mut self, tui: &mut Tui) -> Result<()> {
        tokio::select! {
            Some(event) = tui.next_event() => {
                // Wait for next event and fire required actions for components
                let action_tx = self.action_tx.clone();
                match event {
                    Event::Quit => action_tx.send(Action::Quit)?,
                    Event::Tick => action_tx.send(Action::Tick)?,
                    Event::Render => action_tx.send(Action::Render)?,
                    Event::Resize(x, y) => action_tx.send(Action::Resize(x, y))?,
                    Event::Key(key) => block_in_place(|| self.handle_key_event(key))?,
                    _ => {}
                };

                for component in self.components.iter_mut() {
                    let mut component = component.try_lock()?;
                    if let Some(action) = block_in_place(|| component.handle_events(Some(event.clone())))? {
                        action_tx.send(action)?;
                    }
                }
            }
            Some(ssh_data) = self.ssh_rx.recv() => {
                // Receive keystroke data from SSH connection
                let key_event = KeyCode::from_xterm_seq(&ssh_data[..]).into_key_event();
                block_in_place(|| self.handle_key_event(key_event))?;
            }
        }
        Ok(())
    }
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<()> {
        let action_tx = self.action_tx.clone();
        let Some(keymap) = self.config.keybindings.get(&self.mode) else {
            return Ok(());
        };
        match keymap.get(&vec![key]) {
            Some(action) => {
                info!("Got action: {action:?}");
                action_tx.send(action.clone())?;
            }
            _ => {
                // If the key was not handled as a single key action,
                // then consider it for multi-key combinations
                self.last_tick_key_events.push(key);

                // Check for multi-key combinations
                if let Some(action) = keymap.get(&self.last_tick_key_events) {
                    info!("Got action: {action:?}");
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
                Action::Quit => self.should_quit = true,
                Action::Suspend => self.should_suspend = true,
                Action::Resume => self.should_suspend = false,
                Action::ClearScreen => tui.terminal.try_lock()?.clear()?,
                Action::Resize(w, h) => self.handle_resize(tui, w, h)?,
                Action::Render => self.render(tui)?,
                _ => {}
            }

            for component in self.components.iter_mut() {
                if let Some(action) = component.try_lock()?.update(action.clone())? {
                    self.action_tx.send(action)?
                };
            }
        }
        Ok(())
    }

    fn handle_resize(&mut self, tui: &mut Tui, w: u16, h: u16) -> Result<()> {
        tui.terminal.try_lock()?.resize(Rect::new(0, 0, w, h))?;
        self.render(tui)?;
        Ok(())
    }

    fn render(&mut self, tui: &mut Tui) -> Result<()> {
        tui.terminal.try_lock()?.draw(|frame| {
            for component in self.components.iter_mut() {
                if let Err(err) = component.blocking_lock().draw(frame, frame.area()) {
                    let _ = self
                        .action_tx
                        .send(Action::Error(format!("Failed to draw: {:?}", err)));
                }
            }
        })?;
        Ok(())
    }
}
