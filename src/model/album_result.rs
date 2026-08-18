use serde::{Deserialize, Serialize};
use crate::model::{Artist, Track, TrackResult};

#[derive(Debug, Clone, Deserialize)]
pub struct AlbumPayload {
    pub id:              String,
    pub name:            String,
    pub thumbnail_small: Option<String>,
    pub thumbnail_large: Option<String>,
    #[serde(rename = "type")]
    pub kind:            Option<String>,
    pub year:            Option<String>,
    pub artists:         Vec<Artist>,
    pub tracks:          Vec<Track>,
}

#[derive(Serialize)]
pub struct AlbumResult {
    pub id:              String,
    pub name:            String,
    pub thumbnail_small: Option<String>,
    pub thumbnail_large: Option<String>,
    #[serde(rename = "type")]
    pub kind:            Option<String>,
    pub year:            Option<String>,
    pub artists:         Vec<Artist>,
    pub tracks:          Vec<TrackResult>,
}
