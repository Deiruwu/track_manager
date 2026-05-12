use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use serde_json::Value;
use crate::model::Track;

#[derive(Clone)]
pub struct PythonClient {
    addr: String,
}

impl PythonClient {
    pub fn new(host: &str, port: u16) -> Self {
        Self { addr: format!("{}:{}", host, port) }
    }

    pub async fn call(&self, action: &str, query: &str) -> Result<Vec<Track>, String> {
        let stream = TcpStream::connect(&self.addr).await
            .map_err(|e| format!("No se pudo conectar al hub Python: {e}"))?;

        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();

        let payload = format!(
            "{{\"action\":\"{action}\",\"query\":\"{query}\"}}\n"
        );
        writer.write_all(payload.as_bytes()).await
            .map_err(|e| e.to_string())?;

        let line = lines.next_line().await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Python no respondió".to_string())?;

        // ─── INTERCEPCIÓN DEL PAYLOAD CRUDO ───────────────────────────────
        // Usamos info! para asegurarnos de que salga en tu nivel de log actual.
        // Una vez que caces el null, puedes bajar esto a tracing::debug! o quitarlo.
        tracing::info!("Raw payload desde Python para '{}': {}", query, line);
        // ──────────────────────────────────────────────────────────────────

        let val: Value = serde_json::from_str(&line)
            .map_err(|e| e.to_string())?;

        if val["status"] != "ok" {
            return Err(val["message"]
                .as_str()
                .unwrap_or("Error desconocido")
                .to_string());
        }

        // Aquí es donde actualmente está fallando tu código cuando encuentra el null
        serde_json::from_value(val["data"].clone())
            .map_err(|e| format!("Error deserializando Track: {}", e))
        // Añadí un poco de contexto al error final para distinguirlo mejor
    }
}