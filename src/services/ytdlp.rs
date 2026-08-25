//! Thin async wrappers around the `yt-dlp` and `ffmpeg` command-line tools.
//!
//! This is the only module that spawns subprocesses or touches the filesystem.
//! Pure transformations (JSON → DTO, quality → args, output naming) are
//! delegated to the `domain` layer and to the small private helpers at the
//! bottom of the file.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::Serialize;
use tokio::process::Command;
use uuid::Uuid;

use crate::domain::media::{content_type_for, Quality};
use crate::domain::naming;
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

/// A completed download still sitting on disk.
pub struct DownloadedFile {
    /// Where the file currently is — `<dir>/<uuid>-<title>.<ext>`.
    pub path: PathBuf,
    /// The name a user should see, with the internal id prefix removed.
    pub filename: String,
    pub content_type: &'static str,
}

/// Whether yt-dlp's own output should be shown to the user.
///
/// The web server captures it (so failures can be reported as JSON); the CLI
/// inherits the terminal so the progress bar is visible during a long download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    Quiet,
    Inherit,
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

/// Download `url` at the requested `quality` into `download_dir`, leaving the
/// file on disk. The caller decides what happens to it next.
pub async fn download_to_disk(
    url: &str,
    quality: &Quality,
    download_dir: &Path,
    progress: Progress,
) -> Result<DownloadedFile, AppError> {
    tokio::fs::create_dir_all(download_dir)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create download dir: {e}")))?;

    let file_id = Uuid::new_v4().to_string();
    let output_template = download_dir
        .join(naming::output_template(&file_id))
        .to_string_lossy()
        .into_owned();

    run_ytdlp(
        build_download_args(url, quality, &output_template),
        progress,
    )
    .await?;

    let path = find_downloaded_file(download_dir, &file_id, quality.output_extension())
        .await
        .ok_or(AppError::FileNotFound)?;

    let stored_name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    let filename = stored_name
        .map(|name| naming::display_filename(&name, &file_id))
        .unwrap_or_else(|| format!("{file_id}.mp4"));
    let content_type = content_type_for(path.extension().and_then(|e| e.to_str()));

    Ok(DownloadedFile {
        path,
        filename,
        content_type,
    })
}

/// Download `url` and return the file's bytes, removing the temp file on disk.
/// Used by the HTTP layer, which streams the bytes straight to the client.
pub async fn download(
    url: &str,
    quality: &Quality,
    download_dir: &Path,
) -> Result<DownloadedMedia, AppError> {
    let file = download_to_disk(url, quality, download_dir, Progress::Quiet).await?;

    let bytes = tokio::fs::read(&file.path)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read downloaded file: {e}")))?;

    // Best-effort cleanup; a failure here shouldn't fail the request.
    let _ = tokio::fs::remove_file(&file.path).await;

    Ok(DownloadedMedia {
        filename: file.filename,
        content_type: file.content_type,
        bytes,
    })
}

/// Download `url` and leave it in `dest_dir` under its human-readable name,
/// without overwriting an existing file. Used by the CLI.
///
/// Returns the final path.
pub async fn download_to_dir(
    url: &str,
    quality: &Quality,
    dest_dir: &Path,
    progress: Progress,
) -> Result<PathBuf, AppError> {
    let file = download_to_disk(url, quality, dest_dir, progress).await?;
    let final_path = available_path(dest_dir, &file.filename).await;

    if final_path == file.path {
        return Ok(final_path);
    }

    tokio::fs::rename(&file.path, &final_path)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to name downloaded file: {e}")))?;

    Ok(final_path)
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

/// Run yt-dlp with `args`, mapping a non-zero exit to an [`AppError`].
///
/// In [`Progress::Inherit`] mode yt-dlp writes straight to the terminal, so
/// there is no captured stderr to quote — the user has already seen it.
async fn run_ytdlp(args: Vec<String>, progress: Progress) -> Result<(), AppError> {
    match progress {
        Progress::Quiet => {
            let output = Command::new("yt-dlp")
                .args(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
                .map_err(spawn_error)?;

            if !output.status.success() {
                return Err(AppError::YtDlp(last_error_line(&output.stderr)));
            }
        }
        Progress::Inherit => {
            let status = Command::new("yt-dlp")
                .args(args)
                .status()
                .await
                .map_err(spawn_error)?;

            if !status.success() {
                return Err(AppError::YtDlp(
                    "yt-dlp exited with an error (see its output above)".into(),
                ));
            }
        }
    }

    Ok(())
}

/// Assemble the full yt-dlp argument vector for a download (pure).
fn build_download_args(url: &str, quality: &Quality, output_template: &str) -> Vec<String> {
    let mut args = quality.ytdlp_args();
    args.extend(
        [
            "-o",
            output_template,
            "--no-playlist",
            // Keep the title portion of the filename ASCII and free of spaces,
            // "&" and quotes, so it is safe in a Content-Disposition header.
            "--restrict-filenames",
            url,
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    args
}

/// Locate the file yt-dlp produced; its extension isn't known in advance, so we
/// match on the unique id prefix we supplied in the output template.
///
/// More than one file can match — yt-dlp may leave the source it re-encoded
/// from beside the result — so the choice is delegated to
/// [`naming::pick_output`] rather than taking whatever `read_dir` yields first.
async fn find_downloaded_file(
    dir: &Path,
    prefix: &str,
    expected_ext: Option<&str>,
) -> Option<PathBuf> {
    let mut entries = tokio::fs::read_dir(dir).await.ok()?;
    let mut matches: Vec<String> = Vec::new();

    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(prefix) {
            matches.push(name);
        }
    }

    let borrowed: Vec<&str> = matches.iter().map(String::as_str).collect();
    naming::pick_output(&borrowed, expected_ext).map(|name| dir.join(name))
}

/// The first path in `dir` named `filename` that does not already exist,
/// falling back to `Song (2).mp3`, `Song (3).mp3`, … on collision.
async fn available_path(dir: &Path, filename: &str) -> PathBuf {
    let first = dir.join(filename);
    if !first.exists() {
        return first;
    }

    for n in 2..=999 {
        let candidate = dir.join(naming::numbered_name(filename, n));
        if !candidate.exists() {
            return candidate;
        }
    }

    first
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

#[cfg(test)]
mod tests {
    use super::*;

    fn args_for(quality: Quality) -> Vec<String> {
        build_download_args(
            "https://youtu.be/abc",
            &quality,
            "/tmp/id-%(title)s.%(ext)s",
        )
    }

    #[test]
    fn download_args_restrict_the_filename_character_set() {
        let args = args_for(Quality::Mp3);
        assert!(args.contains(&"--restrict-filenames".to_string()));
    }

    #[test]
    fn download_args_never_use_trim_filenames() {
        // Regression pin: --trim-filenames caps the whole rendered output path,
        // directories included, so with an absolute -o it writes the file
        // outside the requested directory. Title length is bounded by the
        // %(title).100s precision in domain::naming::output_template instead.
        for quality in [Quality::Mp3, Quality::Best, Quality::AudioM4a] {
            assert!(
                !args_for(quality).contains(&"--trim-filenames".to_string()),
                "--trim-filenames truncates the directory part of the path"
            );
        }
    }

    #[test]
    fn download_args_end_with_the_url() {
        let args = args_for(Quality::Best);
        assert_eq!(args.last().unwrap(), "https://youtu.be/abc");
    }

    #[test]
    fn download_args_pass_the_output_template_through() {
        let args = args_for(Quality::Mp3);
        assert!(args
            .windows(2)
            .any(|w| w == ["-o", "/tmp/id-%(title)s.%(ext)s"]));
    }
}
