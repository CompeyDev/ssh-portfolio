#![allow(dead_code)] // Remove this once you start using the code

use std::{sync::Arc, time::Duration};

use color_eyre::Result;
use crossterm::{
    cursor,
    event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event as CrosstermEvent, EventStream, KeyEvent, KeyEventKind, MouseEvent,
    },
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::{FutureExt, StreamExt};
use ratatui::backend::CrosstermBackend as Backend;
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        Mutex,
    },
    task::JoinHandle,
    time::interval,
};
use tokio_util::sync::CancellationToken;
use tracing::error;

use crate::ssh::TermWriter;

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

pub type Terminal = ratatui::Terminal<Backend<TermWriter>>;
pub struct Tui {
    pub terminal: Arc<Mutex<Terminal>>,
    pub task: JoinHandle<()>,
    pub cancellation_token: CancellationToken,
    pub event_rx: UnboundedReceiver<Event>,
    pub event_tx: UnboundedSender<Event>,
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
        event_tx: UnboundedSender<Event>,
        cancellation_token: CancellationToken,
        tick_rate: f64,
        frame_rate: f64,
    ) {
        let mut event_stream = EventStream::new();
        let mut tick_interval = interval(Duration::from_secs_f64(1.0 / tick_rate));
        let mut render_interval = interval(Duration::from_secs_f64(1.0 / frame_rate));

        // if this fails, then it's likely a bug in the calling code
        event_tx
            .send(Event::Init)
            .expect("failed to send init event");
        loop {
            let event = tokio::select! {
                _ = cancellation_token.cancelled() => {
                    break;
                }
                _ = tick_interval.tick() => Event::Tick,
                _ = render_interval.tick() => Event::Render,
                crossterm_event = event_stream.next().fuse() => match crossterm_event {
                    Some(Ok(event)) => match event {
                        CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => Event::Key(key),
                        CrosstermEvent::Mouse(mouse) => Event::Mouse(mouse),
                        CrosstermEvent::Resize(x, y) => Event::Resize(x, y),
                        CrosstermEvent::FocusLost => Event::FocusLost,
                        CrosstermEvent::FocusGained => Event::FocusGained,
                        CrosstermEvent::Paste(s) => Event::Paste(s),
                        _ => continue, // ignore other events
                    }
                    Some(Err(_)) => Event::Error,
                    None => break, // the event stream has stopped and will not produce any more events
                },
            };
            if event_tx.send(event).is_err() {
                // the receiver has been dropped, so there's no point in continuing the loop
                break;
            }
        }
        cancellation_token.cancel();
    }

    pub fn stop(&self) -> Result<()> {
        self.cancel();
        let mut counter = 0;
        while !self.task.is_finished() {
            std::thread::sleep(Duration::from_millis(1));
            counter += 1;
            if counter > 50 {
                self.task.abort();
            }
            if counter > 100 {
                error!("Failed to abort task in 100 milliseconds for unknown reason");
                break;
            }
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

    pub fn exit(&mut self) -> Result<()> {
        self.stop()?;
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

    pub fn suspend(&mut self) -> Result<()> {
        self.exit()?;
        #[cfg(not(windows))]
        signal_hook::low_level::raise(signal_hook::consts::signal::SIGTSTP)?;
        Ok(())
    }

    pub fn resume(&mut self) -> Result<()> {
        self.enter()?;
        Ok(())
    }

    pub async fn next_event(&mut self) -> Option<Event> {
        self.event_rx.recv().await
    }
}

// impl Deref for Tui {
//     type Target = ratatui::Terminal<Backend<TermWriter>>;

//     fn deref(&self) -> &Self::Target {
//         self.terminal.as_ref()
//     }
// }

// impl DerefMut for Tui {
//     fn deref_mut(&mut self) -> &mut Self::Target {
//         self.terminal.get_mut().unwrap()
//     }
// }

impl Drop for Tui {
    fn drop(&mut self) {
        self.exit().unwrap();
    }
}
