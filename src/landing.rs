use std::io;
use std::net::SocketAddr;

use axum::extract::Path;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use rust_embed::Embed;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::instrument;

#[derive(Embed)]
#[folder = "www/build"]
pub struct WebLandingServer;

impl WebLandingServer {
    #[instrument(level = "trace")]
    pub async fn start(addr: SocketAddr) -> io::Result<()> {
        let app = Router::new()
            .route("/", get(handle_index))
            .route("/{*path}", get(handle_static_file))
            .layer(TraceLayer::new_for_http());

        let listener = TcpListener::bind(addr).await?;
        tracing::info!("Web server listening on {}", addr);

        axum::serve(listener, app).await?;
        Ok(())
    }
}

async fn handle_index() -> impl IntoResponse {
    handle_static_file(Path("index.html".into())).await
}

async fn handle_static_file(Path(path): Path<String>) -> impl IntoResponse {
    WebLandingServer::get(&path)
        .map(|file| {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.to_string())], file.data)
        })
        .ok_or((StatusCode::NOT_FOUND, "404 Not Found"))
}
