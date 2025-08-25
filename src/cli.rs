use clap::Parser;
use indoc::formatdoc;

use crate::config::{get_config_dir, get_data_dir};

#[derive(Parser, Debug)]
#[command(author, version = version(), about)]
pub struct Cli {
    /// Tick rate, i.e. number of ticks per second
    #[arg(short, long, value_name = "FLOAT", default_value_t = 4.0)]
    pub tick_rate: f64,

    /// Frame rate, i.e. number of frames per second
    #[arg(short, long, value_name = "FLOAT", default_value_t = 60.0)]
    pub frame_rate: f64,

    /// The host address to start the SSH server on
    #[arg(short = 'H', long, value_name = "ADDRESS", default_value_t = String::from("127.0.0.1"))]
    pub host: String,
    /// The port to start the SSH server on
    #[arg(short = 'P', long, value_name = "PORT", default_value_t = 22)]
    pub ssh_port: u16,
    /// The port to start the web server on
    #[arg(short = 'p', long, value_name = "PORT", default_value_t = 80)]
    pub web_port: u16,
}

pub fn version() -> String {
    let author = clap::crate_authors!();
    let version_message = format!(
        "v{}-{} ({}, {})",
        env!("CARGO_PKG_VERSION"),
        &env!("VERGEN_GIT_SHA")[..7],
        env!("VERGEN_GIT_BRANCH"),
        env!("VERGEN_BUILD_DATE")
    );

    let config_dir_path = get_config_dir().display().to_string();
    let data_dir_path = get_data_dir().display().to_string();

    formatdoc! {"
        {version_message}

        Authors: {author}

        Config directory: {config_dir_path}
        Data directory: {data_dir_path}
    "}
}
