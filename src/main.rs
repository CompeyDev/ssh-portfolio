use std::{net::SocketAddr, sync::{Arc, LazyLock, OnceLock}};

use color_eyre::{eyre::eyre, Result};
use russh::{keys::PrivateKey, server::{Config, Server}, MethodSet};
use ssh::SshServer;
use tokio::net::TcpListener;

mod action;
mod app;
mod cli;
mod components;
mod config;
mod errors;
mod logging;
mod ssh;
mod tui;
mod keycode;

const SOCKET_ADDR: LazyLock<SocketAddr> = LazyLock::new(|| SocketAddr::from(([127, 0, 0, 1], 2222)));
pub static SSH_CONFIG: OnceLock<Arc<Config>> = OnceLock::new();

#[tokio::main]
async fn main() -> Result<()> {
    crate::errors::init()?;
    crate::logging::init()?;

    SSH_CONFIG.get_or_init(|| {
        tracing::debug!("setting up ssh config");

        let mut conf = Config::default();
        conf.methods = MethodSet::NONE;
        conf.keys = vec![
            PrivateKey::from_openssh(include_bytes!("../rsa.pem")).unwrap(),
            PrivateKey::from_openssh(include_bytes!("../ed25519.pem")).unwrap()
        ];
        Arc::new(conf)
    });

    // let args = Cli::parse();
    // let mut app = App::new(args.tick_rate, args.frame_rate)?;
    // app.run().await?;
    
    tracing::info!("attemping to listen on {}", *SOCKET_ADDR);
    SshServer::default().run_on_socket(
        Arc::clone(SSH_CONFIG.get().unwrap()),
        &TcpListener::bind(*SOCKET_ADDR).await?,
    ).await.map_err(|err| eyre!(err))
}
