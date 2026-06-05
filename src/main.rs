//! udown — a tiny self-hosted YouTube downloader.
//!
//! Architecture (each layer depends only on the ones above it):
//!   - `config`   : runtime configuration from the environment
//!   - `error`    : the single `AppError` type + actix `ResponseError` impl
//!   - `domain`   : pure logic (quality → args, URL validation) — no IO
//!   - `services` : side effects (yt-dlp / ffmpeg subprocesses, filesystem)
//!   - `web`      : HTTP DTOs, handlers, and route wiring
//!
//! `main` does nothing but assemble these and start the server.

mod config;
mod domain;
mod error;
mod services;
mod web;

use actix_web::{web as actix, App, HttpServer};

use config::AppConfig;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let config = AppConfig::from_env();

    println!("Starting udown server on http://{}", config.bind_address);
    println!("Endpoints:");
    println!("  GET  /api/health");
    println!("  GET  /api/info?url=<youtube_url>");
    println!(
        "  POST /api/download  {{\"url\": \"...\", \"quality\": \"best|1080|720|audio|mp3\"}}"
    );

    let bind_address = config.bind_address.clone();
    let shared_config = actix::Data::new(config);

    HttpServer::new(move || {
        App::new()
            .app_data(shared_config.clone())
            .configure(web::routes::configure)
    })
    .bind(bind_address)?
    .run()
    .await
}
