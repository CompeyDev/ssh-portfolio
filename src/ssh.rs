use std::{io::Write, net::SocketAddr, sync::Arc};

use async_trait::async_trait;
use ratatui::prelude::CrosstermBackend;
use russh::{
    server::{Auth, Handle, Handler, Msg, Server, Session},
    Channel, ChannelId, CryptoVec, Pty,
};
use tokio::{runtime::Handle as TokioHandle, sync::Mutex};
use tracing::instrument;

use crate::{app::App, tui::Terminal};

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
}

impl Write for TermWriter {
    #[instrument(skip(self, buf), level = "debug")]
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.extend(buf);
        Ok(buf.len())
    }

    #[instrument(skip(self), level = "trace")]
    fn flush(&mut self) -> std::io::Result<()> {
        let handle = TokioHandle::current();
        handle.block_on(async move {
            self.session
                .data(self.channel.id(), self.inner.clone())
                .await
                .map_err(|err| {
                    std::io::Error::other(String::from_iter(err.iter().map(|item| *item as char)))
                })?;

            self.inner.clear();
            Ok(())
        })
    }
}

pub struct SshSession(Option<Arc<Mutex<App>>>);

unsafe impl Send for SshSession {}

impl SshSession {
    pub fn new() -> Self {
        Self(
            App::new(10f64, 60f64)
                .ok()
                .map(|app| Arc::new(Mutex::new(app))),
        )
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
        if let Some(app) = &self.0 {
            let inner_app = Arc::clone(app);
            let term = Terminal::new(CrosstermBackend::new(TermWriter::new(session, channel)))?;
            let writer = Arc::new(Mutex::new(term));
            
            tokio::task::spawn(async move {
                inner_app.lock_owned().await.run(writer).await.unwrap();
            });

            return Ok(true);
        }

        return Err(color_eyre::eyre::eyre!(
            "Failed to initialize App for session"
        ));
    }

    #[instrument(skip(self, _session), level = "trace")]
    async fn pty_request(
        &mut self,
        channel_id: ChannelId,
        term: &str,
        col_width: u32,
        row_height: u32,
        pix_width: u32,
        pix_height: u32,
        modes: &[(Pty, u32)],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::info!("received pty request from channel {channel_id}");
        tracing::debug!("dims: {col_width} * {row_height}, pixel: {pix_width} * {pix_height}");
        Ok(())
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
        // self.0.push((peer_addr.unwrap(), session));
        session
    }
}
