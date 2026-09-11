use std::{fmt::Write, sync::Arc};

use axum::{
    extract::{rejection::QueryRejection, Query},
    http::StatusCode,
    response::Html,
    routing::get,
    Extension, Json, Router,
};
use meilisearch_sdk::documents::DocumentsQuery;
use serde_derive::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tracing::warn;

use crate::{api::downloads::SmartCache, crawler::Context};

const METADATA_FIELDS: [&str; 3] = ["id", "title", "artist"];

type ApiError = (StatusCode, Json<Value>);

#[derive(Deserialize)]
struct CacheQuery {
    limit: Option<usize>,
}

#[derive(Serialize)]
struct Storage {
    used_bytes: usize,
    reserved_bytes: usize,
    total_bytes: usize,
    available_bytes: usize,
}

#[derive(Serialize)]
struct CacheSummary {
    map_count: usize,
    entry_count: usize,
    size_bytes: usize,
}

#[derive(Deserialize, Serialize)]
struct MapMetadata {
    id: i64,
    title: Option<String>,
    artist: Option<String>,
}

#[derive(Serialize)]
struct CachedMap {
    #[serde(flatten)]
    metadata: MapMetadata,
    download_count: u64,
    size_bytes: usize,
}

#[derive(Serialize)]
struct CacheResponse {
    ranking: &'static str,
    storage: Storage,
    cache: CacheSummary,
    metadata_available: bool,
    maps: Vec<CachedMap>,
}

fn api_error(status: StatusCode, message: &'static str) -> ApiError {
    (status, Json(json!({ "ok": false, "message": message })))
}

async fn cache(
    Extension(context): Extension<Arc<Mutex<Context>>>,
    Extension(smart_cache): Extension<Arc<SmartCache>>,
    query: Result<Query<CacheQuery>, QueryRejection>,
) -> Result<Json<CacheResponse>, ApiError> {
    let Query(query) = query.map_err(|_| {
        api_error(
            StatusCode::BAD_REQUEST,
            "limit must be an integer between 1 and 100",
        )
    })?;
    let limit = query.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "limit must be an integer between 1 and 100",
        ));
    }

    let mut snapshot = smart_cache.snapshot();
    let map_count = snapshot.maps.len();
    snapshot.maps.sort_unstable_by(|left, right| {
        right
            .download_count
            .cmp(&left.download_count)
            .then_with(|| left.id.cmp(&right.id))
    });
    snapshot.maps.truncate(limit);
    let mut maps: Vec<CachedMap> = snapshot
        .maps
        .into_iter()
        .map(|map| CachedMap {
            metadata: MapMetadata {
                id: map.id,
                title: None,
                artist: None,
            },
            download_count: map.download_count,
            size_bytes: map.size_bytes,
        })
        .collect();

    let mut metadata_available = true;
    if !maps.is_empty() {
        let client = {
            let context = context.lock().await;
            Arc::clone(&context.meili_client)
        };
        let index = client.index("beatmapset");
        let mut filter = String::with_capacity(maps.len() * 20 + 8);
        filter.push_str("id IN [");
        for (position, map) in maps.iter().enumerate() {
            if position != 0 {
                filter.push(',');
            }
            write!(&mut filter, "{}", map.metadata.id).expect("writing a cache ID to a String");
        }
        filter.push(']');

        match DocumentsQuery::new(&index)
            .with_filter(&filter)
            .with_fields(METADATA_FIELDS)
            .with_limit(maps.len())
            .execute::<MapMetadata>()
            .await
        {
            Ok(mut documents) => {
                documents
                    .results
                    .sort_unstable_by_key(|metadata| metadata.id);
                for map in &mut maps {
                    if let Ok(position) = documents
                        .results
                        .binary_search_by_key(&map.metadata.id, |metadata| metadata.id)
                    {
                        let metadata = &mut documents.results[position];
                        map.metadata.title = metadata.title.take();
                        map.metadata.artist = metadata.artist.take();
                    }
                }
            }
            Err(err) => {
                warn!(error = %err, "Failed to retrieve cached map metadata");
                metadata_available = false;
            }
        }
    }

    Ok(Json(CacheResponse {
        ranking: "download_count",
        storage: Storage {
            used_bytes: snapshot.used_bytes,
            reserved_bytes: snapshot.reserved_bytes,
            total_bytes: snapshot.total_bytes,
            available_bytes: snapshot
                .total_bytes
                .saturating_sub(snapshot.used_bytes)
                .saturating_sub(snapshot.reserved_bytes),
        },
        cache: CacheSummary {
            map_count,
            entry_count: snapshot.entry_count,
            size_bytes: snapshot.used_bytes,
        },
        metadata_available,
        maps,
    }))
}

async fn page() -> Html<&'static str> {
    Html(include_str!("index.html"))
}

pub fn serve() -> Router {
    Router::new()
        .route("/api/v1/cache", get(cache))
        .route("/cache", get(page))
}
