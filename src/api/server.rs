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
}

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
    host:    String,
    port:    u16,
    manager: TrackManager,
}

impl TrackHubServer {
    pub fn new(host: impl Into<String>, port: u16, manager: TrackManager) -> Self {
        Self { host: host.into(), port, manager }
    }

    pub async fn start(self) -> std::io::Result<()> {
        let addr = format!("{}:{}", self.host, self.port);
        let listener = TcpListener::bind(&addr).await?;
        info!("Track Hub escuchando en {addr}");

        loop {
            let (stream, peer) = listener.accept().await?;
            info!("Nueva conexión desde {peer}");

            let manager = self.manager.clone();

            tokio::spawn(async move {
                if let Err(e) = Self::handle_client(stream, manager).await {
                    error!("Error con {peer}: {e}");
                }
                info!("Conexión cerrada con {peer}");
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

            "resolve" => {
                match manager.resolve(&req.query).await {
                    Ok(track) => Response::ok(track),
                    Err(e)    => Response::err(e.to_string()),
                }
            }

            "radio" => {
                match manager.radio(&req.query).await {
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

            _ => Response::err(format!("Acción desconocida: {}", req.action)),
        }
    }
}