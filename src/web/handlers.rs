//! Request handlers. These stay thin: validate input, call a service, shape the
//! response. All error handling is delegated to [`AppError`]'s `ResponseError`
//! impl via the `?` operator.

use actix_web::{web, HttpResponse};

use crate::config::AppConfig;
use crate::domain::{media::Quality, validation};
use crate::error::AppError;
use crate::services::ytdlp;
use crate::web::dto::{DownloadRequest, HealthStatus, InfoQuery};

/// `GET /api/health` — reports whether the external tools are available.
pub async fn health() -> HttpResponse {
    // Probe both tools concurrently.
    let (yt_dlp_available, ffmpeg_available) =
        tokio::join!(ytdlp::yt_dlp_available(), ytdlp::ffmpeg_available());

    HttpResponse::Ok().json(HealthStatus {
        status: "ok",
        yt_dlp_available,
        ffmpeg_available,
    })
}

/// `GET /api/info?url=<youtube_url>` — returns video metadata.
pub async fn get_info(query: web::Query<InfoQuery>) -> Result<HttpResponse, AppError> {
    validation::validate_youtube_url(&query.url)?;
    let info = ytdlp::fetch_info(&query.url).await?;
    Ok(HttpResponse::Ok().json(info))
}

/// `POST /api/download` — downloads the video and streams the file back.
pub async fn download(
    config: web::Data<AppConfig>,
    body: web::Json<DownloadRequest>,
) -> Result<HttpResponse, AppError> {
    validation::validate_youtube_url(&body.url)?;

    let quality = Quality::parse(body.quality.as_deref());
    let media = ytdlp::download(&body.url, &quality, &config.download_dir).await?;

    Ok(HttpResponse::Ok()
        .content_type(media.content_type)
        .insert_header((
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", media.filename),
        ))
        .body(media.bytes))
}
