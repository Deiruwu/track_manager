use crate::model::{Track, TrackResult};
use crate::repository::TrackRepository;
use crate::services::{DownloadError, DownloadService, PythonClient};

#[derive(Debug)]
pub enum TrackManagerError {
    MetadataError(String),
    NoResults,
    DownloadError(DownloadError),
    DatabaseError(String),
}

impl std::fmt::Display for TrackManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            TrackManagerError::MetadataError(e) => write!(f, "Metadata error: {e}"),
            TrackManagerError::NoResults        => write!(f, "No results found"),
            TrackManagerError::DownloadError(e) => write!(f, "Download error: {e}"),
            TrackManagerError::DatabaseError(e) => write!(f, "Database error: {e}"),
        }
    }
}

impl std::error::Error for TrackManagerError {}

impl From<DownloadError> for TrackManagerError {
    fn from(e: DownloadError) -> Self { TrackManagerError::DownloadError(e) }
}

#[derive(Clone)]
pub struct TrackManager {
    pub repo:       TrackRepository,
    downloader: DownloadService,
    python:     PythonClient,
}

impl TrackManager {
    pub fn new(repo: TrackRepository, downloader: DownloadService, python: PythonClient) -> Self {
        Self { repo, downloader, python }
    }

    /// Resuelve un track completo: DB → Python → descarga.
    /// Siempre devuelve un Track completo y coherente con la DB.
    pub async fn resolve(&self, query: &str) -> Result<Track, TrackManagerError> {
        let query = query.trim();

        if let Some(id) = extract_video_id(query) {
            if let Some(cached) = self.db_get(&id).await? {
                if cached.file_path.is_some() { return Ok(cached); }
            }
            let track = self.python_get_by_id(&id).await?;
            return self.download_and_save(track).await;
        }

        let track = self.python_search_first(query).await?;

        if let Some(cached) = self.db_get(&track.id).await? {
            if cached.file_path.is_some() { return Ok(cached); }
        }

        self.download_and_save(track).await
    }

    pub async fn played(&self, id: &str) -> Result<(), TrackManagerError> {
        self.repo.update_played(id).await
            .map_err(|e| TrackManagerError::DatabaseError(e.to_string()))
    }

    /// Radio: resuelve cada track contra DB.
    /// Cached si está descargado, Partial con datos de Python si no.
    pub async fn radio(&self, seed_id: &str) -> Result<Vec<TrackResult>, TrackManagerError> {
        let tracks: Vec<Track> = self.python.call("radio", seed_id).await
            .map_err(TrackManagerError::MetadataError)?;

        let mut cached  = Vec::new();
        let mut partial = Vec::new();

        for track in tracks {
            let db = self.db_get(&track.id).await?;
            let result = TrackResult::from_track_and_db(track, db);
            match &result {
                TrackResult::Cached(_)      => cached.push(result),
                TrackResult::Partial { .. } => partial.push(result),
            }
        }

        cached.truncate(10);
        let needed = 15usize.saturating_sub(cached.len());
        partial.truncate(needed);
        cached.extend(partial);

        Ok(cached)
    }

    /// Album: igual que radio.
    pub async fn album(&self, album_id: &str) -> Result<Vec<TrackResult>, TrackManagerError> {
        // Indicamos explícitamente Vec<Track>
        let tracks: Vec<Track> = self.python.call("album", album_id).await
            .map_err(TrackManagerError::MetadataError)?;

        self.resolve_list(tracks).await
    }


    // ─── Internos ─────────────────────────────────────────────────────────────

    /// Para cada track de Python, consulta DB y construye TrackResult.
    async fn resolve_list(&self, tracks: Vec<Track>) -> Result<Vec<TrackResult>, TrackManagerError> {
        let mut results = Vec::with_capacity(tracks.len());

        for track in tracks {
            let db = self.db_get(&track.id).await?;
            results.push(TrackResult::from_track_and_db(track, db));
        }

        Ok(results)
    }

    async fn db_get(&self, id: &str) -> Result<Option<Track>, TrackManagerError> {
        self.repo.get_by_id(id).await
            .map_err(|e| TrackManagerError::DatabaseError(e.to_string()))
    }

    async fn python_search_first(&self, query: &str) -> Result<Track, TrackManagerError> {
        let tracks: Vec<Track> = self.python.call("search", query).await
            .map_err(TrackManagerError::MetadataError)?;

        tracks.into_iter()
            .next()
            .ok_or(TrackManagerError::NoResults)
    }

    async fn python_get_by_id(&self, id: &str) -> Result<Track, TrackManagerError> {
        let track: Track = self.python.call("track", id).await
            .map_err(TrackManagerError::MetadataError)?;

        Ok(track)
    }

    async fn download_and_save(&self, track: Track) -> Result<Track, TrackManagerError> {
        let path  = self.downloader.download(&track).await?;
        let track = Track { file_path: Some(path), ..track };
        self.repo.insert(&track).await
            .map_err(|e| TrackManagerError::DatabaseError(e.to_string()))?;
        Ok(track)
    }
}

// ─── HELPERS ─────────────────────────────────────────────────────────────────

fn extract_video_id(query: &str) -> Option<String> {
    if query.len() == 11 && query.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return Some(query.to_string());
    }
    if let Some(pos) = query.find("v=") {
        let id = query[pos + 2..].split('&').next().unwrap_or("");
        if id.len() == 11 { return Some(id.to_string()); }
    }
    if let Some(pos) = query.find("youtu.be/") {
        let id = query[pos + 9..].split('?').next().unwrap_or("");
        if id.len() == 11 { return Some(id.to_string()); }
    }
    None
}