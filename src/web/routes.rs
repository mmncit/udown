//! Route table. Kept in one place so the wiring is easy to scan.

use actix_files::Files;
use actix_web::web;

use crate::web::handlers;

/// Register the JSON API under `/api` and serve the static UI from `./static`.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            .route("/health", web::get().to(handlers::health))
            .route("/info", web::get().to(handlers::get_info))
            .route("/download", web::post().to(handlers::download)),
    )
    .service(Files::new("/", "./static").index_file("index.html"));
}
