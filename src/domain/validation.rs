//! URL validation. Pure function, no IO.

use crate::error::AppError;
use url::Url;

/// Hosts we accept download requests for.
const ALLOWED_HOSTS: [&str; 5] = [
    "youtube.com",
    "www.youtube.com",
    "m.youtube.com",
    "youtu.be",
    "www.youtu.be",
];

/// Validate that `raw_url` is an `http(s)` URL pointing at a known YouTube host.
///
/// Returns [`AppError::InvalidRequest`] with a human-readable reason otherwise.
pub fn validate_youtube_url(raw_url: &str) -> Result<(), AppError> {
    let parsed =
        Url::parse(raw_url).map_err(|_| AppError::InvalidRequest("Invalid URL format".into()))?;

    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AppError::InvalidRequest(
            "URL must use http or https".into(),
        ));
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::InvalidRequest("Missing host".into()))?;

    ALLOWED_HOSTS
        .iter()
        .any(|allowed| host.eq_ignore_ascii_case(allowed))
        .then_some(())
        .ok_or_else(|| AppError::InvalidRequest("Only YouTube URLs are supported".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_youtube_urls() {
        for url in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ",
            "http://m.youtube.com/watch?v=abc",
            "https://YouTube.com/watch?v=abc", // case-insensitive host
        ] {
            assert!(validate_youtube_url(url).is_ok(), "should accept {url}");
        }
    }

    #[test]
    fn rejects_non_youtube_host() {
        assert!(validate_youtube_url("https://vimeo.com/12345").is_err());
        assert!(validate_youtube_url("https://evil.com/youtube.com").is_err());
    }

    #[test]
    fn rejects_bad_scheme() {
        assert!(validate_youtube_url("ftp://youtube.com/x").is_err());
        assert!(validate_youtube_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn rejects_malformed_url() {
        assert!(validate_youtube_url("not a url").is_err());
        assert!(validate_youtube_url("").is_err());
    }
}
