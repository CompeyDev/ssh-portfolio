#![feature(optimize_attribute)]

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
#[instrument(name = "main")]
async fn main() -> Result<()> {
    crate::errors::init()?;
    crate::logging::init()?;
    let _ = *OPTIONS; // force clap to run by evaluating it

    eprintln!(" {} v{}\n", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

    let ssh_socket_addr = SSH_SOCKET_ADDR.ok_or(eyre!("Invalid host IP provided"))?;
    let web_server_addr = WEB_SERVER_ADDR.ok_or(eyre!("Invalid host IP provided"))?;

    tokio::try_join!(
        supervised("ssh", || SshServer::start(ssh_socket_addr, ssh_config())),
        supervised("web", || async move {
            WebLandingServer::start(web_server_addr).await.map_err(|err| eyre!(err))
        }),
    )?;

    Ok(())
}

/// Runs a server's `Future`, restarting the task if it panics. A panic in
/// one server's task does not affect the other.
///
/// An error is only returned in case the error is unrecoverable.
async fn supervised<Fut>(name: &str, mut server: impl FnMut() -> Fut) -> Result<()>
where
    Fut: std::future::Future<Output = Result<()>> + Send + 'static, {
    loop {
        match tokio::spawn(server()).await {
            Ok(result) => break result,
            Err(err) if err.is_panic() => {
                tracing::error!("The {name} server panicked; restarting it");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            Err(err) => break Err(eyre!(err)),
        }
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

#[instrument(name = "config")]
fn ssh_config() -> SshConfig {
    let conf = SshConfig {
        methods: MethodSet::empty(),
        keys: CONFIG.private_keys.clone(),
        ..Default::default()
    };

    tracing::debug!("SSH will use {} host keys", conf.keys.len());
    tracing::trace!("SSH config: {:?}", conf);
    conf
}
