use std::{io::Write, net::SocketAddr, sync::Arc};

use async_trait::async_trait;
use color_eyre::eyre::eyre;
use ratatui::prelude::CrosstermBackend;
use russh::{
    server::{Auth, Handle, Handler, Msg, Server, Session},
    Channel, ChannelId, CryptoVec, Pty, 
};
use tokio::{
    runtime::Handle as TokioHandle,
    sync::{mpsc, Mutex, RwLock},
};
use tracing::instrument;

use crate::{
    app::App,
    tui::{Terminal, Tui},
};

#[derive(Debug)]
pub struct TermWriter {
    inner: CryptoVec,

    session: Handle,
    channel: Channel<Msg>,
}

impl TermWriter {
    fn new(session: &mut Session, channel: Channel<Msg>) -> Self {
        Self {
            session: session.handle(),
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
        self.inner.extend(buf);
        Ok(buf.len())
    }

    #[instrument(skip(self), level = "trace")]
    fn flush(&mut self) -> std::io::Result<()> {
        tokio::task::block_in_place(|| self.flush_inner())
    }
}

pub struct SshSession {
    app: Option<Arc<Mutex<App>>>,
    ssh_tx: mpsc::UnboundedSender<Vec<u8>>,
    tui: Arc<RwLock<Option<Tui>>>,
}

unsafe impl Send for SshSession {}

impl SshSession {
    pub fn new() -> Self {
        let (ssh_tx, ssh_rx) = mpsc::unbounded_channel();

        Self {
            app: App::new(10f64, 60f64, ssh_rx)
                .ok()
                .map(|app| Arc::new(Mutex::new(app))),
            tui: Arc::new(RwLock::new(None)),
            ssh_tx,
        }
    }
}

#[async_trait]
impl Handler for SshSession {
    type Error = color_eyre::eyre::Error;

    #[instrument(skip(self), span = "user_login", fields(method = "none"))]
    async fn auth_none(&mut self, user: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    #[instrument(skip(self), span = "channel_establish", level = "trace")]
    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        if let Some(app) = &self.app {
            let session_handle = session.handle();
            let channel_id = channel.id();

            let inner_app = Arc::clone(app);
            let term = Terminal::new(CrosstermBackend::new(TermWriter::new(session, channel)))?;
            let writer = Arc::new(Mutex::new(term));
            let tui = Arc::clone(&self.tui);

            tokio::task::spawn(async move {
                inner_app.lock_owned().await.run(writer, tui).await.unwrap();
                session_handle.close(channel_id).await.unwrap();
                session_handle.exit_status_request(channel_id, 0).await.unwrap();
            });

            return Ok(true);
        }

        Err(eyre!("Failed to initialize App for session"))
    }
    #[instrument(skip(self, session), level = "trace")]
    async fn pty_request(
        &mut self,
        channel_id: ChannelId,
        term: &str,
        col_width: u32,
        row_height: u32,
        pix_width: u32,
        pix_height: u32,
        modes: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::info!("Received pty request from channel {channel_id}; terminal: {term}");
        tracing::debug!("dims: {col_width} * {row_height}, pixel: {pix_width} * {pix_height}");
        
        if !term.contains("xterm") {
            session.channel_failure(channel_id)?;
            return Err(eyre!("Unsupported terminal type: {term}"));
        }

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
        self.ssh_tx
            .send(data.to_vec())
            .map_err(|_| eyre!("Failed to send event keystroke data"))
    }
}

#[derive(Default)]
pub struct SshServer;

#[async_trait]
impl Server for SshServer {
    type Handler = SshSession;

    #[instrument(skip(self))]
    fn new_client(&mut self, peer_addr: Option<SocketAddr>) -> Self::Handler {
        let session = SshSession::new();
        session
    }
}
