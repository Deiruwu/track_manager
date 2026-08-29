use serde::Serialize;
use crate::model::Track;

/// Evento de progreso de descarga, publicado en el `broadcast::channel`
/// compartido y consumido por conexiones que mandaron `{"action":"subscribe"}`.
/// Los campos de progreso van en crudo (bytes, bytes/segundo, segundos) — el
/// consumidor decide cómo formatearlos/abreviarlos, igual que `ArtistResult.views`.
/// Todas las variantes llevan `title`/`thumbnail_small` para que el consumidor
/// pueda pintar el evento sin correlacionarlo con otra respuesta.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum DownloadEvent {
    /// yt-dlp ya arrancó (el proceso hijo hizo spawn con éxito), pero todavía
    /// no llegó ningún tick de progreso real — puede estar resolviendo
    /// formatos/metadata antes de bajar bytes.
    Requested {
        id:              String,
        title:           String,
        thumbnail_small: Option<String>,
    },
    Downloading {
        id:                   String,
        title:                String,
        thumbnail_small:      Option<String>,
        downloaded_bytes:     Option<u64>,
        total_bytes:          Option<u64>,
        speed_bytes_per_sec:  Option<f64>,
        eta_seconds:          Option<u64>,
    },
    Finished {
        id:              String,
        title:           String,
        thumbnail_small: Option<String>,
    },
    Failed {
        id:              String,
        title:           String,
        thumbnail_small: Option<String>,
        message:         String,
    },
    /// Arranca el análisis de BPM/key en background, ya con el audio guardado.
    AnalyzeStarted {
        id:              String,
        title:           String,
        thumbnail_small: Option<String>,
    },
    /// Análisis terminado — manda el track completo (ya con bpm/camelot_key
    /// actualizados) para que el consumidor lo pise directamente sin tener
    /// que volver a pedirlo.
    AnalyzeFinished {
        track: Track,
    },
}
