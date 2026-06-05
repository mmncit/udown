//! Runtime configuration, sourced from the environment with sensible defaults.

use std::env;
use std::path::PathBuf;

/// Application configuration shared across request handlers.
#[derive(Clone, Debug)]
pub struct AppConfig {
    /// Address the HTTP server binds to, e.g. `127.0.0.1:8080`.
    pub bind_address: String,
    /// Directory where downloads are temporarily written.
    pub download_dir: PathBuf,
}

impl AppConfig {
    /// Build configuration from environment variables, falling back to defaults:
    /// - `UDOWN_HOST`         (default `127.0.0.1`)
    /// - `UDOWN_PORT`         (default `8080`)
    /// - `UDOWN_DOWNLOAD_DIR` (default `./downloads`)
    pub fn from_env() -> Self {
        let host = env::var("UDOWN_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = env::var("UDOWN_PORT").unwrap_or_else(|_| "8080".to_string());
        let download_dir =
            env::var("UDOWN_DOWNLOAD_DIR").unwrap_or_else(|_| "./downloads".to_string());

        Self {
            bind_address: format!("{host}:{port}"),
            download_dir: PathBuf::from(download_dir),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:8080".to_string(),
            download_dir: PathBuf::from("./downloads"),
        }
    }
}
