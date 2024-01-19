use std::{sync::Arc, time::{Duration, Instant}};

use meilisearch_sdk::Client;
use tokio::{sync::Mutex, time};
use tracing::{info, error};

use crate::{
    config::Configuration,
    osu::client::{OsuApi, OsuClient},
};

pub struct Context {
    pub config: Arc<Configuration>,
    pub meili_client: Arc<Client>,
    pub osu: OsuClient,
}

// async fn create_index_if_not_exists(context: &Context, index: &str) {
//     let exists = context.elasticsearch.indices().exists(IndicesExistsParts::Index(&[index])).send().await;
//     if exists.is_err() {
//         let _ = context.elasticsearch.indices().create(IndicesCreateParts::Index(index)).send().await;
//     }
// }

async fn crawl_search(context: &Context) {
    // create_index_if_not_exists(context, "beatmapsets").await;
    // create_index_if_not_exists(context, "beatmaps").await;

    let cursor = Mutex::new(String::new());
    *cursor.lock().await = context.config.cursor.clone();
    let mut last_save = Instant::now();
    //making paralel loop to update it in config
    // tokio::spawn(async move {
    //     loop {
    //         let mut config: Configuration = confy::load("mirria", None).unwrap();
    //         config.cursors.graveyard = cursor.lock().await.clone().to_string();
    //     }
    // });

    loop {
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
            break;
        }

        let beatmaps = beatmaps.unwrap();
        *cursor = beatmaps.cursor_string.to_string();

        if Instant::now().duration_since(last_save) > Duration::from_secs(30) {
            last_save = Instant::now();
            let mut config: Configuration = confy::load("mirria", None).unwrap();
            config.cursor = cursor.to_string();
            confy::store("mirria", None, config).unwrap();
            info!("Saving cursor to config");
        }
        let _cursor_string = beatmaps.cursor_string.to_string();

        let crawled_beatmaps = beatmaps.beatmapsets;
        info!("Crawled {} beatmaps", crawled_beatmaps.len());
     
        let index = context.meili_client.index("beatmapset");

        let result = index.add_documents(&crawled_beatmaps.to_vec(), Some("id")).await;
        if result.is_err() {
            error!("{}", result.err().unwrap());
            break;
        }

        let _ = time::sleep(Duration::from_secs(10)).await;
    }
}

pub async fn serve(context: Context) {
    let ctx_arc = Arc::new(context);
    let crawler_ctx = ctx_arc.clone();

    let _ = tokio::try_join!(tokio::spawn(async move {
        crawl_search(crawler_ctx.as_ref()).await
    }));
}
