use std::sync::Arc;

use axum::{
    Extension, Json, Router, extract::Path, http::StatusCode,
    routing::get, response::Result
};
use tokio::sync::Mutex;


use crate::{crawler::Context, osu::types::Beatmapset, ops::{beatmapset::get_beatmapset_by_id as fetch_beatmapset_by_id, beatmapset::get_beatmapset_by_beatmap_id as fetch_beatmapset_by_beatmap_id, beatmaps::DatabaseError}};

async fn get_beatmapset_by_id(
    Extension(ctx): Extension<Arc<Mutex<Context>>>,
    Path(id): Path<String>,
) -> Result<Json<Beatmapset>, StatusCode> {
    let ctx = ctx.lock().await;

    let response = fetch_beatmapset_by_id(ctx.to_owned(), id.parse::<i64>().unwrap_or(0)).await;

    if response.is_err() {
        let error = response.unwrap_err();
        return match error {
            DatabaseError::RecordNotFound => return Err(StatusCode::NOT_FOUND),
            
            _ => Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }

    let beatmapset = response.unwrap();

    return Ok(Json(beatmapset))
}


async fn get_beatmapset_by_beatmap_id(
    Extension(ctx): Extension<Arc<Mutex<Context>>>,
    Path(id): Path<String>,
) -> Result<Json<Beatmapset>, StatusCode> {
    let ctx = ctx.lock().await;
    let response = fetch_beatmapset_by_beatmap_id(ctx.to_owned(), id.parse::<i64>().unwrap_or(0)).await;

    if response.is_err() {
        let error = response.unwrap_err();
        return match error {
            DatabaseError::RecordNotFound => return Err(StatusCode::NOT_FOUND),
            
            _ => Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }

    let beatmapset = response.unwrap();

    return Ok(Json(beatmapset))
}

pub fn serve() -> Router {
    return Router::new()
    .route("/api/v1/beatmapsets/:id", get(get_beatmapset_by_id))
    .route("/api/v1/beatmapsets/beatmap/:id", get(get_beatmapset_by_beatmap_id));
}
