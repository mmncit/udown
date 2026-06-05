//! Media-format logic: requested quality → yt-dlp arguments, and file
//! extension → HTTP content type. All functions here are pure.

/// A requested output quality, parsed from the API's `quality` field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Quality {
    /// Best available video + audio, merged to MP4.
    Best,
    /// Up to 1080p, merged to MP4.
    P1080,
    /// Up to 720p, merged to MP4.
    P720,
    /// Audio only, kept as m4a.
    AudioM4a,
    /// Audio extracted and re-encoded to MP3 (requires ffmpeg).
    Mp3,
    /// A raw yt-dlp format selector passed through verbatim.
    Custom(String),
}

impl Quality {
    /// Parse the optional `quality` field, defaulting to [`Quality::Best`].
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.unwrap_or("best") {
            "best" => Quality::Best,
            "1080" => Quality::P1080,
            "720" => Quality::P720,
            "audio" => Quality::AudioM4a,
            "mp3" => Quality::Mp3,
            other => Quality::Custom(other.to_string()),
        }
    }

    /// The yt-dlp arguments that realise this quality (pure, no IO).
    pub fn ytdlp_args(&self) -> Vec<String> {
        match self {
            Quality::Mp3 => owned([
                "-f",
                "bestaudio/best",
                "--extract-audio",
                "--audio-format",
                "mp3",
                "--audio-quality",
                "0", // 0 = best VBR quality
            ]),
            Quality::Best => video_args("bestvideo+bestaudio/best"),
            Quality::P1080 => {
                video_args("bestvideo[height<=1080]+bestaudio/best[height<=1080]/best")
            }
            Quality::P720 => video_args("bestvideo[height<=720]+bestaudio/best[height<=720]/best"),
            Quality::AudioM4a => video_args("bestaudio[ext=m4a]/bestaudio"),
            Quality::Custom(format) => video_args(format),
        }
    }
}

/// Standard download arguments for a video-style format selector.
fn video_args(format: &str) -> Vec<String> {
    owned(["-f", format, "--merge-output-format", "mp4"])
}

/// Convert a fixed-size array of `&str` into an owned `Vec<String>`.
fn owned<const N: usize>(args: [&str; N]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

/// Map a file extension to the HTTP `Content-Type` to serve it with (pure).
pub fn content_type_for(extension: Option<&str>) -> &'static str {
    match extension {
        Some("mp3") => "audio/mpeg",
        Some("m4a") => "audio/mp4",
        Some("webm") => "audio/webm",
        _ => "video/mp4",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_defaults_to_best() {
        assert_eq!(Quality::parse(None), Quality::Best);
        assert_eq!(Quality::parse(Some("best")), Quality::Best);
    }

    #[test]
    fn parse_known_qualities() {
        assert_eq!(Quality::parse(Some("1080")), Quality::P1080);
        assert_eq!(Quality::parse(Some("720")), Quality::P720);
        assert_eq!(Quality::parse(Some("audio")), Quality::AudioM4a);
        assert_eq!(Quality::parse(Some("mp3")), Quality::Mp3);
    }

    #[test]
    fn parse_unknown_is_custom_passthrough() {
        assert_eq!(
            Quality::parse(Some("worstaudio")),
            Quality::Custom("worstaudio".to_string())
        );
    }

    #[test]
    fn mp3_args_request_mp3_extraction() {
        let args = Quality::Mp3.ytdlp_args();
        assert!(args.contains(&"--extract-audio".to_string()));
        assert!(args.windows(2).any(|w| w == ["--audio-format", "mp3"]));
        // mp3 must not try to merge into an mp4 container
        assert!(!args.contains(&"--merge-output-format".to_string()));
    }

    #[test]
    fn video_qualities_merge_to_mp4() {
        for q in [Quality::Best, Quality::P1080, Quality::P720] {
            let args = q.ytdlp_args();
            assert!(args
                .windows(2)
                .any(|w| w == ["--merge-output-format", "mp4"]));
        }
    }

    #[test]
    fn content_type_mapping() {
        assert_eq!(content_type_for(Some("mp3")), "audio/mpeg");
        assert_eq!(content_type_for(Some("m4a")), "audio/mp4");
        assert_eq!(content_type_for(Some("webm")), "audio/webm");
        assert_eq!(content_type_for(Some("mp4")), "video/mp4");
        assert_eq!(content_type_for(None), "video/mp4");
    }
}
