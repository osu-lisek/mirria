use std::sync::Arc;

use axum::{
    extract::{Query, Request},
    http::StatusCode,
    response::Result,
    routing::get,
    Extension, Json, Router,
};
use serde_derive::{Deserialize, Serialize};

use tracing::error;

use crate::{crawler::Context, osu::types::Beatmapset};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum OsuRuleset {
    #[serde(rename = "osu")]
    Osu,
    #[serde(rename = "taiko")]
    Taiko,
    #[serde(rename = "fruits")]
    Fruits,
    #[serde(rename = "mania")]
    Mania
}

fn serialize_ruleset(ruleset: OsuRuleset) -> String {
    match ruleset {
        OsuRuleset::Osu => "osu",
        OsuRuleset::Taiko => "taiko",
        OsuRuleset::Fruits => "fruits",
        OsuRuleset::Mania => "mania",
    }
    .to_string()
}


#[derive(Deserialize, Debug)]
struct SearchQuery {
    pub query: Option<String>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
    pub statuses: Option<Vec<String>>,
    pub sort: Option<String>,
    pub modes: Option<Vec<OsuRuleset>>,
}

async fn search(
    Extension(ctx): Extension<Arc<Context>>,
    Query(_query): Query<SearchQuery>,
    request: Request,
) -> Result<Json<Vec<Beatmapset>>, StatusCode> {
    let parsed_query =
        serde_qs::from_str(request.uri().query().unwrap_or(""));

    if let Err(_err) = parsed_query {
        return Err(StatusCode::BAD_REQUEST);
    }

    let parsed_query: SearchQuery = parsed_query.unwrap();

    let mapped_statuses = parsed_query
        .statuses
        .unwrap_or(Vec::from(
            ["ranked", "loved", "aproved", "qualified"].map(|x| x.to_string()),
        ))
        .join(", ");
    let modes = parsed_query.modes.unwrap_or(vec![OsuRuleset::Osu, OsuRuleset::Taiko, OsuRuleset::Fruits, OsuRuleset::Mania]).iter().map(
        |x| {
            return format!("(beatmaps.mode = '{}')", serialize_ruleset(x.clone())).to_string();
        },
    ).collect::<Vec<String>>().join(" OR ");

    let sorting = match parsed_query
        .sort
        .unwrap_or("updated_desc".to_string())
        .as_str()
    {
        "updated_asc" => "last_updated:asc",
        "playcount" => "play_count:asc",
        _ => "last_updated:desc",
    };

    let response = ctx
        .meili_client
        .index("beatmapset")
        .search()
        .with_query((parsed_query.query.unwrap_or("".to_string())).as_str())
        .with_filter(format!("(status IN [{}]) AND ({})", mapped_statuses, modes).as_str())
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
        hits.iter().map(|set| set.clone().result).clone().collect(),
    ));
}

pub fn serve() -> Router {
    return Router::new().route("/api/v1/search", get(search));
}
