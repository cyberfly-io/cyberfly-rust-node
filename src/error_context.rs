//! Enhanced Error Types with Context
//!
//! This module provides rich error types that preserve context through
//! the call stack, making debugging and observability much easier.

use thiserror::Error;
use std::fmt;

/// Storage operation errors with contextual information
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Key not found: {key} in database: {db}")]
    KeyNotFound { key: String, db: String },
    
    #[error("Serialization failed for key: {key}, reason: {source}")]
    SerializationError {
        key: String,
        #[source]
        source: serde_json::Error,
    },
    
    #[error("Sled database error: {operation} failed: {source}")]
    SledError {
        operation: String,
        #[source]
        source: sled::Error,
    },
    
    #[error("Iroh blob error: {operation} failed: {source}")]
    IrohError {
        operation: String,
        #[source]
        source: anyhow::Error,
    },
    
    #[error("Cache error: {message}")]
    CacheError { message: String },
    
    #[error("Resource exhausted: {resource}, current: {current}, limit: {limit}")]
    ResourceExhausted {
        resource: String,
        current: usize,
        limit: usize,
    },
    
    #[error("Invalid operation: {reason}")]
    InvalidOperation { reason: String },
    
    #[error("Concurrent modification detected for key: {key}")]
    ConcurrentModification { key: String },
}

/// Network operation errors
#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Peer not found: {peer_id}")]
    PeerNotFound { peer_id: String },
    
    #[error("Connection timeout to peer: {peer_id}, timeout: {timeout_ms}ms")]
    ConnectionTimeout { peer_id: String, timeout_ms: u64 },
    
    #[error("Sync failed with peer: {peer_id}, reason: {reason}")]
    SyncFailed { peer_id: String, reason: String },
    
    #[error("Network operation failed: {operation}: {source}")]
    NetworkOperation {
        operation: String,
        #[source]
        source: anyhow::Error,
    },
}

/// Validation errors with detailed context
#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Invalid signature for key: {key}, public_key: {public_key}")]
    InvalidSignature { key: String, public_key: String },
    
    #[error("Timestamp out of range: {timestamp}, min: {min}, max: {max}")]
    InvalidTimestamp {
        timestamp: i64,
        min: i64,
        max: i64,
    },
    
    #[error("Invalid database name: {name}, reason: {reason}")]
    InvalidDatabaseName { name: String, reason: String },
    
    #[error("Data validation failed: {field}: {reason}")]
    DataValidation { field: String, reason: String },
}

/// Unified application error type
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
    
    #[error("Network error: {0}")]
    Network(#[from] NetworkError),
    
    #[error("Validation error: {0}")]
    Validation(#[from] ValidationError),
    
    #[error("Internal error: {context}: {source}")]
    Internal {
        context: String,
        #[source]
        source: anyhow::Error,
    },
}

/// Result type alias for application operations
pub type AppResult<T> = Result<T, AppError>;

/// Error context builder for adding contextual information
pub struct ErrorContext {
    operation: String,
    metadata: Vec<(String, String)>,
}

impl ErrorContext {
    pub fn new(operation: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            metadata: Vec::new(),
        }
    }
    
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.metadata.push(("key".to_string(), key.into()));
        self
    }
    
    pub fn with_db(mut self, db: impl Into<String>) -> Self {
        self.metadata.push(("database".to_string(), db.into()));
        self
    }
    
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.push((key.into(), value.into()));
        self
    }
    
    pub fn wrap<T, E: std::error::Error + Send + Sync + 'static>(
        self,
        result: Result<T, E>,
    ) -> AppResult<T> {
        result.map_err(|e| {
            let mut context = format!("Operation '{}' failed", self.operation);
            for (k, v) in &self.metadata {
                context.push_str(&format!(", {}: {}", k, v));
            }
            AppError::Internal {
                context,
                source: anyhow::Error::new(e),
            }
        })
    }
}

impl fmt::Display for ErrorContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.operation)?;
        for (k, v) in &self.metadata {
            write!(f, ", {}: {}", k, v)?;
        }
        Ok(())
    }
}

/// Helper macro for creating error context
#[macro_export]
macro_rules! error_context {
    ($op:expr) => {
        $crate::error::ErrorContext::new($op)
    };
    ($op:expr, $($key:expr => $value:expr),+ $(,)?) => {
        {
            let mut ctx = $crate::error::ErrorContext::new($op);
            $(
                ctx = ctx.with_metadata($key, $value.to_string());
            )+
            ctx
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_context() {
        let ctx = ErrorContext::new("test_operation")
            .with_key("my_key")
            .with_db("my_db");
        
        let result: Result<(), std::io::Error> = Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "test error"
        ));
        
        let err = ctx.wrap(result).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("test_operation"));
        assert!(msg.contains("my_key"));
        assert!(msg.contains("my_db"));
    }
}
