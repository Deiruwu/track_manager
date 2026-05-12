use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::model::{Album, Artist};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    // ── Columnas directas de `tracks` ────────────────────────────────────────
    pub id:               String,           // uuid TEXT PRIMARY KEY
    pub title:            String,           // title TEXT NOT NULL
    pub duration_seconds: i32,              // duration_seconds INTEGER NOT NULL
    pub thumbnail_url:    Option<String>,   // thumbnail_url TEXT
    pub bpm:              Option<i32>,      // bpm INTEGER
    pub camelot_key:      Option<String>,   // camelot_key TEXT
    pub file_path:        Option<String>,   // file_path TEXT
    pub added_at:         Option<DateTime<Utc>>, // added_at TIMESTAMPTZ

    // ── Relaciones resueltas ──────────────────────────────────────────────────
    pub album:            Option<Album>,    // JOIN albums ON album_id
    pub artists:          Vec<Artist>,      // JOIN track_artists → artists
}