use std::io;
use std::net::SocketAddr;

use axum::extract::Path;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use rust_embed::Embed;
use tokio::net::TcpListener;
use tracing::instrument;

#[derive(Embed)]
#[folder = "www/build"]
pub struct WebLandingServer;

impl WebLandingServer {
    #[instrument(name = "web")]
    pub async fn start(addr: SocketAddr) -> io::Result<()> {
        let app = Router::new()
            .route("/", get(handle_index))
            .route("/{*path}", get(handle_static_file))
            .layer({
                let layer = tower_http::trace::TraceLayer::new_for_http();
                #[cfg(not(debug_assertions))]
                let layer = layer
                    .make_span_with(move |req: &axum::extract::Request<_>| {
                        let method = req.method().clone();
                        let path = req.uri().path().to_owned();

                        tracing::info_span!("web", method = %method, path = %path)
                    })
                    .on_request(())
                    .on_response(
                        |res: &axum::response::Response<_>,
                         latency: std::time::Duration,
                         _span: &Span| {
                            let status = res.status();
                            tracing::info!(
                                status = %status,
                                latency = ?latency,
                            );
                        },
                    );

                layer
            });

        let listener = TcpListener::bind(addr).await?;
        tracing::info!("Web server listening!");

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
