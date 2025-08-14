use std::{io::Write, net::SocketAddr, sync::Arc};

use async_trait::async_trait;
use color_eyre::eyre::{self, eyre};
use russh::{
    server::{Auth, Config, Handle, Handler, Msg, Server, Session},
    Channel, ChannelId, CryptoVec, Pty,
};
use tokio::{
    net::TcpListener,
    runtime::Handle as TokioHandle,
    sync::{mpsc, oneshot, Mutex, RwLock},
};
use tracing::instrument;

use crate::{
    app::App,
    tui::{backend::SshBackend, Terminal, Tui},
    OPTIONS,
};

#[derive(Debug)]
pub struct TermWriter {
    inner: CryptoVec,

    session: Handle,
    channel: Channel<Msg>,
}

impl TermWriter {
    #[instrument(skip(session, channel), level = "trace", fields(channel_id = %channel.id()))]
    fn new(session: Handle, channel: Channel<Msg>) -> Self {
        tracing::trace!("Acquiring new SSH writer");
        Self {
            session,
            channel,
            inner: CryptoVec::new(),
        }
    }

    fn flush_inner(&mut self) -> std::io::Result<()> {
        let handle = TokioHandle::current();
        handle.block_on(async move {
            self.session
                .data(self.channel.id(), self.inner.clone())
                .await
                .map_err(|err| {
                    std::io::Error::other(String::from_iter(err.iter().map(|item| *item as char)))
                })
                .and_then(|()| Ok(self.inner.clear()))
        })
    }
}

impl Write for TermWriter {
    #[instrument(skip(self, buf), level = "debug")]
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        tracing::trace!(
            "Writing {} bytes into SSH terminal writer buffer",
            buf.len()
        );
        self.inner.extend(buf);
        Ok(buf.len())
    }

    #[instrument(skip(self), level = "trace")]
    fn flush(&mut self) -> std::io::Result<()> {
        tracing::trace!("Flushing SSH terminal writer buffer");
        tokio::task::block_in_place(|| self.flush_inner())
    }
}

pub struct SshSession {
    app: Option<Arc<Mutex<App>>>,
    keystroke_tx: mpsc::UnboundedSender<Vec<u8>>,
    resize_tx: mpsc::UnboundedSender<(u16, u16)>,
    init_dims_tx: Option<oneshot::Sender<((u16, u16), (u16, u16))>>,
    init_dims_rx: Option<oneshot::Receiver<((u16, u16), (u16, u16))>>,
    tui: Arc<RwLock<Option<Tui>>>,
}

impl SshSession {
    pub fn new() -> Self {
        let (keystroke_tx, keystroke_rx) = mpsc::unbounded_channel();
        let (resize_tx, resize_rx) = mpsc::unbounded_channel();
        let (init_dims_tx, init_dims_rx) = oneshot::channel();

        Self {
            app: App::new(
                OPTIONS.tick_rate,
                OPTIONS.frame_rate,
                keystroke_rx,
                resize_rx,
            )
            .ok()
            .map(|app| Arc::new(Mutex::new(app))),
            tui: Arc::new(RwLock::new(None)),
            keystroke_tx,
            resize_tx,
            init_dims_tx: Some(init_dims_tx),
            init_dims_rx: Some(init_dims_rx), // Only an option so that I can take ownership of it
        }
    }

    async fn run_app(
        app: Arc<Mutex<App>>,
        writer: Arc<Mutex<Terminal>>,
        tui: Arc<RwLock<Option<Tui>>>,
        session: &Handle,
        channel_id: ChannelId,
    ) -> eyre::Result<()> {
        app.lock_owned().await.run(writer, tui).await?;
        session
            .close(channel_id)
            .await
            .map_err(|_| eyre!("failed to close session"))?;
        session
            .exit_status_request(channel_id, 0)
            .await
            .map_err(|_| eyre!("failed to send session exit status"))
    }
}

#[async_trait]
impl Handler for SshSession {
    type Error = eyre::Error;

    #[instrument(skip(self), span = "user_login", fields(method = "none"))]
    async fn auth_none(&mut self, user: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    #[instrument(skip(self, session, channel), span = "channel_establish", fields(channel_id = %channel.id()))]
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
            let rx = self.init_dims_rx.take().unwrap();

            tracing::info!("Serving app to open session");
            tokio::task::spawn(async move {
                let result = async || -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                    let ((term_width, term_height), (pixel_width, pixel_height)) = rx.await?;
                    let writer = Arc::new(Mutex::new(Terminal::new(SshBackend::new(
                        TermWriter::new(session_handle.clone(), channel),
                        term_width,
                        term_height,
                        pixel_width,
                        pixel_height,
                    ))?));

                    Self::run_app(inner_app, writer, tui, &session_handle, channel_id).await?;
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

        if !term.contains("xterm") {
            session.channel_failure(channel_id)?;
            return Err(eyre!("Unsupported terminal type: {term}"));
        }

        let tx = self.init_dims_tx.take().unwrap();
        if !tx.is_closed() {
            // If we've not already initialized the terminal, send the initial dimensions
            tracing::debug!("Sending initial pty dimensions");
            tx.send((
                (col_width as u16, row_height as u16),
                (pix_width as u16, pix_height as u16),
            ))
            .map_err(|_| eyre!("Failed to send initial pty dimensions"))?;
        }

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
        tracing::debug!("Received keystroke data from SSH: {:?}, sending", data);
        self.keystroke_tx
            .send(data.to_vec())
            .map_err(|_| eyre!("Failed to send event keystroke data"))
    }

    async fn window_change_request(
        &mut self,
        _: ChannelId,
        col_width: u32,
        row_height: u32,
        _: u32,
        _: u32,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        // TODO: actually make it resize properly
        // That would involve first updating the Backend's size and then updating the rect via the event
        self.resize_tx
            .send((col_width as u16, row_height as u16))
            .map_err(|_| eyre!("Failed to send pty size specifications"))?;

        Ok(())
    }
}

#[derive(Default)]
pub struct SshServer;

impl SshServer {
    #[instrument(level = "trace")]
    pub async fn start(addr: SocketAddr, config: Config) -> eyre::Result<()> {
        let listener = TcpListener::bind(addr).await?;

        Self::default()
            .run_on_socket(Arc::new(config), &listener)
            .await
            .map_err(|err| eyre!(err))
    }
}

#[async_trait]
impl Server for SshServer {
    type Handler = SshSession;

    #[instrument(skip(self))]
    fn new_client(&mut self, peer_addr: Option<SocketAddr>) -> Self::Handler {
        let session = tokio::task::block_in_place(|| SshSession::new());
        session
    }
}
