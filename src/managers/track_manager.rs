use std::path::Path;
use tokio::task::JoinSet;
use tracing::{error, info};
use crate::api::server::SearchFilter;
use crate::lyrics_services::lyrics_client::LyricsClient;
use crate::model::{AlbumPayload, AlbumResult, ArtistPayload, ArtistResult, Track, TrackResult};
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
    lyrics:     LyricsClient,
}

impl TrackManager {
    pub fn new(repo: TrackRepository, downloader: DownloadService, python: PythonClient) -> Self {
        Self { repo, downloader, python, lyrics: LyricsClient::new() }
    }


    // ─── ENDPOINTS DEL DISPATCHER ─────────────────────────────────────────────

    /// Acción "track": Consulta ESTRICTA a base de datos.
    /// Falla rápido (O(1)) si no está el registro o no hay archivo físico.
    /// No hace llamadas a Python ni a YouTube.
    pub async fn get_local_track(&self, query: &str) -> Result<Track, TrackManagerError> {
        let id = extract_video_id(query).unwrap_or_else(|| query.trim().to_string());

        if let Some(cached) = self.db_get(&id).await? {
            if cached.file_path.is_some() {
                return Ok(cached);
            }
        }

        Err(TrackManagerError::NoResults)
    }

    /// Acción "resolve": Extrae metadatos coherentes (DB o Python).
    /// NO bloquea el hilo descargando el audio.
    pub async fn resolve_metadata(&self, query: &str) -> Result<Track, TrackManagerError> {
        let query = query.trim();

        // 1. Si es una URL o ID directo
        if let Some(id) = extract_video_id(query) {
            if let Some(cached) = self.db_get(&id).await? {
                return Ok(cached);
            }
            return self.python_get_by_id(&id).await;
        }

        // 2. Si es texto plano (búsqueda)
        let track = self.python_search_first(query).await?;
        if let Some(cached) = self.db_get(&track.id).await? {
            return Ok(cached);
        }
        let track = self.python_get_by_id(&track.id).await?;

        Ok(track)
    }

    /// Acción "search": Múltiples resultados para mostrar menú al usuario.
    /// Recicla el cliente Python en lugar de spawnear `yt-dlp` crudo en Rust.
    pub async fn search(&self, query: &str, limit: usize, filter: SearchFilter) -> Result<Vec<Track>, TrackManagerError> {
        let mut tracks: Vec<Track> = self.python.call_with_payload(serde_json::json!({
            "action": "search",
            "query":  query,
            "limit":  limit,
            "filter": filter, // Gracias a #[serde(rename_all = "lowercase")], pasará como "songs" o "videos"
        })).await.map_err(TrackManagerError::MetadataError)?;

        for track in tracks.iter_mut() {
            if let Ok(db_track) = self.get_local_track(&track.id).await {
                *track = db_track;
            }
        }

        Ok(tracks)
    }

    /// Acción "download": Orquestador destructivo/escritura.
    /// Verifica cache físico antes de spawnear la descarga para evitar DDoS.
    pub async fn download_track(&self, query: &str) -> Result<Track, TrackManagerError> {
        let track = self.resolve_metadata(query).await?;

        if track.file_path.is_some() {
            return Ok(track);
        }

        self.download_and_save(track).await
    }


    // ─── LÓGICA DE NEGOCIO (Radio, Album, Played) ─────────────────────────────

    pub async fn played(&self, id: &str) -> Result<(), TrackManagerError> {
        self.repo.update_played(id).await
            .map_err(|e| TrackManagerError::DatabaseError(e.to_string()))
    }

    pub async fn radio(&self, seed_id: &str, limit: usize) -> Result<Vec<TrackResult>, TrackManagerError> {
        let tracks: Vec<Track> = self.python.call("radio", seed_id).await
            .map_err(TrackManagerError::MetadataError)?;

        let mut results = self.resolve_list(tracks).await?;
        results.truncate(limit);
        Ok(results)
    }

    pub async fn album(&self, album_id: &str) -> Result<AlbumResult, TrackManagerError> {
        let payload: AlbumPayload = self.python.call("album", album_id).await
            .map_err(TrackManagerError::MetadataError)?;

        let tracks = self.resolve_list(payload.tracks).await?;

        Ok(AlbumResult {
            id: payload.id,
            name: payload.name,
            thumbnail_small: payload.thumbnail_small,
            thumbnail_large: payload.thumbnail_large,
            kind: payload.kind,
            year: payload.year,
            artists: payload.artists,
            tracks,
        })
    }

    pub async fn artist(&self, artist_id: &str, limit: usize) -> Result<ArtistResult, TrackManagerError> {
        let payload: ArtistPayload = self.python.call_with_payload(serde_json::json!({
            "action": "artist",
            "query":  artist_id,
            "limit":  limit,
        })).await.map_err(TrackManagerError::MetadataError)?;

        let songs = self.resolve_list(payload.songs).await?;

        Ok(ArtistResult {
            id: payload.id,
            name: payload.name,
            banner: payload.banner,
            songs,
            albums: payload.albums,
        })
    }



