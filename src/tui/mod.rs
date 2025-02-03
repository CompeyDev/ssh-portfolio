#![allow(dead_code)] // TODO: Remove this once you start using the code

use std::{sync::Arc, time::Duration};

use backend::SshBackend;
use color_eyre::Result;
use crossterm::{
    cursor,
    event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        KeyEvent, MouseEvent,
    },
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use serde::{Deserialize, Serialize};
use status::TuiStatus;
use tokio::{
    runtime::Handle,
    sync::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        Mutex, RwLock,
    },
    task::{block_in_place, JoinHandle},
    time::{interval, sleep, timeout},
};
use tokio_util::sync::CancellationToken;
use tracing::error;

pub(crate) mod backend;
pub(crate) mod status;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Event {
    Init,
    Quit,
    Error,
    Closed,
    Tick,
    Render,
    FocusGained,
    FocusLost,
    Paste(String),
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
}

pub type Terminal = ratatui::Terminal<SshBackend>;

#[derive(Debug)]
pub struct Tui {
    pub terminal: Arc<Mutex<Terminal>>,
    pub task: JoinHandle<()>,
    pub cancellation_token: CancellationToken,
    pub event_rx: UnboundedReceiver<Event>,
    pub event_tx: UnboundedSender<Event>,
    pub resume_tx: Option<UnboundedSender<()>>,
    pub status: Arc<RwLock<TuiStatus>>,
    pub frame_rate: f64,
    pub tick_rate: f64,
    pub mouse: bool,
    pub paste: bool,
}

impl Tui {
    pub fn new(terminal: Arc<Mutex<Terminal>>) -> Result<Self> {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Ok(Self {
            terminal,
            task: tokio::spawn(async {}),
            cancellation_token: CancellationToken::new(),
            event_rx,
            event_tx,
            resume_tx: Option::None,
            status: Arc::new(RwLock::new(TuiStatus::Active)),
            frame_rate: 60.0,
            tick_rate: 4.0,
            mouse: false,
            paste: false,
        })
    }

    pub fn tick_rate(mut self, tick_rate: f64) -> Self {
        self.tick_rate = tick_rate;
        self
    }

    pub fn frame_rate(mut self, frame_rate: f64) -> Self {
        self.frame_rate = frame_rate;
        self
    }

    pub fn mouse(mut self, mouse: bool) -> Self {
        self.mouse = mouse;
        self
    }

    pub fn paste(mut self, paste: bool) -> Self {
        self.paste = paste;
        self
    }

    pub fn start(&mut self) {
        self.cancel(); // Cancel any existing task
        self.cancellation_token = CancellationToken::new();
        let event_loop = Self::event_loop(
            self.status.clone(),
            self.event_tx.clone(),
            self.cancellation_token.clone(),
            self.tick_rate,
            self.frame_rate,
        );
        self.task = tokio::spawn(async {
            event_loop.await;
        });
    }

    async fn event_loop(
        status: Arc<RwLock<TuiStatus>>,
        event_tx: UnboundedSender<Event>,
        cancellation_token: CancellationToken,
        tick_rate: f64,
        frame_rate: f64,
    ) {
        let mut tick_interval = interval(Duration::from_secs_f64(1.0 / tick_rate));
        let mut render_interval = interval(Duration::from_secs_f64(1.0 / frame_rate));

        // if this fails, then it's likely a bug in the calling code
        event_tx
            .send(Event::Init)
            .expect("failed to send init event");

        let suspension_status = Arc::clone(&status);
        loop {
            let _guard = suspension_status.read().await;
            let event = tokio::select! {
                _ = cancellation_token.cancelled() => {
                    break;
                }
                _ = tick_interval.tick() => Event::Tick,
                _ = render_interval.tick() => Event::Render,
            };

            if event_tx.send(event).is_err() {
                // the receiver has been dropped, so there's no point in continuing the loop
                break;
            }
        }
        cancellation_token.cancel();
    }

    async fn await_shutdown(&self) {
        while !self.task.is_finished() {
            sleep(Duration::from_millis(1)).await;
        }
    }

    pub async fn stop(&self) -> Result<()> {
        self.cancel();
        
        let attempt_timeout = Duration::from_millis(50);
        let abort_shutdown = async {
            while !self.task.is_finished() {
                self.task.abort();
            }
        };

        if let Err(_) = timeout(attempt_timeout, self.await_shutdown()).await {
            timeout(attempt_timeout, abort_shutdown)
                .await
                .inspect_err(|_| {
                    error!("Failed to abort task in 100 milliseconds for unknown reason")
                })?;
        }

        Ok(())
    }

    pub fn enter(&mut self) -> Result<()> {
        let mut term = self.terminal.try_lock()?;
        // crossterm::terminal::enable_raw_mode()?; // TODO: Enable raw mode for pty
        crossterm::execute!(term.backend_mut(), EnterAlternateScreen, cursor::Hide)?;

        if self.mouse {
            crossterm::execute!(term.backend_mut(), EnableMouseCapture)?;
        }

        if self.paste {
            crossterm::execute!(term.backend_mut(), EnableBracketedPaste)?;
        }

        drop(term);

        self.start();
        Ok(())
    }

    pub async fn exit(&mut self) -> Result<()> {
        self.stop().await?;
        // TODO: enable raw mode for pty
        if true || crossterm::terminal::is_raw_mode_enabled()? {
            let mut term = self.terminal.try_lock()?;
            term.flush()?;

            if self.paste {
                crossterm::execute!(term.backend_mut(), DisableBracketedPaste)?;
            }

            if self.mouse {
                crossterm::execute!(term.backend_mut(), DisableMouseCapture)?;
            }

            crossterm::execute!(term.backend_mut(), LeaveAlternateScreen, cursor::Show)?;
            // crossterm::terminal::disable_raw_mode()?; // TODO: disable raw mode
        }
        Ok(())
    }

    pub fn cancel(&self) {
        self.cancellation_token.cancel();
    }

    pub async fn suspend(&mut self) -> Result<Arc<CancellationToken>> {
        // Exit the current Tui
        self.exit().await?;

        // Update the status and initialize a cancellation token
        let token = Arc::new(CancellationToken::new());
        let suspension = Arc::new(Mutex::new(()));
        *self.status.write().await = TuiStatus::Suspended(Arc::clone(&suspension));

        // Spawn a task holding on the lock until a notification interrupts it
        let status = Arc::clone(&self.status);
        let lock_release = Arc::clone(&token);
        tokio::task::spawn(async move {
            tokio::select! {
                _ = lock_release.cancelled() => {
                    // Lock was released, update the status
                    *status.write().await = TuiStatus::Active;
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(u64::MAX)) => {
                    // Hold on to the lock until notified
                    let _ = suspension.lock().await;
                }
            }
        });

        Ok(token)
    }

    pub async fn next_event(&mut self) -> Option<Event> {
        self.event_rx.recv().await
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        block_in_place(|| {
            let handle = Handle::current();
            let _ = handle.block_on(async {
                self.exit()
                    .await
                    .inspect_err(|err| error!("Failed to exit Tui: {err}"))
            });
        })
    }
}
