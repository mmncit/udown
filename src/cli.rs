//! Command-line argument parsing.
//!
//! Pure: [`parse`] turns an argument slice into a [`Command`] and nothing else,
//! so every accepted and rejected form is unit-testable without running a
//! download. `main` is the only place that acts on the result.

use std::path::PathBuf;

/// What the user asked the binary to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Start the HTTP server and web UI (the default with no arguments).
    Serve,
    /// Print usage.
    Help,
    /// Download one URL straight to disk.
    Download {
        url: String,
        /// A `quality` value as understood by `domain::media::Quality::parse`.
        quality: String,
        /// Where to write the file; `None` means the current directory.
        out_dir: Option<PathBuf>,
    },
}

/// Usage text, printed for `--help` and for a bad invocation.
pub const USAGE: &str = "\
udown — a tiny self-hosted YouTube downloader.

USAGE:
    udown                              Start the web UI + API on http://127.0.0.1:8080
    udown serve                        Same as above
    udown mp3 <URL> [-o DIR]           Download the audio as MP3
    udown get <URL> [-q Q] [-o DIR]    Download at a given quality
    udown --help                       Show this message

OPTIONS:
    -q, --quality Q    best (default), 1080, 720, audio (m4a), mp3,
                       or a raw yt-dlp format selector
    -o, --out DIR      Directory to write into (default: the current directory)

EXAMPLES:
    udown mp3 'https://www.youtube.com/watch?v=dQw4w9WgXcQ'
    udown mp3 'https://youtu.be/dQw4w9WgXcQ' -o ~/Music
    udown get 'https://youtu.be/dQw4w9WgXcQ' -q 720

The server writes to a temp directory (UDOWN_DOWNLOAD_DIR, default ./downloads)
and deletes each file after streaming it. The CLI keeps the file instead, named
after the video title.
";

/// Parse the arguments following the program name.
pub fn parse(args: &[String]) -> Result<Command, String> {
    let Some(first) = args.first() else {
        return Ok(Command::Serve);
    };

    match first.as_str() {
        "serve" => Ok(Command::Serve),
        "help" | "-h" | "--help" => Ok(Command::Help),
        "mp3" => parse_download(&args[1..], "mp3"),
        "get" => parse_download(&args[1..], "best"),
        other if other.starts_with("http") => Err(format!(
            "Missing command. Did you mean `udown mp3 {other}` or `udown get {other}`?"
        )),
        other => Err(format!("Unknown command: {other}")),
    }
}

/// Parse `<URL> [-q QUALITY] [-o DIR]` for the `mp3` / `get` subcommands.
fn parse_download(args: &[String], default_quality: &str) -> Result<Command, String> {
    let mut url: Option<String> = None;
    let mut quality = default_quality.to_string();
    let mut out_dir: Option<PathBuf> = None;

    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "-q" | "--quality" => {
                quality = value_for(arg, rest.next())?;
            }
            "-o" | "--out" | "--out-dir" => {
                out_dir = Some(PathBuf::from(value_for(arg, rest.next())?));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("Unknown option: {flag}"));
            }
            positional if url.is_none() => {
                url = Some(positional.to_string());
            }
            extra => {
                return Err(format!("Unexpected argument: {extra}"));
            }
        }
    }

    let url = url.ok_or_else(|| "Missing URL.".to_string())?;

    Ok(Command::Download {
        url,
        quality,
        out_dir,
    })
}

/// Read the value that must follow `flag`.
fn value_for(flag: &str, next: Option<&String>) -> Result<String, String> {
    next.filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .ok_or_else(|| format!("{flag} needs a value."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(args: &[&str]) -> Result<Command, String> {
        parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    const URL: &str = "https://youtu.be/dQw4w9WgXcQ";

    #[test]
    fn no_arguments_starts_the_server() {
        assert_eq!(parse_str(&[]), Ok(Command::Serve));
        assert_eq!(parse_str(&["serve"]), Ok(Command::Serve));
    }

    #[test]
    fn help_flags_request_usage() {
        for flag in ["help", "-h", "--help"] {
            assert_eq!(parse_str(&[flag]), Ok(Command::Help));
        }
    }

    #[test]
    fn mp3_subcommand_defaults_to_mp3_quality_and_current_dir() {
        assert_eq!(
            parse_str(&["mp3", URL]),
            Ok(Command::Download {
                url: URL.to_string(),
                quality: "mp3".to_string(),
                out_dir: None,
            })
        );
    }

    #[test]
    fn get_subcommand_defaults_to_best() {
        let Ok(Command::Download { quality, .. }) = parse_str(&["get", URL]) else {
            panic!("expected a download");
        };
        assert_eq!(quality, "best");
    }

    #[test]
    fn quality_and_out_dir_can_be_overridden() {
        assert_eq!(
            parse_str(&["get", URL, "-q", "720", "-o", "/tmp/media"]),
            Ok(Command::Download {
                url: URL.to_string(),
                quality: "720".to_string(),
                out_dir: Some(PathBuf::from("/tmp/media")),
            })
        );
    }

    #[test]
    fn flags_may_precede_the_url() {
        let Ok(Command::Download { url, quality, .. }) =
            parse_str(&["mp3", "-o", "/tmp", URL, "-q", "audio"])
        else {
            panic!("expected a download");
        };
        assert_eq!(url, URL);
        assert_eq!(quality, "audio");
    }

    #[test]
    fn long_option_names_are_accepted() {
        assert_eq!(
            parse_str(&["get", URL, "--quality", "1080", "--out-dir", "."]),
            Ok(Command::Download {
                url: URL.to_string(),
                quality: "1080".to_string(),
                out_dir: Some(PathBuf::from(".")),
            })
        );
    }

    #[test]
    fn a_missing_url_is_rejected() {
        assert!(parse_str(&["mp3"]).is_err());
        assert!(parse_str(&["get", "-q", "720"]).is_err());
    }

    #[test]
    fn a_flag_without_its_value_is_rejected() {
        assert!(parse_str(&["mp3", URL, "-o"]).is_err());
        assert!(parse_str(&["mp3", URL, "-q"]).is_err());
    }

    #[test]
    fn unknown_options_and_extra_positionals_are_rejected() {
        assert!(parse_str(&["mp3", URL, "--verbose"]).is_err());
        assert!(parse_str(&["mp3", URL, URL]).is_err());
    }

    #[test]
    fn a_bare_url_suggests_a_subcommand() {
        let error = parse_str(&[URL]).unwrap_err();
        assert!(error.contains("udown mp3"), "unhelpful message: {error}");
    }

    #[test]
    fn an_unknown_command_is_rejected() {
        assert!(parse_str(&["download"]).is_err());
    }
}
