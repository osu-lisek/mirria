use std::{sync::Arc};

use axum::{
    extract::{Query, Request},
    http::StatusCode,
    response::Result,
    routing::get,
    Extension, Json, Router,
};
use serde_derive::Deserialize;

use tracing::{error};

use crate::{crawler::Context, osu::types::Beatmapset};
#[derive(Deserialize, Debug)]
struct SearchQuery {
    pub query: Option<String>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
    pub statues: Option<Vec<String>>,
    pub sort: Option<String>
}

async fn search(
    Extension(ctx): Extension<Arc<Context>>,
    Query(_query): Query<SearchQuery>,
    request: Request,
) -> Result<Json<Vec<Beatmapset>>, StatusCode> {
    let parsed_query: SearchQuery = serde_qs::from_str(request.uri().query().unwrap_or("")).unwrap();
    let mapped_statuses = parsed_query
        .statues
        .unwrap_or(Vec::from(["ranked", "loved", "aproved", "qualified"].map(|x| x.to_string()))).iter().map(
            |x| {
                return format!("status = {}", x.to_string()).to_string();
            },
        ).collect::<Vec<String>>().join(" OR ");

    let sorting = match parsed_query.sort.unwrap_or("updated_desc".to_string()).as_str() {
        "updated_asc" => "last_updated:asc",
        "playcount" => "play_count:asc",
        _ => "last_updated:desc",
    };

    let response = ctx
        .meili_client
        .index("beatmapset")
        .search()
        .with_query((parsed_query.query.unwrap_or("".to_string())).as_str())
        .with_filter(format!("{}", mapped_statuses).as_str())
        .with_sort(&[sorting])
        .with_offset(parsed_query.offset.unwrap_or(0) as usize)
        .with_limit(parsed_query.limit.unwrap_or(50) as usize)
        .execute::<Beatmapset>()
        .await;

    if response.is_err() {
        let error = response.unwrap_err();
        error!("{}", error);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let beatmapets = response.unwrap();


    let hits = beatmapets.hits.clone();

    return Ok(Json(
        hits.iter()
            .map(|set| set.clone().result)
            .clone()
            .collect(),
    ));
}

pub fn serve() -> Router {
    return Router::new().route("/api/v1/search", get(search));
}
