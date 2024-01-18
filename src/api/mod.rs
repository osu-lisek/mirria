pub mod beatmapsets;
pub mod beatmaps;
pub mod downloads;
pub mod search;

use std::sync::Arc;

use axum::{Router, Extension};
use tower::ServiceBuilder;
use tracing::info;

use crate::crawler::Context;


pub async fn serve(ctx: Context) {
    let arc_ctx = Arc::new(ctx);

    let layer_ctx = ServiceBuilder::new()
    .layer(Extension(arc_ctx.clone()));

    let router = Router::new()
    .merge(crate::api::beatmapsets::serve())
    .merge(crate::api::beatmaps::serve())
    .merge(crate::api::downloads::serve())
    .merge(crate::api::search::serve())
    .layer(layer_ctx);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, router).await.unwrap();
}