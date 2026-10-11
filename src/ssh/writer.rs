use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use russh::server::{Handle, Msg};
use russh::{Channel, CryptoVec};
use tokio::sync::mpsc;
use tracing::instrument;

/// Number of frames to tolerate in a queue before considering a desync between the
/// server and the client. Redraw triggered if the threshold is exceeded.
const FRAME_QUEUE_DEPTH: usize = 8;

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
    pub(super) fn new(session: Handle, channel: Channel<Msg>) -> Self {
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
