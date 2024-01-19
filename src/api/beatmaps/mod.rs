use std::sync::Arc;

use axum::{
    Extension, Json, Router, extract::{Path}, http::StatusCode,
    routing::get, response::Result
};

use tracing::{error};

use crate::{crawler::Context, osu::types::{Beatmapset, Beatmap}};

async fn get_beatmap_by_id(
    Extension(ctx): Extension<Arc<Context>>,
    Path(id): Path<String>,
) -> Result<Json<Beatmap>, StatusCode> {
    let response = ctx.meili_client
    .index("beatmapset")
    .search()
    .with_filter(format!("beatmaps.id = {}", id).as_str())
    .execute::<Beatmapset>()
    .await;

    if response.is_err() {
        let error = response.unwrap_err();
        error!("{}", error);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let beatmapset = response.unwrap();
    
    let hits = beatmapset.hits;
    if hits.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }


    let beatmapset = &hits.first().unwrap().result;

    //Finding the beatmap
    let beatmap = beatmapset.beatmaps.iter().find(|x| x.id.to_string() == id).unwrap();
    return Ok(Json(beatmap.clone()))
}


async fn get_beatmap_by_hash(
    Extension(ctx): Extension<Arc<Context>>,
    Path(checksum): Path<String>,
) -> Result<Json<Beatmapset>, StatusCode> {
    let response = ctx.meili_client
    .index("beatmapset")
    .search()
    .with_filter(format!("beatmaps.checksum = {}", checksum).as_str())
    .execute::<Beatmapset>()
    .await;

    if response.is_err() {
        let error = response.unwrap_err();
        error!("{}", error);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let beatmapset = response.unwrap();
    
    let hits = beatmapset.hits;
    if hits.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }


    let beatmapset = &hits.first().unwrap().result;

    return Ok(Json(beatmapset.clone()))
}



pub fn serve() -> Router {
    return Router::new()
    .route("/api/v1/beatmaps/md5/:checksum", get(get_beatmap_by_hash))
    .route("/api/v1/beatmaps/:id", get(get_beatmap_by_id));
}
