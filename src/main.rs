use std::net::SocketAddr;

use clap::Parser as _;
use cli::Cli;
use color_eyre::eyre::eyre;
use color_eyre::Result;
use lazy_static::lazy_static;
use russh::server::Config as SshConfig;
use russh::MethodSet;
use ssh::SshServer;

#[cfg(feature = "blog")]
pub(crate) use atproto::com;
#[cfg(feature = "blog")]
pub(crate) use atrium_api::*;
use tracing::instrument;

use crate::config::Config;
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

#[rustfmt::skip]
lazy_static! {
    pub(crate) static ref CONFIG: Config = Config::new().expect("Config loading error, see above");
    pub(crate) static ref OPTIONS: Cli = Cli::parse();
    pub(crate) static ref SSH_SOCKET_ADDR: Option<SocketAddr> = Some(SocketAddr::from((host_ip().ok()?, OPTIONS.ssh_port)));
    pub(crate) static ref WEB_SERVER_ADDR: Option<SocketAddr> = Some(SocketAddr::from((host_ip().ok()?, OPTIONS.web_port)));
}

#[tokio::main]
async fn main() -> Result<()> {
    crate::errors::init()?;
    crate::logging::init()?;
    let _ = *OPTIONS; // force clap to run by evaluating it

    let ssh_socket_addr = SSH_SOCKET_ADDR.ok_or(eyre!("Invalid host IP provided"))?;
    let web_server_addr = WEB_SERVER_ADDR.ok_or(eyre!("Invalid host IP provided"))?;

    let ssh_config = tokio::task::block_in_place(ssh_config);
    tokio::select! {
        ssh_res = SshServer::start(ssh_socket_addr, ssh_config) => ssh_res,
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
                octet_str
                    .parse::<u8>()
                    .map_err(|_| eyre!("Octet component out of range (expected u8)"))
            })
            .collect::<Result<Vec<u8>>>()?,
    )
    .map_err(|_| eyre!("Invalid host IP provided"))
}

#[instrument]
fn ssh_config() -> SshConfig {
    let conf = SshConfig {
        methods: MethodSet::NONE,
        keys: CONFIG.private_keys.clone(),
        ..Default::default()
    };

    tracing::info!("SSH will use {} host keys", conf.keys.len());
    tracing::trace!("SSH config: {:?}", conf);
    conf
}
