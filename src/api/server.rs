use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::net::TcpListener;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{error, info};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::managers::TrackManager;

// ── Protocolo ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct Request {
    action: String,
    #[serde(default)]
    query:  String,
    #[serde(default = "default_radio_limit")]
    limit:  usize,
}

fn default_radio_limit() -> usize { 15 }

#[derive(Serialize)]
#[serde(untagged)]
enum Response {
    Ok    { status: &'static str, data: Value    },
    Error { status: &'static str, message: String },
}

impl Response {
    fn ok(data: impl Serialize) -> Self {
        Self::Ok { status: "ok", data: serde_json::to_value(data).unwrap() }
    }
    fn err(msg: impl Into<String>) -> Self {
        Self::Error { status: "error", message: msg.into() }
    }
}

// ── Server ────────────────────────────────────────────────────────────────────

pub struct TrackHubServer {
    host:        String,
    port:        u16,
    manager:     TrackManager,
    connections: Arc<AtomicUsize>,
}

impl TrackHubServer {
    pub fn new(host: impl Into<String>, port: u16, manager: TrackManager) -> Self {
        Self { host: host.into(), port, manager, connections: Arc::new(AtomicUsize::new(0)) }
    }

    pub async fn start(self) -> std::io::Result<()> {
        let addr = format!("{}:{}", self.host, self.port);
        let listener = TcpListener::bind(&addr).await?;
        info!("Track Hub escuchando en {addr}");

        loop {
            let (stream, peer) = listener.accept().await?;
            info!("Nueva conexión desde {peer}");

            let manager = self.manager.clone();
            let connections = Arc::clone(&self.connections);

            let active = connections.fetch_add(1, Ordering::Relaxed) + 1;
            tracing::debug!("Conexión abierta desde {peer} — activas: {active}");

            tokio::spawn(async move {
                if let Err(e) = Self::handle_client(stream, manager).await {
                    error!("Error con {peer}: {e}");
                }
                let active = connections.fetch_sub(1, Ordering::Relaxed) - 1;
                info!("Conexión cerrada con {peer} — activas: {active}");
            });
        }
    }

    async fn handle_client(
        stream: tokio::net::TcpStream,
        manager: TrackManager,
    ) -> std::io::Result<()> {
        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();

        while let Some(line) = lines.next_line().await? {
            let line = line.trim().to_string();
            if line.is_empty() { continue; }

            info!("← {line}");

            let response = match serde_json::from_str::<Request>(&line) {
                Err(e)  => Response::err(format!("JSON inválido: {e}")),
                Ok(req) => Self::dispatch(req, &manager).await,
            };

            let json = serde_json::to_string(&response).unwrap();

            info!("→ {}", serde_json::from_str::<serde_json::Value>(&json)
                .and_then(|v| serde_json::to_string_pretty(&v))
                .unwrap_or(json.clone()));

            writer.write_all(format!("{json}\n").as_bytes()).await?;
        }

        Ok(())
    }

    async fn dispatch(req: Request, manager: &TrackManager) -> Response {
        match req.action.as_str() {

            // 1. RESOLVE: Obtiene metadatos (locales o remotos). NO descarga el audio.
            // Ideal para "Now Playing" o para verificar info antes de encolar.
            "resolve" => {
                match manager.resolve_metadata(&req.query).await {
                    Ok(track) => Response::ok(track),
                    Err(e)    => Response::err(e.to_string()),
                }
            }

            // 2. SEARCH: Devuelve 'limit' resultados de YouTube.
            "search" => {
                match manager.search(&req.query, req.limit).await {
                    Ok(results) => Response::ok(results),
                    Err(e)      => Response::err(e.to_string()),
                }
            }

            // 3. DOWNLOAD: Fuerza la descarga a Opus, análisis y guardado en BD.
            "download" => {
                match manager.download_track(&req.query).await {
                    Ok(track) => Response::ok(track),
                    Err(e)    => Response::err(e.to_string()),
                }
            }

            // 4. REMOVE: Purga la base de datos y elimina el .opus físico.
            "remove" => {
                match manager.repo.delete_track(&req.query).await {
                    Ok(_)  => Response::ok(serde_json::json!({"message": "deleted"})),
                    Err(e) => Response::err(e.to_string()),
                }
            }

            "radio" => {
                match manager.radio(&req.query, req.limit).await {
                    Ok(results) => Response::ok(results),
                    Err(e)      => Response::err(e.to_string()),
                }
            }

            "album" => {
                match manager.album(&req.query).await {
                    Ok(results) => Response::ok(results),
                    Err(e)      => Response::err(e.to_string()),
                }
            }

            "get_all_ids" => {
                match manager.repo.get_all_ids().await {
                    Ok(ids) => Response::ok(ids),
                    Err(e)  => Response::err(e.to_string()),
                }
            }

            "played" => {
                match manager.played(&req.query).await {
                    Ok(_)  => Response::ok(serde_json::json!({"message": "ok"})),
                    Err(e) => Response::err(e.to_string()),
                }
            }

            _ => Response::err(format!("Acción desconocida: {}", req.action)),
        }
    }
}