use std::{
    borrow::BorrowMut,
    fs::write,
    io::{Error, ErrorKind},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};


use serde_derive::Deserialize;
use tokio::fs::File;
use tracing::{error, info};


use crate::{config::Configuration};

use super::types::SearchResponse;

#[derive(Debug, Clone)]
pub struct OsuClient {
    access_token: String,
    refresh_token: String,
    token_expires_at: u64,
}

#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub expires_in: u64,
    pub refresh_token: String,
}

#[derive(Debug, Deserialize)]
pub struct UserResponse {
    pub username: String,
    pub id: i32,
}

pub trait OsuApi {
    async fn from_tokens(
        config: Configuration,
        access_token: String,
        refresh_token: String,
    ) -> Result<OsuClient, Error>;
    async fn refresh_token(&mut self, config: Configuration) -> Result<bool, Error>;
    async fn search_beatmapsets(
        &self,
        nsfw: bool,
        sort: String,
        status: String,
        cursor_string: Option<String>,
    ) -> Option<SearchResponse>;

    async fn download_if_not_exists(
        &self,
        id: i64,
        path_to_beatmaps: String,
        force: bool
    ) -> Result<Vec<u8>, Error>;
    async fn fetch_user(&self) -> Result<UserResponse, Error>;
}

pub async fn log_in_using_credentials(
    config: Configuration,
    username: String,
    password: String,
) -> Result<TokenResponse, Error> {
    let client = reqwest::Client::new();

    let form = reqwest::multipart::Form::new()
        .text("grant_type", "password")
        .text("username", username)
        .text("password", password)
        .text("client_id", "5")
        .text("client_secret", "FGc9GAtyHzeQDshWP5Ah7dega8hJACAJpQtw6OXk")
        .text("scope", "*");

    let response = client
        .post("https://osu.ppy.sh/oauth/token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .multipart(form)
        .send()
        .await
        .unwrap();

    let resp = response.json::<TokenResponse>().await.unwrap();

    let mut new_config = config.clone().borrow_mut().to_owned();
    new_config.osu_access_token = resp.access_token.clone();
    new_config.osu_refresh_token = resp.refresh_token.clone();
    new_config.osu_token_expires_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + resp.expires_in;

    confy::store("mirria", None, new_config).expect("Error while saving config.");

    Ok(resp)
}

impl OsuApi for OsuClient {
    async fn fetch_user(&self) -> Result<UserResponse, Error> {
        let client = reqwest::Client::new();

        let response = client
            .get("https://osu.ppy.sh/api/v2/me")
            .header("Accept", "application/json")
            .bearer_auth(self.clone().access_token)
            .send()
            .await;

        if let Err(_err) = response {
            return Err(Error::new(ErrorKind::Other, "Error while fetching current user."))
        }

        let response = response.unwrap();

        let user = response.json::<UserResponse>().await;

        if user.is_err() {
            return Err(Error::new(ErrorKind::Other, "Looks like osu! servers down, or did html instead of json."))
        }

        let user = user.unwrap();
        Ok(user)
    }

    async fn from_tokens(
        config: Configuration,
        access_token: String,
        refresh_token: String,
    ) -> Result<OsuClient, Error> {
        if config.osu_token_expires_at
            < SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
        {
            let mut client = OsuClient {
                access_token: access_token,
                refresh_token: refresh_token,
                token_expires_at: config.osu_token_expires_at,
            };

            let is_success = client.refresh_token(config.clone()).await;

            if is_success.is_err() {
                error!("Failed to refresh token");
                return Err(Error::new(ErrorKind::Other, "Failed to refresh tokens"));
            }

            let user = client.fetch_user().await;

            if user.is_err() {
                return Err(Error::new(ErrorKind::Other, "Failed to fetch user"));
            }

            let user = user.unwrap();

            info!("Logged in as {} (https://osu.ppy.sh/users/{})", user.username, user.id);

            return Ok(client);
        }

        //Validating tokens

        let client = OsuClient {
            access_token: String::from(access_token),
            refresh_token: String::from(refresh_token),
            token_expires_at: config.osu_token_expires_at,
        };


        let user = client.fetch_user().await;

        if user.is_err() {
            return Err(Error::new(ErrorKind::Other, "Failed to fetch user"));
        }

        let user = user.unwrap();

        info!("Logged in as {} (https://osu.ppy.sh/users/{})", user.username, user.id);

        Ok(client)
    }

    async fn refresh_token(&mut self, config: Configuration) -> Result<bool, Error> {
        let client = reqwest::Client::new();

        let form = reqwest::multipart::Form::new()
            .text("grant_type", "refresh_token")
            .text("refresh_token", self.clone().refresh_token)
            .text("client_id", "5")
            .text("client_secret", "FGc9GAtyHzeQDshWP5Ah7dega8hJACAJpQtw6OXk")
            .text("scope", "*");

        let response = client
            .post("https://osu.ppy.sh/oauth/token")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "application/json")
            .multipart(form)
            .send()
            .await
            .unwrap();

        if !response.status().is_success() {
            return Err(Error::new(ErrorKind::Other, "Failed to refresh token"));
        }
        let resp = response.json::<TokenResponse>().await.unwrap();

        let mut new_config = config.clone().borrow_mut().to_owned();
        new_config.osu_access_token = resp.access_token.clone();
        new_config.osu_refresh_token = resp.refresh_token.clone();
        new_config.osu_token_expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + resp.expires_in;

        confy::store("mirria", None, new_config).expect("Error while saving config.");

        self.access_token = resp.access_token;
        self.refresh_token = resp.refresh_token;
        self.token_expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + resp.expires_in;

        info!("Token refreshed.");

        Ok(true)
    }

    async fn search_beatmapsets(
        &self,
        nsfw: bool,
        sort: String,
        status: String,
        cursor_string: Option<String>,
    ) -> Option<SearchResponse> {
        let client = reqwest::Client::new();

        let response = client
            .get("https://osu.ppy.sh/api/v2/beatmapsets/search")
            .query(&[
                ("nsfw", nsfw.to_string()),
                ("sort", sort),
                ("s", status),
                ("cursor_string", cursor_string.unwrap_or(String::new())),
            ])
            .bearer_auth(self.clone().access_token)
            .send()
            .await
            .unwrap();

        // let serialization_response = response.json::<SearchResponse>().await;
        let text = response.text().await.unwrap();
        let jd: &mut serde_json::Deserializer<serde_json::de::StrRead<'_>> =
            &mut serde_json::Deserializer::from_str(text.as_str());

        let result: Result<SearchResponse, _> = serde_path_to_error::deserialize(jd);
        match result {
            Ok(v) => return Some(v),
            Err(err) => {
                let path = err.path().to_string();
                error!("Failed to parse json, here path: {}", path);
                return None;
            }
        }
        // Some(serialization_response.unwrap())
    }
    async fn download_if_not_exists(
        &self,
        id: i64,
        path_to_beatmaps: String,
        force: bool
    ) -> Result<Vec<u8>, Error> {
        let data_folder = Path::new(path_to_beatmaps.as_str());
        let path_to_save = data_folder.join(format!("{}.osz", id));

        let file = File::open(path_to_save.clone()).await;

        if file.is_ok() && !force {
            return Ok(Vec::new())
        }
   
        let client = reqwest::Client::new();

        let response = client
            .get(format!(
                "https://osu.ppy.sh/api/v2/beatmapsets/{}/download",
                id
            ))
            .bearer_auth(self.clone().access_token)
            .send()
            .await
            .unwrap();

        info!("got response");

        if !response.status().is_success() {
            let status = response.status().as_u16();
            error!("Invalid status: {}", status);
            return Err(Error::new(
                ErrorKind::Other,
                "Error while downloading file.",
            ));
        }

        let bytes = response.bytes().await.unwrap();
        //Saving it to data folder

        let result = write(path_to_save, bytes.clone());
        if result.is_err() {
            error!("Failed to save beatmap: {:#?}", result.unwrap_err());
        }
        Ok(bytes.to_vec())
    }
}
