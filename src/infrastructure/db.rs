use sqlx::postgres::{PgPool, PgPoolOptions};
use std::env;
use std::time::Duration;

/// Inicializa el pool de conexiones a PostgreSQL usando la variable de entorno DATABASE_URL.
pub async fn init_db_pool() -> Result<PgPool, sqlx::Error> {
    // Extraemos la URL.
    // Práctica recomendada: Fallar rápido (Fail-fast) usando `expect`.
    // Mala práctica a evitar: Proveer un fallback hardcodeado tipo `.unwrap_or("postgres://...".into())`.
    // Si el entorno está mal configurado, la aplicación no debe arrancar.
    let db_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL debe estar definida en el entorno o en el archivo .env");

    // Construcción del Pool.
    // Cuándo NO usar `PgPool::connect(&db_url)` directamente: En cualquier entorno que no sea
    // un script desechable o testing local trivial. `connect()` usa defaults peligrosos
    // (ej. timeouts infinitos) que pueden saturar la DB o agotar los hilos de Tokio si hay un pico de latencia.
    let pool = PgPoolOptions::new()
        // Limita el número de conexiones simultáneas. Un valor muy alto satura la RAM de Postgres
        // (cada conexión es un proceso en PG). 10-20 suele ser un buen punto de partida para apps web concurrentes.
        .max_connections(15)
        // TRADEOFF (acquire_timeout): Si el pool está lleno, ¿cuánto esperamos por una conexión libre?
        // Si no pones esto, las peticiones se encolan infinitamente. 3-5 segundos aborta el request
        // rápido devolviendo un error 500/503, protegiendo al sistema de una cascada de fallos.
        .acquire_timeout(Duration::from_secs(3))
        // Limpia conexiones que llevan mucho tiempo sin usarse (evita state leaks y alivia a Postgres).
        .idle_timeout(Duration::from_secs(600))
        // Valida que la conexión realmente funcione antes de devolver el pool
        .connect(&db_url)
        .await?;

    Ok(pool)
}