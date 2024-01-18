mod config;
mod osu;
mod crawler;
mod api;

use std::{time::{Instant, UNIX_EPOCH, SystemTime}, fs::copy, sync::Arc};

use clap::Parser;
use confy::ConfyError;
use elasticsearch::{http::transport::Transport, Elasticsearch};
use tracing::{info, error, level_filters::LevelFilter};
use tracing_subscriber::util::SubscriberInitExt;

use crate::{config::{Configuration, CONFIG_VERSION, Config}, crawler::Context};
use crate::osu::client::{OsuClient, OsuApi};

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


    let mut client: Option<OsuClient> = None;

    if configuration.osu_access_token == "" || configuration.osu_refresh_token == "" {
        info!("Creating token");
        let configuration: Configuration = configuration.clone();
        client = Some(OsuClient::from_credentials(configuration.clone(), configuration.osu_username, configuration.osu_password).await.await.unwrap());
    }

    let configuration: Configuration = configuration.clone();
    client = Some(OsuClient::from_tokens(configuration.clone(), configuration.clone().osu_access_token, configuration.clone().osu_refresh_token).await);


    info!("Client has been initialized");
    let transport = Transport::single_node(&configuration.clone().elasticsearch_url).unwrap();

    let elastic_client = Elasticsearch::new(transport);

    info!("Elasticsearch client has been initialized");

    let ping_response = elastic_client.ping().send().await;
    if ping_response.is_err() {
        error!("Error while pinging Elasticsearch");
        return;
    }
    info!("Elasticsearch is up and running");

    let context = Context {
        config: Arc::new(configuration.clone()),
        elasticsearch: Arc::new(elastic_client),
        osu: client.unwrap()
    };

    let configuration_env: Config = Config::parse();

    match configuration_env.app_component.as_str() {
        "crawler" => crawler::serve(context).await,
        "api" => api::serve(context).await,
        _ => error!("Unknown component")
    }
}
