use std::io::Write;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use color_eyre::eyre::{self, eyre};
use russh::server::{Auth, Config, Handle, Handler, Msg, Server, Session};
use russh::{Channel, ChannelId, CryptoVec, Pty};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch, Mutex, RwLock};
use tracing::instrument;

use crate::app::App;
use crate::tui::backend::SshBackend;
use crate::tui::probe::Probe;
use crate::tui::terminal::{TerminalGeometry, TerminalInfo};
use crate::tui::{Terminal, Tui};
use crate::OPTIONS;

/// Number of frames to tolerate in a queue before considering a desync between the
/// server and the client. Redraw triggered if the threshold is exceeded.
const FRAME_QUEUE_DEPTH: usize = 8;

const PROBE_TIMEOUT: Duration = Duration::from_millis(1000);

/// A sink implementing [`Write`] which draws frames that ratatui renders over an SSH
/// connection.
#[derive(Debug)]
pub struct TermWriter {
    sink: Vec<u8>,
    tx: mpsc::Sender<Vec<u8>>,
    desynced: Arc<AtomicBool>,
    queued: Arc<AtomicUsize>,
}

impl TermWriter {
    #[instrument(skip(session, channel), level = "trace", fields(channel_id = %channel.id()))]
    fn new(session: Handle, channel: Channel<Msg>) -> Self {
        tracing::trace!("Acquiring new SSH writer");
        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(FRAME_QUEUE_DEPTH);
        let channel_id = channel.id();
        let queued = Arc::new(AtomicUsize::new(0));
        let drain_queued = Arc::clone(&queued);

        // NOTE: We spawn two separate tasks to drain the request and response channels.
        //
        // `Handle::data` sends on a *bounded* channel, so awaiting it from the `flush`
        // parks the render thread whenever the client has a desync because it is called
        // from inside the draw loop, which holds a lock on the TUI, which causes a full
        // deadlock. We fixed this by moving the await into these drain tasks instead of
        // within the main draw loop

        let mut incoming = channel;
        tokio::spawn(async move {
            // recv - Drain all the message requests from the queue. Unless read, the
            // internal buffer fills up (max size = 100 by default) and the session
            // fully deadlocks. Discovered when bursts of quick resizes caused the
            // server to become fully unresponsive
            while incoming.wait().await.is_some() {}
            tracing::debug!("SSH channel closed, stopping request drain");
        });

        tokio::spawn(async move {
            // send - Drain all data to be sent and zap it to the client
            while let Some(data) = rx.recv().await {
                let sent = session.data(channel_id, CryptoVec::from(data)).await;
                drain_queued.fetch_sub(1, Ordering::Release);
                if sent.is_err() {
                    tracing::debug!("SSH channel closed, stopping writer drain");
                    break;
                }
            }
        });

        Self { sink: Vec::new(), tx, desynced: Arc::new(AtomicBool::new(false)), queued }
    }

    pub fn desync_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.desynced)
    }

    pub fn queued_frames(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.queued)
    }
}

impl Write for TermWriter {
    #[instrument(skip(self, buf), level = "debug")]
    #[optimize(speed)]
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        tracing::trace!("Writing {} bytes into SSH terminal writer buffer", buf.len());
        self.sink.extend_from_slice(buf);
        Ok(buf.len())
    }

    #[instrument(skip(self), level = "trace")]
    #[optimize(speed)]
    fn flush(&mut self) -> std::io::Result<()> {
        tracing::trace!("Flushing SSH terminal writer buffer");
        if self.sink.is_empty() {
            return Ok(());
        }

        match self.tx.try_send(std::mem::take(&mut self.sink)) {
            Ok(()) => {
                self.queued.fetch_add(1, Ordering::Acquire);
                Ok(())
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                // Client and the internal ratatui state have a mismatch. Trigger a
                // full clear and redraw for the next frame
                self.desynced.store(true, Ordering::Relaxed);
                Ok(())
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                Err(std::io::Error::other("SSH channel closed"))
            }
        }
    }
}

pub struct SshSession {
    terminal_info: Arc<RwLock<TerminalInfo>>,
    app: Option<Arc<Mutex<App>>>,
    keystroke_tx: mpsc::UnboundedSender<Vec<u8>>,
    geometry_tx: watch::Sender<TerminalGeometry>,
    tui: Arc<RwLock<Option<Tui>>>,
    probe: Arc<Mutex<Option<Probe>>>,
}

