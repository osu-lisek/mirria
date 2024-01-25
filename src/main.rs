mod config;
mod osu;
mod crawler;
mod api;
mod ops;

use std::{time::Instant, fs::copy, sync::Arc};

use clap::Parser;
use confy::ConfyError;
use meilisearch_sdk::Client;
use tracing::{info, error, level_filters::LevelFilter};
use tracing_subscriber::util::SubscriberInitExt;

use crate::{config::{Configuration, CONFIG_VERSION, Config}, crawler::Context, osu::client::log_in_using_credentials};
use crate::osu::client::{OsuClient, OsuApi};

async fn ensure_filters(client: Client, index: impl ToString, filters: &[&str]) {
    let filter = client.get_index(index.to_string()).await;

    match filter {
        Ok(filter) => {
            let mut filters_to_add = Vec::new();
            let filter_names = filter.get_filterable_attributes().await.unwrap();
            for &filter_name in filters {
                if !filter_names.contains(&filter_name.to_string()) {
                    filters_to_add.push(filter_name);
                }
            }

            info!("Filterable atrributes of {}: {:#?}", index.to_string(), filter_names);

            if !filters_to_add.is_empty() {
                info!("Updating filters");
                let update_filter_task = filter.set_filterable_attributes(filters).await;
                match update_filter_task {
                    Err(err) => {
                        error!("Failed to run update task, {}", err)
                    },
                    Ok(task) => {
                        info!("Task has been enqueued, id: {}. awaiting", task.task_uid);
                        match task.wait_for_completion(&client, None, None).await {
                            Err(err) => {
                                error!("Failed to run update task, {}", err)
                            },
                            Ok(_) => {
                                info!("Task has been completed");
                            }
                        }
                    }
                }
            }
        },
        Err(_) => {
            
        }
    };
}


async fn ensure_sort(client: Client, index: impl ToString, sort: &[&str]) {
    let filter = client.get_index(index.to_string()).await;

    match filter {
        Ok(filter) => {
            let mut filters_to_add = Vec::new();
            let filter_names = filter.get_sortable_attributes().await.unwrap();
            for &filter_name in sort {
                if !filter_names.contains(&filter_name.to_string()) {
                    filters_to_add.push(filter_name);
                }
            }

            info!("Sortable atrributes of {}: {:#?}", index.to_string(), filter_names);

            if !filters_to_add.is_empty() {
                info!("Updating sortable attributes");
                let update_filter_task = filter.set_sortable_attributes(sort).await;
                match update_filter_task {
                    Err(err) => {
                        error!("Failed to run update task, {}", err)
                    },
                    Ok(task) => {
                        info!("Task has been enqueued, id: {}. awaiting", task.task_uid);
                        match task.wait_for_completion(&client, None, None).await {
                            Err(err) => {
                                error!("Failed to run update task, {}", err)
                            },
                            Ok(_) => {
                                info!("Task has been completed");
                            }
                        }
                    }
                }
            }
        },
        Err(_) => {
            
        }
    };
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
        let response = log_in_using_credentials(configuration.clone(), configuration.osu_username, configuration.osu_password).await;
        
        if response.is_err() {
            error!("Error while creating token");
            error!("{:#?}", response.unwrap_err());
            return;
        }
        let response = response.unwrap();
        access_token = response.access_token;
        refresh_token = response.refresh_token;
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


    ensure_filters(meiliclient.clone(), "beatmapset", &["beatmaps.id", "id", "title", "title_unicode", "beatmaps.checksum", "beatmaps.mode", "status"]).await;
    ensure_filters(meiliclient.clone(), "downloads", &["id"]).await;
    ensure_sort(meiliclient.clone(), "beatmapset", &["id", "title", "title_unicode", "last_updated", "ranked_date", "submitted_date", "play_count"]).await;
    


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
