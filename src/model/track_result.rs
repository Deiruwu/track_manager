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
            _ => Self::Partial {
                id:               python_track.id,
                title:            python_track.title,
                artists:          python_track.artists,
                album:            python_track.album,
                duration_seconds: python_track.duration_seconds,
                thumbnail_small:  python_track.thumbnail_small,
                thumbnail_large:  python_track.thumbnail_large,
            },
        }
    }
}