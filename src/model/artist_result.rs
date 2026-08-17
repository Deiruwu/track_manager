use serde::{Deserialize, Serialize};
use crate::model::{AlbumStub, Track, TrackResult};

/// Forma cruda que manda Python para la acción "artist".
#[derive(Debug, Clone, Deserialize)]
pub struct ArtistPayload {
    pub id:           String,
    pub name:         String,
    pub banner:       Option<String>,
    pub avatar_small: Option<String>,
    pub avatar_large: Option<String>,
    pub songs:        Vec<Track>,
    pub albums:       Vec<AlbumStub>,
}

/// Forma final expuesta al cliente para la acción "artist".
#[derive(Serialize)]
pub struct ArtistResult {
    pub id:           String,
    pub name:         String,
    pub banner:       Option<String>,
    pub avatar_small: Option<String>,
    pub avatar_large: Option<String>,
    pub songs:        Vec<TrackResult>,
    pub albums:       Vec<AlbumStub>,
}
