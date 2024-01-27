pub mod beatmaps;
pub mod beatmapsets;
pub mod downloads;
pub mod search;

use std::sync::Arc;

use axum::{routing::get, Extension, Router};
use axum_prometheus::{metrics_exporter_prometheus::PrometheusBuilder, PrometheusMetricLayerBuilder};
use tokio::sync::Mutex;
use tower::ServiceBuilder;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing::Level;

use crate::crawler::Context;


pub async fn serve(ctx: Context) {
    let ctx = Arc::new(Mutex::new(ctx.clone()));
    let prometeus_layer = PrometheusMetricLayerBuilder::new().with_prefix("mirria").build();
    let metric_handle = PrometheusBuilder::new()
    .install_recorder()
    .unwrap();

    let layer_ctx = ServiceBuilder::new()
        .layer(TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO).include_headers(true)))
        .layer(Extension(ctx));

    let router = Router::new()
        .merge(crate::api::beatmapsets::serve())
        .merge(crate::api::beatmaps::serve())
        .merge(crate::api::downloads::serve())
        .merge(crate::api::search::serve())
        .route("/metrics", get(|| async move { metric_handle.render() }))
        .layer(layer_ctx)
        .layer(prometeus_layer);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, router).await.unwrap();
}