    /// Acción "resolve_many": Resuelve hasta 250 IDs.
    /// 1. Batch query a Postgres (2 queries totales, sin N+1).
    /// 2. Los IDs ausentes se mandan a Python en paralelo.
    pub async fn resolve_many(&self, ids: &[String]) -> Result<Vec<Track>, TrackManagerError> {
        const MAX: usize = 250;
        let ids = if ids.len() > MAX { &ids[..MAX] } else { ids };

        // 1. Lo que ya está en BD (2 queries, sin importar cuántos IDs)
        let found = self.repo.get_many_by_ids(ids).await
            .map_err(|e| TrackManagerError::DatabaseError(e.to_string()))?;

        let found_ids: std::collections::HashSet<&str> =
            found.iter().map(|t| t.id.as_str()).collect();

        // 2. Los que faltan → Python en paralelo
        let missing: Vec<&String> = ids.iter()
            .filter(|id| !found_ids.contains(id.as_str()))
            .collect();

        let mut set = JoinSet::new();
        for id in missing {
            let python = self.python.clone();
            let id = id.clone();
            set.spawn(async move {
                python.call::<Track>("track", &id).await.ok()
            });
        }

        let mut from_yt: Vec<Track> = Vec::new();
        while let Some(res) = set.join_next().await {
            if let Ok(Some(track)) = res {
                from_yt.push(track);
            }
        }

        let mut all = found;
        all.extend(from_yt);
        Ok(all)
    }

    // ─── INTERNOS (I/O y Helpers) ─────────────────────────────────────────────

    async fn resolve_list(&self, tracks: Vec<Track>) -> Result<Vec<TrackResult>, TrackManagerError> {
        let ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();

        let found = self.repo.get_many_by_ids(&ids).await
            .map_err(|e| TrackManagerError::DatabaseError(e.to_string()))?;

        let by_id: std::collections::HashMap<&str, &Track> =
            found.iter().map(|t| (t.id.as_str(), t)).collect();

        let results = tracks.into_iter().map(|track| {
            let db = by_id.get(track.id.as_str()).map(|&t| t.clone());
            TrackResult::from_track_and_db(track, db)
        }).collect();

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
        // 1. Descarga del audio (Bloquea esta request, pero no el servidor TCP)
        let path = self.downloader.download(&track).await?;
        let saved_track = Track { file_path: Some(path.clone()), ..track };

        // 2. Persistencia inicial en DB
        self.repo.insert(&saved_track).await
            .map_err(|e| TrackManagerError::DatabaseError(e.to_string()))?;

        // 3. Fire and Forget: análisis BPM/key (como ya estaba)
        let repo_bg = self.repo.clone();
        let id_bg = saved_track.id.clone();
        let path_bg = path.clone();
        let python_bg = self.python.clone();

        tokio::spawn(async move {
            info!("Iniciando análisis asíncrono para {}", id_bg);

            match python_bg.call::<serde_json::Value>("analyze_local_file", &path_bg).await {
                Ok(metadata) => {
                    let bpm = metadata["bpm"].as_i64().map(|v| v as i32);
                    let camelot = metadata["camelotKey"].as_str().map(|s| s.to_string());

                    if let Err(e) = repo_bg.update_analysis_data(&id_bg, bpm, camelot.clone()).await {
                        error!("Fallo SQL al guardar análisis para {}: {}", id_bg, e);
                    } else {
                        info!("Análisis persistido para {}: BPM={:?}, Key={:?}", id_bg, bpm, camelot);
                    }
                }
                Err(e) => error!("Fallo RPC hacia el analizador en Python para {}: {}", id_bg, e),
            }
        });

        let lyrics_client = self.lyrics.clone();
        let lyrics_id = saved_track.id.clone();
        let lyrics_title = saved_track.title.clone();
        let lyrics_artist = saved_track.artists
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "Desconocido".to_string());
        let lyrics_duration = saved_track.duration_seconds;
        let lyrics_path = Path::new(&path).with_extension("lrc");

        tokio::spawn(async move {
            info!("Buscando letras para {} — {}", lyrics_id, lyrics_title);

            match lyrics_client
                .find_best_lyrics(&lyrics_title, &lyrics_artist, lyrics_duration)
                .await
            {
                Ok(response) => {
                    let Some(content) = response.best_content() else {
                        info!("LRCLIB respondió pero sin contenido usable para {}", lyrics_id);
                        return;
                    };

                    match tokio::fs::write(&lyrics_path, content).await {
                        Ok(()) => info!("Letras guardadas para {}", lyrics_id),
                        Err(e) => error!("No se pudo escribir .lrc para {}: {}", lyrics_id, e),
                    }
                }
                Err(e) => {
                    info!("Sin letras en LRCLIB para {} — {}: {}", lyrics_id, lyrics_title, e);
                }
            }
        });

        Ok(saved_track)
    }

}

// ─── PARSERS EXTRACCIÓN ───────────────────────────────────────────────────────

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