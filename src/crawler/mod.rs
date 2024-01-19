use std::{sync::Arc, time::{Duration, Instant}};

use elasticsearch::{Elasticsearch, indices::{IndicesCreateParts, IndicesExistsParts}};
use tokio::{sync::Mutex, time};
use tracing::{info, warn};

use crate::{
    config::Configuration,
    osu::client::{OsuApi, OsuClient},
};

pub struct Context {
    pub config: Arc<Configuration>,
    pub elasticsearch: Arc<Elasticsearch>,
    pub osu: OsuClient,
}

async fn create_index_if_not_exists(context: &Context, index: &str) {
    let exists = context.elasticsearch.indices().exists(IndicesExistsParts::Index(&[index])).send().await;
    if exists.is_err() {
        let _ = context.elasticsearch.indices().create(IndicesCreateParts::Index(index)).send().await;
    }
}

async fn crawl_search(context: &Context) {
    create_index_if_not_exists(context, "beatmapsets").await;
    create_index_if_not_exists(context, "beatmaps").await;

    let cursor = Mutex::new(String::new());
    *cursor.lock().await = context.config.cursors.graveyard.clone();
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
            config.cursors.graveyard = cursor.to_string();
            confy::store("mirria", None, config).unwrap();
            info!("Saving cursor to config");
        }
        let cursor_string = beatmaps.cursor_string.to_string();

        let crawled_beatmaps = beatmaps.beatmapsets;
        info!("Crawled {} beatmaps", crawled_beatmaps.len());
        info!("{}", cursor_string);
        // info!("{}", last_beatmap);
        //Putting all beatmaps in elastic search

        // crawled_beatmaps.iter().for_each(|beatmapset| async {
        //     let response = context.elasticsearch.index(elasticsearch::IndexParts::Index("beatmapset")).body(beatmapset).send().await;
            
        //     if response.is_err() {
        //         let err = response.err().unwrap();
        //         info!("{}", err);
        //         return;
        //     }

        //     let response = response.unwrap();
        //     info!("{:#?}", response.status_code());
        //     beatmapset.beatmaps.iter().for_each(|beatmap| async {
        //         let _ = context.elasticsearch.index(elasticsearch::IndexParts::Index("beatmap")).body(beatmap).send().await;
        //     });
        // });
        for set in crawled_beatmaps {
            let response = context.elasticsearch.index(elasticsearch::IndexParts::Index("beatmapset")).body(&set).send().await;
                     
            if response.is_err() {
                let err = response.err().unwrap();
                info!("{}", err);
                return;
            }

            for map in set.beatmaps {
                if map.checksum.is_none() {
                    warn!("Beatmap {} has null checksum, insertion of this beatmap will be ignored", map.id);
                    continue;
                }
                let _ = context.elasticsearch.index(elasticsearch::IndexParts::Index("beatmap")).body(&map).send().await;
            }
        }

        let _ = time::sleep(Duration::from_secs(15));
    }
}

pub async fn serve(context: Context) {
    let ctx_arc = Arc::new(context);
    let crawler_ctx = ctx_arc.clone();

    let _ = tokio::try_join!(tokio::spawn(async move {
        crawl_search(crawler_ctx.as_ref()).await
    }));
}
