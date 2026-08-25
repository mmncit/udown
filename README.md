# udown

A tiny self-hosted YouTube video/audio downloader. A Rust [actix-web](https://actix.rs/) server wraps [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) behind a clean HTTP API and serves a single-page web UI for fetching video info and downloading files straight to your browser — or skip the browser entirely and run `udown mp3 <url>` from the terminal.

## Features

- **Web UI** — paste a URL, pick a quality, download. No build step for the frontend (plain `static/index.html`).
- **CLI** — `udown mp3 <url>` writes the file straight to disk, no server needed.
- **Quality presets** — Best, 1080p, 720p, audio-only (m4a), or **MP3** (re-encoded audio).
- **Title-named files** — downloads arrive as `Me_at_the_zoo.mp3`, not a uuid.
- **URL validation** — only `http(s)` YouTube hosts (`youtube.com`, `youtu.be`, etc.) are accepted.
- **Auto-cleanup** — files served over HTTP are streamed to the client and deleted from disk afterward.
- **Health check** — verifies `yt-dlp` and `ffmpeg` are installed and reachable.

## Requirements

- [Rust](https://www.rust-lang.org/tools/install) (edition 2021) and Cargo
- [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) on your `PATH`
- [`ffmpeg`](https://ffmpeg.org/) — required by yt-dlp to re-encode audio to MP3 and to merge separate video/audio streams into MP4

On macOS:

```bash
make deps          # installs yt-dlp and ffmpeg via Homebrew
```

## Quick start

### As a command (recommended for MP3)

```bash
make deps          # yt-dlp + ffmpeg
make install       # puts `udown` in ~/.cargo/bin
```

Then, from anywhere:

```bash
udown mp3 'https://www.youtube.com/watch?v=jNQXAC9IVRw'
```

```
Saved ./Me_at_the_zoo.mp3
```

### As a server with a web UI

```bash
make run           # cargo run (debug)
```

Then open **http://127.0.0.1:8080** in your browser.

For a release build:

```bash
make release       # builds in release mode and runs ./target/release/udown
```

## Command line

```
udown                              Start the web UI + API on http://127.0.0.1:8080
udown serve                        Same as above
udown mp3 <URL> [-o DIR]           Download the audio as MP3
udown get <URL> [-q Q] [-o DIR]    Download at a given quality
udown --help                       Show usage
```

| Option | Description |
| --- | --- |
| `-q`, `--quality` | `best` (default for `get`), `1080`, `720`, `audio` (m4a), `mp3`, or a raw yt-dlp format selector |
| `-o`, `--out` | Directory to write into — defaults to the **current directory** |

```bash
udown mp3 'https://youtu.be/dQw4w9WgXcQ' -o ~/Music
udown get 'https://youtu.be/dQw4w9WgXcQ' -q 720
```

Notes:

- Files are named after the video title, restricted to ASCII and capped at 100
  characters of title. An existing file is never overwritten — the next
  download becomes `Song (2).mp3`.
- `yt-dlp`'s progress output goes straight to your terminal.
- A missing `yt-dlp`, or a missing `ffmpeg` for a format that needs it, fails
  immediately with the `brew install` line to fix it. Exit code is `1` for a
  failed download and `2` for a bad invocation.
- `UDOWN_DOWNLOAD_DIR` configures the **server's** staging directory, where each
  file is deleted right after being streamed. It does not affect the CLI, whose
  output directory is a destination rather than scratch space — use `-o`.

## Makefile targets

| Target | Description |
| --- | --- |
| `make build` | Build in release mode |
| `make run` | Run the server (debug) |
| `make release` | Build in release mode and run the binary |
| `make install` | Install the `udown` command into `~/.cargo/bin` |
| `make uninstall` | Remove the installed `udown` command |
| `make test` | Run the unit test suite |
| `make check` | `cargo check` (compile without producing a binary) |
| `make fmt` | `cargo fmt` |
| `make lint` | `cargo clippy` |
| `make deps` | Install `yt-dlp` and `ffmpeg` (macOS / Homebrew) |
| `make clean` | `cargo clean` and remove `downloads/` |

## API

The server listens on `127.0.0.1:8080`.

### `GET /api/health`

Returns server status and whether `yt-dlp` is available.

```json
{ "status": "ok", "yt_dlp_available": true, "ffmpeg_available": true }
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
| `quality` | string | `best` (default), `1080`, `720`, `audio` (m4a), `mp3`, or a raw yt-dlp format string |

> `mp3` extracts the audio track and re-encodes it to MP3 (best VBR quality) — this requires `ffmpeg`. Example:
>
> ```bash
> curl -X POST http://127.0.0.1:8080/api/download \
>   -H "Content-Type: application/json" \
>   -d '{"url": "https://www.youtube.com/watch?v=...", "quality": "mp3"}' \
>   -o song.mp3
> ```

## How it works

1. The frontend (`static/index.html`) calls `/api/info` to preview the title and `/api/download` to fetch the file.
2. Each request is validated against an allowlist of YouTube hosts.
3. The server shells out to `yt-dlp`, writing to `./downloads/<uuid>-<title>.<ext>`, then merges to MP4 (or re-encodes to MP3) via ffmpeg.
4. The finished file is read into the response, sent as an `attachment` named after the title, and removed from disk.

The CLI shares steps 2 and 3 and then simply renames the file, dropping the uuid
prefix, so it stays on disk under its title.

> The uuid prefix exists because the output extension isn't known until yt-dlp
> has picked a format — it's how the finished file is located afterwards. More
> than one file can carry that prefix (yt-dlp can leave the source it re-encoded
> from beside the result), so for MP3 — the one format whose extension is known
> up front — the `.mp3` is chosen explicitly rather than by directory order.
> Truncating the title is done with the `%(title).100s` template precision, and
> deliberately **not** with yt-dlp's `--trim-filenames`, which caps the length of
> the whole rendered path (directories included) and will silently write outside
> the requested directory.

## Project layout

The code is organised into layers, each depending only on the ones above it:

```
udown/
├── src/
│   ├── main.rs            # dispatch: serve, or run one download to disk
│   ├── config.rs          # AppConfig — env-driven settings (host/port/dir)
│   ├── error.rs           # AppError + actix ResponseError (one error type)
│   ├── cli.rs             # pure argument parsing → Serve | Help | Download
│   ├── domain/            # pure logic — no IO, fully unit-tested
│   │   ├── media.rs       #   Quality enum → yt-dlp args, content-type mapping
│   │   ├── naming.rs      #   output template, display name, collision names
│   │   └── validation.rs  #   validate_youtube_url
│   ├── services/          # side effects
│   │   └── ytdlp.rs       #   yt-dlp / ffmpeg subprocess + filesystem wrappers
│   └── web/               # HTTP layer (actix-web)
│       ├── dto.rs         #   request/response shapes
│       ├── handlers.rs    #   thin handlers returning Result<_, AppError>
│       └── routes.rs      #   route table
├── static/index.html      # single-page web UI
├── Cargo.toml             # dependencies
└── Makefile               # build/run/lint shortcuts
```

**Design notes**

- **Layered & pure-core.** `domain` is free of IO and framework types, so its
  logic is trivially unit-testable; `services` isolates all subprocess and
  filesystem effects; `web` only translates HTTP ↔ domain.
- **One error type.** Every fallible path returns `Result<_, AppError>`. Because
  `AppError` implements actix's `ResponseError`, handlers use `?` and the right
  status code + JSON body is produced automatically — no nested `match` trees.
- **Configuration via env.** `UDOWN_HOST`, `UDOWN_PORT`, `UDOWN_DOWNLOAD_DIR`
  override the defaults (`127.0.0.1`, `8080`, `./downloads`).

Run the unit tests with `cargo test` (or `make` + `cargo test`).

## Notes

- The server binds to `127.0.0.1` only — it is intended for **local use**, not public hosting. There is no authentication, rate limiting, or sandboxing around the `yt-dlp` subprocess.
- Downloads are buffered fully into memory before being sent, so very large videos will use proportional RAM.
