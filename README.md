# udown

A tiny self-hosted YouTube video/audio downloader. A Rust [actix-web](https://actix.rs/) server wraps [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) behind a clean HTTP API and serves a single-page web UI for fetching video info and downloading files straight to your browser.

## Features

- **Web UI** — paste a URL, pick a quality, download. No build step for the frontend (plain `static/index.html`).
- **Quality presets** — Best, 1080p, 720p, or audio-only (m4a).
- **URL validation** — only `http(s)` YouTube hosts (`youtube.com`, `youtu.be`, etc.) are accepted.
- **Auto-cleanup** — downloaded files are streamed to the client and deleted from disk afterward.
- **Health check** — verifies `yt-dlp` is installed and reachable.

## Requirements

- [Rust](https://www.rust-lang.org/tools/install) (edition 2021) and Cargo
- [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) on your `PATH`
- [`ffmpeg`](https://ffmpeg.org/) — required by yt-dlp to merge separate video/audio streams into MP4

On macOS:

```bash
make deps          # installs yt-dlp via Homebrew
brew install ffmpeg
```

## Quick start

```bash
make run           # cargo run (debug)
```

Then open **http://127.0.0.1:8080** in your browser.

For a release build:

```bash
make release       # builds in release mode and runs ./target/release/udown
```

## Makefile targets

| Target | Description |
| --- | --- |
| `make build` | Build in release mode |
| `make run` | Run the server (debug) |
| `make release` | Build in release mode and run the binary |
| `make check` | `cargo check` (compile without producing a binary) |
| `make fmt` | `cargo fmt` |
| `make lint` | `cargo clippy` |
| `make deps` | Install `yt-dlp` (macOS / Homebrew) |
| `make clean` | `cargo clean` and remove `downloads/` |

## API

The server listens on `127.0.0.1:8080`.

### `GET /api/health`

Returns server status and whether `yt-dlp` is available.

```json
{ "status": "ok", "yt_dlp_available": true }
```

### `GET /api/info?url=<youtube_url>`

Returns metadata for a video without downloading it.

```bash
curl "http://127.0.0.1:8080/api/info?url=https://www.youtube.com/watch?v=dQw4w9WgXcQ"
```

```json
{
  "title": "...",
  "duration": "3:33",
  "formats": [
    { "format_id": "137", "ext": "mp4", "resolution": "1920x1080", "filesize": 12345678 }
  ]
}
```

### `POST /api/download`

Downloads the video and streams the file back as an attachment.

```bash
curl -X POST http://127.0.0.1:8080/api/download \
  -H "Content-Type: application/json" \
  -d '{"url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ", "quality": "720"}' \
  -o video.mp4
```

**Body fields:**

| Field | Type | Description |
| --- | --- | --- |
| `url` | string | YouTube URL (required) |
| `quality` | string | `best` (default), `1080`, `720`, `audio`, or a raw yt-dlp format string |

## How it works

1. The frontend (`static/index.html`) calls `/api/info` to preview the title and `/api/download` to fetch the file.
2. Each request is validated against an allowlist of YouTube hosts.
3. The server shells out to `yt-dlp`, writing to `./downloads/<uuid>.<ext>`, then merges to MP4 via ffmpeg.
4. The finished file is read into the response, sent as an `attachment`, and removed from disk.

## Project layout

```
udown/
├── src/main.rs        # actix-web server + yt-dlp wrapper
├── static/index.html  # single-page web UI
├── Cargo.toml         # dependencies
└── Makefile           # build/run/lint shortcuts
```

## Notes

- The server binds to `127.0.0.1` only — it is intended for **local use**, not public hosting. There is no authentication, rate limiting, or sandboxing around the `yt-dlp` subprocess.
- Downloads are buffered fully into memory before being sent, so very large videos will use proportional RAM.
