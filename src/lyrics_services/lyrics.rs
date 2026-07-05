use serde::Deserialize;

/// Respuesta cruda de LRCLIB (`/api/get` y `/api/search`).
#[derive(Debug, Clone, Deserialize)]
pub struct LrcLibResponse {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(rename = "trackName")]
    pub track_name: Option<String>,
    #[serde(rename = "artistName")]
    pub artist_name: Option<String>,
    #[serde(rename = "albumName")]
    pub album_name: Option<String>,
    pub duration: Option<f64>,
    #[serde(default)]
    pub instrumental: bool,
    #[serde(rename = "plainLyrics")]
    pub plain_lyrics: Option<String>,
    #[serde(rename = "syncedLyrics")]
    pub synced_lyrics: Option<String>,
}

impl LrcLibResponse {
    /// Prioriza el .lrc sincronizado; si no hay, cae al plano.
    /// `None` si el track es instrumental o no hay ningún texto usable.
    pub fn best_content(&self) -> Option<&str> {
        if self.instrumental {
            return None;
        }

        self.synced_lyrics
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| self.plain_lyrics.as_deref().filter(|s| !s.trim().is_empty()))
    }
}
