use crate::lyrics_services::lyrics::LrcLibResponse;

const BASE_URL: &str = "https://lrclib.net/api";
const USER_AGENT: &str = "lyra_track_manager/1.0 (+https://github.com/dei)";

#[derive(Debug)]
pub enum LyricsError {
    Network(String),
    NotFound,
}

impl std::fmt::Display for LyricsError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            LyricsError::Network(e) => write!(f, "Error de red hacia LRCLIB: {e}"),
            LyricsError::NotFound => write!(f, "LRCLIB no encontró letras"),
        }
    }
}

impl std::error::Error for LyricsError {}

#[derive(Clone)]
pub struct LyricsClient {
    http: reqwest::Client,
}

impl LyricsClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .expect("no se pudo construir el cliente HTTP de lyrics");

        Self { http }
    }

    /// `/api/get`: matching preciso por título + artista + duración.
    /// LRCLIB filtra por duración con tolerancia chica (pocos segundos),
    /// así que puede dar NotFound aunque el registro exista si hay
    /// diferencia de redondeo entre la duración medida por yt-dlp y la
    /// que tiene LRCLIB.
    async fn get_by_metadata(
        &self,
        track_name: &str,
        artist_name: &str,
        duration_seconds: i32,
    ) -> Result<LrcLibResponse, LyricsError> {
        let query = [
            ("track_name", track_name.to_string()),
            ("artist_name", artist_name.to_string()),
            ("duration", duration_seconds.to_string()),
        ];

        let resp = self
            .http
            .get(format!("{BASE_URL}/get"))
            .query(&query)
            .send()
            .await
            .map_err(|e| LyricsError::Network(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(LyricsError::NotFound);
        }

        resp.error_for_status()
            .map_err(|e| LyricsError::Network(e.to_string()))?
            .json::<LrcLibResponse>()
            .await
            .map_err(|e| LyricsError::Network(e.to_string()))
    }

    /// `/api/search`: matching laxo por texto, sin filtro de duración.
    /// Devuelve el primer resultado si existe.
    async fn search(&self, track_name: &str, artist_name: Option<&str>) -> Result<LrcLibResponse, LyricsError> {
        let mut query: Vec<(&str, &str)> = vec![("track_name", track_name)];
        if let Some(artist) = artist_name {
            query.push(("artist_name", artist));
        }

        let resp = self
            .http
            .get(format!("{BASE_URL}/search"))
            .query(&query)
            .send()
            .await
            .map_err(|e| LyricsError::Network(e.to_string()))?
            .error_for_status()
            .map_err(|e| LyricsError::Network(e.to_string()))?;

        let mut results = resp
            .json::<Vec<LrcLibResponse>>()
            .await
            .map_err(|e| LyricsError::Network(e.to_string()))?;

        if results.is_empty() {
            return Err(LyricsError::NotFound);
        }

        Ok(results.remove(0))
    }

    /// Estrategia de 3 intentos, de más preciso a más laxo. El primero
    /// que devuelva contenido usable (best_content() no vacío) gana:
    ///
    ///   1. /get   con título + artista principal + duración
    ///   2. /search con título + artista principal
    ///   3. /search solo con título
    ///
    /// Si los tres fallan, devuelve NotFound — el caller decide qué
    /// hacer (en track_manager, solo loguear y seguir).
    pub async fn find_best_lyrics(
        &self,
        track_name: &str,
        primary_artist: &str,
        duration_seconds: i32,
    ) -> Result<LrcLibResponse, LyricsError> {
        if let Ok(r) = self.get_by_metadata(track_name, primary_artist, duration_seconds).await {
            if r.best_content().is_some() {
                return Ok(r);
            }
        }

        if let Ok(r) = self.search(track_name, Some(primary_artist)).await {
            if r.best_content().is_some() {
                return Ok(r);
            }
        }

        if let Ok(r) = self.search(track_name, None).await {
            if r.best_content().is_some() {
                return Ok(r);
            }
        }

        Err(LyricsError::NotFound)
    }
}

impl Default for LyricsClient {
    fn default() -> Self {
        Self::new()
    }
}