pub mod beatmaps;
pub mod beatmapsets;
pub mod downloads;
pub mod search;

use std::sync::Arc;

use axum::{Extension, Router};
use tokio::sync::Mutex;
use tower::ServiceBuilder;
use tower_http::trace::{DefaultOnRequest, TraceLayer, DefaultOnResponse};
use tracing::Level;

use crate::crawler::Context;

pub async fn serve(ctx: Context) {
    let ctx = Arc::new(Mutex::new(ctx.clone()));

    let layer_ctx = ServiceBuilder::new()
        .layer(TraceLayer::new_for_http()
        .on_request(DefaultOnRequest::new().level(Level::INFO))
        .on_response(DefaultOnResponse::new().level(Level::INFO).latency_unit(tower_http::LatencyUnit::Millis)))
        .layer(Extension(ctx));

    let router = Router::new()
        .merge(crate::api::beatmapsets::serve())
        .merge(crate::api::beatmaps::serve())
        .merge(crate::api::downloads::serve())
        .merge(crate::api::search::serve())
        .layer(layer_ctx);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, router).await.unwrap();
}
