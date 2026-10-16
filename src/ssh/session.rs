use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use color_eyre::eyre::{self, eyre};
use russh::server::{Auth, Handle, Handler, Msg, Session};
use russh::{Channel, ChannelId, CryptoVec, Pty};
use tokio::sync::{mpsc, watch, Mutex, RwLock};
use tracing::instrument;

use super::writer::TermWriter;
use crate::app::App;
use crate::tui::backend::SshBackend;
use crate::tui::probe::Probe;
use crate::tui::terminal::{TerminalGeometry, TerminalInfo};
use crate::tui::{Terminal, Tui};
use crate::OPTIONS;

/// Client input is buffered. The sender is throttled by this limit, so that
/// one client cannot exhaust the memory with keystroke data. russh stops reading
/// beyond this limit, and the client must wait before sending more data.
const KEYSTROKE_CHANNEL_CAP: usize = 256;
const MAX_USERNAME_LEN: usize = 64;

const PROBE_TIMEOUT: Duration = Duration::from_millis(1000);

pub struct SshSession {
    terminal_info: Arc<RwLock<TerminalInfo>>,
    username: Arc<RwLock<Option<String>>>,
    app: Option<Arc<Mutex<App>>>,
    keystroke_tx: mpsc::Sender<Vec<u8>>,
    geometry_tx: watch::Sender<TerminalGeometry>,
    tui: Arc<RwLock<Option<Tui>>>,
    probe: Arc<Mutex<Option<Probe>>>,
}

impl SshSession {
    pub fn new(peer_ip: Option<IpAddr>) -> Self {
        let (keystroke_tx, keystroke_rx) = mpsc::channel(KEYSTROKE_CHANNEL_CAP);
        let (geometry_tx, geometry_rx) = watch::channel(TerminalGeometry::default());

        let term_info = Arc::new(RwLock::new(TerminalInfo::default()));
        let username = Arc::new(RwLock::new(None));

        Self {
            terminal_info: Arc::clone(&term_info),
            username: Arc::clone(&username),
            app: App::new(
                term_info,
                username,
                peer_ip,
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
        keystroke_tx: &mpsc::Sender<Vec<u8>>,
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
            if !info.has_font_size() {
                // PTY request did not hand us any pixel dimensions, and we must query it
                // over the probe from the terminal
                if let Some(size) = outcome.cell_size {
                    info.set_font_size(size);
                }
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
            let _ = keystroke_tx.send(outcome.pending_input).await;
        }
    }

    /// Start the probe by writing its payload to the client and set its timeout.
    ///
    /// NOTE: This method immediately returns after the payload has reached the
    /// client, and does not wait for the actual probing to finish.
    async fn start_probe(
        &mut self,
        channel_id: ChannelId,
        grid: (u16, u16),
        session: &mut Session,
    ) {
        if self.probe.lock().await.is_some() {
            tracing::debug!("Capability probe already in progress, not restarting");
            return;
        }

        *self.probe.lock().await = Some(Probe::new(grid));

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
        // Sanitize the username: strip control characters & bound it by length
        let name: String =
            user.chars().filter(|c| !c.is_control()).take(MAX_USERNAME_LEN).collect();
        if !name.is_empty() {
            *self.username.write().await = Some(name);
        }

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
        self.start_probe(channel_id, (col_width as u16, row_height as u16), session).await;

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
                        .await
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
            .await
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
