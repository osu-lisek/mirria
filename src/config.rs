use serde_derive::{Serialize, Deserialize};

pub const CONFIG_VERSION: i32 = 3;

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct Meili {
    pub url: String,
    pub key: String
}


#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Configuration {
    pub version: i32,
    pub osu_username: String,
    pub osu_password: String,
    pub osu_access_token: String,
    pub osu_refresh_token: String,
    pub osu_token_expires_at: u64,
    pub cursor: String,
    pub meilisearch: Meili,
    pub beatmaps_folder: String
}


impl ::std::default::Default for Configuration  {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            osu_username: String::new(),
            osu_password: String::new(),
            osu_access_token: String::new(),
            osu_refresh_token: String::new(),
            osu_token_expires_at: 0,
            cursor: String::new(),
            meilisearch: Default::default(),
            beatmaps_folder: String::new()
        }
    }
}

#[derive(clap::Parser, Clone)]
pub struct Config {
    #[clap(long, env)]
    pub app_component: String,
}