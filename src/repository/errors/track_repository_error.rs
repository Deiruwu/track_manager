#[derive(Debug)]
pub enum RepositoryError {
    Sqlx(sqlx::Error),
}

impl std::fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RepositoryError::Sqlx(e) => write!(f, "Database error: {}", e),
        }
    }
}

impl std::error::Error for RepositoryError {}

impl From<sqlx::Error> for RepositoryError {
    fn from(e: sqlx::Error) -> Self { RepositoryError::Sqlx(e) }
}
