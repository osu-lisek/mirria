use std::{sync::Arc, env::join_paths, path::Path as path_sys};

use axum::{extract::Path, Extension, Router, routing::get, response::{IntoResponse, Response}, body::Body};
use serde_json::json;
use tokio::io::AsyncReadExt;
use tracing::error;

use crate::{crawler::Context, osu::client::OsuApi};



async fn download(
    Extension(ctx): Extension<Arc<Context>>,
    Path(id): Path<String>
) -> Response {

    if ctx.osu.download_if_not_exists(id.clone(), ctx.config.clone().beatmaps_folder.clone()).await.is_err() {
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
    
    Response::builder()
    .header("Content-Type", "application/x-osu-beatmap-archive")
    .header("Content-Disposition", format!("attachment; filename={}.osr", id))
    .body(Body::from(content))
    .unwrap()    
}


pub fn serve() -> Router {
    return Router::new()
    .route("/api/v1/download/:id", get(download));
}
