use std::path::PathBuf;
use dotenvy::dotenv;

mod infrastructure;
mod model;
mod repository;
mod managers;
mod services;
pub mod api;
pub mod utils;
pub mod lyrics_services;

use repository::TrackRepository;
use crate::infrastructure::init_db_pool;
use crate::api::TrackHubServer;
use crate::managers::TrackManager;
use crate::services::{DownloadService, PythonClient, PythonMicroservice};

const DOWNLOAD_EVENTS_CAPACITY: usize = 256;


#[tokio::main]
async fn main() {
    let _ = dotenv();
    tracing_subscriber::fmt::init();

    let pool = match init_db_pool().await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[Database] Error conectándose a la base de datos: {}", e);
            std::process::exit(1);
        }
    };

    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    sqlx::migrate::Migrator::new(migrations_dir)
        .await
        .expect("[Database] Error cargando migraciones")
        .run(&pool)
        .await
        .expect("[Database] Error corriendo migraciones");

    let metada_services = PythonMicroservice::new("Music_Services/.venv", "Music_Services/hub.py");

    match metada_services.spawn_service().await {
        Ok(_) => println!("[Python] Microservicio iniciado."),
        Err(e) => eprintln!("[Python] Error iniciando microservicio: {}", e),
    }


    let music_storage = std::env::var("MUSIC_STORAGE_PATH")
        .expect("MUSIC_STORAGE_PATH no definida en .env");

    let (download_events_tx, _) = tokio::sync::broadcast::channel(DOWNLOAD_EVENTS_CAPACITY);
    let download_service = DownloadService::new(PathBuf::from(music_storage), download_events_tx.clone());


    let server_host = std::env::var("SERVER_HOST")
        .unwrap_or_else(|_| "0.0.0.0".to_string());

    let server_port: u16 = std::env::var("SERVER_PORT")
        .unwrap_or_else(|_| "7878".to_string())
        .parse()
        .expect("SERVER_PORT debe ser un número válido");

    let python_host = std::env::var("PYTHON_HOST")
        .unwrap_or_else(|_| "127.0.0.1".to_string());

    let python_port: u16 = std::env::var("PYTHON_PORT")
        .unwrap_or_else(|_| "9999".to_string())
        .parse()
        .expect("PYTHON_PORT debe ser un número válido");


    let python_client = PythonClient::new(python_host, python_port);

    let track_repo = TrackRepository::new(pool.clone());
    let track_manager = TrackManager::new(track_repo.clone(), download_service, python_client, download_events_tx);

    println!("Conexión a music_center establecida. Repo listo.");

    TrackHubServer::new(server_host, server_port, track_manager.clone())
        .start()
        .await
        .expect("El servidor falló");
}