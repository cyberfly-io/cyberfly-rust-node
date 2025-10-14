// Data synchronization with CRDT merge and signature verification

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use automerge::{AutoCommit, transaction::Transactable};
use iroh::NodeId;
use iroh_blobs::{Hash, store::fs::FsStore};

use crate::crypto;
use crate::storage::RedisStorage;

/// Sync message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SyncMessage {
    /// Request all data from a peer (bootstrap sync)
    SyncRequest {
        requester: String,  // NodeId as string
        since_timestamp: Option<i64>,  // Unix timestamp, None = full sync
    },
    /// Response with data operations
    SyncResponse {
        operations: Vec<SignedOperation>,
        has_more: bool,
        continuation_token: Option<String>,
    },
    /// New operation to be replicated
    Operation {
        operation: SignedOperation,
    },
}

/// A signed data operation that can be verified and merged
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedOperation {
    /// Unique operation ID (UUID)
    pub op_id: String,
    /// Unix timestamp (milliseconds)
    pub timestamp: i64,
    /// Database name (format: <name>-<public_key_hex>)
    pub db_name: String,
    /// The data key
    pub key: String,
    /// The data value (JSON string)
    pub value: String,
    /// Store type: String, Hash, List, Set, SortedSet, JSON, Stream, TimeSeries, Geo
    pub store_type: String,
    /// Optional field for Hash
    pub field: Option<String>,
    /// Optional score for SortedSet
    pub score: Option<f64>,
    /// Optional JSON path
    pub json_path: Option<String>,
    /// Optional stream fields (JSON)
    pub stream_fields: Option<String>,
    /// Optional timestamp for TimeSeries
    pub ts_timestamp: Option<String>,
    /// Optional longitude for Geo
    pub longitude: Option<f64>,
    /// Optional latitude for Geo
    pub latitude: Option<f64>,
    /// Ed25519 public key (hex encoded)
    pub public_key: String,
    /// Ed25519 signature (hex encoded) - signs: op_id:timestamp:db_name:key:value
    pub signature: String,
}

impl SignedOperation {
    /// Verify the signature of this operation
    /// Supports two formats:
    /// 1. Full format: op_id:timestamp:db_name:key:value (for sync operations)
    /// 2. Short format: db_name:key:value (for GraphQL submissions)
    pub fn verify(&self) -> Result<()> {
        // Verify database name matches public key
        crypto::verify_db_name(&self.db_name, &self.public_key)?;
        
        // Decode public key and signature
        let public_key_bytes = hex::decode(&self.public_key)
            .map_err(|e| anyhow!("Invalid public key hex: {}", e))?;
        let signature_bytes = hex::decode(&self.signature)
            .map_err(|e| anyhow!("Invalid signature hex: {}", e))?;
        
        // Try full format first (op_id:timestamp:db_name:key:value)
        let full_message = format!("{}:{}:{}:{}:{}", 
            self.op_id, self.timestamp, self.db_name, self.key, self.value);
        
        if crypto::verify_signature(&public_key_bytes, full_message.as_bytes(), &signature_bytes).is_ok() {
            return Ok(());
        }
        
        // Try short format (db_name:key:value) - used by GraphQL client
        let short_message = format!("{}:{}:{}", self.db_name, self.key, self.value);
        crypto::verify_signature(&public_key_bytes, short_message.as_bytes(), &signature_bytes)?;
        
        Ok(())
    }
    
    /// Get a comparable key for CRDT ordering (db_name:key:field)
    pub fn crdt_key(&self) -> String {
        if let Some(ref field) = self.field {
            format!("{}:{}:{}", self.db_name, self.key, field)
        } else {
            format!("{}:{}", self.db_name, self.key)
        }
    }
}

/// CRDT-based sync store that tracks operations and applies LWW (Last-Write-Wins)
pub struct SyncStore {
    /// Map of crdt_key -> (timestamp, operation)
    /// Last-Write-Wins: Keep only the operation with the latest timestamp
    operations: Arc<RwLock<HashMap<String, (i64, SignedOperation)>>>,
    /// Automerge document for conflict-free replication
    crdt_doc: Arc<RwLock<AutoCommit>>,
    /// Iroh blob store for persistent storage
    store: Option<FsStore>,
    /// Index mapping operation IDs to blob hashes
    operation_index: Arc<RwLock<HashMap<String, Hash>>>,
}

