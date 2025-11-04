pub mod config;
pub mod crdt;
pub mod crypto;
pub mod error;
pub mod error_context;
pub mod filters;
pub mod graphql;
pub mod graphql_indexing;
pub mod indexing;
pub mod ipfs;
pub mod iroh_network;
pub mod kadena;
pub mod metrics;
pub mod mqtt_bridge;
pub mod resource_manager;
pub mod retry;
pub mod state_manager;
pub mod storage;
pub mod sync;

// Re-export commonly used types for easier testing
pub use crate::crdt::CrdtStore;
pub use crate::crypto::{verify_signature, validate_timestamp, secure_hex_decode, verify_db_name_secure, constant_time_eq, generate_db_name, verify_db_name, extract_name_from_db};
pub use crate::error::DbError;
pub use crate::graphql::{QueryRoot, MutationRoot, SubscriptionRoot, ApiSchema, SignedData, StorageResult, QueryResult};
pub use crate::indexing::{IndexManager, SecondaryIndex, IndexType, QueryOperator, QueryResult as IndexQueryResult};
pub use crate::storage::{RedisStorage, StoreType, SignatureMetadata, StoredEntry, SortedSetEntry, BatchWriter, BatchWriterStats};
pub use crate::sync::{SyncStore, SyncManager, SignedOperation, SyncMessage};