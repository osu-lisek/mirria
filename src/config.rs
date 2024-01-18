use serde_derive::{Serialize, Deserialize};

pub const CONFIG_VERSION: i32 = 3;

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct Cursors {
    pub graveyard: String,
    pub ranked: String,
    pub loved: String,
    pub approved: String,
    pub qualified: String
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Configuration {
    pub version: i32,
    pub osu_username: String,
    pub osu_password: String,
    pub osu_access_token: String,
    pub osu_refresh_token: String,
    pub osu_token_expires_at: u64,
    pub cursors: Cursors,
    pub elasticsearch_url: String,
    pub beatmaps_folder: String
}


impl ::std::default::Default for Configuration  {
    fn default() -> Self {
        Self { version: CONFIG_VERSION, osu_username: Default::default(), osu_password: Default::default(), osu_access_token: Default::default(), osu_refresh_token: Default::default(), cursors: Default::default(), osu_token_expires_at: Default::default(), elasticsearch_url: Default::default(), beatmaps_folder: Default::default() }
    }
}

#[derive(clap::Parser, Clone)]
pub struct Config {
    #[clap(long, env)]
    pub app_component: String,
}