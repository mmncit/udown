//! Pure domain logic — no IO, no framework types.
//!
//! Everything here is deterministic and unit-testable in isolation:
//! mapping a requested quality to yt-dlp arguments, choosing a content type,
//! and validating incoming URLs.

pub mod media;
pub mod validation;
