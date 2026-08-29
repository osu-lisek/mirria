use std::{
    borrow::BorrowMut,
    io::{Error, ErrorKind},
    sync::atomic::{AtomicBool, Ordering},
};

use chrono::Local;
use confy::ConfyError;
use reqwest::StatusCode;
use serde_derive::Deserialize;
use tracing::{error, info};

use crate::config::Configuration;

use super::types::SearchResponse;

static DOWNLOAD_TOKEN_REFRESHING: AtomicBool = AtomicBool::new(false);
static DOWNLOAD_TOKEN_REFRESHED: tokio::sync::Notify = tokio::sync::Notify::const_new();

struct DownloadTokenRefreshGuard;

impl Drop for DownloadTokenRefreshGuard {
    fn drop(&mut self) {
        DOWNLOAD_TOKEN_REFRESHING.store(false, Ordering::Release);
        DOWNLOAD_TOKEN_REFRESHED.notify_waiters();
    }
}

#[derive(Debug, Clone)]
pub struct OsuClient {
    access_token: String,
    refresh_token: String,
    token_expires_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub expires_in: i64,
    pub token_type: String,
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
        &mut self,
        nsfw: bool,
        sort: String,
        status: String,
        cursor_string: Option<String>,
    ) -> Option<SearchResponse>;

    async fn begin_download(&mut self, id: i64, video: bool) -> Result<reqwest::Response, Error>;
    async fn fetch_user(&self) -> Result<UserResponse, Error>;

    async fn refresh_token_if_required(&mut self) -> bool;

    fn load_config(self) -> Result<Configuration, ConfyError>;
}

