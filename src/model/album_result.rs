use serde::{Deserialize, Serialize};
use crate::model::{Track, TrackResult};

/// Forma cruda que manda Python para la acción "album".
#[derive(Debug, Clone, Deserialize)]
pub struct AlbumPayload {
    pub id:              String,
    pub name:            String,
    pub thumbnail_small: Option<String>,
    pub thumbnail_large: Option<String>,
    #[serde(rename = "type")]
    pub kind:            Option<String>,
    pub tracks:          Vec<Track>,
}

/// Forma final expuesta al cliente para la acción "album".
#[derive(Serialize)]
pub struct AlbumResult {
    pub id:              String,
    pub name:            String,
    pub thumbnail_small: Option<String>,
    pub thumbnail_large: Option<String>,
    #[serde(rename = "type")]
    pub kind:            Option<String>,
    pub tracks:          Vec<TrackResult>,
}
