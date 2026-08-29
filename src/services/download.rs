use std::path::PathBuf;
use std::process::Stdio;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::broadcast;
use crate::model::Track;
use crate::services::events::DownloadEvent;

#[derive(Debug)]
pub enum DownloadError {
    IoError(std::io::Error),
    YtDlpFailed(String),
    FileNotFound(String),
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            DownloadError::IoError(e)       => write!(f, "IO error: {}", e),
            DownloadError::YtDlpFailed(e)   => write!(f, "yt-dlp failed: {}", e),
            DownloadError::FileNotFound(id) => write!(f, "Archivo no encontrado: {}", id),
        }
    }
}

impl std::error::Error for DownloadError {}

impl From<std::io::Error> for DownloadError {
    fn from(e: std::io::Error) -> Self { DownloadError::IoError(e) }
}

/// Forma tolerante del dict de progreso de yt-dlp (`%(progress)j`). No es un
/// contrato público estable de yt-dlp — todos los campos son opcionales para
/// que un cambio de formato en una futura versión no tumbe la descarga, solo
/// deje de publicar ese tick puntual.
#[derive(Debug, Deserialize, Default)]
struct YtDlpProgress {
    status:           Option<String>,
    downloaded_bytes: Option<f64>,
    total_bytes:      Option<f64>,
    #[serde(default)]
    total_bytes_estimate: Option<f64>,
    speed:            Option<f64>,
    eta:              Option<f64>,
}

#[derive(Clone)]
pub struct DownloadService {
    cache_dir: PathBuf,
    events:    broadcast::Sender<DownloadEvent>,
}

impl DownloadService {
    pub fn new(cache_dir: impl Into<PathBuf>, events: broadcast::Sender<DownloadEvent>) -> Self {
        Self { cache_dir: cache_dir.into(), events }
    }

    pub async fn download(&self, track: &Track) -> Result<String, DownloadError> {
        tokio::fs::create_dir_all(&self.cache_dir).await?;

        let output_template = self.cache_dir
            .join("%(id)s.%(ext)s")
            .to_string_lossy()
            .to_string();

        let url = format!("https://www.youtube.com/watch?v={}", track.id);

        let mut child = Command::new("yt-dlp")
            .args([
                "-f", "ba[ext=webm]/ba[ext=opus]/ba",
                "-x",
                "--audio-format", "opus",
                "-r", "3M",
                "-o", &output_template,
                "--newline",
                "--progress-template", "download:%(progress)j",
                &url,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        let track_id = track.id.clone();
        let events = self.events.clone();
        let stdout_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(progress) = serde_json::from_str::<YtDlpProgress>(&line) else { continue };
                if progress.status.as_deref() != Some("downloading") {
                    continue;
                }

                let _ = events.send(DownloadEvent::Downloading {
                    id: track_id.clone(),
                    downloaded_bytes: progress.downloaded_bytes.map(|b| b as u64),
                    total_bytes: progress.total_bytes
                        .or(progress.total_bytes_estimate)
                        .map(|b| b as u64),
                    speed_bytes_per_sec: progress.speed,
                    eta_seconds: progress.eta.map(|e| e as u64),
                });
            }
        });

        let stderr_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            let mut buf = String::new();
            while let Ok(Some(line)) = lines.next_line().await {
                buf.push_str(&line);
                buf.push('\n');
            }
            buf
        });

        let status = child.wait().await?;
        let _ = stdout_task.await;
        let stderr_output = stderr_task.await.unwrap_or_default();

        if !status.success() {
            let clean_err = stderr_output.lines()
                .find(|l| l.contains("ERROR:"))
                .unwrap_or("Error desconocido de yt-dlp");

            return Err(DownloadError::YtDlpFailed(clean_err.to_string()));
        }

        self.find_file(&track.id).await
    }

    async fn find_file(&self, video_id: &str) -> Result<String, DownloadError> {
        // `download()` siempre fuerza --audio-format opus, así que el archivo
        // de salida es determinístico. Antes esto escaneaba el directorio
        // buscando cualquier nombre que empezara con `video_id`, y podía
        // devolver el .lrc de letras (mismo prefijo, escrito por separado en
        // background) en vez del .opus si `read_dir` los listaba en ese orden.
        let path = self.cache_dir.join(format!("{video_id}.opus"));
        if tokio::fs::try_exists(&path).await? {
            return Ok(path.to_string_lossy().to_string());
        }
        Err(DownloadError::FileNotFound(video_id.to_string()))
    }
}
