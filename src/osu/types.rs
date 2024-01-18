use serde_derive::Deserialize;
use serde_derive::Serialize;
use serde_json::Value;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub beatmapsets: Vec<Beatmapset>,
    pub search: Search,
    #[serde(rename = "recommended_difficulty")]
    pub recommended_difficulty: f64,
    pub error: Option<Value>,
    pub total: i64,
    pub cursor: Cursor,
    #[serde(rename = "cursor_string")]
    pub cursor_string: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Beatmapset {
    pub artist: String,
    #[serde(rename = "artist_unicode")]
    pub artist_unicode: String,
    pub covers: Covers,
    pub creator: String,
    #[serde(rename = "favourite_count")]
    pub favourite_count: i64,
    pub hype: Option<Value>,
    pub id: i64,
    pub nsfw: bool,
    pub offset: i64,
    #[serde(rename = "play_count")]
    pub play_count: i64,
    #[serde(rename = "preview_url")]
    pub preview_url: String,
    pub source: String,
    pub spotlight: bool,
    pub status: String,
    pub title: String,
    #[serde(rename = "title_unicode")]
    pub title_unicode: String,
    #[serde(rename = "track_id")]
    pub track_id: Option<i64>,
    #[serde(rename = "user_id")]
    pub user_id: i64,
    pub video: bool,
    pub bpm: f64,
    #[serde(rename = "can_be_hyped")]
    pub can_be_hyped: bool,
    #[serde(rename = "deleted_at")]
    pub deleted_at: Option<String>,
    #[serde(rename = "discussion_enabled")]
    pub discussion_enabled: bool,
    #[serde(rename = "discussion_locked")]
    pub discussion_locked: bool,
    #[serde(rename = "is_scoreable")]
    pub is_scoreable: bool,
    #[serde(rename = "last_updated")]
    pub last_updated: String,
    #[serde(rename = "legacy_thread_url")]
    pub legacy_thread_url: String,
    #[serde(rename = "nominations_summary")]
    pub nominations_summary: NominationsSummary,
    pub ranked: i64,
    #[serde(rename = "ranked_date")]
    pub ranked_date: Option<String>,
    pub storyboard: bool,
    #[serde(rename = "submitted_date")]
    pub submitted_date: String,
    pub tags: String,
    pub availability: Availability,
    #[serde(rename = "has_favourited")]
    pub has_favourited: bool,
    pub beatmaps: Vec<Beatmap>,
    #[serde(rename = "pack_tags")]
    pub pack_tags: Vec<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Covers {
    pub cover: String,
    #[serde(rename = "cover@2x")]
    pub cover_2x: String,
    pub card: String,
    #[serde(rename = "card@2x")]
    pub card_2x: String,
    pub list: String,
    #[serde(rename = "list@2x")]
    pub list_2x: String,
    pub slimcover: String,
    #[serde(rename = "slimcover@2x")]
    pub slimcover_2x: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NominationsSummary {
    pub current: i64,
    pub required: i64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    #[serde(rename = "download_disabled")]
    pub download_disabled: bool,
    #[serde(rename = "more_information")]
    pub more_information: Option<Value>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Beatmap {
    #[serde(rename = "beatmapset_id")]
    pub beatmapset_id: i64,
    #[serde(rename = "difficulty_rating")]
    pub difficulty_rating: f64,
    pub id: i64,
    pub mode: String,
    pub status: String,
    #[serde(rename = "total_length")]
    pub total_length: i64,
    #[serde(rename = "user_id")]
    pub user_id: i64,
    pub version: String,
    pub accuracy: f64,
    pub ar: f64,
    pub bpm: f64,
    pub convert: bool,
    #[serde(rename = "count_circles")]
    pub count_circles: i64,
    #[serde(rename = "count_sliders")]
    pub count_sliders: i64,
    #[serde(rename = "count_spinners")]
    pub count_spinners: i64,
    pub cs: f64,
    #[serde(rename = "deleted_at")]
    pub deleted_at: Value,
    pub drain: f64,
    #[serde(rename = "hit_length")]
    pub hit_length: i64,
    #[serde(rename = "is_scoreable")]
    pub is_scoreable: bool,
    #[serde(rename = "last_updated")]
    pub last_updated: String,
    #[serde(rename = "mode_int")]
    pub mode_int: i64,
    pub passcount: i64,
    pub playcount: i64,
    pub ranked: i64,
    pub url: String,
    pub checksum: String,
    #[serde(rename = "max_combo")]
    pub max_combo: Option<i64>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Search {
    pub sort: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cursor {
    #[serde(rename = "last_update")]
    pub last_update: i64,
    pub id: i64,
}
