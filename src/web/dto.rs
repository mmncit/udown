//! Data Transfer Objects for the HTTP boundary — the shapes clients send and
//! receive. Domain/service types are deliberately kept separate from these.

use serde::{Deserialize, Serialize};

/// Body of `POST /api/download`.
#[derive(Debug, Deserialize)]
pub struct DownloadRequest {
    pub url: String,
    /// One of `best`, `1080`, `720`, `audio`, `mp3`, or a raw yt-dlp format.
    pub quality: Option<String>,
}

/// Query string of `GET /api/info`.
#[derive(Debug, Deserialize)]
pub struct InfoQuery {
    pub url: String,
}

/// Body of `GET /api/health`.
#[derive(Debug, Serialize)]
pub struct HealthStatus {
    pub status: &'static str,
    pub yt_dlp_available: bool,
    pub ffmpeg_available: bool,
}
