//! Application-wide error type.
//!
//! A single [`AppError`] flows through every layer. It implements actix-web's
//! [`ResponseError`], so handlers can simply return `Result<_, AppError>` and
//! the framework renders the correct status code and JSON body automatically —
//! no nested `match` pyramids in the handlers.

use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use serde::Serialize;
use thiserror::Error;

/// Every failure mode the API can surface, tagged with the HTTP status it maps to.
#[derive(Debug, Error)]
pub enum AppError {
    /// The client sent something invalid (bad URL, unsupported host, …). → 400
    #[error("{0}")]
    InvalidRequest(String),

    /// yt-dlp ran but reported an error (e.g. unavailable video). → 400
    #[error("yt-dlp error: {0}")]
    YtDlp(String),

    /// An external tool (yt-dlp / ffmpeg) could not be executed. → 500
    #[error("{0}")]
    ToolUnavailable(String),

    /// The download finished but the output file could not be located. → 500
    #[error("download completed but file not found")]
    FileNotFound,

    /// Any other internal failure (filesystem, parsing, …). → 500
    #[error("{0}")]
    Internal(String),
}

/// JSON body returned for every error: `{ "error": "<message>" }`.
#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            AppError::InvalidRequest(_) | AppError::YtDlp(_) => StatusCode::BAD_REQUEST,
            AppError::ToolUnavailable(_) | AppError::FileNotFound | AppError::Internal(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(ErrorBody {
            error: self.to_string(),
        })
    }
}