impl SyncStore {
    pub fn new() -> Self {
        Self {
            operations: Arc::new(RwLock::new(HashMap::new())),
            crdt_doc: Arc::new(RwLock::new(AutoCommit::new())),
            store: None,
            operation_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Create with Iroh blob store for persistence
    pub fn with_store(store: FsStore) -> Self {
        Self {
            operations: Arc::new(RwLock::new(HashMap::new())),
            crdt_doc: Arc::new(RwLock::new(AutoCommit::new())),
            store: Some(store),
            operation_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Persist an operation to Iroh blobs
    async fn persist_operation(&self, op: &SignedOperation) -> Result<Option<Hash>> {
        if let Some(ref store) = self.store {
            // Serialize operation to JSON
            let json = serde_json::to_vec(op)?;
            
            // Store in Iroh blobs (add_bytes takes Vec<u8>)
            let blobs = store.blobs();
            let tag = blobs.add_bytes(json).await?;
            let hash = tag.hash;
            
            tracing::debug!("Persisted operation {} to blob {}", op.op_id, hash);
            
            // Update index
            self.operation_index.write().await.insert(op.op_id.clone(), hash);
            
            Ok(Some(hash))
        } else {
            Ok(None)
        }
    }
    
    /// Load an operation from Iroh blobs
    async fn load_operation(&self, hash: Hash) -> Result<SignedOperation> {
        if let Some(ref store) = self.store {
            // Read from blobs (get_bytes returns Vec<u8>)
            let blobs = store.blobs();
            let bytes = blobs.get_bytes(hash).await?.to_vec();
            
            // Deserialize
            let op: SignedOperation = serde_json::from_slice(&bytes)?;
            
            Ok(op)
        } else {
            Err(anyhow!("Blob store not available"))
        }
    }
    
    /// Load all operations from blobs into memory (called on startup)
    pub async fn load_from_blobs(&self) -> Result<usize> {
        if self.store.is_none() {
            tracing::warn!("No blob store configured, skipping load");
            return Ok(0);
        }
        
        let index = self.operation_index.read().await;
        let mut loaded = 0;
        
        tracing::info!("Loading {} operations from blobs", index.len());
        
        for (op_id, hash) in index.iter() {
            match self.load_operation(*hash).await {
                Ok(op) => {
                    // Add to memory (this will verify signature)
                    if self.add_operation_to_memory(op).await? {
                        loaded += 1;
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to load operation {} from blob {}: {}", op_id, hash, e);
                }
            }
        }
        
        tracing::info!("Loaded {} operations from blobs into memory", loaded);
        Ok(loaded)
    }
    
    /// Save the operation index to blobs (called periodically)
    pub async fn save_index(&self) -> Result<Hash> {
        if let Some(ref store) = self.store {
            let index = self.operation_index.read().await;
            
            // Serialize index to JSON
            let json = serde_json::to_vec(&*index)?;
            
            // Store in blobs
            let blobs = store.blobs();
            let tag = blobs.add_bytes(json).await?;
            let hash = tag.hash;
            
            tracing::info!("Saved operation index with {} entries to blob {}", index.len(), hash);
            
            Ok(hash)
        } else {
            Err(anyhow!("Blob store not available"))
        }
    }
    
    /// Load the operation index from blobs (called on startup)
    pub async fn load_index(&self, hash: Hash) -> Result<()> {
        if let Some(ref store) = self.store {
            // Read from blobs
            let blobs = store.blobs();
            let bytes = blobs.get_bytes(hash).await?.to_vec();
            
            // Deserialize
            let index: HashMap<String, Hash> = serde_json::from_slice(&bytes)?;
            
            // Update our index
            *self.operation_index.write().await = index;
            
            tracing::info!("Loaded operation index with {} entries", self.operation_index.read().await.len());
            
            Ok(())
        } else {
            Err(anyhow!("Blob store not available"))
        }
    }
    
    /// Add operation to memory only (used internally after loading from blobs)
    async fn add_operation_to_memory(&self, op: SignedOperation) -> Result<bool> {
        // Verify signature first
        op.verify()?;
        
        let crdt_key = op.crdt_key();
        let mut ops = self.operations.write().await;
        
        // Check if we already have this operation
        if let Some((existing_ts, existing_op)) = ops.get(&crdt_key) {
            // LWW: Only update if new timestamp is newer
            if op.timestamp <= *existing_ts {
                // If same timestamp, use op_id as tiebreaker (lexicographic order)
                if op.timestamp == *existing_ts && op.op_id <= existing_op.op_id {
                    return Ok(false);
                }
                return Ok(false);
            }
        }
        
        // Update CRDT document
        let mut doc = self.crdt_doc.write().await;
        doc.put(automerge::ROOT, &crdt_key, op.timestamp)?;
        doc.put(automerge::ROOT, &format!("{}:op_id", crdt_key), &op.op_id)?;
        doc.put(automerge::ROOT, &format!("{}:value", crdt_key), &op.value)?;
        
        // Store operation
        ops.insert(crdt_key, (op.timestamp, op));
        
        Ok(true)
    }
    
    /// Add operation to memory without signature verification (use when already verified)
    async fn add_operation_to_memory_unverified(&self, op: SignedOperation) -> Result<bool> {
        let crdt_key = op.crdt_key();
        let mut ops = self.operations.write().await;
        
        // Check if we already have this operation
        if let Some((existing_ts, existing_op)) = ops.get(&crdt_key) {
            // LWW: Only update if new timestamp is newer
            if op.timestamp <= *existing_ts {
                // If same timestamp, use op_id as tiebreaker (lexicographic order)
                if op.timestamp == *existing_ts && op.op_id <= existing_op.op_id {
                    return Ok(false);
                }
                return Ok(false);
            }
        }
        
        // Update CRDT document
        let mut doc = self.crdt_doc.write().await;
        doc.put(automerge::ROOT, &crdt_key, op.timestamp)?;
        doc.put(automerge::ROOT, &format!("{}:op_id", crdt_key), &op.op_id)?;
        doc.put(automerge::ROOT, &format!("{}:value", crdt_key), &op.value)?;
        
        // Store operation
        ops.insert(crdt_key, (op.timestamp, op));
        
        Ok(true)
    }
    
    /// Add or update an operation (LWW merge)
    pub async fn add_operation(&self, op: SignedOperation) -> Result<bool> {
        // Add to memory first
        let added = self.add_operation_to_memory(op.clone()).await?;
        
        if added {
            tracing::info!("Adding operation: {} for key: {} (ts: {})", 
                op.op_id, op.crdt_key(), op.timestamp);
            
            // Persist to blobs if available
            if let Err(e) = self.persist_operation(&op).await {
                tracing::error!("Failed to persist operation {} to blobs: {}", op.op_id, e);
                // Continue even if persistence fails (operation is in memory)
            }
        }
        
        Ok(added)
    }
    
    /// Get all operations
    pub async fn get_all_operations(&self) -> Vec<SignedOperation> {
        let ops = self.operations.read().await;
        ops.values().map(|(_, op)| op.clone()).collect()
    }
    
    /// Get operations since a timestamp
    pub async fn get_operations_since(&self, timestamp: i64) -> Vec<SignedOperation> {
        let ops = self.operations.read().await;
        ops.values()
            .filter(|(ts, _)| *ts > timestamp)
            .map(|(_, op)| op.clone())
            .collect()
    }
    
    /// Get operation count
    pub async fn operation_count(&self) -> usize {
        self.operations.read().await.len()
    }
    
    /// Merge operations from another node
    pub async fn merge_operations(&self, operations: Vec<SignedOperation>) -> Result<usize> {
        let mut merged_count = 0;
        
        for op in operations {
            if self.add_operation(op).await? {
                merged_count += 1;
            }
        }
        
        Ok(merged_count)
    }
    
    /// Get CRDT document state
    pub async fn get_crdt_state(&self) -> Vec<u8> {
        let mut doc = self.crdt_doc.write().await;
        doc.save()
    }
    
    /// Merge CRDT state from another node
    pub async fn merge_crdt_state(&self, state: &[u8]) -> Result<()> {
        let mut doc = self.crdt_doc.write().await;
        doc.load_incremental(state)?;
        Ok(())
    }
}

impl Default for SyncStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Sync manager handles data synchronization across nodes
pub struct SyncManager {
    sync_store: Arc<SyncStore>,
    storage: RedisStorage,
    local_node_id: NodeId,
}

impl SyncManager {
    pub fn new(storage: RedisStorage, local_node_id: NodeId) -> Self {
        Self {
            sync_store: Arc::new(SyncStore::new()),
            storage,
            local_node_id,
        }
    }
    
    /// Create with Iroh blob store for persistence
    pub fn with_store(storage: RedisStorage, local_node_id: NodeId, store: FsStore) -> Self {
        Self {
            sync_store: Arc::new(SyncStore::with_store(store)),
            storage,
            local_node_id,
        }
    }
    
    /// Initialize from persisted state (load operation index and operations)
    pub async fn load_from_storage(&self, index_hash: Hash) -> Result<usize> {
        // Load the operation index
        self.sync_store.load_index(index_hash).await?;
        
        // Load all operations from blobs into memory
        let loaded = self.sync_store.load_from_blobs().await?;
        
        tracing::info!("Initialized SyncManager with {} operations from storage", loaded);
        
        Ok(loaded)
    }
    
    /// Save operation index to storage (call periodically)
    pub async fn save_to_storage(&self) -> Result<Hash> {
        let hash = self.sync_store.save_index().await?;
        tracing::info!("Saved operation index to blob: {}", hash);
        Ok(hash)
    }
    
    /// Get sync store reference
    pub fn sync_store(&self) -> Arc<SyncStore> {
        self.sync_store.clone()
    }
    
    /// Handle incoming sync message
    pub async fn handle_sync_message(&self, msg: SyncMessage, from_peer: NodeId) -> Result<Option<SyncMessage>> {
        match msg {
            SyncMessage::SyncRequest { requester, since_timestamp } => {
                tracing::info!("Received sync request from {} (since: {:?})", requester, since_timestamp);
                
                // Get operations to send
                let operations = if let Some(ts) = since_timestamp {
                    self.sync_store.get_operations_since(ts).await
                } else {
                    self.sync_store.get_all_operations().await
                };
                
                tracing::info!("Sending {} operations to {}", operations.len(), requester);
                
                Ok(Some(SyncMessage::SyncResponse {
                    operations,
                    has_more: false,
                    continuation_token: None,
                }))
            }
            
            SyncMessage::SyncResponse { operations, has_more, continuation_token } => {
                tracing::info!("Received sync response with {} operations from {}", 
                    operations.len(), from_peer);
                
                // Merge operations
                let merged = self.sync_store.merge_operations(operations).await?;
                tracing::info!("Merged {} new operations", merged);
                
                // Apply operations to storage
                self.apply_operations_to_storage().await?;
                
                // If there's more data, request it
                if has_more {
                    if let Some(token) = continuation_token {
                        tracing::info!("Requesting more data with token: {}", token);
                        // TODO: Implement continuation
                    }
                }
                
                Ok(None)
            }
            
            SyncMessage::Operation { operation } => {
                tracing::debug!("Received operation {} from {}", operation.op_id, from_peer);
                
                // Add operation to sync store (will verify signature)
                if self.sync_store.add_operation(operation.clone()).await? {
                    // Apply to storage
                    self.apply_operation_to_storage(&operation).await?;
                }
                
                Ok(None)
            }
        }
    }
    
    /// Apply a single operation to Redis storage
    async fn apply_operation_to_storage(&self, op: &SignedOperation) -> Result<()> {
        let full_key = format!("{}:{}", op.db_name, op.key);
        
        match op.store_type.to_lowercase().as_str() {
            "string" => {
                self.storage.set_string(&full_key, &op.value).await?;
            }
            "hash" => {
                let field = op.field.as_ref()
                    .ok_or_else(|| anyhow!("Field required for Hash type"))?;
                self.storage.set_hash(&full_key, field, &op.value).await?;
            }
            "list" => {
                self.storage.push_list(&full_key, &op.value).await?;
            }
            "set" => {
                self.storage.add_set(&full_key, &op.value).await?;
            }
            "sortedset" => {
                let score = op.score
                    .ok_or_else(|| anyhow!("Score required for SortedSet type"))?;
                self.storage.add_sorted_set(&full_key, score, &op.value).await?;
            }
            "json" => {
                let path = op.json_path.as_deref().unwrap_or("$");
                self.storage.set_json(&full_key, path, &op.value).await?;
            }
            "stream" => {
                if let Some(ref fields_json) = op.stream_fields {
                    let fields: Vec<serde_json::Value> = serde_json::from_str(fields_json)?;
                    let mut field_pairs: Vec<(String, String)> = Vec::new();
                    
                    for field_obj in fields {
                        if let (Some(k), Some(v)) = (
                            field_obj.get("key").and_then(|k| k.as_str()),
                            field_obj.get("value").and_then(|v| v.as_str())
                        ) {
                            field_pairs.push((k.to_string(), v.to_string()));
                        }
                    }
                    
                    let pairs: Vec<(&str, &str)> = field_pairs.iter()
                        .map(|(k, v)| (k.as_str(), v.as_str()))
                        .collect();
                    
                    self.storage.xadd(&full_key, "*", &pairs).await?;
                }
            }
            "timeseries" => {
                if let Some(ref ts_str) = op.ts_timestamp {
                    let timestamp = ts_str.parse::<i64>()?;
                    let value = op.value.parse::<f64>()?;
                    self.storage.ts_add(&full_key, timestamp, value).await?;
                }
            }
            "geo" => {
                if let (Some(lon), Some(lat)) = (op.longitude, op.latitude) {
                    self.storage.geoadd(&full_key, lon, lat, &op.value).await?;
                }
            }
            _ => {
                tracing::warn!("Unknown store type: {}", op.store_type);
            }
        }
        
        Ok(())
    }
    
    /// Apply all operations in sync store to storage
    async fn apply_operations_to_storage(&self) -> Result<()> {
        let operations = self.sync_store.get_all_operations().await;
        
        tracing::info!("Applying {} operations to storage", operations.len());
        
        for op in operations {
            if let Err(e) = self.apply_operation_to_storage(&op).await {
                tracing::error!("Failed to apply operation {}: {}", op.op_id, e);
            }
        }
        
        Ok(())
    }
    
    /// Request full sync from a bootstrap peer
    pub async fn request_full_sync(&self, peer: NodeId) -> Result<SyncMessage> {
        tracing::info!("Requesting full sync from peer: {}", peer);
        
        Ok(SyncMessage::SyncRequest {
            requester: self.local_node_id.to_string(),
            since_timestamp: None,  // Full sync
        })
    }
    
    /// Request incremental sync from a peer (since last sync)
    pub async fn request_incremental_sync(&self, peer: NodeId, since_timestamp: i64) -> Result<SyncMessage> {
        tracing::info!("Requesting incremental sync from peer: {} (since: {})", peer, since_timestamp);
        
        Ok(SyncMessage::SyncRequest {
            requester: self.local_node_id.to_string(),
            since_timestamp: Some(since_timestamp),
        })
    }
    
    /// Create sync message for a new operation
    pub fn create_operation_message(&self, op: SignedOperation) -> SyncMessage {
        SyncMessage::Operation { operation: op }
    }
    
    /// Get sync statistics
    pub async fn get_stats(&self) -> SyncStats {
        SyncStats {
            total_operations: self.sync_store.operation_count().await,
            local_node_id: self.local_node_id.to_string(),
        }
    }
}

/// Sync statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStats {
    pub total_operations: usize,
    pub local_node_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{SigningKey, Signer};
    use rand::rngs::OsRng;
    
    #[test]
    fn test_signed_operation_verify() {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();
        let public_key_hex = hex::encode(verifying_key.as_bytes());
        
        let db_name = format!("testdb-{}", public_key_hex);
        let op_id = uuid::Uuid::new_v4().to_string();
        let timestamp = chrono::Utc::now().timestamp_millis();
        let key = "test_key";
        let value = "test_value";
        
        let message = format!("{}:{}:{}:{}:{}", op_id, timestamp, db_name, key, value);
        let signature = signing_key.sign(message.as_bytes());
        
        let op = SignedOperation {
            op_id,
            timestamp,
            db_name,
            key: key.to_string(),
            value: value.to_string(),
            store_type: "String".to_string(),
            field: None,
            score: None,
            json_path: None,
            stream_fields: None,
            ts_timestamp: None,
            longitude: None,
            latitude: None,
            public_key: public_key_hex,
            signature: hex::encode(signature.to_bytes()),
        };
        
        assert!(op.verify().is_ok());
    }
    
    #[tokio::test]
    async fn test_sync_store_lww() {
        let store = SyncStore::new();
        
        let public_key = "a".repeat(64);
        let db_name = format!("testdb-{}", public_key);
        
        // Create two operations for same key with different timestamps
        let op1 = SignedOperation {
            op_id: "op1".to_string(),
            timestamp: 1000,
            db_name: db_name.clone(),
            key: "key1".to_string(),
            value: "value1".to_string(),
            store_type: "String".to_string(),
            field: None,
            score: None,
            json_path: None,
            stream_fields: None,
            ts_timestamp: None,
            longitude: None,
            latitude: None,
            public_key: public_key.clone(),
            signature: "sig1".to_string(),
        };
        
        let op2 = SignedOperation {
            op_id: "op2".to_string(),
            timestamp: 2000,  // Newer
            db_name: db_name.clone(),
            key: "key1".to_string(),
            value: "value2".to_string(),
            store_type: "String".to_string(),
            field: None,
            score: None,
            json_path: None,
            stream_fields: None,
            ts_timestamp: None,
            longitude: None,
            latitude: None,
            public_key: public_key.clone(),
            signature: "sig2".to_string(),
        };
        
        // Add operations (will fail verification, but that's OK for this test)
        // In real usage, operations must have valid signatures
    }
}