impl SshSession {
    pub fn new() -> Self {
        let (keystroke_tx, keystroke_rx) = mpsc::unbounded_channel();
        let (geometry_tx, geometry_rx) = watch::channel(TerminalGeometry::default());

        let term_info = Arc::new(RwLock::new(TerminalInfo::default()));

        Self {
            terminal_info: Arc::clone(&term_info),
            app: App::new(
                term_info,
                OPTIONS.tick_rate,
                OPTIONS.frame_rate,
                keystroke_rx,
                geometry_rx,
            )
            .inspect_err(|err| tracing::error!("Failed to create app: {err}"))
            .ok()
            .map(|app| Arc::new(Mutex::new(app))),
            tui: Arc::new(RwLock::new(None)),
            keystroke_tx,
            geometry_tx,
            probe: Arc::new(Mutex::new(None)),
        }
    }

    /// Finalizes the probe, writes its results into the session's state and replays any user
    /// input keystrokes that were captured in the probing process.
    async fn finish_probe(
        probe: &Arc<Mutex<Option<Probe>>>,
        terminal_info: &Arc<RwLock<TerminalInfo>>,
        keystroke_tx: &mpsc::UnboundedSender<Vec<u8>>,
    ) {
        let Some(probe) = probe.lock().await.take() else {
            return;
        };

        let outcome = probe.finish();
        tracing::info!(
            "Capability probe concluded: {:?}, cell size {:?}, name {:?}",
            outcome.caps,
            outcome.cell_size,
            outcome.name,
        );

        // Set the probe's outcomes
        {
            let mut info = terminal_info.write().await;
            info.set_probed(outcome.caps);

            #[cfg(feature = "blog")]
            if let Some(size) = outcome.cell_size {
                info.set_font_size(size);
            }

            if let Some(name) = outcome.name {
                info.set_reported_name(name);
            }
        }

        // Forward any user inputs from during the probe period
        if !outcome.pending_input.is_empty() {
            tracing::debug!(
                "Replaying {} byte(s) typed during the probe",
                outcome.pending_input.len()
            );
            let _ = keystroke_tx.send(outcome.pending_input);
        }
    }

    /// Start the probe by writing its payload to the client and set its timeout.
    /// 
    /// NOTE: This method immediately returns after the payload has reached the 
    /// client, and does not wait for the actual probing to finish.
    async fn start_probe(&mut self, channel_id: ChannelId, session: &mut Session) {
        if self.probe.lock().await.is_some() {
            tracing::debug!("Capability probe already in progress, not restarting");
            return;
        }

        *self.probe.lock().await = Some(Probe::new());

        let handle = session.handle();
        if handle.data(channel_id, CryptoVec::from(Probe::query())).await.is_err() {
            tracing::debug!("Channel closed before the capability probe could be written");
            let _ = self.probe.lock().await.take();
            return;
        }

        tracing::debug!("Capability probe written, waiting up to {PROBE_TIMEOUT:?}");

        let probe = Arc::clone(&self.probe);
        let terminal_info = Arc::clone(&self.terminal_info);
        let keystroke_tx = self.keystroke_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(PROBE_TIMEOUT).await;
            if probe.lock().await.is_some() {
                tracing::info!("Capability probe timed out, falling back");
            }

            Self::finish_probe(&probe, &terminal_info, &keystroke_tx).await;
        });
    }

    async fn set_geometry(&self, geometry: TerminalGeometry) {
        #[cfg(feature = "blog")]
        if let Some(font_size) = geometry.font_size() {
            self.terminal_info.write().await.set_font_size(font_size);
        }

        let _ = self.geometry_tx.send(geometry);
    }

    async fn run_app(
        app: Arc<Mutex<App>>,
        term: Arc<Mutex<Terminal>>,
        tui: Arc<RwLock<Option<Tui>>>,
        session: &Handle,
        channel_id: ChannelId,
    ) -> eyre::Result<()> {
        app.lock_owned().await.run(term, tui).await?;
        session.close(channel_id).await.map_err(|_| eyre!("failed to close session"))?;
        session
            .exit_status_request(channel_id, 0)
            .await
            .map_err(|_| eyre!("failed to send session exit status"))
    }
}

impl Handler for SshSession {
    type Error = eyre::Error;

