use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use crate::model::Track;

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

#[derive(Clone)]
pub struct DownloadService {
    cache_dir: PathBuf,
}

impl DownloadService {
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self { cache_dir: cache_dir.into() }
    }

    pub async fn download(&self, track: &Track) -> Result<String, DownloadError> {
        tokio::fs::create_dir_all(&self.cache_dir).await?;

        let output_template = self.cache_dir
            .join("%(id)s.%(ext)s")
            .to_string_lossy()
            .to_string();

        let url = format!("https://www.youtube.com/watch?v={}", track.id);

        let status = Command::new("yt-dlp")
            .args([
                "-x",
                "--audio-quality", "0",
                "--extractor-args", "youtube:player_client=android",
                "-r", "2M",
                "-o", &output_template,
                &url,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await?;

        if !status.success() {
            return Err(DownloadError::YtDlpFailed(format!("exit status: {}", status)));
        }

        self.find_file(&track.id).await
    }

    async fn find_file(&self, video_id: &str) -> Result<String, DownloadError> {
        let mut entries = tokio::fs::read_dir(&self.cache_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_name().to_string_lossy().starts_with(video_id) {
                return Ok(entry.path().to_string_lossy().to_string());
            }
        }
        Err(DownloadError::FileNotFound(video_id.to_string()))
    }
}