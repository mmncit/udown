//! udown — a tiny self-hosted YouTube downloader.
//!
//! Architecture (each layer depends only on the ones above it):
//!   - `cli`      : pure argument parsing
//!   - `config`   : runtime configuration from the environment
//!   - `error`    : the single `AppError` type + actix `ResponseError` impl
//!   - `domain`   : pure logic (quality → args, URL validation, naming) — no IO
//!   - `services` : side effects (yt-dlp / ffmpeg subprocesses, filesystem)
//!   - `web`      : HTTP DTOs, handlers, and route wiring
//!
//! `main` does nothing but assemble these: either start the server, or run one
//! download straight to disk.

mod cli;
mod config;
mod domain;
mod error;
mod services;
mod web;

use std::path::PathBuf;

use actix_web::{web as actix, App, HttpServer};

use cli::Command;
use config::AppConfig;
use domain::{media::Quality, validation};
use error::AppError;
use services::ytdlp::{self, Progress};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match cli::parse(&args) {
        Ok(Command::Serve) => serve().await,
        Ok(Command::Help) => {
            print!("{}", cli::USAGE);
            Ok(())
        }
        Ok(Command::Download {
            url,
            quality,
            out_dir,
        }) => {
            if let Err(error) = download(&url, &quality, out_dir).await {
                eprintln!("error: {error}");
                std::process::exit(1);
            }
            Ok(())
        }
        Err(message) => {
            eprintln!("error: {message}\n");
            eprint!("{}", cli::USAGE);
            std::process::exit(2);
        }
    }
}

/// Start the HTTP server and the web UI.
async fn serve() -> std::io::Result<()> {
    let config = AppConfig::from_env();

    println!("Starting udown server on http://{}", config.bind_address);
    println!("Endpoints:");
    println!("  GET  /api/health");
    println!("  GET  /api/info?url=<youtube_url>");
    println!(
        "  POST /api/download  {{\"url\": \"...\", \"quality\": \"best|1080|720|audio|mp3\"}}"
    );
    println!("\nOr download without the browser:  udown mp3 <url>");

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

/// Run one download to disk, reporting where the file landed.
async fn download(url: &str, quality: &str, out_dir: Option<PathBuf>) -> Result<(), AppError> {
    validation::validate_youtube_url(url)?;

    let quality = Quality::parse(Some(quality));
    check_tools(&quality).await?;

    let dest = out_dir.unwrap_or_else(|| PathBuf::from("."));
    let path = ytdlp::download_to_dir(url, &quality, &dest, Progress::Inherit).await?;

    println!("Saved {}", path.display());
    Ok(())
}

/// Fail early, with an actionable message, when a required tool is missing —
/// rather than letting yt-dlp fail halfway through with its own wording.
async fn check_tools(quality: &Quality) -> Result<(), AppError> {
    if !ytdlp::yt_dlp_available().await {
        return Err(AppError::ToolUnavailable(
            "yt-dlp is not on your PATH. Install it with `brew install yt-dlp` (or `make deps`)."
                .into(),
        ));
    }

    if quality.needs_ffmpeg() && !ytdlp::ffmpeg_available().await {
        return Err(AppError::ToolUnavailable(
            "This format needs ffmpeg, which is not on your PATH. Install it with `brew install ffmpeg`."
                .into(),
        ));
    }

    Ok(())
}