    #[instrument(skip(self), name = "user_login", fields(method = "none"))]
    async fn auth_none(&mut self, user: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    #[instrument(skip(self, session, channel), name = "channel_establish", fields(channel_id = %channel.id()))]
    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        if let Some(app) = &self.app {
            let session_handle = session.handle();
            let channel_id = channel.id();

            let inner_app = Arc::clone(app);
            let tui = Arc::clone(&self.tui);
            let mut geometry_rx = self.geometry_tx.subscribe();

            tracing::info!("Serving app to open session");
            tokio::task::spawn(async move {
                let result =
                    async || -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                        // Wait for an initial size notification
                        geometry_rx.changed().await?;
                        let geometry = *geometry_rx.borrow_and_update();

                        let writer = Arc::new(Mutex::new(Terminal::new(SshBackend::new(
                            TermWriter::new(session_handle.clone(), channel),
                            geometry.cols,
                            geometry.rows,
                            geometry.pixel_width,
                            geometry.pixel_height,
                        ))?));

                        Self::run_app(inner_app, writer, tui, &session_handle, channel_id)
                            .await?;
                        Ok(())
                    };

                match result().await {
                    Ok(()) => tracing::info!("Session exited successfully"),
                    Err(err) => {
                        tracing::error!("Session errored: {err}");
                        let _ = session_handle.channel_failure(channel_id).await;
                    }
                }
            });

            return Ok(true);
        }

        Err(eyre!("Failed to initialize App for session"))
    }

    #[instrument(skip_all, fields(channel_id = %channel_id))]
    async fn pty_request(
        &mut self,
        channel_id: ChannelId,
        term: &str,
        col_width: u32,
        row_height: u32,
        pix_width: u32,
        pix_height: u32,
        _modes: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::info!("PTY requested by terminal: {term}");
        tracing::debug!("dims: {col_width} * {row_height}, pixel: {pix_width} * {pix_height}");

        // Start probing client terminal for its capabilities
        self.start_probe(channel_id, session).await;

        tracing::debug!("Publishing initial pty geometry");
        self.set_geometry(TerminalGeometry {
            cols: col_width as u16,
            rows: row_height as u16,
            pixel_width: pix_width as u16,
            pixel_height: pix_height as u16,
        })
        .await;

        session.channel_success(channel_id)?;
        Ok(())
    }

    #[instrument(skip(self, _session), level = "trace")]
    async fn data(
        &mut self,
        _channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // If the probe is open, have it gobble all incoming data until we get the info
        // that we need. After it finishes, have it forward the user inputs as required
        {
            let mut guard = self.probe.lock().await;
            if let Some(probe) = guard.as_mut() {
                probe.feed(data);
                let typed = probe.take_input();
                let fenced = probe.is_fenced();
                drop(guard);

                if !typed.is_empty() {
                    tracing::debug!("Keystroke during probe window: {typed:?}, sending");
                    self.keystroke_tx
                        .send(typed)
                        .map_err(|_| eyre!("Failed to send event keystroke data"))?;
                }

                if fenced {
                    Self::finish_probe(&self.probe, &self.terminal_info, &self.keystroke_tx)
                        .await;
                }

                return Ok(());
            }
        }

        tracing::debug!("Received keystroke data from SSH: {:?}, sending", data);
        self.keystroke_tx
            .send(data.to_vec())
            .map_err(|_| eyre!("Failed to send event keystroke data"))
    }

    #[instrument(skip_all, fields(channel_id = %_channel_id))]
    async fn window_change_request(
        &mut self,
        _channel_id: ChannelId,
        col_width: u32,
        row_height: u32,
        pix_width: u32,
        pix_height: u32,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::info!("Terminal window resized by client, notifying components");
        tracing::debug!("dims: {col_width} * {row_height}, pixel: {pix_width} * {pix_height}");

        self.set_geometry(TerminalGeometry {
            cols: col_width as u16,
            rows: row_height as u16,
            pixel_width: pix_width as u16,
            pixel_height: pix_height as u16,
        })
        .await;

        Ok(())
    }
}

#[derive(Default)]
pub struct SshServer;

impl SshServer {
    #[instrument(skip(config), name = "ssh")]
    pub async fn start(addr: SocketAddr, config: Config) -> eyre::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        tracing::info!("SSH server listening!");

        Self.run_on_socket(Arc::new(config), &listener).await.map_err(|err| eyre!(err))
    }
}

impl Server for SshServer {
    type Handler = SshSession;

    #[instrument(skip(self))]
    fn new_client(&mut self, peer_addr: Option<SocketAddr>) -> Self::Handler {
        tokio::task::block_in_place(SshSession::new)
    }
}
