use std::{convert::Infallible, sync::Arc};

use axum::{
    extract::{Path, Query, Request},
    http::StatusCode,
    response::Result,
    routing::get,
    Extension, Json, Router,
};
use serde_derive::Deserialize;
use serde_json::{json, Value};
use tracing::{error, info};

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
    Query(query): Query<SearchQuery>,
    request: Request,
) -> Result<Json<Vec<Value>>, StatusCode> {
    let parsed_query: SearchQuery = serde_qs::from_str(request.uri().query().unwrap_or("")).unwrap();
    let mapped_statuses: Vec<Value> = parsed_query
        .statues
        .unwrap_or(Vec::from(["ranked", "loved", "aproved", "qualified"].map(
            |x| {
                return x.to_string();
            },
        )))
        .iter()
        .map(|x| {
            return json!({"match": { "status": x.to_string()}});
        })
        .collect();

    let sorting = match parsed_query.sort.unwrap_or("updated_desc".to_string()).as_str() {
        "updated_desc" => json!({"last_updated": "desc"}),
        "updated_asc" => json!({"last_updated": "asc"}),
        "playcount" => json!({"play_count": "asc"}),
        _ => json!({"last_updated": "desc"}),
    };

    let q = json!({
        "query": {
            "bool": {
                "filter": [{
                    "bool": {
                        "should": mapped_statuses
                    }
                }],
                "must": [
                    {
                        "multi_match": {
                            "query": query.query.unwrap_or("".to_string())
                          }
                    }
                ]
            }
        },
        "size": query.limit.unwrap_or(10),
        "from": query.offset.unwrap_or(0),
        "sort": sorting
    });

    info!("{}", q.clone().to_string());
    let beatmap = ctx
        .elasticsearch
        .search(elasticsearch::SearchParts::Index(&["beatmapset"]))
        .body(q.clone())
        .send()
        .await;

    if beatmap.is_err() {
        let error = beatmap.unwrap_err();
        error!("{}", error);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let beatmap = beatmap.unwrap();

    if !beatmap.status_code().is_success() {
        info!("{:#?}", beatmap);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let mut beatmap = beatmap.json::<Value>().await.unwrap();

    let hits = beatmap
        .get("hits")
        .unwrap()
        .get("hits")
        .unwrap()
        .as_array()
        .unwrap();

    return Ok(Json(
        hits.iter()
            .map(|set| {
                return set.get("_source").unwrap().clone();
            })
            .clone()
            .collect(),
    ));
}

pub fn serve() -> Router {
    return Router::new().route("/api/v1/search", get(search));
}
