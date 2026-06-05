use actix_web::{web, App, HttpServer, HttpResponse};
use actix_files::Files;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use uuid::Uuid;
use url::Url;

#[derive(Deserialize)]
struct DownloadRequest {
    url: String,
    /// Optional: "best", "audio", "720", "1080", etc.
    quality: Option<String>,
}

#[derive(Serialize)]
struct VideoInfo {
    title: String,
    duration: Option<String>,
    formats: Vec<FormatInfo>,
}

#[derive(Serialize)]
struct FormatInfo {
    format_id: String,
    ext: String,
    resolution: Option<String>,
    filesize: Option<u64>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

const DOWNLOAD_DIR: &str = "./downloads";

/// Validate that the URL is a legitimate YouTube URL.
fn validate_youtube_url(raw_url: &str) -> Result<(), String> {
    let parsed = Url::parse(raw_url).map_err(|_| "Invalid URL format".to_string())?;

    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err("URL must use http or https".to_string()),
    }

    let host = parsed.host_str().ok_or("Missing host")?;
    let allowed_hosts = [
        "youtube.com",
        "www.youtube.com",
        "m.youtube.com",
        "youtu.be",
        "www.youtu.be",
    ];

    if !allowed_hosts.iter().any(|h| host.eq_ignore_ascii_case(h)) {
        return Err("Only YouTube URLs are supported".to_string());
    }

    Ok(())
}

/// GET /api/info?url=<youtube_url>
/// Returns video metadata (title, duration, available formats).
async fn get_info(query: web::Query<DownloadRequest>) -> HttpResponse {
    if let Err(e) = validate_youtube_url(&query.url) {
        return HttpResponse::BadRequest().json(ErrorResponse { error: e });
    }

    let output = Command::new("yt-dlp")
        .args(["--dump-json", "--no-download", "--no-playlist", &query.url])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => {
            let json: serde_json::Value = match serde_json::from_slice(&out.stdout) {
                Ok(v) => v,
                Err(_) => {
                    return HttpResponse::InternalServerError().json(ErrorResponse {
                        error: "Failed to parse video info".into(),
                    });
                }
            };

            let title = json["title"].as_str().unwrap_or("Unknown").to_string();
            let duration = json["duration_string"]
                .as_str()
                .map(|s| s.to_string());

            let formats = json["formats"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|f| {
                            Some(FormatInfo {
                                format_id: f["format_id"].as_str()?.to_string(),
                                ext: f["ext"].as_str().unwrap_or("mp4").to_string(),
                                resolution: f["resolution"].as_str().map(|s| s.to_string()),
                                filesize: f["filesize"].as_u64(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            HttpResponse::Ok().json(VideoInfo {
                title,
                duration,
                formats,
            })
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            HttpResponse::BadRequest().json(ErrorResponse {
                error: format!("yt-dlp error: {}", stderr.lines().last().unwrap_or("Unknown error")),
            })
        }
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
            error: format!("Failed to run yt-dlp: {}. Is it installed?", e),
        }),
    }
}

/// POST /api/download
/// Downloads the video and streams the file back to the client.
async fn download_video(body: web::Json<DownloadRequest>) -> HttpResponse {
    if let Err(e) = validate_youtube_url(&body.url) {
        return HttpResponse::BadRequest().json(ErrorResponse { error: e });
    }

    // Ensure download directory exists
    let download_dir = PathBuf::from(DOWNLOAD_DIR);
    if let Err(e) = tokio::fs::create_dir_all(&download_dir).await {
        return HttpResponse::InternalServerError().json(ErrorResponse {
            error: format!("Failed to create download dir: {}", e),
        });
    }

    let file_id = Uuid::new_v4();
    let output_template = download_dir
        .join(format!("{}.%(ext)s", file_id))
        .to_string_lossy()
        .to_string();

    let quality = body.quality.as_deref().unwrap_or("best");

    // Build the yt-dlp arguments. "mp3" extracts audio and re-encodes to MP3
    // (requires ffmpeg); everything else downloads video merged into MP4.
    let mut args: Vec<String> = Vec::new();
    if quality == "mp3" {
        args.extend([
            "-f".into(), "bestaudio/best".into(),
            "--extract-audio".into(),
            "--audio-format".into(), "mp3".into(),
            "--audio-quality".into(), "0".into(), // 0 = best VBR quality
        ]);
    } else {
        let format_arg = match quality {
            "audio" => "bestaudio[ext=m4a]/bestaudio".to_string(),
            "720" => "bestvideo[height<=720]+bestaudio/best[height<=720]/best".to_string(),
            "1080" => "bestvideo[height<=1080]+bestaudio/best[height<=1080]/best".to_string(),
            "best" => "bestvideo+bestaudio/best".to_string(),
            custom => custom.to_string(),
        };
        args.extend([
            "-f".into(), format_arg,
            "--merge-output-format".into(), "mp4".into(),
        ]);
    }
    args.extend([
        "-o".into(), output_template.clone(),
        "--no-playlist".into(),
        body.url.clone(),
    ]);

    let output = Command::new("yt-dlp")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => {
            // Find the downloaded file (yt-dlp replaces %(ext)s with the actual extension)
            let mut found_file: Option<PathBuf> = None;

            if let Ok(mut entries) = tokio::fs::read_dir(&download_dir).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with(&file_id.to_string()) {
                        found_file = Some(entry.path());
                        break;
                    }
                }
            }

            match found_file {
                Some(path) => {
                    match tokio::fs::read(&path).await {
                        Ok(bytes) => {
                            let filename = path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| format!("{}.mp4", file_id));

                            // Clean up the file after reading
                            let _ = tokio::fs::remove_file(&path).await;

                            let content_type = match path.extension().and_then(|e| e.to_str()) {
                                Some("mp3") => "audio/mpeg",
                                Some("m4a") => "audio/mp4",
                                Some("webm") => "audio/webm",
                                _ => "video/mp4",
                            };

                            HttpResponse::Ok()
                                .content_type(content_type)
                                .insert_header((
                                    "Content-Disposition",
                                    format!("attachment; filename=\"{}\"", filename),
                                ))
                                .body(bytes)
                        }
                        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
                            error: format!("Failed to read downloaded file: {}", e),
                        }),
                    }
                }
                None => HttpResponse::InternalServerError().json(ErrorResponse {
                    error: "Download completed but file not found".into(),
                }),
            }
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            HttpResponse::BadRequest().json(ErrorResponse {
                error: format!(
                    "yt-dlp error: {}",
                    stderr.lines().last().unwrap_or("Unknown error")
                ),
            })
        }
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
            error: format!("Failed to run yt-dlp: {}. Is it installed?", e),
        }),
    }
}

/// GET /api/health
async fn health() -> HttpResponse {
    let yt_dlp_ok = Command::new("yt-dlp")
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);

    // ffmpeg is required to merge video+audio (best/1080/720) and to encode MP3.
    let ffmpeg_ok = Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);

    HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "yt_dlp_available": yt_dlp_ok,
        "ffmpeg_available": ffmpeg_ok,
    }))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("Starting udown server on http://127.0.0.1:8080");
    println!("Endpoints:");
    println!("  GET  /api/health");
    println!("  GET  /api/info?url=<youtube_url>");
    println!("  POST /api/download  {{\"url\": \"...\", \"quality\": \"best\"}}");

    HttpServer::new(|| {
        App::new()
            .route("/api/health", web::get().to(health))
            .route("/api/info", web::get().to(get_info))
            .route("/api/download", web::post().to(download_video))
            .service(Files::new("/", "./static").index_file("index.html"))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
