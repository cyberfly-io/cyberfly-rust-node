use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("Signature verification failed: {0}")]
    SignatureError(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("CRDT merge error: {0}")]
    CrdtError(String),

    #[error("Invalid data format: {0}")]
    InvalidData(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Internal error: {0}")]
    InternalError(String),
}

impl From<anyhow::Error> for DbError {
    fn from(err: anyhow::Error) -> Self {
        DbError::InternalError(err.to_string())
    }
}
