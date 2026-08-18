use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use serde_json::{json, Value};
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

    /// Llamada simple: action + query string.
    pub async fn call<T: DeserializeOwned>(&self, action: &str, query: &str) -> Result<T, String> {
        self.call_with_payload(json!({
            "action": action,
            "query":  query,
        })).await
    }

    /// Llamada con payload arbitrario — para search (limit, filter) u otros casos.
    pub async fn call_with_payload<T: DeserializeOwned>(&self, payload: Value) -> Result<T, String> {
        let stream = TcpStream::connect(&self.addr).await
            .map_err(|e| format!("No se pudo conectar al hub Python: {e}"))?;

        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();

        writer.write_all((payload.to_string() + "\n").as_bytes()).await
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

        Self::sanitize(&mut val["data"]);

        serde_json::from_value::<T>(val["data"].take())
            .map_err(|e| format!("Error deserializando el payload: {}", e))
    }

    // ─── Helper interno ───────────────────────────────────────────────────────

    fn sanitize(data: &mut Value) {
        let sanitize_track = |track: &mut Value| {
            if track["thumbnail_url"].is_null() {
                if let Some(url) = track["thumbnail"]["url"].as_str().map(str::to_string) {
                    track["thumbnail_url"] = Value::String(url);
                }
            }
            track.as_object_mut().map(|o| o.remove("thumbnail"));

            // El nombre solo (p. ej. "jon-YAKITORY") puede ser un canal/uploader
            // genérico reutilizado por varios artistas reales distintos — hashear
            // solo por nombre los agruparía como si fueran el mismo artista. Se
            // combina con el track_id para que cada canción sin resolver tenga su
            // propia identidad sintética.
            let track_id = track.get("id").and_then(|v| v.as_str()).map(str::to_string);

            if let Some(artists) = track.get_mut("artists").and_then(|a| a.as_array_mut()) {
                for artist in artists {
                    let unresolved = artist["id"].is_null() || artist["id"].as_str() == Some("");
                    if unresolved {
                        if let (Some(tid), Some(name)) = (&track_id, artist["name"].as_str()) {
                            let fallback_id = generate_fallback_id("yt_gen", &format!("{tid}:{name}"));
                            artist["id"] = Value::String(fallback_id);
                        }
                    }
                }
            }
        };

        if let Some(tracks) = data.as_array_mut() {
            for track in tracks { sanitize_track(track); }
        } else if data.is_object() {
            sanitize_track(data);

            for key in ["tracks", "songs"] {
                if let Some(nested) = data.get_mut(key).and_then(|v| v.as_array_mut()) {
                    for track in nested { sanitize_track(track); }
                }
            }
        }
    }
}