pub async fn log_in_using_credentials(config: Configuration) -> Result<TokenResponse, Error> {
    let client = reqwest::Client::new();
    let params = [
        ("grant_type", "password"),
        ("client_id", "5"),
        ("client_secret", "FGc9GAtyHzeQDshWP5Ah7dega8hJACAJpQtw6OXk"),
        ("username", &config.osu_username),
        ("password", &config.osu_password),
        ("scope", "*"),
    ];

    let response = client
        .post("https://osu.ppy.sh/oauth/token")
        .header("Accept", "application/json")
        .form(&params)
        .send()
        .await
        .unwrap();

    if response.status() != StatusCode::OK {
        return Err(Error::new(
            ErrorKind::Other,
            format!(
                "Error to create token, response: {}",
                response.text().await.unwrap()
            ),
        ));
    }

    let resp = response.json::<TokenResponse>().await.unwrap();

    let mut new_config = config.clone().borrow_mut().to_owned();
    new_config.osu_access_token = resp.access_token.clone();
    new_config.osu_refresh_token = resp.refresh_token.clone();
    new_config.osu_token_expires_at = Local::now().timestamp() + resp.expires_in;

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
            return Err(Error::new(
                ErrorKind::Other,
                "Error while fetching current user.",
            ));
        }

        let response = response.unwrap();

        let user = response.json::<UserResponse>().await;

        if user.is_err() {
            return Err(Error::new(
                ErrorKind::Other,
                "Looks like osu! servers down, or did html instead of json.",
            ));
        }

        let user = user.unwrap();
        Ok(user)
    }

    async fn from_tokens(
        config: Configuration,
        access_token: String,
        refresh_token: String,
    ) -> Result<OsuClient, Error> {
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

        info!("Logged in as {}!", user.username);

        Ok(client)
    }

    async fn refresh_token(&mut self, config: Configuration) -> Result<bool, Error> {
        let client = reqwest::Client::new();

        let form = [
            ("grant_type", "refresh_token"),
            ("refresh_token", &config.osu_refresh_token),
            ("client_id", "5"),
            ("client_secret", "FGc9GAtyHzeQDshWP5Ah7dega8hJACAJpQtw6OXk"),
            ("scope", "*"),
        ];

        let response = client
            .post("https://osu.ppy.sh/oauth/token")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "application/json")
            .form(&form)
            .send()
            .await
            .map_err(Error::other)?;

        if !response.status().is_success() {
            return Err(Error::other("Failed to refresh token"));
        }
        let resp = response
            .json::<TokenResponse>()
            .await
            .map_err(Error::other)?;

        let mut new_config = config.clone().borrow_mut().to_owned();
        new_config.osu_access_token = resp.access_token.clone();
        new_config.osu_refresh_token = resp.refresh_token.clone();
        new_config.osu_token_expires_at = Local::now().timestamp() + resp.expires_in;

        confy::store("mirria", None, new_config)
            .map_err(|error| Error::other(format!("Failed to save refreshed token: {error}")))?;

        self.access_token = resp.access_token;
        self.refresh_token = resp.refresh_token;
        self.token_expires_at = Local::now().timestamp() + resp.expires_in;

        info!("Token refreshed.");

        Ok(true)
    }

    async fn search_beatmapsets(
        &mut self,
        nsfw: bool,
        sort: String,
        status: String,
        cursor_string: Option<String>,
    ) -> Option<SearchResponse> {
        if self.clone().refresh_token_if_required().await {
            let config = self.clone().load_config();
            if let Err(error) = config {
                error!("Error while reloading config: {}", error);
                return None;
            }

            let config = config.unwrap();

            self.access_token = config.osu_access_token;
            self.refresh_token = config.osu_refresh_token;
            self.token_expires_at = config.osu_token_expires_at;
        }

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
                error!("Failed to parse json, here path: {} ({})", path, err);
                return None;
            }
        }
        // Some(serialization_response.unwrap())
    }
    async fn begin_download(&mut self, id: i64, video: bool) -> Result<reqwest::Response, Error> {
        loop {
            let config = self.clone().load_config().map_err(|error| {
                error!("Error while loading configuration for map download: {error}");
                Error::other("Failed to load download credentials")
            })?;

            if config.osu_access_token != self.access_token {
                self.access_token.clone_from(&config.osu_access_token);
                self.refresh_token.clone_from(&config.osu_refresh_token);
                self.token_expires_at = config.osu_token_expires_at;
            }
            if Local::now().timestamp() < self.token_expires_at {
                break;
            }

            let refreshed = DOWNLOAD_TOKEN_REFRESHED.notified();
            if DOWNLOAD_TOKEN_REFRESHING
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                let _refresh_guard = DownloadTokenRefreshGuard;
                if !self.refresh_token(config).await.map_err(|error| {
                    error!("Failed to refresh token for map download: {error}");
                    Error::other("Failed to refresh download credentials")
                })? {
                    return Err(Error::other("Failed to refresh download credentials"));
                }
                break;
            }

            if DOWNLOAD_TOKEN_REFRESHING.load(Ordering::Acquire) {
                refreshed.await;
            }
        }

        reqwest::Client::new()
            .get(beatmapset_download_url(id, video))
            .bearer_auth(&self.access_token)
            .send()
            .await
            .map_err(Error::other)
    }

    async fn refresh_token_if_required(&mut self) -> bool {
        let config = self.clone().load_config().expect("Failed to load config");

        if config.osu_access_token != self.access_token {
            return true;
        }
        let date_time = Local::now().timestamp();
        if date_time > config.osu_token_expires_at {
            match self.refresh_token(config).await {
                Ok(success) => {
                    info!("Refreshed token. {}", success);
                    return success;
                }
                Err(err) => {
                    error!("Failed to refresh token: {}", err);
                    return false;
                }
            }
        }

        false
    }

    fn load_config(self) -> Result<Configuration, ConfyError> {
        let configuration: Result<Configuration, ConfyError> = confy::load("mirria", None);

        configuration
    }
}

pub(crate) fn beatmapset_download_url(id: i64, video: bool) -> String {
    let base = format!("https://osu.ppy.sh/api/v2/beatmapsets/{id}/download");
    if video {
        base
    } else {
        format!("{base}?noVideo=1")
    }
}

#[cfg(test)]
mod tests {
    use super::beatmapset_download_url;

    #[test]
    fn download_url_omits_video_option_by_default_and_uses_official_no_video_flag() {
        assert_eq!(
            beatmapset_download_url(42, true),
            "https://osu.ppy.sh/api/v2/beatmapsets/42/download"
        );
        assert_eq!(
            beatmapset_download_url(42, false),
            "https://osu.ppy.sh/api/v2/beatmapsets/42/download?noVideo=1"
        );
    }
}
