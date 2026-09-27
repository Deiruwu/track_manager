use std::path::Path;
use tokio::sync::broadcast;
use tokio::task::JoinSet;
use tracing::{error, info};
use crate::api::server::SearchFilter;
use crate::lyrics_services::lyrics_client::LyricsClient;
use crate::model::{Album, AlbumPayload, AlbumResult, Artist, ArtistPayload, ArtistProfileResult, ArtistResult, SearchItem, Track, TrackResult};
use crate::repository::TrackRepository;
use crate::services::{DownloadError, DownloadService, DownloadEvent, PythonClient};

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
    events:     broadcast::Sender<DownloadEvent>,
}

impl TrackManager {
    pub fn new(
        repo: TrackRepository,
        downloader: DownloadService,
        python: PythonClient,
        events: broadcast::Sender<DownloadEvent>,
    ) -> Self {
        Self { repo, downloader, python, lyrics: LyricsClient::new(), events }
    }

    /// Nueva conexión a los eventos de progreso de descarga — usado por la
    /// acción "subscribe" del servidor TCP.
    pub fn subscribe_downloads(&self) -> broadcast::Receiver<DownloadEvent> {
        self.events.subscribe()
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

    /// Acción "search_items": búsqueda heterogénea (canciones, videos, álbumes,
    /// artistas o todo mezclado). Los tracks ya descargados se reemplazan por
    /// su fila de BD, igual que en `search`.
    pub async fn search_items(&self, query: &str, limit: usize, filter: SearchFilter) -> Result<Vec<SearchItem>, TrackManagerError> {
        let mut items: Vec<SearchItem> = self.python.call_with_payload(serde_json::json!({
            "action": "search_items",
            "query":  query,
            "limit":  limit,
            "filter": filter,
        })).await.map_err(TrackManagerError::MetadataError)?;

        for item in items.iter_mut() {
            if let SearchItem::Track(track) = item {
                if let Ok(db_track) = self.get_local_track(&track.id).await {
                    *track = db_track;
                }
            }
        }

        Ok(items)
    }

    /// Acción "download": Orquestador destructivo/escritura.
    /// Verifica cache físico antes de spawnear la descarga para evitar DDoS.
    pub async fn download_track(&self, query: &str) -> Result<Track, TrackManagerError> {
        let track = self.resolve_metadata(query).await?;

        if track.file_path.is_some() {
            return Ok(track);
        }

        self.download_and_save(track, false).await
    }

    /// Acción "redownload": vuelve a bajar el audio aunque ya exista archivo o `file_path`.
    pub async fn redownload_track(&self, query: &str) -> Result<Track, TrackManagerError> {
        let track = self.resolve_metadata(query).await?;
        self.download_and_save(Track { file_path: None, ..track }, true).await
    }

    /// Acción "refresh_metadata": vuelve a pedir título/artistas/álbum/portadas
    /// a Python y los reescribe en BD, sin tocar archivo ni análisis. Publica
    /// `MetadataUpdated`/`MetadataFailed`.
    pub async fn refresh_metadata(&self, query: &str) -> Result<Track, TrackManagerError> {
        let id = extract_video_id(query).unwrap_or_else(|| query.trim().to_string());
        let result = self.rewrite_metadata(&id).await;

        let event = match &result {
            Ok(track) => DownloadEvent::MetadataUpdated { track: track.clone() },
            Err(e) => {
                let known = self.db_get(&id).await.ok().flatten();
                DownloadEvent::MetadataFailed {
                    title:           known.as_ref().map(|t| t.title.clone()).unwrap_or_else(|| id.clone()),
                    thumbnail_small: known.and_then(|t| t.thumbnail_small),
                    id,
                    message:         e.to_string(),
                }
            }
        };
        let _ = self.events.send(event);

        result
    }

    /// Acción "refresh_lyrics": vuelve a buscar la letra de un track ya
    /// descargado. Publica `LyricsFound`/`LyricsNotFound`.
    pub async fn refresh_lyrics(&self, query: &str) -> Result<Track, TrackManagerError> {
        let track = self.get_local_track(query).await?;
        fetch_and_store_lyrics(&self.lyrics, &self.events, &track).await;
        Ok(track)
    }

    /// Acción "reanalyze": vuelve a calcular BPM/key de un track ya descargado.
    pub async fn reanalyze(&self, query: &str) -> Result<Track, TrackManagerError> {
        let track = self.get_local_track(query).await?;
        analyze_and_persist(&self.repo, &self.python, &self.events, track).await
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

        let tracks = self.resolve_list_normalized(payload.tracks).await?;

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

    /// Acción "artist_profile": versión ligera del artista (id, nombre,
    /// thumbnail_small/large), una sola llamada a Python — sin canciones ni discografía.
    pub async fn artist_profile(&self, artist_id: &str) -> Result<ArtistProfileResult, TrackManagerError> {
        self.python.call("artist_profile", artist_id).await
            .map_err(TrackManagerError::MetadataError)
    }

    pub async fn artist(&self, artist_id: &str, limit: usize) -> Result<ArtistResult, TrackManagerError> {
        let payload: ArtistPayload = self.python.call_with_payload(serde_json::json!({
            "action": "artist",
            "query":  artist_id,
            "limit":  limit,
        })).await.map_err(TrackManagerError::MetadataError)?;

        let songs = self.resolve_list_normalized(payload.songs).await?;

        // Python ya no ordena la discografía (para evitar la llamada HTTP extra
        // que cuesta order='Recency' en get_artist_albums) — se ordena acá.
        let mut albums = payload.albums;
        albums.sort_by_key(|a| std::cmp::Reverse(album_year_key(&a.year)));

        Ok(ArtistResult {
            id: payload.id,
            name: payload.name,
            banner: payload.banner,
            views: payload.views,
            songs,
            albums,
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

    /// Igual que `resolve_list`, pero además sobreescribe `artists`/`album` de
    /// los tracks ya cacheados (con archivo descargado) con la versión fresca
    /// de Python. `resolve_list` por sí solo devuelve la fila de BD tal cual
    /// para esos tracks, que puede traer un artista/álbum sin resolver desde
    /// antes de que Python empezara a normalizarlos con el contexto del álbum
    /// o artista consultado. Se usa en `album()` y `artist()`, donde sí existe
    /// ese contexto de normalización; no en `radio()`, donde no lo hay.
    async fn resolve_list_normalized(&self, tracks: Vec<Track>) -> Result<Vec<TrackResult>, TrackManagerError> {
        let fresh_by_id: std::collections::HashMap<String, (Vec<Artist>, Option<Album>)> =
            tracks.iter()
                .map(|t| (t.id.clone(), (t.artists.clone(), t.album.clone())))
                .collect();

        // Persistimos en background la metadata "fresca" (con contexto de álbum/
        // artista) que se usa para pisar los tracks cacheados más abajo. Sin esto,
        // la corrección solo vivía en la respuesta de esta llamada puntual: un
        // `resolve` posterior sobre el mismo id volvía a leer la fila sin
        // rehidratar y caía en la búsqueda difusa de Python (fuente de los
        // resultados "TEMA", sin artist id ni album).
        let repo_bg = self.repo.clone();
        let tracks_bg = tracks.clone();
        tokio::spawn(async move {
            for track in tracks_bg {
                if let Err(e) = repo_bg.upsert_track_metadata(&track).await {
                    error!("No se pudo rehidratar metadata de {}: {}", track.id, e);
                }
            }
        });

        let mut results = self.resolve_list(tracks).await?;

        for result in results.iter_mut() {
            if let TrackResult::Cached(cached) = result {
                if let Some((artists, album)) = fresh_by_id.get(&cached.id) {
                    cached.artists = artists.clone();
                    cached.album = album.clone();
                }
            }
        }

        Ok(results)
    }

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

    /// Pide el track fresco a Python y lo reescribe en BD; devuelve la fila resultante.
    async fn rewrite_metadata(&self, id: &str) -> Result<Track, TrackManagerError> {
        let fresh = self.python_get_by_id(id).await?;

        self.repo.upsert_track_metadata(&fresh).await
            .map_err(|e| TrackManagerError::DatabaseError(e.to_string()))?;

        Ok(self.db_get(id).await?.unwrap_or(fresh))
    }

    async fn python_get_by_id(&self, id: &str) -> Result<Track, TrackManagerError> {
        let track: Track = self.python.call("track", id).await
            .map_err(TrackManagerError::MetadataError)?;

        Ok(track)
    }

    async fn download_and_save(&self, track: Track, force_overwrite: bool) -> Result<Track, TrackManagerError> {
        // 1. Descarga del audio (Bloquea esta request, pero no el servidor TCP)
        let path = match self.downloader.download(&track, force_overwrite).await {
            Ok(path) => path,
            Err(e) => {
                let _ = self.events.send(DownloadEvent::Failed {
                    id: track.id.clone(),
                    title: track.title.clone(),
                    thumbnail_small: track.thumbnail_small.clone(),
                    message: e.to_string(),
                });
                return Err(e.into());
            }
        };
        let saved_track = Track { file_path: Some(path.clone()), ..track };

        // 2. Persistencia inicial en DB
        if let Err(e) = self.repo.insert(&saved_track).await {
            let _ = self.events.send(DownloadEvent::Failed {
                id: saved_track.id.clone(),
                title: saved_track.title.clone(),
                thumbnail_small: saved_track.thumbnail_small.clone(),
                message: e.to_string(),
            });
            return Err(TrackManagerError::DatabaseError(e.to_string()));
        }

        let _ = self.events.send(DownloadEvent::Finished {
            id: saved_track.id.clone(),
            title: saved_track.title.clone(),
            thumbnail_small: saved_track.thumbnail_small.clone(),
        });

        // 3. Fire and Forget: análisis BPM/key (como ya estaba)
        let repo_bg = self.repo.clone();
        let python_bg = self.python.clone();
        let events_bg = self.events.clone();
        let track_bg = saved_track.clone();

        tokio::spawn(async move {
            let _ = analyze_and_persist(&repo_bg, &python_bg, &events_bg, track_bg).await;
        });

        let lyrics_bg = self.lyrics.clone();
        let events_bg = self.events.clone();
        let track_bg = saved_track.clone();

        tokio::spawn(async move {
            fetch_and_store_lyrics(&lyrics_bg, &events_bg, &track_bg).await;
        });

        Ok(saved_track)
    }

}

/// Analiza BPM/key del archivo del track en Python, lo persiste y publica
/// `AnalyzeStarted` y luego `AnalyzeFinished`/`AnalyzeFailed`. Devuelve el track ya analizado.
async fn analyze_and_persist(
    repo: &TrackRepository,
    python: &PythonClient,
    events: &broadcast::Sender<DownloadEvent>,
    track: Track,
) -> Result<Track, TrackManagerError> {
    let Some(path) = track.file_path.clone() else {
        return Err(TrackManagerError::NoResults);
    };

    info!("Iniciando análisis asíncrono para {}", track.id);

    let _ = events.send(DownloadEvent::AnalyzeStarted {
        id:              track.id.clone(),
        title:           track.title.clone(),
        thumbnail_small: track.thumbnail_small.clone(),
    });

    let analysis = async {
        let metadata = python.call::<serde_json::Value>("analyze_local_file", &path).await.map_err(|e| {
            error!("Fallo RPC hacia el analizador en Python para {}: {}", track.id, e);
            TrackManagerError::MetadataError(e)
        })?;

        let bpm = metadata["bpm"].as_i64().map(|v| v as i32);
        let camelot = metadata["camelotKey"].as_str().map(|s| s.to_string());

        repo.update_analysis_data(&track.id, bpm, camelot.clone()).await.map_err(|e| {
            error!("Fallo SQL al guardar análisis para {}: {}", track.id, e);
            TrackManagerError::DatabaseError(e.to_string())
        })?;

        info!("Análisis persistido para {}: BPM={:?}, Key={:?}", track.id, bpm, camelot);
        Ok::<_, TrackManagerError>((bpm, camelot))
    }.await;

    match analysis {
        Ok((bpm, camelot_key)) => {
            let analyzed_track = Track { bpm, camelot_key, ..track };
            let _ = events.send(DownloadEvent::AnalyzeFinished { track: analyzed_track.clone() });
            Ok(analyzed_track)
        }
        Err(e) => {
            let _ = events.send(DownloadEvent::AnalyzeFailed {
                id:              track.id,
                title:           track.title,
                thumbnail_small: track.thumbnail_small,
                message:         e.to_string(),
            });
            Err(e)
        }
    }
}

/// Busca la letra en LRCLIB, la guarda como `.lrc` junto al audio y publica
/// `LyricsFound`/`LyricsNotFound`. Devuelve si se guardó una letra.
async fn fetch_and_store_lyrics(
    lyrics: &LyricsClient,
    events: &broadcast::Sender<DownloadEvent>,
    track: &Track,
) -> bool {
    let Some(audio_path) = track.file_path.as_deref() else {
        return false;
    };
    let lyrics_path = Path::new(audio_path).with_extension("lrc");
    let artist = track.artists
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "Desconocido".to_string());

    info!("Buscando letras para {} — {}", track.id, track.title);

    let found = match lyrics.find_best_lyrics(&track.title, &artist, track.duration_seconds).await {
        Ok(response) => match response.best_content() {
            Some(content) => match tokio::fs::write(&lyrics_path, content).await {
                Ok(()) => {
                    info!("Letras guardadas para {}", track.id);
                    true
                }
                Err(e) => {
                    error!("No se pudo escribir .lrc para {}: {}", track.id, e);
                    false
                }
            },
            None => {
                info!("LRCLIB respondió pero sin contenido usable para {}", track.id);
                false
            }
        },
        Err(e) => {
            info!("Sin letras en LRCLIB para {} — {}: {}", track.id, track.title, e);
            false
        }
    };

    let (id, title, thumbnail_small) = (track.id.clone(), track.title.clone(), track.thumbnail_small.clone());
    let _ = events.send(if found {
        DownloadEvent::LyricsFound { id, title, thumbnail_small }
    } else {
        DownloadEvent::LyricsNotFound { id, title, thumbnail_small }
    });

    found
}

// ─── PARSERS EXTRACCIÓN ───────────────────────────────────────────────────────

fn album_year_key(year: &Option<String>) -> i32 {
    year.as_deref()
        .filter(|y| !y.is_empty() && y.chars().all(|c| c.is_ascii_digit()))
        .and_then(|y| y.parse().ok())
        .unwrap_or(0)
}

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