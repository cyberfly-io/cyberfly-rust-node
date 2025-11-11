use std::fmt;
use thiserror::Error;

// Common error message constants to reduce string allocations
pub const STORAGE_NOT_FOUND: &str = "Storage not found";
pub const SYNC_MANAGER_NOT_FOUND: &str = "SyncManager not found";
pub const IPFS_STORAGE_NOT_FOUND: &str = "IPFS storage not found";
pub const ENDPOINT_NOT_FOUND: &str = "Endpoint not found";
pub const MQTT_STORE_NOT_FOUND: &str = "MQTT message store not found";
pub const MQTT_BRIDGE_NOT_AVAILABLE: &str = "MQTT bridge not available";
pub const INVALID_TIMESTAMP: &str = "Invalid timestamp";
pub const INVALID_TIMESTAMP_FORMAT: &str = "Invalid timestamp format";
pub const MESSAGE_BROADCAST_NOT_FOUND: &str = "Message broadcast channel not found";
pub const SYNC_OUTBOUND_NOT_FOUND: &str = "Sync outbound sender not found";
pub const DISCOVERED_PEERS_NOT_FOUND: &str = "Discovered peers map not found";

/// Enhanced error types with context and recovery information
#[derive(Error, Debug, Clone)]
pub enum DbError {
    #[error("Signature verification failed: {0}")]
    SignatureError(String),
    
    #[error("Storage operation failed: {0}")]
    StorageError(String),
    
    #[error("Network operation failed: {0}")]
    NetworkError(String),
    
    #[error("CRDT operation failed: {0}")]
    CrdtError(String),
    
    #[error("Invalid data: {0}")]
    InvalidData(String),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("Internal error: {0}")]
    InternalError(String),
    
    // Optimized static error variant - no heap allocation
    #[error("{0}")]
    StaticError(&'static str),
    
    // New enhanced error types
    #[error("Sync operation failed: {0}")]
    SyncError(String),
    
    #[error("Rate limit exceeded: {0}")]
    RateLimitError(String),
    
    #[error("Authentication failed: {0}")]
    AuthError(String),
    
    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),
    
    #[error("Operation timed out: {0}")]
    TimeoutError(String),
}

impl DbError {
    pub fn is_recoverable(&self) -> bool {
        match self {
            DbError::NetworkError(_) => true,
            DbError::TimeoutError(_) => true,
            DbError::RateLimitError(_) => true,
            DbError::ResourceExhausted(_) => true,
            DbError::SyncError(_) => true,
            DbError::StorageError(_) => true, // Most storage errors are transient
            DbError::StaticError(_) => false, // Static errors are usually unrecoverable
            _ => false,
        }
    }
}

/// Result type with enhanced error context
pub type Result<T> = std::result::Result<T, DbError>;

impl From<anyhow::Error> for DbError {
    fn from(err: anyhow::Error) -> Self {
        DbError::InternalError(err.to_string())
    }
}

impl From<std::io::Error> for DbError {
    fn from(err: std::io::Error) -> Self {
        DbError::StorageError(err.to_string())
    }
}

impl From<serde_json::Error> for DbError {
    fn from(err: serde_json::Error) -> Self {
        DbError::InvalidData(format!("JSON error: {}", err))
    }
}
