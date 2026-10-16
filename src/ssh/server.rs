use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use color_eyre::eyre;
use russh::server::{run_stream, Config, Server};
use tokio::net::TcpListener;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tracing::instrument;

use super::session::SshSession;

#[derive(Default)]
pub struct SshServer;

impl SshServer {
    #[instrument(skip(config), name = "ssh")]
    pub async fn start(addr: SocketAddr, config: Config) -> eyre::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        tracing::info!("SSH server listening!");

        let config = Arc::new(config);
        let gate = ConnectionGate::new();

        loop {
            let (stream, peer) = match listener.accept().await {
                Ok(pair) => pair,
                Err(err) => {
                    tracing::warn!("Failed to accept SSH connection: {err}");
                    continue;
                }
            };

            let Some(guard) = gate.admit(peer.ip()) else {
                continue;
            };

            let handler = Self.new_client(Some(peer));
            let config = Arc::clone(&config);
            tokio::spawn(async move {
                let _guard = guard;
                match run_stream(config, stream, handler).await {
                    Ok(session) => {
                        if let Err(err) = session.await {
                            tracing::debug!("SSH session ended with error: {err}");
                        }
                    }
                    Err(err) => tracing::debug!("Failed to start SSH session: {err}"),
                }
            });
        }
    }
}

impl Server for SshServer {
    type Handler = SshSession;

    #[instrument(skip(self))]
    fn new_client(&mut self, peer_addr: Option<SocketAddr>) -> Self::Handler {
        let peer_ip = peer_addr.map(|addr| addr.ip());
        tokio::task::block_in_place(|| SshSession::new(peer_ip))
    }
}

const MAX_CONNECTIONS: usize = 256;
const MAX_CONNECTIONS_PER_IP: usize = 8;

/// Number of live connections keyed by IP
type PeerCounts = Arc<std::sync::Mutex<HashMap<IpAddr, usize>>>;

/// By default, russh does not have a limit on the number of possible connections for
/// `run_on_socket`, so we manage our own admission system, capped by a global and
/// per-IP maximum.
///
/// [`SshServer::start`] drives the acceptance by itself and asks the gate to admit
/// connections.
struct ConnectionGate {
    /// Global permits, one taken per live connection
    slots: Arc<Semaphore>,
    peers: PeerCounts,
}

impl ConnectionGate {
    fn new() -> Self {
        Self { slots: Arc::new(Semaphore::new(MAX_CONNECTIONS)), peers: Arc::default() }
    }

    /// Admits a connection from `ip`, returning a guard which must be held to denote
    /// the connection as active, or `None` if a limit has been reached.
    ///
    /// The guard must be dropped to indicate the connection has been lost.
    fn admit(&self, ip: IpAddr) -> Option<ConnectionGuard> {
        // Refuse rather than queue when at capacity
        let Ok(permit) = Arc::clone(&self.slots).try_acquire_owned() else {
            tracing::warn!("Connection cap ({MAX_CONNECTIONS}) reached, refusing {ip}");
            return None;
        };

        let mut peers = self.peers.lock().unwrap_or_else(|err| err.into_inner());
        let count = peers.entry(ip).or_insert(0);
        if *count >= MAX_CONNECTIONS_PER_IP {
            tracing::warn!("Per-IP cap reached for {ip}, refusing");
            return None;
        }
        *count += 1;

        Some(ConnectionGuard { ip, peers: Arc::clone(&self.peers), _permit: permit })
    }
}

/// Holds a connection's global slot and per-IP count, releasing both on drop
struct ConnectionGuard {
    ip: IpAddr,
    peers: PeerCounts,
    _permit: OwnedSemaphorePermit,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        let mut peers = self.peers.lock().unwrap_or_else(|err| err.into_inner());
        if let Some(count) = peers.get_mut(&self.ip) {
            *count -= 1;
            if *count == 0 {
                peers.remove(&self.ip);
            }
        }
    }
}
