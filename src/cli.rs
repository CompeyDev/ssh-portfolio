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
    #[arg(short = 'P', long, value_name = "PORT", default_value_t = 2222)]
    pub ssh_port: u16,
    /// The port to start the web server on
    #[arg(short = 'p', long, value_name = "PORT", default_value_t = 80)]
    pub web_port: u16,
}

const VERSION_MESSAGE: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "-",
    env!("VERGEN_GIT_DESCRIBE"),
    " (",
    env!("VERGEN_BUILD_DATE"),
    ")"
);

pub fn version() -> String {
    let author = clap::crate_authors!();

    // let current_exe_path = PathBuf::from(clap::crate_name!()).display().to_string();
    let config_dir_path = get_config_dir().display().to_string();
    let data_dir_path = get_data_dir().display().to_string();

    formatdoc! {"
        {VERSION_MESSAGE}

        Authors: {author}

        Config directory: {config_dir_path}
        Data directory: {data_dir_path}
    "}
}
