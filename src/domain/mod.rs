//! Pure domain logic — no IO, no framework types.
//!
//! Everything here is deterministic and unit-testable in isolation:
//! mapping a requested quality to yt-dlp arguments, choosing a content type,
//! naming the output file, and validating incoming URLs.

pub mod media;
pub mod naming;
pub mod validation;
