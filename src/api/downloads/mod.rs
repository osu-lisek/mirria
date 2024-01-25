use std::{sync::Arc, path::Path as path_sys};

use axum::{extract::Path, Extension, Router, routing::get, response::Response, body::Body};
use chrono::{DateTime, Local};
use serde_json::json;
use tokio::{io::AsyncReadExt, sync::Mutex};
use tracing::{error, info};

use crate::{crawler::Context, osu::client::OsuApi, ops::{beatmapset::get_beatmapset_by_id, DownloadIndex}};

async fn create_new_index(ctx: Context, id: i64) -> Option<DownloadIndex> {
    let download_index = Some(DownloadIndex { id: id, date: Local::now().timestamp()});
    if let Err(err) = ctx.meili_client.index("downloads").add_documents(&[download_index.clone().unwrap()], Some("id")).await {
        error!("Failed to create index: {}", err);
    }else{
        info!("Created new index: {:#?}", download_index);
    }

    download_index
}

async fn get_index_or_create(ctx: Context, id: i64) -> Option<DownloadIndex> {
    let index_response = ctx.meili_client
    .index("downloads")
    .search()
    .with_filter(format!("id = {}", id).as_str())
    .execute::<DownloadIndex>()
    .await;
    


    if let Err(err) = index_response {
        //Index not found, creating inserting index
        let download_index = create_new_index(ctx, id).await;
        error!("Failed to get index: {}, created new one: {:#?}", err, download_index);

        return download_index
    }

    let index = index_response.unwrap();

    if index.hits.len() < 1 {
        //Creating new index
        let download_index = create_new_index(ctx.clone(), id).await;

        return download_index;
    }

    //Unpacking first index and putting it in download_index
    if let Some(index) = index.hits.first() {
        return Some(index.result.clone());
    }

    None
}

async fn download(
    Extension(ctx): Extension<Arc<Mutex<Context>>>,
    Path(id): Path<i64>
) -> Response {

    let mut ctx = ctx.lock().await;
    let config = ctx.config.clone();
    let mut redownload_required = false;

    let index = get_index_or_create(ctx.to_owned(), id).await;
    if index.is_none() {
        Response::builder().status(500).body(Body::from(json!({"ok": false, "message": "Internal database exception"}).to_string())).unwrap();
    }
   
    let beatmapset = get_beatmapset_by_id(ctx.to_owned(), id).await;

    match beatmapset {
        Ok(set) => {
            //parsing time from last_updated field
            let date = DateTime::parse_from_rfc3339(&set.last_updated);
            if date.is_err() {
                return Response::builder().body(Body::from(json!({"ok": false, "message": "Failed to parse date"}).to_string())).unwrap();
            }
            let date = date.unwrap();
            let last_updated = date.timestamp();

            //if last updated is bigger than last download date
            if last_updated > index.unwrap().date {
                info!("Redownloading {}, it is too old", id);

                redownload_required = true;
            }
        }
        _ => {}
    }

    if ctx.osu.download_if_not_exists(id.clone(), config.beatmaps_folder.clone(), redownload_required).await.is_err() {
        return Response::builder().body(Body::from(json!({"ok": false, "message": "Failed to download file"}).to_string())).unwrap();
    }

    let data_folder = path_sys::new(ctx.config.beatmaps_folder.as_str());
    let mut file = match tokio::fs::File::open(data_folder.join(format!("{}.osz", id))).await {
        Ok(file) => file,
        Err(err) => {
            error!("Failed to open file: {}", err);
            return Response::builder().body(Body::from(json!({"ok": false, "message": "Failed to download file"}).to_string())).unwrap()
        },
    };

    let mut content = Vec::new();
    file.read_to_end(&mut content).await.unwrap();
    
    let mut file_name = format!("{}.osz", id);    
    let beatmapset = get_beatmapset_by_id(ctx.to_owned(), id).await;
    
    if let Ok(beatmapset) = beatmapset {
        file_name = format!("{} {} - {}.osz", beatmapset.id, beatmapset.artist, beatmapset.title);
    }

    Response::builder()
    .header("Content-Type", "application/x-osu-beatmap-archive")
    .header("Content-Disposition", format!("attachment; filename={}", file_name))
    .body(Body::from(content))
    .unwrap()    
}


pub fn serve() -> Router {
    return Router::new()
    .route("/api/v1/download/:id", get(download));
}
