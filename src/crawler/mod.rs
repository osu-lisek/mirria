use std::{sync::Arc, time::{Duration, Instant}};

use meilisearch_sdk::Client;
use tokio::{sync::Mutex, time};
use tracing::{error, info, warn};

use crate::{
    config::Configuration,
    osu::client::{OsuApi, OsuClient},
};

#[derive(Clone, Debug)]
pub struct Context {
    pub config: Arc<Configuration>,
    pub meili_client: Arc<Client>,
    pub osu: OsuClient
}


async fn crawl_search(context: Mutex<Context>) {
    let cursor = Mutex::new(String::new());
    *cursor.lock().await = context.lock().await.config.cursor.clone();
    let mut last_save = Instant::now();

    loop {
        let mut context = context.lock().await;
        let mut cursor = cursor.lock().await;
        info!("Crawling beatmaps with cursor {}", cursor);

        let beatmaps = context
            .osu
            .search_beatmapsets(
                true,
                String::from("updated_asc"),
                String::from("any"),
                Some(cursor.to_owned()),
            )
            .await;
        //Setting new cursor

        if !beatmaps.is_some() {
            warn!("Failed to crawl maps, gonna retry in 1 minute.");
            let _ = time::sleep(Duration::from_secs(60)).await;
            continue;
        }

        let beatmaps = beatmaps.unwrap();


        if Instant::now().duration_since(last_save) > Duration::from_secs(30) {
            last_save = Instant::now();
            let mut config: Configuration = confy::load("mirria", None).unwrap();
            config.cursor = cursor.to_string();
            confy::store("mirria", None, config).unwrap();
            info!("Saving cursor to config");
        }

        let crawled_beatmaps = beatmaps.beatmapsets;
        info!("Crawled {} beatmaps", crawled_beatmaps.len());
     
        let index = context.meili_client.index("beatmapset");

        let result = index.add_documents(&crawled_beatmaps.to_vec(), Some("id")).await;
        if result.is_err() {
            error!("{}", result.err().unwrap());
            break;
        }

        if crawled_beatmaps.len() < 50 {
            info!("End of search reached, waiting 3 minutes for new beatmaps");
            let _ = time::sleep(Duration::from_secs(60*3)).await;
            continue;
        }

        if let Some(beatmap_cursor) = beatmaps.cursor_string {
            *cursor = beatmap_cursor;
        }

        let _ = time::sleep(Duration::from_secs(3)).await;
    }
}

pub async fn serve(context: Context) {
    let crawler_ctx = Mutex::new(context.clone());

    let _ = tokio::try_join!(tokio::spawn(async move {
        crawl_search(crawler_ctx).await
    }));
}
