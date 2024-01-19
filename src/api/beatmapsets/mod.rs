use std::sync::Arc;

use axum::{
    Extension, Json, Router, extract::Path, http::StatusCode,
    routing::get, response::Result
};

use tracing::{error};

use crate::{crawler::Context, osu::types::Beatmapset};

async fn get_beatmapset_by_id(
    Extension(ctx): Extension<Arc<Context>>,
    Path(id): Path<String>,
) -> Result<Json<Beatmapset>, StatusCode> {
    let search_result = ctx.meili_client.index("beatmapset")
    .search()
    .with_filter(format!("id = {}", id).as_str())
    .execute::<Beatmapset>()
    .await;

    if search_result.is_err() {
        let error = search_result.unwrap_err();
        error!("{}", error);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let hits = search_result.unwrap().hits;
    
    if hits.clone().is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }


    let beatmap = &hits.first().unwrap().result;
    return Ok(Json(beatmap.clone()))
}


pub fn serve() -> Router {
    return Router::new()
    .route("/api/v1/beatmapsets/:id", get(get_beatmapset_by_id));
}
