

use serde_derive::{Deserialize, Serialize};

pub mod beatmaps;
pub mod beatmapset;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DownloadIndex {
    pub id: i64,
    pub date: i64
}