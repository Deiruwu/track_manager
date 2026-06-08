#[derive(Debug)]
pub enum RepositoryError {
    Sqlx(sqlx::Error),
    Io(std::io::Error),
    Custom(String),
}

impl std::fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RepositoryError::Sqlx(e) => write!(f, "Database error: {}", e),
            RepositoryError::Io(e) => write!(f, "I/O error: {}", e),
            RepositoryError::Custom(msg) => write!(f, "Error: {}", msg),
        }
    }
}

impl std::error::Error for RepositoryError {}

impl From<sqlx::Error> for RepositoryError {
    fn from(e: sqlx::Error) -> Self { RepositoryError::Sqlx(e) }
}

impl From<std::io::Error> for RepositoryError {
    fn from(e: std::io::Error) -> Self { RepositoryError::Io(e) }
}