use std::io::stderr;

use color_eyre::eyre::eyre;
use color_eyre::Result;
use tracing_error::ErrorLayer;
use tracing_subscriber::prelude::*;
use tracing_subscriber::util::TryInitError;
use tracing_subscriber::{fmt, EnvFilter};

use crate::config;

lazy_static::lazy_static! {
    pub static ref LOG_ENV: String = format!("{}_LOG", config::PROJECT_NAME.clone());
    pub static ref LOG_FILE: String = format!("{}.log", env!("CARGO_PKG_NAME"));
}

pub fn init() -> Result<()> {
    //
    // File initialization
    //

    let directory = config::get_data_dir();
    std::fs::create_dir_all(directory.clone())?;

    let log_path = directory.join(LOG_FILE.clone());
    let log_file = std::fs::File::create(log_path)?;

    //
    // Filtering
    //

    // Stage 1: Construct base filter
    let env_filter = EnvFilter::builder().with_default_directive(
        if cfg!(debug_assertions) {
            tracing::Level::DEBUG.into()
        } else {
            tracing::Level::INFO.into()
        },
    );

    // Stage 2: Attempt to read from {RUST|CRATE_NAME}_LOG env var or ignore
    let env_filter = env_filter
        .try_from_env()
        .unwrap_or_else(|_| {
            env_filter.with_env_var(LOG_ENV.to_string()).from_env_lossy()
        })
        .add_directive("russh::cipher=info".parse().unwrap())
        .add_directive("tui_markdown=info".parse().unwrap());

    // Stage 3: Enable directives to reduce verbosity for release mode builds
    #[cfg(not(debug_assertions))]
    let env_filter = env_filter
        .add_directive("tokio_util=info".parse().unwrap())
        .add_directive("futures=info".parse().unwrap())
        .add_directive("russh=info".parse().unwrap())
        .add_directive("crossterm=info".parse().unwrap())
        .add_directive("ratatui=info".parse().unwrap());

    //
    // Subscription
    //

    // Build the subscriber and apply it
    tracing_subscriber::registry()
        .with(env_filter)
        .with(ErrorLayer::default())
        .with(
            // Logging to file
            fmt::layer()
                .with_file(true)
                .with_line_number(true)
                .with_writer(log_file)
                .with_target(false)
                .with_ansi(false),
        )
        .with({
            // Logging to stderr
            let layer = fmt::layer()
                .with_writer(stderr)
                .with_timer(tracing_subscriber::fmt::time())
                .with_thread_ids(true)
                .with_ansi(true);

            // Enable compact mode for release logs
            #[cfg(not(debug_assertions))]
            let layer = layer
                .compact()
                .without_time()
                .with_span_events(
                    tracing_subscriber::fmt::format::FmtSpan::NONE,
                )
                .with_target(false)
                .with_thread_ids(false);
            layer
        })
        .try_init()
        .map_err(|err: TryInitError| eyre!(err))
}
