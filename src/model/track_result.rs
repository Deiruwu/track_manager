use serde::Serialize;
use crate::model::{Album, Artist, Track};

#[derive(Serialize)]
#[serde(tag = "state")]
pub enum TrackResult {
    #[serde(rename = "cached")]
    Cached(Track),
    #[serde(rename = "partial")]
    Partial {
        id:               String,
        title:            String,
        artists:          Vec<Artist>,
        album:            Option<Album>,
        duration_seconds: i32,
        thumbnail_small:  Option<String>,
        thumbnail_large:  Option<String>,
    },
}

impl TrackResult {
    pub fn from_track_and_db(python_track: Track, db_track: Option<Track>) -> Self {
        match db_track {
            Some(cached) if cached.file_path.is_some() => Self::Cached(cached),
            db_track => Self::Partial {
                // Las canciones top de un artista llegan sin duración: se usa la ya guardada, si hay.
                duration_seconds: match python_track.duration_seconds {
                    0 => db_track.map_or(0, |known| known.duration_seconds),
                    seconds => seconds,
                },
                id:               python_track.id,
                title:            python_track.title,
                artists:          python_track.artists,
                album:            python_track.album,
                thumbnail_small:  python_track.thumbnail_small,
                thumbnail_large:  python_track.thumbnail_large,
            },
        }
    }
}