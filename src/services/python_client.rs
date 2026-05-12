use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use serde_json::Value;
use crate::model::Track;

pub use crate::utils::hash::generate_fallback_id;

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

        // 1. Convertimos el JSON crudo en un Value mutable
        let mut val: Value = serde_json::from_str(&line)
            .map_err(|e| e.to_string())?;

        if val["status"] != "ok" {
            return Err(val["message"]
                .as_str()
                .unwrap_or("Error desconocido")
                .to_string());
        }

        // ─── CAPA ANTICORRUPCIÓN ───────────────────────────────────────
        // Navegamos por el árbol del JSON: data -> array de tracks -> array de artists
        if let Some(tracks) = val["data"].as_array_mut() {
            for track in tracks {
                if let Some(artists) = track["artists"].as_array_mut() {
                    for artist in artists {
                        // Si detectamos la basura de YouTube (null)
                        if artist["id"].is_null() {
                            if let Some(name) = artist["name"].as_str() {
                                // Generamos el ID y lo inyectamos directamente en el JSON
                                let fallback_id = generate_fallback_id("yt_gen", name);
                                artist["id"] = Value::String(fallback_id);
                            }
                        }
                    }
                }
            }
        }
        // ───────────────────────────────────────────────────────────────

        // 2. Le pasamos el JSON ya purificado a Serde.
        // Usamos .take() en lugar de .clone() para mover la memoria sin copiarla,
        // maximizando el rendimiento en Rust.
        serde_json::from_value(val["data"].take())
            .map_err(|e| format!("Error deserializando Track: {}", e))
    }
}