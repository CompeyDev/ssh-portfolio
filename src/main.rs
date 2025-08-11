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

#[cfg(feature = "blog")]
pub(crate) use atproto::com;
#[cfg(feature = "blog")]
pub(crate) use atrium_api::*;

mod action;
mod app;
#[cfg(feature = "blog")]
pub(crate) mod atproto;
mod cli;
mod components;
mod config;
mod errors;
mod keycode;
mod logging;
mod ssh;
mod tui;

const SSH_KEYS: &[&[u8]] = &[
    include_bytes!(concat!(env!("OUT_DIR"), "/rsa.pem")),
    include_bytes!(concat!(env!("OUT_DIR"), "/ecdsa.pem")),
    include_bytes!(concat!(env!("OUT_DIR"), "/ed25519.pem")),
];
lazy_static! {
    pub(crate) static ref OPTIONS: Cli = Cli::parse();
    pub(crate) static ref SOCKET_ADDR: Option<SocketAddr> = Some(SocketAddr::from((
        // Convert the hostname IP to a fixed size array of [u8; 4]
        TryInto::<[u8; 4]>::try_into(
            OPTIONS
                .host
                .splitn(4, ".")
                .map(|octet_str| u8::from_str_radix(octet_str, 10)
                    .map_err(|_| eyre!("Octet component out of range (expected u8)")))
                .collect::<Result<Vec<u8>>>()
                .ok()?
        )
        .ok()?,

        // The port to listen on
        OPTIONS.port
    )));
}

#[tokio::main]
async fn main() -> Result<()> {
    crate::errors::init()?;
    crate::logging::init()?;
    let _ = *OPTIONS; // force clap to run by evaluating it

    let socket_addr = SOCKET_ADDR.ok_or(eyre!("Invalid host IP provided"))?;
    let config = ssh_config();

    tracing::info!("Attempting to listen on {}", socket_addr);
    SshServer::default()
        .run_on_socket(Arc::new(config), &TcpListener::bind(socket_addr).await?)
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
