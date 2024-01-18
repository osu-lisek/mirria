use std::sync::Arc;

use axum::{
    Extension, Json, Router, extract::{Path, Query}, http::StatusCode,
    routing::get, response::Result
};
use serde_json::{json, Value};
use tracing::{error, info};

use crate::{crawler::Context, osu::types::Beatmapset};

async fn get_beatmap_by_id(
    Extension(ctx): Extension<Arc<Context>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let beatmap = ctx
        .elasticsearch
        .search(elasticsearch::SearchParts::Index(&["beatmap"]))
        .body(json!({
            "query": {
                "match": {
                    "id": id
                }
            }
        }))
        .send().await;

    if beatmap.is_err() {
        let error = beatmap.unwrap_err();
        error!("{}", error);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let beatmap = beatmap.unwrap();
    let mut beatmap = beatmap.json::<Value>().await.unwrap();
    
    let hits = beatmap.get("hits").unwrap().get("hits").unwrap().as_array().unwrap();
    if hits.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }


    let beatmap = hits.first().unwrap().get("_source").unwrap();
    return Ok(Json(beatmap.clone()))
}


async fn get_beatmap_by_hash(
    Extension(ctx): Extension<Arc<Context>>,
    Path(checksum): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let beatmap = ctx
        .elasticsearch
        .search(elasticsearch::SearchParts::Index(&["beatmapset"]))
        .body(json!({
            "query": {
                "match": {
                    "beatmaps.checksum": checksum
                }
            }
        }))
        .send().await;

    if beatmap.is_err() {
        let error = beatmap.unwrap_err();
        error!("{}", error);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let beatmap = beatmap.unwrap();
    let mut beatmap = beatmap.json::<Value>().await.unwrap();
    
    let hits = beatmap.get("hits").unwrap().get("hits").unwrap().as_array().unwrap();
    if hits.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }


    let beatmap = hits.first().unwrap().get("_source").unwrap();
    return Ok(Json(beatmap.clone()))
}



pub fn serve() -> Router {
    return Router::new()
    .route("/api/v1/beatmaps/md5/:checksum", get(get_beatmap_by_hash))
    .route("/api/v1/beatmaps/:id", get(get_beatmap_by_id));
}
