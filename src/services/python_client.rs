use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use serde_json::Value;
use serde::de::DeserializeOwned;

pub use crate::utils::hash::generate_fallback_id;

#[derive(Clone)]
pub struct PythonClient {
    addr: String,
}

impl PythonClient {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self { addr: format!("{}:{}", host.into(), port) }
    }

    pub async fn call<T: DeserializeOwned>(&self, action: &str, query: &str) -> Result<T, String> {
        let stream = TcpStream::connect(&self.addr).await
            .map_err(|e| format!("No se pudo conectar al hub Python: {e}"))?;

        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();

        let payload = format!("{{\"action\":\"{action}\",\"query\":\"{query}\"}}\n");
        writer.write_all(payload.as_bytes()).await
            .map_err(|e| e.to_string())?;

        let line = lines.next_line().await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Python no respondio".to_string())?;

        let mut val: Value = serde_json::from_str(&line)
            .map_err(|e| e.to_string())?;

        if val["status"] != "ok" {
            return Err(val["message"]
                .as_str()
                .unwrap_or("Error desconocido")
                .to_string());
        }

        // ─── CAPA ANTICORRUPCIÓN DINÁMICA ──────────────────────────────
        let sanitize_track = |track: &mut Value| {
            if track["thumbnail_url"].is_null() {
                if let Some(url) = track["thumbnail"]["url"].as_str().map(str::to_string) {
                    track["thumbnail_url"] = Value::String(url);
                }
            }
            track.as_object_mut().map(|o| o.remove("thumbnail"));

            // artist.id nulo → fallback generado
            if let Some(artists) = track.get_mut("artists").and_then(|a| a.as_array_mut()) {
                for artist in artists {
                    if artist["id"].is_null() {
                        if let Some(name) = artist["name"].as_str() {
                            let fallback_id = generate_fallback_id("yt_gen", name);
                            artist["id"] = Value::String(fallback_id);
                        }
                    }
                }
            }
        };

        let data_ref = &mut val["data"];
        if let Some(tracks) = data_ref.as_array_mut() {
            for track in tracks {
                sanitize_track(track);
            }
        } else if data_ref.is_object() {
            sanitize_track(data_ref);
        }
        // ───────────────────────────────────────────────────────────────

        serde_json::from_value::<T>(val["data"].take())
            .map_err(|e| format!("Error deserializando el payload: {}", e))
    }
}