use serde_derive::Deserialize;
use serde_derive::Serialize;
use serde_json::Value;
use serde_with::serde_as;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub beatmapsets: Vec<Beatmapset>,
    pub search: Search,
    #[serde(rename = "recommended_difficulty")]
    pub recommended_difficulty: Option<f64>,
    pub error: Option<Value>,
    pub total: i64,
    pub cursor: Option<Cursor>,
    #[serde(rename = "cursor_string")]
    pub cursor_string: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Beatmapset {
#[serde(rename = "id")]
    pub mapset_id: i64,

    pub artist: String,

    #[serde(rename = "artist_unicode")]
    pub artist_unicode: Option<String>,

    pub title: String,

    #[serde(rename = "title_unicode")]
    pub title_unicode: Option<String>,

    pub creator: String, // String representation of the creator

    #[serde(rename = "user_id")]
    pub creator_id: i64,

    pub status: String,

    pub bpm: f64,

    #[serde(rename = "play_count")]
    pub playcount: i64,

    #[serde(rename = "favourite_count")]
    pub favourite_count: i64,

    pub nsfw: bool,

    pub video: bool,

    pub storyboard: bool,

    #[serde(rename = "is_scoreable")]
    pub is_scoreable: Option<bool>,

    pub source: String,

    pub tags: String,

    #[serde(rename = "preview_url")]
    pub preview_url: String,

    pub offset: i64,

    pub spotlight: bool,

    pub ranked: i64,

    #[serde(rename = "last_updated")]
    pub last_updated: String,

    #[serde(rename = "submitted_date")]
    pub submitted_date: String,

    #[serde(rename = "ranked_date")]
    pub ranked_date: Option<String>,

    #[serde(rename = "deleted_at")]
    pub deleted_at: Option<String>,

    #[serde(rename = "can_be_hyped")]
    pub can_be_hyped: bool,

    pub hype: Option<Value>,

    #[serde(rename = "discussion_enabled")]
    pub discussion_enabled: bool,

    #[serde(rename = "discussion_locked")]
    pub discussion_locked: bool,

    #[serde(rename = "legacy_thread_url")]
    pub legacy_thread_url: Option<String>,

    #[serde(rename = "nominations_summary")]
    pub nominations_summary: Value,

    pub availability: Value, // Using Value to remain generic

    pub covers: Value, // Using Value to remain generic

    #[serde(rename = "track_id")]
    pub track_id: Option<i64>,

    #[serde(rename = "has_favourited")]
    #[serde(default)]
    pub has_favourited: bool,

    // Nested data
    pub beatmaps: Vec<Beatmap>, // Uses the merged Beatmap struct from earlier

    #[serde(rename = "pack_tags")]
    pub pack_tags: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ratings: Option<Vec<i64>>,
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
    pub mapset_id: i64,

    #[serde(rename = "difficulty_rating")]
    pub stars: f64,

    #[serde(rename = "id")]
    pub map_id: i64,

    pub mode: String,
    
    #[serde(rename = "mode_int")]
    pub mode_int: i64,

    pub status: String,

    #[serde(rename = "total_length")]
    pub seconds_total: i64,

    #[serde(rename = "hit_length")]
    pub seconds_drain: i64,

    #[serde(rename = "user_id")]
    pub creator_id: i64,

    pub version: String,

    #[serde(rename = "accuracy")]
    pub od: f64,

    pub ar: f64,

    pub bpm: f64,

    pub convert: bool,

    pub count_circles: Option<i64>,
    pub count_sliders: Option<i64>,
    pub count_spinners: Option<i64>,

    pub cs: f64,

    #[serde(rename = "drain")]
    pub hp: f64,

    pub is_scoreable: Option<bool>,
    #[serde(default)]
    pub last_updated: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,

    pub passcount: i64,
    pub playcount: i64,
    pub ranked: i64,
    pub url: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,

    #[serde(rename = "max_combo", skip_serializing_if = "Option::is_none")]
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
