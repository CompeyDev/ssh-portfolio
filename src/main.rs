use std::{net::SocketAddr, sync::Arc};

use clap::Parser as _;
use cli::Cli;
use color_eyre::{eyre::eyre, Result};
use lazy_static::lazy_static;
use russh::{
    keys::PrivateKey,
    server::{Config, Server},
    MethodSet,
};
use ssh::SshServer;
use tokio::net::TcpListener;

mod action;
mod app;
mod cli;
mod components;
mod config;
mod errors;
mod keycode;
mod logging;
mod ssh;
mod tui;

const SSH_KEYS: &[&[u8]] = &[
    include_bytes!("../rsa.pem"),
    include_bytes!("../ed25519.pem"),
];
lazy_static! {
    pub(crate) static ref OPTIONS: Cli = Cli::parse();
    pub(crate) static ref SOCKET_ADDR: SocketAddr = SocketAddr::from(([127, 0, 0, 1], 2222));
}

#[tokio::main]
async fn main() -> Result<()> {
    crate::errors::init()?;
    crate::logging::init()?;

    let config = ssh_config();
    tracing::info!("Attempting to listen on {}", *SOCKET_ADDR);
    SshServer::default()
        .run_on_socket(Arc::new(config), &TcpListener::bind(*SOCKET_ADDR).await?)
        .await
        .map_err(|err| eyre!(err))
}

fn ssh_config() -> Config {
    let mut conf = Config::default();
    conf.methods = MethodSet::NONE;
    conf.keys = SSH_KEYS
        .to_vec()
        .iter()
        .filter_map(|pem| PrivateKey::from_openssh(pem).ok())
        .collect();
    
    tracing::trace!("SSH config: {:#?}", conf);
    conf
}
