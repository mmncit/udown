//! Thin async wrappers around the `yt-dlp` and `ffmpeg` command-line tools.
//!
//! This is the only module that spawns subprocesses or touches the filesystem.
//! Pure transformations (JSON → DTO, quality → args) are delegated to the
//! `domain` layer and to the small private helpers at the bottom of the file.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::Serialize;
use tokio::process::Command;
use uuid::Uuid;

use crate::domain::media::{content_type_for, Quality};
use crate::error::AppError;

/// Metadata about a video, as returned by `GET /api/info`.
#[derive(Debug, Serialize)]
pub struct VideoInfo {
    pub title: String,
    pub duration: Option<String>,
    pub formats: Vec<FormatInfo>,
}

/// A single available format for a video.
#[derive(Debug, Serialize)]
pub struct FormatInfo {
    pub format_id: String,
    pub ext: String,
    pub resolution: Option<String>,
    pub filesize: Option<u64>,
}

/// The bytes and metadata of a completed download, ready to stream to a client.
pub struct DownloadedMedia {
    pub filename: String,
    pub content_type: &'static str,
    pub bytes: Vec<u8>,
}

/// Whether `yt-dlp` is installed and runnable.
pub async fn yt_dlp_available() -> bool {
    command_succeeds("yt-dlp", "--version").await
}

/// Whether `ffmpeg` is installed and runnable (needed for merging and MP3).
pub async fn ffmpeg_available() -> bool {
    command_succeeds("ffmpeg", "-version").await
}

/// Fetch a video's metadata without downloading it.
pub async fn fetch_info(url: &str) -> Result<VideoInfo, AppError> {
    let output = Command::new("yt-dlp")
        .args(["--dump-json", "--no-download", "--no-playlist", url])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_error)?;

    if !output.status.success() {
        return Err(AppError::YtDlp(last_error_line(&output.stderr)));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| AppError::Internal("Failed to parse video info".into()))?;

    Ok(parse_video_info(&json))
}

/// Download `url` at the requested `quality` into `download_dir`, returning the
/// file's bytes. The on-disk temp file is removed before returning.
pub async fn download(
    url: &str,
    quality: &Quality,
    download_dir: &Path,
) -> Result<DownloadedMedia, AppError> {
    tokio::fs::create_dir_all(download_dir)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create download dir: {e}")))?;

    let file_id = Uuid::new_v4().to_string();
    let output_template = download_dir
        .join(format!("{file_id}.%(ext)s"))
        .to_string_lossy()
        .into_owned();

    let output = Command::new("yt-dlp")
        .args(build_download_args(url, quality, &output_template))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_error)?;

    if !output.status.success() {
        return Err(AppError::YtDlp(last_error_line(&output.stderr)));
    }

    let path = find_downloaded_file(download_dir, &file_id)
        .await
        .ok_or(AppError::FileNotFound)?;

    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read downloaded file: {e}")))?;

    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("{file_id}.mp4"));
    let content_type = content_type_for(path.extension().and_then(|e| e.to_str()));

    // Best-effort cleanup; a failure here shouldn't fail the request.
    let _ = tokio::fs::remove_file(&path).await;

    Ok(DownloadedMedia {
        filename,
        content_type,
        bytes,
    })
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Run `program <flag>` and report whether it exited successfully.
async fn command_succeeds(program: &str, flag: &str) -> bool {
    Command::new(program)
        .arg(flag)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Assemble the full yt-dlp argument vector for a download (pure).
fn build_download_args(url: &str, quality: &Quality, output_template: &str) -> Vec<String> {
    let mut args = quality.ytdlp_args();
    args.extend([
        "-o".to_string(),
        output_template.to_string(),
        "--no-playlist".to_string(),
        url.to_string(),
    ]);
    args
}

/// Locate the file yt-dlp produced; its extension isn't known in advance, so we
/// match on the unique id prefix we supplied in the output template.
async fn find_downloaded_file(dir: &Path, prefix: &str) -> Option<PathBuf> {
    let mut entries = tokio::fs::read_dir(dir).await.ok()?;
    while let Ok(Some(entry)) = entries.next_entry().await {
        if entry.file_name().to_string_lossy().starts_with(prefix) {
            return Some(entry.path());
        }
    }
    None
}

/// Map yt-dlp's `--dump-json` output onto our [`VideoInfo`] DTO (pure).
fn parse_video_info(json: &serde_json::Value) -> VideoInfo {
    let formats = json["formats"]
        .as_array()
        .map(|arr| arr.iter().filter_map(parse_format).collect())
        .unwrap_or_default();

    VideoInfo {
        title: json["title"].as_str().unwrap_or("Unknown").to_string(),
        duration: json["duration_string"].as_str().map(str::to_string),
        formats,
    }
}

/// Map a single format entry; `None` if it lacks a `format_id`.
fn parse_format(f: &serde_json::Value) -> Option<FormatInfo> {
    Some(FormatInfo {
        format_id: f["format_id"].as_str()?.to_string(),
        ext: f["ext"].as_str().unwrap_or("mp4").to_string(),
        resolution: f["resolution"].as_str().map(str::to_string),
        filesize: f["filesize"].as_u64(),
    })
}

/// The last (most relevant) line of yt-dlp's stderr, for error messages.
fn last_error_line(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr)
        .lines()
        .last()
        .unwrap_or("Unknown error")
        .to_string()
}

/// Turn a failure to spawn yt-dlp into a friendly error.
fn spawn_error(e: std::io::Error) -> AppError {
    AppError::ToolUnavailable(format!("Failed to run yt-dlp: {e}. Is it installed?"))
}
