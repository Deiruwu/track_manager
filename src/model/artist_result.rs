use serde::{Deserialize, Serialize};
use crate::model::{AlbumStub, Track, TrackResult};

#[derive(Debug, Clone, Deserialize)]
pub struct ArtistPayload {
    pub id:     String,
    pub name:   String,
    pub banner: Option<String>,
    pub songs:  Vec<Track>,
    pub albums: Vec<AlbumStub>,
}

#[derive(Serialize)]
pub struct ArtistResult {
    pub id:     String,
    pub name:   String,
    pub banner: Option<String>,
    pub songs:  Vec<TrackResult>,
    pub albums: Vec<AlbumStub>,
}
