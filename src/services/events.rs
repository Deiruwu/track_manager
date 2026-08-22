use serde::Serialize;

/// Evento de progreso de descarga, publicado en el `broadcast::channel`
/// compartido y consumido por conexiones que mandaron `{"action":"subscribe"}`.
/// Los campos de progreso van en crudo (bytes, bytes/segundo, segundos) — el
/// consumidor decide cómo formatearlos/abreviarlos, igual que `ArtistResult.views`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum DownloadEvent {
    Downloading {
        id:                   String,
        downloaded_bytes:     Option<u64>,
        total_bytes:          Option<u64>,
        speed_bytes_per_sec:  Option<f64>,
        eta_seconds:          Option<u64>,
    },
    Finished {
        id: String,
    },
    Failed {
        id:      String,
        message: String,
    },
}
