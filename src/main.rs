use dotenvy::dotenv;

mod infrastructure;
mod model;
mod repository;
mod managers;
mod services;
pub mod api;

use repository::TrackRepository;
use crate::infrastructure::init_db_pool;
use crate::api::TrackHubServer;
use crate::managers::TrackManager;
use crate::services::{DownloadService, PythonClient, PythonMicroservice};


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

    let metada_services = PythonMicroservice::new("Music_Services/.venv", "Music_Services/hub.py");

    match metada_services.spawn_service().await {
        Ok(_) => println!("[Python] Microservicio iniciado."),
        Err(e) => eprintln!("[Python] Error iniciando microservicio: {}", e),
    }


    let home_dir = std::env::var("HOME").expect("No se pudo obtener la ruta del directorio de inicio");
    let dowload_service = DownloadService::new(home_dir + "/music_storage");


    let python_client = PythonClient::new("127.0.0.1", 9999);

    let track_repo = TrackRepository::new(pool.clone());
    let track_manager = TrackManager::new(track_repo.clone(), dowload_service, python_client);

    println!("Conexión a music_center establecida. Repo listo.");

    TrackHubServer::new("0.0.0.0", 7878, track_manager.clone())
        .start()
        .await
        .expect("El servidor falló");
}