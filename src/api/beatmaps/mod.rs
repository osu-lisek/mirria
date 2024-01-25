
use std::sync::Arc;

use axum::{
    extract::Path, http::StatusCode, response::Result, routing::get, Extension, Json, Router,
};
use tokio::sync::Mutex;

use crate::{
    crawler::Context,
    ops::{beatmaps::{get_beatmap_by_id as get_beatmap_from_db, DatabaseError}, beatmapset::get_beatmapset_by_hash},
    osu::types::{Beatmap, Beatmapset},
};

async fn get_beatmap_by_id(
    Extension(ctx): Extension<Arc<Mutex<Context>>>,
    Path(id): Path<String>,
) -> Result<Json<Beatmap>, StatusCode> {
    let ctx = ctx.lock().await;
    let response = get_beatmap_from_db(ctx.to_owned(), id.parse::<i64>().unwrap_or(0)).await;

    if response.is_err() {
        let error = response.unwrap_err();
        return match error {
            DatabaseError::RecordNotFound => return Err(StatusCode::NOT_FOUND),
            
            _ => Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }

    let beatmap = response.unwrap();

    return Ok(Json(beatmap.clone()));
}

async fn get_beatmap_by_hash(
    Extension(ctx): Extension<Arc<Mutex<Context>>>,
    Path(checksum): Path<String>,
) -> Result<Json<Beatmapset>, StatusCode> {
    let ctx = ctx.lock().await;
    let response = get_beatmapset_by_hash(ctx.to_owned(), checksum).await;

    if response.is_err() {
        let error = response.unwrap_err();
        return match error {
            DatabaseError::RecordNotFound => return Err(StatusCode::NOT_FOUND),
            
            _ => Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }

    let beatmapset = response.unwrap();

    return Ok(Json(beatmapset.clone()));
}

pub fn serve() -> Router {
    return Router::new()
        .route("/api/v1/beatmaps/md5/:checksum", get(get_beatmap_by_hash))
        .route("/api/v1/beatmaps/:id", get(get_beatmap_by_id));
}
