use axum::{routing::get, Router};

pub fn serve() -> Router {
    // Liveness only: dependency outages must not make the HTTP server unhealthy.
    Router::new().route("/health", get(|| async { "ok" }))
}
