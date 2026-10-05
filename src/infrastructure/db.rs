use std::env;
use std::time::Duration;

#[cfg(all(feature = "postgres", feature = "sqlite"))]
compile_error!("Los features `postgres` y `sqlite` son excluyentes: usa `--no-default-features --features sqlite`.");

#[cfg(not(any(feature = "postgres", feature = "sqlite")))]
compile_error!("Activa uno de los features `postgres` o `sqlite`.");

/// Backend de BD elegido por feature de cargo.
#[cfg(feature = "postgres")]
pub type Db = sqlx::Postgres;
#[cfg(feature = "sqlite")]
pub type Db = sqlx::Sqlite;

pub type DbPool = sqlx::Pool<Db>;

/// Migraciones embebidas del backend activo.
#[cfg(feature = "postgres")]
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("migrations/postgres");
#[cfg(feature = "sqlite")]
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("migrations/sqlite");

/// Inicializa el pool de conexiones a PostgreSQL a partir de las variables POSTGRES_*.
#[cfg(feature = "postgres")]
pub async fn init_db_pool() -> Result<DbPool, sqlx::Error> {
    use sqlx::postgres::PgPoolOptions;

    // Extraemos la URL.
    // Si el entorno está mal configurado, la aplicación no debe arrancar.
    let db_url = format!(
        "postgres://{}:{}@{}/{}",
        env::var("POSTGRES_USER").expect("POSTGRES_USER no definida"),
        env::var("POSTGRES_PASSWORD").expect("POSTGRES_PASSWORD no definida"),
        env::var("POSTGRES_HOST").expect("POSTGRES_HOST no definida"),
        env::var("POSTGRES_DB").expect("POSTGRES_DB no definida"),
    );

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

/// Inicializa el pool SQLite en `DATABASE_URL` (por defecto `sqlite://track_manager.db`), creando el archivo si no existe.
#[cfg(feature = "sqlite")]
pub async fn init_db_pool() -> Result<DbPool, sqlx::Error> {
    use std::str::FromStr;
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

    let db_url = env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://track_manager.db".to_string());

    let options = SqliteConnectOptions::from_str(&db_url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));

    SqlitePoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect_with(options)
        .await
}
