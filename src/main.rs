mod config;
mod osu;
mod crawler;
mod api;
mod ops;

use std::{time::Instant, fs::copy, sync::Arc};

use clap::Parser;
use confy::ConfyError;
use meilisearch_sdk::client::Client;
use tracing::{info, error, level_filters::LevelFilter};
use tracing_subscriber::util::SubscriberInitExt;

use crate::{config::{Configuration, CONFIG_VERSION, Config}, crawler::Context, osu::client::log_in_using_credentials};
use crate::osu::client::{OsuClient, OsuApi};

async fn ensure_index(client: &Client, index: &str, primary_key: &str) -> bool {
    if client.get_index(index).await.is_ok() {
        return true;
    }

    match client.create_index(index, Some(primary_key)).await {
        Ok(task) => match task.wait_for_completion(client, None, None).await {
            Ok(_) => true,
            Err(error) => {
                error!("Failed to create Meilisearch index {index}: {error}");
                false
            }
        },
        Err(error) => {
            error!("Failed to enqueue Meilisearch index {index}: {error}");
            false
        }
    }
}

async fn ensure_filters(client: &Client, index: &str, required: &[&str]) -> bool {
    let index_handle = match client.get_index(index).await {
        Ok(index) => index,
        Err(error) => {
            error!("Failed to load Meilisearch index {index}: {error}");
            return false;
        }
    };
    let mut attributes = match index_handle.get_filterable_attributes().await {
        Ok(attributes) => attributes,
        Err(error) => {
            error!("Failed to load filterable attributes for {index}: {error}");
            return false;
        }
    };
    let original_len = attributes.len();
    for &attribute in required {
        if !attributes.iter().any(|current| current == attribute) {
            attributes.push(attribute.to_owned());
        }
    }
    if attributes.len() == original_len {
        return true;
    }

    match index_handle.set_filterable_attributes(&attributes).await {
        Ok(task) => match task.wait_for_completion(client, None, None).await {
            Ok(_) => {
                info!("Filterable attributes for {index} are ready");
                true
            }
            Err(error) => {
                error!("Failed to apply filterable attributes for {index}: {error}");
                false
            }
        },
        Err(error) => {
            error!("Failed to enqueue filterable attributes for {index}: {error}");
            false
        }
    }
}

async fn ensure_sort(client: &Client, index: &str, required: &[&str]) -> bool {
    let index_handle = match client.get_index(index).await {
        Ok(index) => index,
        Err(error) => {
            error!("Failed to load Meilisearch index {index}: {error}");
            return false;
        }
    };
    let mut attributes = match index_handle.get_sortable_attributes().await {
        Ok(attributes) => attributes,
        Err(error) => {
            error!("Failed to load sortable attributes for {index}: {error}");
            return false;
        }
    };
    let original_len = attributes.len();
    for &attribute in required {
        if !attributes.iter().any(|current| current == attribute) {
            attributes.push(attribute.to_owned());
        }
    }
    if attributes.len() == original_len {
        return true;
    }

    match index_handle.set_sortable_attributes(&attributes).await {
        Ok(task) => match task.wait_for_completion(client, None, None).await {
            Ok(_) => {
                info!("Sortable attributes for {index} are ready");
                true
            }
            Err(error) => {
                error!("Failed to apply sortable attributes for {index}: {error}");
                false
            }
        },
        Err(error) => {
            error!("Failed to enqueue sortable attributes for {index}: {error}");
            false
        }
    }
}


