use std::net::SocketAddr;

use clap::Parser as _;
use cli::Cli;
use color_eyre::{eyre::eyre, Result};
use lazy_static::lazy_static;
use russh::{keys::PrivateKey, server::Config, MethodSet};
use ssh::SshServer;

#[cfg(feature = "blog")]
pub(crate) use atproto::com;
#[cfg(feature = "blog")]
pub(crate) use atrium_api::*;

use crate::landing::WebLandingServer;

mod action;
mod app;
#[cfg(feature = "blog")]
pub(crate) mod atproto;
mod cli;
mod components;
mod config;
mod errors;
mod keycode;
mod landing;
mod logging;
mod ssh;
mod tui;

const SSH_KEYS: &[&[u8]] = &[
    include_bytes!(concat!(env!("OUT_DIR"), "/rsa.pem")),
    include_bytes!(concat!(env!("OUT_DIR"), "/ecdsa.pem")),
    include_bytes!(concat!(env!("OUT_DIR"), "/ed25519.pem")),
];

#[rustfmt::skip]
lazy_static! {
    pub(crate) static ref OPTIONS: Cli = Cli::parse();
    pub(crate) static ref SSH_SOCKET_ADDR: Option<SocketAddr> = SocketAddr::try_from((host_ip().ok()?, OPTIONS.ssh_port)).ok();
    pub(crate) static ref WEB_SERVER_ADDR: Option<SocketAddr> = SocketAddr::try_from((host_ip().ok()?, OPTIONS.web_port)).ok();
}

#[tokio::main]
async fn main() -> Result<()> {
    crate::errors::init()?;
    crate::logging::init()?;
    let _ = *OPTIONS; // force clap to run by evaluating it

    let ssh_socket_addr = SSH_SOCKET_ADDR.ok_or(eyre!("Invalid host IP provided"))?;
    let web_server_addr = WEB_SERVER_ADDR.ok_or(eyre!("Invalid host IP provided"))?;

    tokio::select! {
        ssh_res = SshServer::start(ssh_socket_addr, ssh_config()) => ssh_res,
        web_res = WebLandingServer::start(web_server_addr) => web_res.map_err(|err| eyre!(err)),
    }
}

/// Converts the supplied hostname IP via CLI to a fixed size array of `[u8; 4]`, defaults to `127.0.0.1`
pub fn host_ip() -> Result<[u8; 4]> {
    TryInto::<[u8; 4]>::try_into(
        OPTIONS
            .host
            .splitn(4, ".")
            .map(|octet_str| {
                u8::from_str_radix(octet_str, 10)
                    .map_err(|_| eyre!("Octet component out of range (expected u8)"))
            })
            .collect::<Result<Vec<u8>>>()?,
    )
    .map_err(|_| eyre!("Invalid host IP provided"))
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
