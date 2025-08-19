use std::io;
use std::net::SocketAddr;

use actix_web::middleware::Logger;
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use rust_embed::Embed;
use tracing::instrument;

macro_rules! embedded_route {
    ($path:expr) => {
        match WebLandingServer::get($path) {
            Some(content) => HttpResponse::Ok()
                .content_type(mime_guess::from_path($path).first_or_octet_stream().as_ref())
                .body(content.data.into_owned()),
            None => HttpResponse::NotFound().body("404 Not Found"),
        }
    };

    ($name:ident,$pat:expr,$path:literal) => {
        #[actix_web::get($pat)]
        async fn $name() -> impl Responder {
            embedded_route!($path)
        }
    };

    ($name:ident,$pat:expr) => {
        #[actix_web::get($pat)]
        async fn $name(path: web::Path<String>) -> impl Responder {
            embedded_route!(path.as_str())
        }
    };
}

#[derive(Embed)]
#[folder = "www/build"]
pub struct WebLandingServer;

impl WebLandingServer {
    #[instrument(level = "trace")]
    pub async fn start(addr: SocketAddr) -> io::Result<()> {
        tracing::info!("Web server listening on {}", addr);
        HttpServer::new(|| {
            // TODO: register a default service for a nicer 404 page
            App::new().service(index).service(favicon).service(dist).wrap(Logger::default())
        })
        .bind(addr)?
        .run()
        .await
    }
}

embedded_route!(index, "/", "index.html");
embedded_route!(favicon, "/favicon.png", "favicon.png");
embedded_route!(dist, "/{path:.*}");