#[tokio::main]
async fn main() {
    tracing_subscriber::FmtSubscriber::builder()
    .with_level(true)
    .with_max_level(LevelFilter::INFO)
    .with_file(false)
    .with_thread_names(false)
    .finish().init();

    let cfg_path = confy::get_configuration_file_path("mirria", None).unwrap();
    info!("Configuration file path: {}", cfg_path.display());

    let cfg: Result<Configuration, ConfyError> = confy::load("mirria", None);
    
    if cfg.is_err() {
        let err = cfg.unwrap_err();
        match err {
            ConfyError::BadYamlData(err) => {
                let config_file = format!("config.bak.{}", Instant::now().elapsed().as_secs());

                copy(cfg_path, "config.old.yml").expect("Error while copying configuration file");
                let result = confy::store("mirria", None, Configuration::default());
                if result.is_err() {
                    error!("Error while storing configuration");
                    error!("{:#?}", result.unwrap_err());
                    return;
                }
                error!("Configuration version is higher than the current version");
                error!("Old configuration has been copied to {} and default has been stored to config.yml", config_file);
                error!("Error while loading configuration");
                error!("{:#?}", err);
                return;
            },
            _ => {
                error!("Error while loading configuration");
                error!("{:#?}", err);
                return;
            }
        }
    }
    

    let configuration: Configuration = cfg.unwrap();
    
    if configuration.version < CONFIG_VERSION {
        let result = confy::store("config.yml", None, Configuration::default());
        if result.is_err() {
            error!("Error while storing configuration");
            error!("{:#?}", result.unwrap_err());
            return;
        }

        info!("Configuration has been generated, config it and run it again.");
        return;
    }


    if configuration.version > CONFIG_VERSION {
        let config_file = format!("config.bak.{}", Instant::now().elapsed().as_secs());

        copy("config.yml", config_file.clone()).expect("Error while copying configuration file");
        let result = confy::store_path("config.yml", Configuration::default());
        if result.is_err() {
            error!("Error while storing configuration");
            error!("{:#?}", result.unwrap_err());
            return;
        }
        error!("Configuration version is higher than the current version");
        error!("Old configuration has been copied to {} and default has been stored to config.yml", config_file);
        return;
    }

    info!("Configuration has been loaded");


    let mut access_token = configuration.osu_access_token.clone();
    let mut refresh_token = configuration.osu_refresh_token.clone();

    if !configuration.has_authorization() {
        info!("Creating token");
        let configuration: Configuration = configuration.clone();
        let response = log_in_using_credentials(configuration.clone()).await;
        
        if response.is_err() {
            error!("Error while creating token");
            error!("{:#?}", response.unwrap_err());
            return;
        }
        let response = response.unwrap();
        access_token = response.access_token;
        info!("Token has been created");
    }

    let osu_client = OsuClient::from_tokens(configuration.clone(), access_token, refresh_token).await;    
    
    if osu_client.is_err() {
        error!("Error while creating osu client");
        error!("{:#?}", osu_client.unwrap_err());
        return;
    }

    info!("Client has been initialized");
    let meiliclient = Client::new(configuration.clone().meilisearch.url, Some(configuration.clone().meilisearch.key));

    if meiliclient.is_err() {
        error!("Error while creating meilisearch client");
        error!("{:#?}", meiliclient.unwrap_err());
        return;
    }

    let meiliclient = meiliclient.unwrap();

    if !ensure_index(&meiliclient, "beatmapset", "id").await
        || !ensure_index(&meiliclient, "downloads", "id").await
    {
        return;
    }
    if !ensure_filters(
        &meiliclient,
        "beatmapset",
        &[
            "beatmaps.id",
            "id",
            "title",
            "title_unicode",
            "beatmaps.checksum",
            "beatmaps.mode",
            "status",
        ],
    )
    .await
        || !ensure_filters(&meiliclient, "downloads", &["id"]).await
        || !ensure_sort(
            &meiliclient,
            "beatmapset",
            &[
                "id",
                "title",
                "title_unicode",
                "last_updated",
                "ranked_date",
                "submitted_date",
                "play_count",
            ],
        )
        .await
        || !ensure_sort(&meiliclient, "downloads", &["count", "date", "id"]).await
    {
        error!("Required Meilisearch settings could not be applied");
        return;
    }
    


    info!("Meiliclient is up and running");

    let context = Context {
        config: Arc::new(configuration.clone()),
        meili_client: Arc::new(meiliclient),
        osu: osu_client.unwrap()
    };

    let configuration_env: Config = Config::parse();

    match configuration_env.app_component.as_str() {
        "crawler" => crawler::serve(context).await,
        "api" => api::serve(context).await,
        _ => error!("Unknown component")
    }
}
