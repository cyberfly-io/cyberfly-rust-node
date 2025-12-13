//! Storage Module
//! 
//! This module provides a distributed, content-addressed storage system
//! using Sled (embedded B-tree DB) + Iroh Blobs (content-addressed storage).
//!
//! ## Architecture
//! - **BlobStorage**: Main storage interface (public API)
//! - **TieredCache**: Two-tier LRU cache (hot/warm) with Arc for zero-copy reads
//! - **BatchWriter**: Parallel write processing with semaphore-based concurrency control
//!
//! ## Components
//! - Core storage: Sled index + Iroh blobs
//! - Caching: Moka async cache with 55k total capacity
//! - Metrics: Prometheus integration for observability
//! - Concurrency: Tokio async + blocking thread pools for I/O

use anyhow::Result;
use async_graphql::SimpleObject;
use iroh_blobs::store::fs::FsStore;
use iroh_blobs::Hash;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use moka::future::Cache as MokaCache;
use sled::Db as SledDb;
use crate::metrics::{self, Timer};
use tokio::sync::Semaphore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StoreType {
    String,
    Hash,
    List,
    Set,
    SortedSet,
    Json,
    Stream,
    TimeSeries,
    Geo,
}

// Metadata for signed data verification
#[derive(Debug, Clone, Serialize, Deserialize, SimpleObject)]
pub struct SignatureMetadata {
    pub public_key: String,
    pub signature: String,
    pub timestamp: i64,
}

/// TTL (Time-To-Live) metadata for data expiration
#[derive(Debug, Clone, Serialize, Deserialize, SimpleObject)]
pub struct TtlMetadata {
    /// TTL duration in seconds (None = no expiration)
    pub ttl_seconds: Option<u64>,
    /// Absolute expiration timestamp (Unix millis)
    pub expires_at: Option<i64>,
    /// Creation timestamp (Unix millis)
    pub created_at: i64,
}

impl TtlMetadata {
    /// Create new TTL metadata with optional TTL
    pub fn new(ttl_seconds: Option<u64>) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        
        let expires_at = ttl_seconds.map(|ttl| now + (ttl as i64 * 1000));
        
        Self {
            ttl_seconds,
            expires_at,
            created_at: now,
        }
    }
    
    /// Check if this entry has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;
            now > expires_at
        } else {
            false
        }
    }
    
    /// Get remaining TTL in seconds (None if no expiration or expired)
    pub fn remaining_ttl_seconds(&self) -> Option<u64> {
        if let Some(expires_at) = self.expires_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;
            if now < expires_at {
                Some(((expires_at - now) / 1000) as u64)
            } else {
                None // Expired
            }
        } else {
            None // No expiration set
        }
    }
}

// Data structures for different Redis-like types
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StringValue {
    value: String,
    metadata: Option<SignatureMetadata>,
    ttl: Option<TtlMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HashValue {
    fields: HashMap<String, String>,
    metadata: Option<SignatureMetadata>,
    ttl: Option<TtlMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ListValue {
    items: Vec<String>,
    metadata: Option<SignatureMetadata>,
    ttl: Option<TtlMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SetValue {
    members: HashSet<String>,
    metadata: Option<SignatureMetadata>,
    ttl: Option<TtlMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SortedSetValue {
    // Store JSON objects with scores for sorting
    // Key is the serialized JSON, value is the score (timestamp)
    members: BTreeMap<String, f64>,
    metadata: Option<SignatureMetadata>,
    ttl: Option<TtlMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortedSetEntry {
    pub score: f64,
    pub data: serde_json::Value,
    pub metadata: Option<SignatureMetadata>,
}

/// Unified stored entry representation for get_all
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEntry {
    pub key: String,
    pub store_type: StoreType,
    pub value: serde_json::Value,
    pub metadata: Option<SignatureMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonValue {
    data: serde_json::Value,
    // Track _id for deduplication if present
    id: Option<String>,
    metadata: Option<SignatureMetadata>,
    ttl: Option<TtlMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StreamValue {
    entries: Vec<(String, Vec<(String, String)>)>,
    metadata: Option<SignatureMetadata>,
    ttl: Option<TtlMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TimeSeriesValue {
    points: BTreeMap<i64, f64>,
    metadata: Option<SignatureMetadata>,
    ttl: Option<TtlMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GeoValue {
    locations: HashMap<String, (f64, f64)>,
    metadata: Option<SignatureMetadata>,
    ttl: Option<TtlMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum StoredValue {
    String(StringValue),
    Hash(HashValue),
    List(ListValue),
    Set(SetValue),
    SortedSet(SortedSetValue),
    Json(JsonValue),
    Stream(StreamValue),
    TimeSeries(TimeSeriesValue),
    Geo(GeoValue),
}

/// Tiered cache for performance optimization
/// Hot tier: frequently accessed, small (5k entries)
/// Warm tier: less frequently accessed, larger (50k entries)
/// Uses Arc to eliminate clones on cache hit
struct TieredCache {
    hot: MokaCache<String, Arc<StoredValue>>,
    warm: MokaCache<String, Arc<StoredValue>>,
}

impl TieredCache {
    fn new() -> Self {
        Self {
            hot: MokaCache::builder()
                .max_capacity(5_000)
                .time_to_live(std::time::Duration::from_secs(300)) // 5 minutes
                .build(),
            warm: MokaCache::builder()
                .max_capacity(50_000)
                .time_to_live(std::time::Duration::from_secs(3600)) // 1 hour
                .build(),
        }
    }

    async fn get(&self, key: &str) -> Option<Arc<StoredValue>> {
        // Check hot tier first (most frequent)
        if let Some(value) = self.hot.get(key) {
            metrics::CACHE_HITS.inc();
            metrics::CACHE_HOT_HITS.inc();
            return Some(value);
        }
        
        // Check warm tier
        if let Some(value) = self.warm.get(key) {
            // Promote to hot tier on access
            self.hot.insert(key.to_string(), Arc::clone(&value)).await;
            metrics::CACHE_HITS.inc();
            metrics::CACHE_WARM_HITS.inc();
            return Some(value);
        }
        
        // Cache miss
        metrics::CACHE_MISSES.inc();
        None
    }

    async fn insert(&self, key: String, value: StoredValue) {
        let arc_value = Arc::new(value);
        // Insert only to hot tier; warm tier is for demoted entries
        // This reduces memory usage and write amplification
        self.hot.insert(key, arc_value).await;
        
        // Update cache size metrics
        self.update_size_metrics();
    }

    async fn invalidate(&self, key: &str) {
        self.hot.invalidate(key).await;
        self.warm.invalidate(key).await;
        
        // Update cache size metrics
        self.update_size_metrics();
    }
    
    fn update_size_metrics(&self) {
        metrics::CACHE_SIZE_HOT.set(self.hot.entry_count() as i64);
        metrics::CACHE_SIZE_WARM.set(self.warm.entry_count() as i64);
    }
}

pub struct BlobStorage {
    store: FsStore,
    sled_db: SledDb,
    index_tree: sled::Tree,
    cache: Arc<TieredCache>,
}

impl Clone for BlobStorage {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            sled_db: self.sled_db.clone(),
            index_tree: self.index_tree.clone(),
            cache: Arc::clone(&self.cache),
        }
    }
}

/// BatchWriter for parallel write processing with bounded concurrency
/// Processes multiple writes concurrently using semaphore-based control
pub struct BatchWriter {
    storage: BlobStorage,
    semaphore: Arc<Semaphore>,
    max_concurrent: usize,
}

impl BatchWriter {
    /// Create a new BatchWriter with specified concurrency limit
    /// 
    /// # Arguments
    /// * `storage` - The underlying storage backend
    /// * `max_concurrent` - Maximum number of concurrent write operations (default: 10)
    pub fn new(storage: BlobStorage, max_concurrent: usize) -> Self {
        tracing::info!("BatchWriter initialized with max_concurrent={}", max_concurrent);
        Self {
            storage,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_concurrent,
        }
    }

    /// Write a single item with concurrency control
    pub async fn write_one(
        &self,
        key: String,
        value: StoredValue,
        store_type: StoreType,
    ) -> Result<()> {
        // Acquire permit (blocks if max concurrent writes reached)
        let _permit = self.semaphore.acquire().await
            .map_err(|e| anyhow::anyhow!("Semaphore acquire failed: {}", e))?;
        
        // Perform the write
        self.storage.store_value(&key, value, store_type).await
    }

    /// Write multiple items in parallel with bounded concurrency
    /// 
    /// # Arguments
    /// * `items` - Vector of (key, value, store_type) tuples to write
    /// 
    /// # Returns
    /// Vector of Results, one per item (in same order as input)
    pub async fn write_batch(
        &self,
        items: Vec<(String, StoredValue, StoreType)>,
    ) -> Vec<Result<()>> {
        let batch_size = items.len();
        tracing::debug!("BatchWriter processing {} items with max_concurrent={}", 
            batch_size, self.max_concurrent);

        // Process all writes concurrently (semaphore limits actual parallelism)
        let mut handles = Vec::with_capacity(batch_size);
        
        for (key, value, store_type) in items {
            let storage = self.storage.clone();
            let semaphore = Arc::clone(&self.semaphore);
            
            let handle = tokio::spawn(async move {
                // Acquire permit
                let _permit = semaphore.acquire().await
                    .map_err(|e| anyhow::anyhow!("Semaphore acquire failed: {}", e))?;
                
                // Perform write
                storage.store_value(&key, value, store_type).await
            });
            
            handles.push(handle);
        }

        // Wait for all writes to complete
        let mut results = Vec::with_capacity(batch_size);
        for handle in handles {
            let result = match handle.await {
                Ok(write_result) => write_result,
                Err(e) => Err(anyhow::anyhow!("Task join error: {}", e)),
            };
            results.push(result);
        }

        tracing::debug!("BatchWriter completed {} items", batch_size);
        results
    }

    /// Get statistics about current batch writer state
    pub fn stats(&self) -> BatchWriterStats {
        BatchWriterStats {
            max_concurrent: self.max_concurrent,
            available_permits: self.semaphore.available_permits(),
        }
    }
}

/// Statistics about BatchWriter state
#[derive(Debug, Clone)]
pub struct BatchWriterStats {
    pub max_concurrent: usize,
    pub available_permits: usize,
}

// Helper methods for interacting with sled index
impl BlobStorage {
    fn index_get(&self, key: &str) -> Result<Option<(String, StoreType)>> {
        if let Ok(Some(v)) = self.index_tree.get(key.as_bytes()) {
            let tuple: (String, StoreType) = bincode::deserialize(&v)?;
            Ok(Some(tuple))
        } else {
            Ok(None)
        }
    }

    fn index_exists(&self, key: &str) -> Result<bool> {
        Ok(self.index_tree.contains_key(key.as_bytes())?)
    }

    fn index_remove(&self, key: &str) -> Result<()> {
        self.index_tree.remove(key.as_bytes())?;
        Ok(())
    }

    fn index_keys_with_prefix(&self, prefix: &str) -> Result<Vec<String>> {
        let mut res = Vec::new();
        let prefix_bytes = prefix.as_bytes();
        for item in self.index_tree.scan_prefix(prefix_bytes) {
            let (k, _v) = item?;
            res.push(String::from_utf8(k.to_vec())?);
        }
        Ok(res)
    }
    
    // Async version of index operations using blocking pool
    async fn index_get_async(&self, key: &str) -> Result<Option<(String, StoreType)>> {
        let index_tree = self.index_tree.clone();
        let key_owned = key.to_string();
        
        tokio::task::spawn_blocking(move || {
            if let Ok(Some(v)) = index_tree.get(key_owned.as_bytes()) {
                let tuple: (String, StoreType) = bincode::deserialize(&v)?;
                Ok(Some(tuple))
            } else {
                Ok(None)
            }
        })
        .await
        .map_err(|e| anyhow::anyhow!("Thread join error: {}", e))?
    }
    
    async fn index_keys_with_prefix_async(&self, prefix: &str) -> Result<Vec<String>> {
        let index_tree = self.index_tree.clone();
        let prefix_owned = prefix.to_string();
        
        tokio::task::spawn_blocking(move || {
            let mut res = Vec::new();
            let prefix_bytes = prefix_owned.as_bytes();
            for item in index_tree.scan_prefix(prefix_bytes) {
                let (k, _v) = item?;
                res.push(String::from_utf8(k.to_vec())?);
            }
            Ok::<Vec<String>, anyhow::Error>(res)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Thread join error: {}", e))?
    }
}

impl BlobStorage {
    pub async fn new(store: FsStore, sled_path: Option<PathBuf>) -> Result<Self> {
        tracing::info!("Initializing BlobStorage with FsStore");

        // Determine sled DB path
        let sled_path = sled_path.unwrap_or_else(|| PathBuf::from("./data/sled_db"));
        
        // Production-optimized Sled configuration
        tracing::info!("Configuring Sled with production settings");
        let sled_config = sled::Config::new()
            .path(&sled_path)
            // Large cache for hot data (2GB)
            .cache_capacity(2 * 1024 * 1024 * 1024)
            // Batch writes for better throughput (flush every 1 second)
            .flush_every_ms(Some(1000))
            // Use high-throughput mode for write-heavy workloads
            .mode(sled::Mode::HighThroughput)
            // Enable compression to save disk space
            .use_compression(true)
            // Not temporary - this is production data
            .temporary(false);
        
        let sled_db = sled_config.open()?;
        let index_tree = sled_db.open_tree("storage_index")?;
        
        tracing::info!("Sled configured: cache=2GB, flush=1s, mode=HighThroughput, compression=enabled");

        // Create tiered cache with Arc for zero-copy reads
        // Hot tier: 5k entries, 5min TTL (most frequent)
        // Warm tier: 50k entries, 1hr TTL (less frequent)
        let cache = TieredCache::new();
        tracing::info!("Tiered cache configured: hot=5k/5min, warm=50k/1hr, Arc-based zero-copy");

        let storage = Self {
            store,
            sled_db,
            index_tree,
            cache: Arc::new(cache),
        };

        tracing::info!("BlobStorage initialized successfully with sled index at {:?}", sled_path);
        Ok(storage)
    }

    /// Create a BatchWriter for parallel write processing
    /// 
    /// # Arguments
    /// * `max_concurrent` - Maximum number of concurrent write operations (default: 10)
    /// 
    /// # Returns
    /// A BatchWriter instance configured for this storage
    pub fn batch_writer(&self, max_concurrent: Option<usize>) -> BatchWriter {
        let concurrency = max_concurrent.unwrap_or(10);
        BatchWriter::new(self.clone(), concurrency)
    }

    async fn load_index(&self) -> Result<()> {
        // Index is persisted in sled and is available on-disk; nothing to load into memory.
        Ok(())
    }

    /// Save the current storage index to blobs and return the blob hash
    pub async fn save_index_hash(&self) -> Result<Hash> {
        // Index is persisted in sled; we don't snapshot it to FsStore anymore.
        Err(anyhow::anyhow!("Storage index is persisted in sled only; no blob snapshot available"))
    }

    /// Load the storage index from a specific blob hash (restores the in-memory index)
    pub async fn load_index_from_hash(&self, _hash: Hash) -> Result<()> {
        // Index is persisted in sled; we don't load a snapshot from FsStore anymore.
        Err(anyhow::anyhow!("Storage index is persisted in sled only; load from blob is not supported"))
    }

    async fn save_index(&self) -> Result<()> {
        // Index is persisted in sled; do not snapshot to FsStore to keep index-only in sled.
        Ok(())
    }

    async fn store_value(
        &self,
        key: &str,
        value: StoredValue,
        store_type: StoreType,
    ) -> Result<()> {
        let timer = Timer::new();
        
        // OPTIMIZED: Use bincode instead of JSON for internal storage (3-5x faster, smaller)
        // Only use JSON for external APIs that require it
        let value_clone = value.clone();
        let value_bytes = tokio::task::spawn_blocking(move || {
            bincode::serialize(&value_clone)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Thread join error: {}", e))??;
        
        // Store in Iroh blobs (async I/O)
        let blobs = self.store.blobs();
        let tag = blobs.add_bytes(value_bytes).await?;
        let hash_str = tag.hash.to_string();

        // Offload Sled write to blocking thread pool
        let index_tree = self.index_tree.clone();
        let key_owned = key.to_string();
        let store_type_clone = store_type.clone();
        
        tokio::task::spawn_blocking(move || {
            let val = bincode::serialize(&(hash_str.clone(), store_type_clone))?;
            index_tree.insert(key_owned.as_bytes(), val)?;
            Ok::<_, anyhow::Error>(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("Thread join error: {}", e))??;

        // Update cache (fast, in-memory, Arc-based)
        self.cache.insert(key.to_string(), value).await;
        
        timer.observe_duration_seconds(&metrics::WRITE_LATENCY);
        metrics::STORAGE_WRITES.inc();
        
        tracing::debug!(key = %key, blob = %tag.hash, "Stored key in blob and updated index");

        Ok(())
    }

    /// Helper to extract TTL metadata from a StoredValue
    fn get_ttl_metadata(value: &StoredValue) -> Option<&TtlMetadata> {
        match value {
            StoredValue::String(v) => v.ttl.as_ref(),
            StoredValue::Hash(v) => v.ttl.as_ref(),
            StoredValue::List(v) => v.ttl.as_ref(),
            StoredValue::Set(v) => v.ttl.as_ref(),
            StoredValue::SortedSet(v) => v.ttl.as_ref(),
            StoredValue::Json(v) => v.ttl.as_ref(),
            StoredValue::Stream(v) => v.ttl.as_ref(),
            StoredValue::TimeSeries(v) => v.ttl.as_ref(),
            StoredValue::Geo(v) => v.ttl.as_ref(),
        }
    }

    /// Check if a value has expired based on TTL
    fn is_value_expired(value: &StoredValue) -> bool {
        if let Some(ttl) = Self::get_ttl_metadata(value) {
            ttl.is_expired()
        } else {
            false
        }
    }

    async fn get_value(&self, key: &str) -> Result<Option<StoredValue>> {
        let timer = Timer::new();
        
        // Check cache first (fast path, Arc-based zero-copy)
        if let Some(value) = self.cache.get(key).await {
            // Check TTL before returning cached value
            if Self::is_value_expired(&value) {
                // Expired - invalidate cache and delete from storage
                self.cache.invalidate(key).await;
                self.delete_key(key).await.ok(); // Best effort cleanup
                metrics::TTL_KEYS_EXPIRED.inc();
                timer.observe_duration_seconds(&metrics::READ_LATENCY);
                metrics::STORAGE_READS.inc();
                return Ok(None);
            }
            timer.observe_duration_seconds(&metrics::READ_LATENCY);
            metrics::STORAGE_READS.inc();
            return Ok(Some((*value).clone()));
        }

        // Offload Sled lookup to blocking thread pool to prevent executor blocking
        let index_tree = self.index_tree.clone();
        let key_owned = key.to_string();
        
        let index_result = tokio::task::spawn_blocking(move || {
            index_tree.get(key_owned.as_bytes())
        })
        .await
        .map_err(|e| anyhow::anyhow!("Thread join error: {}", e))??;
        
        let (hash_str, _store_type) = match index_result {
            Some(v) => {
                let tuple: (String, StoreType) = bincode::deserialize(&v)?;
                tuple
            }
            None => {
                timer.observe_duration_seconds(&metrics::READ_LATENCY);
                metrics::STORAGE_READS.inc();
                return Ok(None);
            }
        };

        // Fetch from Iroh blobs (already async)
        let hash: Hash = hash_str.parse()?;
        let blobs = self.store.blobs();
        let value_bytes = blobs.get_bytes(hash).await?.to_vec();
        
        // OPTIMIZED: Use bincode instead of JSON for deserialization (matches store_value optimization)
        let value = tokio::task::spawn_blocking(move || {
            bincode::deserialize::<StoredValue>(&value_bytes)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Thread join error: {}", e))??;

        // Check TTL before caching and returning
        if Self::is_value_expired(&value) {
            // Expired - delete from storage
            self.delete_key(key).await.ok(); // Best effort cleanup
            metrics::TTL_KEYS_EXPIRED.inc();
            timer.observe_duration_seconds(&metrics::READ_LATENCY);
            metrics::STORAGE_READS.inc();
            return Ok(None);
        }

        // Update cache (Arc-based)
        self.cache.insert(key.to_string(), value.clone()).await;

        timer.observe_duration_seconds(&metrics::READ_LATENCY);
        metrics::STORAGE_READS.inc();

        Ok(Some(value))
    }

    // String Operations
    pub async fn set_string(&self, key: &str, value: &str) -> Result<()> {
        self.set_string_with_metadata(key, value, None).await
    }

    pub async fn set_string_with_metadata(
        &self,
        key: &str,
        value: &str,
        metadata: Option<SignatureMetadata>,
    ) -> Result<()> {
        self.set_string_with_ttl(key, value, metadata, None).await
    }

    /// Set string with optional TTL (seconds)
    pub async fn set_string_with_ttl(
        &self,
        key: &str,
        value: &str,
        metadata: Option<SignatureMetadata>,
        ttl_seconds: Option<u64>,
    ) -> Result<()> {
        let ttl = ttl_seconds.map(|s| TtlMetadata::new(Some(s)));
        if ttl.is_some() {
            metrics::TTL_KEYS_TOTAL.inc();
        }
        let stored_value = StoredValue::String(StringValue {
            value: value.to_string(),
            metadata,
            ttl,
        });
        self.store_value(key, stored_value, StoreType::String).await
    }

    pub async fn get_string(&self, key: &str) -> Result<Option<String>> {
        match self.get_value(key).await? {
            Some(StoredValue::String(sv)) => Ok(Some(sv.value)),
            None => Ok(None),
            _ => Err(anyhow::anyhow!("Key is not a string type")),
        }
    }

    // Hash Operations
    pub async fn set_hash(&self, key: &str, field: &str, value: &str) -> Result<()> {
        self.set_hash_with_metadata(key, field, value, None).await
    }

    pub async fn set_hash_with_metadata(
        &self,
        key: &str,
        field: &str,
        value: &str,
        metadata: Option<SignatureMetadata>,
    ) -> Result<()> {
        self.set_hash_with_ttl(key, field, value, metadata, None).await
    }

    /// Set hash field with optional TTL (seconds)
    pub async fn set_hash_with_ttl(
        &self,
        key: &str,
        field: &str,
        value: &str,
        metadata: Option<SignatureMetadata>,
        ttl_seconds: Option<u64>,
    ) -> Result<()> {
        let mut hash_value = match self.get_value(key).await? {
            Some(StoredValue::Hash(hv)) => hv,
            None => {
                let ttl = ttl_seconds.map(|s| TtlMetadata::new(Some(s)));
                if ttl.is_some() {
                    metrics::TTL_KEYS_TOTAL.inc();
                }
                HashValue {
                    fields: HashMap::new(),
                    metadata: metadata.clone(),
                    ttl,
                }
            },
            _ => return Err(anyhow::anyhow!("Key is not a hash type")),
        };

        hash_value
            .fields
            .insert(field.to_string(), value.to_string());
        if metadata.is_some() {
            hash_value.metadata = metadata;
        }
        self.store_value(key, StoredValue::Hash(hash_value), StoreType::Hash)
            .await
    }

    pub async fn get_hash(&self, key: &str, field: &str) -> Result<Option<String>> {
        match self.get_value(key).await? {
            Some(StoredValue::Hash(hv)) => Ok(hv.fields.get(field).cloned()),
            None => Ok(None),
            _ => Err(anyhow::anyhow!("Key is not a hash type")),
        }
    }

    pub async fn get_all_hash(&self, key: &str) -> Result<Vec<(String, String)>> {
        match self.get_value(key).await? {
            Some(StoredValue::Hash(hv)) => Ok(hv.fields.into_iter().collect()),
            None => Ok(Vec::new()),
            _ => Err(anyhow::anyhow!("Key is not a hash type")),
        }
    }

    // List Operations
    pub async fn push_list(&self, key: &str, value: &str) -> Result<()> {
        self.push_list_with_metadata(key, value, None).await
    }

    pub async fn push_list_with_metadata(
        &self,
        key: &str,
        value: &str,
        metadata: Option<SignatureMetadata>,
    ) -> Result<()> {
        self.push_list_with_ttl(key, value, metadata, None).await
    }

    /// Push to list with optional TTL (seconds)
    pub async fn push_list_with_ttl(
        &self,
        key: &str,
        value: &str,
        metadata: Option<SignatureMetadata>,
        ttl_seconds: Option<u64>,
    ) -> Result<()> {
        let mut list_value = match self.get_value(key).await? {
            Some(StoredValue::List(lv)) => lv,
            None => {
                let ttl = ttl_seconds.map(|s| TtlMetadata::new(Some(s)));
                if ttl.is_some() {
                    metrics::TTL_KEYS_TOTAL.inc();
                }
                ListValue {
                    items: Vec::new(),
                    metadata: metadata.clone(),
                    ttl,
                }
            },
            _ => return Err(anyhow::anyhow!("Key is not a list type")),
        };

        list_value.items.push(value.to_string());
        if metadata.is_some() {
            list_value.metadata = metadata;
        }
        self.store_value(key, StoredValue::List(list_value), StoreType::List)
            .await
    }

    pub async fn get_list(&self, key: &str, start: isize, stop: isize) -> Result<Vec<String>> {
        match self.get_value(key).await? {
            Some(StoredValue::List(lv)) => {
                let len = lv.items.len() as isize;
                let start = if start < 0 {
                    (len + start).max(0)
                } else {
                    start.min(len)
                } as usize;
                let stop = if stop < 0 {
                    (len + stop + 1).max(0)
                } else {
                    (stop + 1).min(len)
                } as usize;

                if start >= stop {
                    return Ok(Vec::new());
                }

                Ok(lv.items[start..stop].to_vec())
            }
            None => Ok(Vec::new()),
            _ => Err(anyhow::anyhow!("Key is not a list type")),
        }
    }

    // Set Operations
    pub async fn add_set(&self, key: &str, member: &str) -> Result<()> {
        self.add_set_with_metadata(key, member, None).await
    }

    pub async fn add_set_with_metadata(
        &self,
        key: &str,
        member: &str,
        metadata: Option<SignatureMetadata>,
    ) -> Result<()> {
        self.add_set_with_ttl(key, member, metadata, None).await
    }

    /// Add to set with optional TTL (seconds)
    pub async fn add_set_with_ttl(
        &self,
        key: &str,
        member: &str,
        metadata: Option<SignatureMetadata>,
        ttl_seconds: Option<u64>,
    ) -> Result<()> {
        let mut set_value = match self.get_value(key).await? {
            Some(StoredValue::Set(sv)) => sv,
            None => {
                let ttl = ttl_seconds.map(|s| TtlMetadata::new(Some(s)));
                if ttl.is_some() {
                    metrics::TTL_KEYS_TOTAL.inc();
                }
                SetValue {
                    members: HashSet::new(),
                    metadata: metadata.clone(),
                    ttl,
                }
            },
            _ => return Err(anyhow::anyhow!("Key is not a set type")),
        };

        set_value.members.insert(member.to_string());
        if metadata.is_some() {
            set_value.metadata = metadata;
        }
        self.store_value(key, StoredValue::Set(set_value), StoreType::Set)
            .await
    }

    pub async fn get_set(&self, key: &str) -> Result<Vec<String>> {
        match self.get_value(key).await? {
            Some(StoredValue::Set(sv)) => Ok(sv.members.into_iter().collect()),
            None => Ok(Vec::new()),
            _ => Err(anyhow::anyhow!("Key is not a set type")),
        }
    }

    // Sorted Set Operations
    pub async fn add_sorted_set(&self, key: &str, score: f64, member: &str) -> Result<()> {
        self.add_sorted_set_with_metadata(key, score, member, None)
            .await
    }

    pub async fn add_sorted_set_with_metadata(
        &self,
        key: &str,
        score: f64,
        member: &str,
        metadata: Option<SignatureMetadata>,
    ) -> Result<()> {
        self.add_sorted_set_with_ttl(key, score, member, metadata, None).await
    }

    /// Add to sorted set with optional TTL (seconds)
    pub async fn add_sorted_set_with_ttl(
        &self,
        key: &str,
        score: f64,
        member: &str,
        metadata: Option<SignatureMetadata>,
        ttl_seconds: Option<u64>,
    ) -> Result<()> {
        let mut sorted_set_value = match self.get_value(key).await? {
            Some(StoredValue::SortedSet(ssv)) => ssv,
            None => {
                let ttl = ttl_seconds.map(|s| TtlMetadata::new(Some(s)));
                if ttl.is_some() {
                    metrics::TTL_KEYS_TOTAL.inc();
                }
                SortedSetValue {
                    members: BTreeMap::new(),
                    metadata: metadata.clone(),
                    ttl,
                }
            },
            _ => return Err(anyhow::anyhow!("Key is not a sorted set type")),
        };

        sorted_set_value.members.insert(member.to_string(), score);
        if metadata.is_some() {
            sorted_set_value.metadata = metadata;
        }
        self.store_value(
            key,
            StoredValue::SortedSet(sorted_set_value),
            StoreType::SortedSet,
        )
        .await
    }

    // Add JSON object to sorted set with deduplication by _id
    pub async fn add_sorted_set_json(&self, key: &str, score: f64, json_str: &str) -> Result<()> {
        self.add_sorted_set_json_with_metadata(key, score, json_str, None)
            .await
    }

    pub async fn add_sorted_set_json_with_metadata(
        &self,
        key: &str,
        score: f64,
        json_str: &str,
        metadata: Option<SignatureMetadata>,
    ) -> Result<()> {
        let json_data: serde_json::Value = serde_json::from_str(json_str)?;

        // Check for _id and remove old entries with same _id
        if let Some(doc_id) = json_data.get("_id").and_then(|v| v.as_str()) {
            self.delete_sorted_set_by_id(key, doc_id).await?;
        }

        let mut sorted_set_value = match self.get_value(key).await? {
            Some(StoredValue::SortedSet(ssv)) => ssv,
            None => SortedSetValue {
                members: BTreeMap::new(),
                metadata: metadata.clone(),
                ttl: None,
            },
            _ => return Err(anyhow::anyhow!("Key is not a sorted set type")),
        };

        sorted_set_value.members.insert(json_str.to_string(), score);
        if metadata.is_some() {
            sorted_set_value.metadata = metadata;
        }
        self.store_value(
            key,
            StoredValue::SortedSet(sorted_set_value),
            StoreType::SortedSet,
        )
        .await
    }

    // Delete sorted set entries with matching _id
    async fn delete_sorted_set_by_id(&self, key: &str, target_id: &str) -> Result<()> {
        if let Ok(Some(StoredValue::SortedSet(mut ssv))) = self.get_value(key).await {
            let to_remove: Vec<String> = ssv
                .members
                .keys()
                .filter(|member_str| {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(member_str) {
                        json.get("_id").and_then(|v| v.as_str()) == Some(target_id)
                    } else {
                        false
                    }
                })
                .cloned()
                .collect();

            for member in to_remove {
                ssv.members.remove(&member);
            }

            self.store_value(key, StoredValue::SortedSet(ssv), StoreType::SortedSet)
                .await?;
        }
        Ok(())
    }

    // Get sorted set with parsed JSON objects
    pub async fn get_sorted_set_json(
        &self,
        key: &str,
        start: isize,
        stop: isize,
    ) -> Result<Vec<SortedSetEntry>> {
        match self.get_value(key).await? {
            Some(StoredValue::SortedSet(ssv)) => {
                let mut items: Vec<_> = ssv.members.into_iter().collect();
                items.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

                let len = items.len() as isize;
                let start = if start < 0 {
                    (len + start).max(0)
                } else {
                    start.min(len)
                } as usize;
                let stop = if stop < 0 {
                    (len + stop + 1).max(0)
                } else {
                    (stop + 1).min(len)
                } as usize;

                if start >= stop {
                    return Ok(Vec::new());
                }

                Ok(items[start..stop]
                    .iter()
                    .filter_map(|(member, score)| {
                        serde_json::from_str(member)
                            .ok()
                            .map(|data| SortedSetEntry {
                                score: *score,
                                data,
                                metadata: ssv.metadata.clone(),
                            })
                    })
                    .collect())
            }
            None => Ok(Vec::new()),
            _ => Err(anyhow::anyhow!("Key is not a sorted set type")),
        }
    }

    pub async fn get_sorted_set(
        &self,
        key: &str,
        start: isize,
        stop: isize,
    ) -> Result<Vec<String>> {
        match self.get_value(key).await? {
            Some(StoredValue::SortedSet(ssv)) => {
                let mut items: Vec<_> = ssv.members.into_iter().collect();
                items.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

                let len = items.len() as isize;
                let start = if start < 0 {
                    (len + start).max(0)
                } else {
                    start.min(len)
                } as usize;
                let stop = if stop < 0 {
                    (len + stop + 1).max(0)
                } else {
                    (stop + 1).min(len)
                } as usize;

                if start >= stop {
                    return Ok(Vec::new());
                }

                Ok(items[start..stop]
                    .iter()
                    .map(|(member, _)| member.clone())
                    .collect())
            }
            None => Ok(Vec::new()),
            _ => Err(anyhow::anyhow!("Key is not a sorted set type")),
        }
    }

    pub async fn get_sorted_set_with_scores(
        &self,
        key: &str,
        start: isize,
        stop: isize,
    ) -> Result<Vec<(String, f64)>> {
        match self.get_value(key).await? {
            Some(StoredValue::SortedSet(ssv)) => {
                let mut items: Vec<_> = ssv.members.into_iter().collect();
                items.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

                let len = items.len() as isize;
                let start = if start < 0 {
                    (len + start).max(0)
                } else {
                    start.min(len)
                } as usize;
                let stop = if stop < 0 {
                    (len + stop + 1).max(0)
                } else {
                    (stop + 1).min(len)
                } as usize;

                if start >= stop {
                    return Ok(Vec::new());
                }

                Ok(items[start..stop].to_vec())
            }
            None => Ok(Vec::new()),
            _ => Err(anyhow::anyhow!("Key is not a sorted set type")),
        }
    }

    // Key Operations
    pub async fn exists(&self, key: &str) -> Result<bool> {
        self.index_exists(key)
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        let timer = Timer::new();
        
        {
            self.index_remove(key)?;
        }

        // Invalidate cache entry in tiered cache
        self.cache.invalidate(key).await;

        self.save_index().await?;
        
        timer.observe_duration_seconds(&metrics::DELETE_LATENCY);
        metrics::STORAGE_DELETES.inc();
        
        Ok(())
    }

    // JSON Operations
    pub async fn set_json(&self, key: &str, _path: &str, value: &str) -> Result<()> {
        self.set_json_with_metadata(key, _path, value, None).await
    }

    pub async fn set_json_with_metadata(
        &self,
        key: &str,
        _path: &str,
        value: &str,
        metadata: Option<SignatureMetadata>,
    ) -> Result<()> {
        self.set_json_with_ttl(key, _path, value, metadata, None).await
    }

    /// Set JSON with optional TTL (seconds)
    pub async fn set_json_with_ttl(
        &self,
        key: &str,
        _path: &str,
        value: &str,
        metadata: Option<SignatureMetadata>,
        ttl_seconds: Option<u64>,
    ) -> Result<()> {
        let json_data: serde_json::Value = serde_json::from_str(value)?;

        // Extract _id if present for deduplication
        let id = json_data
            .get("_id")
            .and_then(|v| v.as_str())
            .map(String::from);

        // If _id exists, remove old entries with same _id
        if let Some(ref doc_id) = id {
            self.delete_json_by_id(key, doc_id).await?;
        }
        
        let ttl = ttl_seconds.map(|s| TtlMetadata::new(Some(s)));
        if ttl.is_some() {
            metrics::TTL_KEYS_TOTAL.inc();
        }

        let json_value = JsonValue {
            data: json_data,
            id,
            metadata,
            ttl,
        };
        self.store_value(key, StoredValue::Json(json_value), StoreType::Json)
            .await
    }

    // Delete JSON documents with matching _id
    async fn delete_json_by_id(&self, key_prefix: &str, target_id: &str) -> Result<()> {
        let keys_to_check = self.index_keys_with_prefix(key_prefix)?;

        for key in keys_to_check {
            if let Ok(Some(StoredValue::Json(jv))) = self.get_value(&key).await {
                if jv.id.as_deref() == Some(target_id) {
                    // Remove from cache and index
                    self.cache.invalidate(&key).await;
                    self.index_remove(&key)?;
                }
            }
        }
        Ok(())
    }

    pub async fn get_json(&self, key: &str, _path: Option<&str>) -> Result<Option<String>> {
        match self.get_value(key).await? {
            Some(StoredValue::Json(jv)) => Ok(Some(serde_json::to_string(&jv.data)?)),
            None => Ok(None),
            _ => Err(anyhow::anyhow!("Key is not a JSON type")),
        }
    }

    pub async fn filter_json(&self, key: &str, _json_path: &str) -> Result<Option<String>> {
        // Evaluate JSONPath expression against stored JSON and return matched values
        let json_opt = self.get_json(key, None).await?;
        if json_opt.is_none() {
            return Ok(None);
        }

        let json_str = json_opt.unwrap();
        let doc: serde_json::Value = serde_json::from_str(&json_str)?;

        // Use jsonpath_lib to evaluate the expression
        match jsonpath_lib::select(&doc, _json_path) {
            Ok(matches) => {
                // Serialize matched values to JSON array or single value
                if matches.len() == 1 {
                    Ok(Some(serde_json::to_string(&matches[0])?))
                } else {
                    Ok(Some(serde_json::to_string(&matches)?))
                }
            }
            Err(e) => Err(anyhow::anyhow!("JSONPath error: {}", e)),
        }
    }

    pub async fn json_type(&self, key: &str, _path: Option<&str>) -> Result<Option<String>> {
        match self.get_value(key).await? {
            Some(StoredValue::Json(jv)) => {
                let type_str = match jv.data {
                    serde_json::Value::Null => "null",
                    serde_json::Value::Bool(_) => "boolean",
                    serde_json::Value::Number(_) => "number",
                    serde_json::Value::String(_) => "string",
                    serde_json::Value::Array(_) => "array",
                    serde_json::Value::Object(_) => "object",
                };
                Ok(Some(type_str.to_string()))
            }
            None => Ok(None),
            _ => Err(anyhow::anyhow!("Key is not a JSON type")),
        }
    }

    // Stream Operations
    pub async fn xadd(&self, key: &str, id: &str, fields: &[(String, String)]) -> Result<String> {
        self.xadd_with_metadata(key, id, fields, None).await
    }

    pub async fn xadd_with_metadata(
        &self,
        key: &str,
        id: &str,
        fields: &[(String, String)],
        metadata: Option<SignatureMetadata>,
    ) -> Result<String> {
        self.xadd_with_ttl(key, id, fields, metadata, None).await
    }

    /// Add to stream with optional TTL (seconds)
    pub async fn xadd_with_ttl(
        &self,
        key: &str,
        id: &str,
        fields: &[(String, String)],
        metadata: Option<SignatureMetadata>,
        ttl_seconds: Option<u64>,
    ) -> Result<String> {
        let mut stream_value = match self.get_value(key).await? {
            Some(StoredValue::Stream(sv)) => sv,
            None => {
                let ttl = ttl_seconds.map(|s| TtlMetadata::new(Some(s)));
                if ttl.is_some() {
                    metrics::TTL_KEYS_TOTAL.inc();
                }
                StreamValue {
                    entries: Vec::new(),
                    metadata: metadata.clone(),
                    ttl,
                }
            },
            _ => return Err(anyhow::anyhow!("Key is not a stream type")),
        };

        let entry_id = if id == "*" {
            format!("{}-0", chrono::Utc::now().timestamp_millis())
        } else {
            id.to_string()
        };

        stream_value
            .entries
            .push((entry_id.clone(), fields.to_vec()));
        if metadata.is_some() {
            stream_value.metadata = metadata;
        }
        self.store_value(key, StoredValue::Stream(stream_value), StoreType::Stream)
            .await?;

        Ok(entry_id)
    }

    pub async fn xread(
        &self,
        _keys: &[String],
        _ids: &[String],
        _count: Option<usize>,
        _block: Option<u64>,
    ) -> Result<Vec<(String, Vec<(String, Vec<(String, String)>)>)>> {
        Ok(Vec::new())
    }

    pub async fn xrange(
        &self,
        key: &str,
        start: &str,
        end: &str,
        count: Option<usize>,
    ) -> Result<Vec<(String, Vec<(String, String)>)>> {
        match self.get_value(key).await? {
            Some(StoredValue::Stream(sv)) => {
                let entries: Vec<_> = sv
                    .entries
                    .into_iter()
                    .filter(|(id, _)| {
                        (start == "-" || id.as_str() >= start) && (end == "+" || id.as_str() <= end)
                    })
                    .take(count.unwrap_or(usize::MAX))
                    .collect();

                Ok(entries)
            }
            None => Ok(Vec::new()),
            _ => Err(anyhow::anyhow!("Key is not a stream type")),
        }
    }

    // Get last N entries from stream (reverse order)
    pub async fn xrevrange(
        &self,
        key: &str,
        start: &str,
        end: &str,
        count: Option<usize>,
    ) -> Result<Vec<(String, Vec<(String, String)>)>> {
        match self.get_value(key).await? {
            Some(StoredValue::Stream(sv)) => {
                let mut entries: Vec<_> = sv
                    .entries
                    .into_iter()
                    .filter(|(id, _)| {
                        (end == "-" || id.as_str() >= end) && (start == "+" || id.as_str() <= start)
                    })
                    .collect();

                // Reverse the order (latest first)
                entries.reverse();

                if let Some(limit) = count {
                    entries.truncate(limit);
                }

                Ok(entries)
            }
            None => Ok(Vec::new()),
            _ => Err(anyhow::anyhow!("Key is not a stream type")),
        }
    }

    pub async fn xlen(&self, key: &str) -> Result<usize> {
        match self.get_value(key).await? {
            Some(StoredValue::Stream(sv)) => Ok(sv.entries.len()),
            None => Ok(0),
            _ => Err(anyhow::anyhow!("Key is not a stream type")),
        }
    }

    pub async fn filter_stream(
        &self,
        key: &str,
        start: &str,
        end: &str,
        pattern: Option<&str>,
    ) -> Result<Vec<(String, Vec<(String, String)>)>> {
        match self.get_value(key).await? {
            Some(StoredValue::Stream(sv)) => {
                let entries: Vec<_> = sv
                    .entries
                    .into_iter()
                    .filter(|(id, fields)| {
                        let in_range = (start == "-" || id.as_str() >= start)
                            && (end == "+" || id.as_str() <= end);

                        if !in_range {
                            return false;
                        }

                        if let Some(pat) = pattern {
                            fields.iter().any(|(_, value)| value.contains(pat))
                        } else {
                            true
                        }
                    })
                    .collect();

                Ok(entries)
            }
            None => Ok(Vec::new()),
            _ => Err(anyhow::anyhow!("Key is not a stream type")),
        }
    }

    // TimeSeries Operations
    pub async fn ts_add(&self, key: &str, timestamp: i64, value: f64) -> Result<()> {
        self.ts_add_with_metadata(key, timestamp, value, None).await
    }

    pub async fn ts_add_with_metadata(
        &self,
        key: &str,
        timestamp: i64,
        value: f64,
        metadata: Option<SignatureMetadata>,
    ) -> Result<()> {
        self.ts_add_with_ttl(key, timestamp, value, metadata, None).await
    }

    /// Add time series data with optional TTL (seconds)
    pub async fn ts_add_with_ttl(
        &self,
        key: &str,
        timestamp: i64,
        value: f64,
        metadata: Option<SignatureMetadata>,
        ttl_seconds: Option<u64>,
    ) -> Result<()> {
        let mut ts_value = match self.get_value(key).await? {
            Some(StoredValue::TimeSeries(tsv)) => tsv,
            None => {
                let ttl = ttl_seconds.map(|s| TtlMetadata::new(Some(s)));
                if ttl.is_some() {
                    metrics::TTL_KEYS_TOTAL.inc();
                }
                TimeSeriesValue {
                    points: BTreeMap::new(),
                    metadata: metadata.clone(),
                    ttl,
                }
            },
            _ => return Err(anyhow::anyhow!("Key is not a timeseries type")),
        };

        ts_value.points.insert(timestamp, value);
        if metadata.is_some() {
            ts_value.metadata = metadata;
        }
        self.store_value(
            key,
            StoredValue::TimeSeries(ts_value),
            StoreType::TimeSeries,
        )
        .await
    }

    pub async fn ts_range(
        &self,
        key: &str,
        from_timestamp: i64,
        to_timestamp: i64,
    ) -> Result<Vec<(i64, f64)>> {
        match self.get_value(key).await? {
            Some(StoredValue::TimeSeries(tsv)) => {
                let points: Vec<_> = tsv
                    .points
                    .range(from_timestamp..=to_timestamp)
                    .map(|(ts, val)| (*ts, *val))
                    .collect();
                Ok(points)
            }
            None => Ok(Vec::new()),
            _ => Err(anyhow::anyhow!("Key is not a timeseries type")),
        }
    }

    pub async fn ts_get(&self, key: &str) -> Result<Option<(i64, f64)>> {
        match self.get_value(key).await? {
            Some(StoredValue::TimeSeries(tsv)) => {
                Ok(tsv.points.iter().last().map(|(ts, val)| (*ts, *val)))
            }
            None => Ok(None),
            _ => Err(anyhow::anyhow!("Key is not a timeseries type")),
        }
    }

    pub async fn filter_timeseries(
        &self,
        key: &str,
        from_timestamp: i64,
        to_timestamp: i64,
        min_value: Option<f64>,
        max_value: Option<f64>,
    ) -> Result<Vec<(i64, f64)>> {
        match self.get_value(key).await? {
            Some(StoredValue::TimeSeries(tsv)) => {
                let points: Vec<_> = tsv
                    .points
                    .range(from_timestamp..=to_timestamp)
                    .filter(|(_, val)| {
                        let above_min = min_value.is_none_or(|min| **val >= min);
                        let below_max = max_value.is_none_or(|max| **val <= max);
                        above_min && below_max
                    })
                    .map(|(ts, val)| (*ts, *val))
                    .collect();
                Ok(points)
            }
            None => Ok(Vec::new()),
            _ => Err(anyhow::anyhow!("Key is not a timeseries type")),
        }
    }

    // Geo Operations
    pub async fn geoadd(
        &self,
        key: &str,
        longitude: f64,
        latitude: f64,
        member: &str,
    ) -> Result<usize> {
        self.geoadd_with_metadata(key, longitude, latitude, member, None)
            .await
    }

    pub async fn geoadd_with_metadata(
        &self,
        key: &str,
        longitude: f64,
        latitude: f64,
        member: &str,
        metadata: Option<SignatureMetadata>,
    ) -> Result<usize> {
        self.geoadd_with_ttl(key, longitude, latitude, member, metadata, None).await
    }

    /// Add geospatial data with optional TTL (seconds)
    pub async fn geoadd_with_ttl(
        &self,
        key: &str,
        longitude: f64,
        latitude: f64,
        member: &str,
        metadata: Option<SignatureMetadata>,
        ttl_seconds: Option<u64>,
    ) -> Result<usize> {
        let mut geo_value = match self.get_value(key).await? {
            Some(StoredValue::Geo(gv)) => gv,
            None => {
                let ttl = ttl_seconds.map(|s| TtlMetadata::new(Some(s)));
                if ttl.is_some() {
                    metrics::TTL_KEYS_TOTAL.inc();
                }
                GeoValue {
                    locations: HashMap::new(),
                    metadata: metadata.clone(),
                    ttl,
                }
            },
            _ => return Err(anyhow::anyhow!("Key is not a geo type")),
        };

        geo_value
            .locations
            .insert(member.to_string(), (longitude, latitude));
        if metadata.is_some() {
            geo_value.metadata = metadata;
        }
        self.store_value(key, StoredValue::Geo(geo_value), StoreType::Geo)
            .await?;
        Ok(1)
    }

    pub async fn georadius(
        &self,
        key: &str,
        longitude: f64,
        latitude: f64,
        radius: f64,
        unit: &str,
    ) -> Result<Vec<String>> {
        let radius_km = match unit {
            "m" => radius / 1000.0,
            "km" => radius,
            "mi" => radius * 1.60934,
            "ft" => radius * 0.0003048,
            _ => radius,
        };

        match self.get_value(key).await? {
            Some(StoredValue::Geo(gv)) => {
                let members: Vec<_> = gv
                    .locations
                    .into_iter()
                    .filter(|(_, (lon, lat))| {
                        let distance = Self::haversine_distance(latitude, longitude, *lat, *lon);
                        distance <= radius_km
                    })
                    .map(|(member, _)| member)
                    .collect();
                Ok(members)
            }
            None => Ok(Vec::new()),
            _ => Err(anyhow::anyhow!("Key is not a geo type")),
        }
    }

    pub async fn georadiusbymember(
        &self,
        key: &str,
        member: &str,
        radius: f64,
        unit: &str,
    ) -> Result<Vec<String>> {
        let (longitude, latitude) = match self.get_value(key).await? {
            Some(StoredValue::Geo(gv)) => match gv.locations.get(member) {
                Some(coords) => *coords,
                None => return Ok(Vec::new()),
            },
            None => return Ok(Vec::new()),
            _ => return Err(anyhow::anyhow!("Key is not a geo type")),
        };

        self.georadius(key, longitude, latitude, radius, unit).await
    }

    pub async fn geopos(&self, key: &str, member: &str) -> Result<Option<(f64, f64)>> {
        match self.get_value(key).await? {
            Some(StoredValue::Geo(gv)) => Ok(gv.locations.get(member).copied()),
            None => Ok(None),
            _ => Err(anyhow::anyhow!("Key is not a geo type")),
        }
    }

    pub async fn geodist(
        &self,
        key: &str,
        member1: &str,
        member2: &str,
        unit: Option<&str>,
    ) -> Result<Option<f64>> {
        match self.get_value(key).await? {
            Some(StoredValue::Geo(gv)) => {
                let coord1 = gv.locations.get(member1);
                let coord2 = gv.locations.get(member2);

                if let (Some((lon1, lat1)), Some((lon2, lat2))) = (coord1, coord2) {
                    let distance_km = Self::haversine_distance(*lat1, *lon1, *lat2, *lon2);

                    let distance = match unit.unwrap_or("m") {
                        "m" => distance_km * 1000.0,
                        "km" => distance_km,
                        "mi" => distance_km / 1.60934,
                        "ft" => distance_km / 0.0003048,
                        _ => distance_km * 1000.0,
                    };

                    Ok(Some(distance))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
            _ => Err(anyhow::anyhow!("Key is not a geo type")),
        }
    }

    pub async fn filter_geo_by_radius(
        &self,
        key: &str,
        longitude: f64,
        latitude: f64,
        max_radius: f64,
        unit: &str,
    ) -> Result<Vec<String>> {
        self.georadius(key, longitude, latitude, max_radius, unit)
            .await
    }

    pub async fn georadius_with_coords(
        &self,
        key: &str,
        longitude: f64,
        latitude: f64,
        radius: f64,
        unit: &str,
    ) -> Result<Vec<(String, f64, f64)>> {
        let radius_km = match unit {
            "m" => radius / 1000.0,
            "km" => radius,
            "mi" => radius * 1.60934,
            "ft" => radius * 0.0003048,
            _ => radius,
        };

        match self.get_value(key).await? {
            Some(StoredValue::Geo(gv)) => {
                let members: Vec<_> = gv
                    .locations
                    .into_iter()
                    .filter(|(_, (lon, lat))| {
                        let distance = Self::haversine_distance(latitude, longitude, *lat, *lon);
                        distance <= radius_km
                    })
                    .map(|(member, (lon, lat))| (member, lon, lat))
                    .collect();
                Ok(members)
            }
            None => Ok(Vec::new()),
            _ => Err(anyhow::anyhow!("Key is not a geo type")),
        }
    }

    pub async fn georadiusbymember_with_coords(
        &self,
        key: &str,
        member: &str,
        radius: f64,
        unit: &str,
    ) -> Result<Vec<(String, f64, f64)>> {
        let (longitude, latitude) = match self.get_value(key).await? {
            Some(StoredValue::Geo(gv)) => match gv.locations.get(member) {
                Some(coords) => *coords,
                None => return Ok(Vec::new()),
            },
            None => return Ok(Vec::new()),
            _ => return Err(anyhow::anyhow!("Key is not a geo type")),
        };

        self.georadius_with_coords(key, longitude, latitude, radius, unit)
            .await
    }

    fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
        const EARTH_RADIUS_KM: f64 = 6371.0;

        let lat1_rad = lat1.to_radians();
        let lat2_rad = lat2.to_radians();
        let delta_lat = (lat2 - lat1).to_radians();
        let delta_lon = (lon2 - lon1).to_radians();

        let a = (delta_lat / 2.0).sin().powi(2)
            + lat1_rad.cos() * lat2_rad.cos() * (delta_lon / 2.0).sin().powi(2);
        let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

        EARTH_RADIUS_KM * c
    }

    // Filter Operations
    pub async fn filter_hash(
        &self,
        key: &str,
        field_pattern: &str,
    ) -> Result<Vec<(String, String)>> {
        match self.get_value(key).await? {
            Some(StoredValue::Hash(hv)) => Ok(hv
                .fields
                .into_iter()
                .filter(|(field, _)| field.contains(field_pattern))
                .collect()),
            None => Ok(Vec::new()),
            _ => Err(anyhow::anyhow!("Key is not a hash type")),
        }
    }

    pub async fn filter_list(&self, key: &str, value_pattern: &str) -> Result<Vec<String>> {
        match self.get_value(key).await? {
            Some(StoredValue::List(lv)) => Ok(lv
                .items
                .into_iter()
                .filter(|v| v.contains(value_pattern))
                .collect()),
            None => Ok(Vec::new()),
            _ => Err(anyhow::anyhow!("Key is not a list type")),
        }
    }

    pub async fn filter_set(&self, key: &str, member_pattern: &str) -> Result<Vec<String>> {
        match self.get_value(key).await? {
            Some(StoredValue::Set(sv)) => Ok(sv
                .members
                .into_iter()
                .filter(|m| m.contains(member_pattern))
                .collect()),
            None => Ok(Vec::new()),
            _ => Err(anyhow::anyhow!("Key is not a set type")),
        }
    }

    pub async fn filter_sorted_set(
        &self,
        key: &str,
        min_score: f64,
        max_score: f64,
    ) -> Result<Vec<String>> {
        match self.get_value(key).await? {
            Some(StoredValue::SortedSet(ssv)) => {
                let mut items: Vec<_> = ssv
                    .members
                    .into_iter()
                    .filter(|(_, score)| *score >= min_score && *score <= max_score)
                    .collect();
                items.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
                Ok(items.into_iter().map(|(member, _)| member).collect())
            }
            None => Ok(Vec::new()),
            _ => Err(anyhow::anyhow!("Key is not a sorted set type")),
        }
    }

    pub async fn filter_sorted_set_with_scores(
        &self,
        key: &str,
        min_score: f64,
        max_score: f64,
    ) -> Result<Vec<(String, f64)>> {
        match self.get_value(key).await? {
            Some(StoredValue::SortedSet(ssv)) => {
                let mut items: Vec<_> = ssv
                    .members
                    .into_iter()
                    .filter(|(_, score)| *score >= min_score && *score <= max_score)
                    .collect();
                items.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
                Ok(items)
            }
            None => Ok(Vec::new()),
            _ => Err(anyhow::anyhow!("Key is not a sorted set type")),
        }
    }

    // Get all stream keys for a database
    pub async fn get_all_streams(&self, db_prefix: &str) -> Result<Vec<String>> {
        let prefix = format!("{}:", db_prefix);
        let mut stream_keys = Vec::new();
        for key in self.index_keys_with_prefix(&prefix)? {
            if let Some((_, stype)) = self.index_get(&key)? {
                if matches!(stype, StoreType::Stream) {
                    stream_keys.push(key);
                }
            }
        }
        Ok(stream_keys)
    }

    /// Return all JSON documents stored under a database prefix.
    /// Keys are expected to be namespaced like `db_prefix:...`.
    pub async fn get_all_json(&self, db_prefix: &str) -> Result<Vec<String>> {
        let prefix = format!("{}:", db_prefix);
        let mut docs = Vec::new();

        for key in self.index_keys_with_prefix(&prefix)? {
            if let Some((_, stype)) = self.index_get(&key)? {
                if matches!(stype, StoreType::Json) {
                    if let Some(json_str) = self.get_json(&key, None).await? {
                        docs.push(json_str);
                    }
                }
            }
        }

        Ok(docs)
    }

    /// Return all JSON documents with optional signature metadata for a database prefix.
    /// Each item is (key, parsed_json_value, optional SignatureMetadata)
    pub async fn get_all_json_with_meta(
        &self,
        db_prefix: &str,
    ) -> Result<Vec<(String, serde_json::Value, Option<SignatureMetadata>)>> {
        let prefix = format!("{}:", db_prefix);
        let keys = self.index_keys_with_prefix(&prefix)?;
        let mut res: Vec<(String, serde_json::Value, Option<SignatureMetadata>)> = Vec::with_capacity(keys.len());

        for key in keys {
            if let Some((_, stype)) = self.index_get(&key)? {
                if matches!(stype, StoreType::Json) {
                    if let Ok(Some(stored)) = self.get_value(&key).await {
                        if let StoredValue::Json(jv) = stored {
                            res.push((key, jv.data, jv.metadata));
                        }
                    }
                }
            }
        }

        Ok(res)
    }

    /// Return all string entries for a database prefix (key, value, metadata)
    pub async fn get_all_strings(
        &self,
        db_prefix: &str,
    ) -> Result<Vec<(String, String, Option<SignatureMetadata>)>> {
        let prefix = format!("{}:", db_prefix);
        let keys = self.index_keys_with_prefix(&prefix)?;
        let mut out = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some((_, stype)) = self.index_get(&key)? {
                if matches!(stype, StoreType::String) {
                    if let Ok(Some(StoredValue::String(sv))) = self.get_value(&key).await {
                        out.push((key, sv.value, sv.metadata));
                    }
                }
            }
        }
        Ok(out)
    }

    /// Return all hash entries for a database prefix (key, fields, metadata)
    pub async fn get_all_hashes(
        &self,
        db_prefix: &str,
    ) -> Result<Vec<(String, Vec<(String, String)>, Option<SignatureMetadata>)>> {
        let prefix = format!("{}:", db_prefix);
        let keys = self.index_keys_with_prefix(&prefix)?;
        let mut out = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some((_, stype)) = self.index_get(&key)? {
                if matches!(stype, StoreType::Hash) {
                    if let Ok(Some(StoredValue::Hash(hv))) = self.get_value(&key).await {
                        out.push((key, hv.fields.into_iter().collect(), hv.metadata));
                    }
                }
            }
        }
        Ok(out)
    }

    /// Return all list entries for a database prefix (key, items, metadata)
    pub async fn get_all_lists(
        &self,
        db_prefix: &str,
    ) -> Result<Vec<(String, Vec<String>, Option<SignatureMetadata>)>> {
        let prefix = format!("{}:", db_prefix);
        let keys = self.index_keys_with_prefix(&prefix)?;
        let mut out = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some((_, stype)) = self.index_get(&key)? {
                if matches!(stype, StoreType::List) {
                    if let Ok(Some(StoredValue::List(lv))) = self.get_value(&key).await {
                        out.push((key, lv.items, lv.metadata));
                    }
                }
            }
        }
        Ok(out)
    }

    /// Return all set entries for a database prefix (key, members, metadata)
    pub async fn get_all_sets(
        &self,
        db_prefix: &str,
    ) -> Result<Vec<(String, Vec<String>, Option<SignatureMetadata>)>> {
        let prefix = format!("{}:", db_prefix);
        let keys = self.index_keys_with_prefix(&prefix)?;
        let mut out = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some((_, stype)) = self.index_get(&key)? {
                if matches!(stype, StoreType::Set) {
                    if let Ok(Some(StoredValue::Set(sv))) = self.get_value(&key).await {
                        out.push((key, sv.members.into_iter().collect(), sv.metadata));
                    }
                }
            }
        }
        Ok(out)
    }

    /// Return all sorted set entries for a database prefix (key, members with scores, metadata)
    pub async fn get_all_sorted_sets(
        &self,
        db_prefix: &str,
    ) -> Result<Vec<(String, Vec<(String, f64)>, Option<SignatureMetadata>)>> {
        let prefix = format!("{}:", db_prefix);
        let keys = self.index_keys_with_prefix(&prefix)?;
        let mut out = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some((_, stype)) = self.index_get(&key)? {
                if matches!(stype, StoreType::SortedSet) {
                    if let Ok(Some(StoredValue::SortedSet(ssv))) = self.get_value(&key).await {
                        let members: Vec<(String, f64)> = ssv.members.into_iter().collect();
                        out.push((key, members, ssv.metadata));
                    }
                }
            }
        }
        Ok(out)
    }

    /// Alias for existing get_all_json_with_meta for naming consistency
    pub async fn get_all_jsons(
        &self,
        db_prefix: &str,
    ) -> Result<Vec<(String, serde_json::Value, Option<SignatureMetadata>)>> {
        self.get_all_json_with_meta(db_prefix).await
    }

    /// Return all stream entries (key, entries, metadata)
    pub async fn get_all_stream_entries(
        &self,
        db_prefix: &str,
    ) -> Result<Vec<(String, Vec<(String, Vec<(String, String)>)>, Option<SignatureMetadata>)>> {
        let prefix = format!("{}:", db_prefix);
        let keys = self.index_keys_with_prefix(&prefix)?;
        let mut out = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some((_, stype)) = self.index_get(&key)? {
                if matches!(stype, StoreType::Stream) {
                    if let Ok(Some(StoredValue::Stream(sv))) = self.get_value(&key).await {
                        out.push((key, sv.entries, sv.metadata));
                    }
                }
            }
        }
        Ok(out)
    }

    /// Return all timeseries entries (key, points, metadata)
    pub async fn get_all_timeseries(
        &self,
        db_prefix: &str,
    ) -> Result<Vec<(String, Vec<(i64, f64)>, Option<SignatureMetadata>)>> {
        let prefix = format!("{}:", db_prefix);
        let keys = self.index_keys_with_prefix(&prefix)?;
        let mut out = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some((_, stype)) = self.index_get(&key)? {
                if matches!(stype, StoreType::TimeSeries) {
                    if let Ok(Some(StoredValue::TimeSeries(tsv))) = self.get_value(&key).await {
                        let points: Vec<(i64, f64)> = tsv.points.into_iter().collect();
                        out.push((key, points, tsv.metadata));
                    }
                }
            }
        }
        Ok(out)
    }

    /// Return all geo entries (key, list of (member, lon, lat), metadata)
    pub async fn get_all_geo(
        &self,
        db_prefix: &str,
    ) -> Result<Vec<(String, Vec<(String, f64, f64)>, Option<SignatureMetadata>)>> {
        let prefix = format!("{}:", db_prefix);
        let keys = self.index_keys_with_prefix(&prefix)?;
        let mut out = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some((_, stype)) = self.index_get(&key)? {
                if matches!(stype, StoreType::Geo) {
                    if let Ok(Some(StoredValue::Geo(gv))) = self.get_value(&key).await {
                        let locations: Vec<(String, f64, f64)> = gv
                            .locations
                            .into_iter()
                            .map(|(m, (lon, lat))| (m, lon, lat))
                            .collect();
                        out.push((key, locations, gv.metadata));
                    }
                }
            }
        }
        Ok(out)
    }

    // Scan keys by pattern (similar to Redis SCAN)
    pub async fn scan_keys(&self, pattern: &str) -> Result<Vec<String>> {
        // Simple pattern matching: * for wildcard
        let regex_pattern = pattern.replace("*", ".*").replace("?", ".");

        let re = regex::Regex::new(&regex_pattern)?;

        let mut matching_keys = Vec::new();
        for item in self.index_tree.iter() {
            let (k, _v) = item?;
            let key = String::from_utf8(k.to_vec())?;
            if re.is_match(&key) {
                matching_keys.push(key);
            }
        }
        Ok(matching_keys)
    }

    // Get keys by store type
    pub async fn get_keys_by_type(
        &self,
        db_prefix: &str,
        store_type: StoreType,
    ) -> Result<Vec<String>> {
        let prefix = format!("{}:", db_prefix);
        let mut keys = Vec::new();
        for key in self.index_keys_with_prefix(&prefix)? {
            if let Some((_, _stype)) = self.index_get(&key)? {
                if std::mem::discriminant(&_stype) == std::mem::discriminant(&store_type) {
                    keys.push(key);
                }
            }
        }
        Ok(keys)
    }

    /// Return all entries across all supported store types for a given DB prefix.
    /// Each entry is returned as a `StoredEntry` with a JSON-serializable `value` and optional metadata.
    pub async fn get_all(&self, db_prefix: &str) -> Result<Vec<StoredEntry>> {
        let prefix = format!("{}:", db_prefix);
        let mut res: Vec<StoredEntry> = Vec::new();

        for key in self.index_keys_with_prefix(&prefix)? {
            if let Some((_, _stype)) = self.index_get(&key)? {
                // Load the stored value and convert to a JSON representation depending on type
                if let Ok(Some(stored)) = self.get_value(&key).await {
                    match stored {
                        StoredValue::String(sv) => {
                            res.push(StoredEntry {
                                key: key.clone(),
                                store_type: StoreType::String,
                                value: serde_json::json!({"value": sv.value}),
                                metadata: sv.metadata,
                            });
                        }
                        StoredValue::Hash(hv) => {
                            res.push(StoredEntry {
                                key: key.clone(),
                                store_type: StoreType::Hash,
                                value: serde_json::to_value(hv.fields)?,
                                metadata: hv.metadata,
                            });
                        }
                        StoredValue::List(lv) => {
                            res.push(StoredEntry {
                                key: key.clone(),
                                store_type: StoreType::List,
                                value: serde_json::to_value(lv.items)?,
                                metadata: lv.metadata,
                            });
                        }
                        StoredValue::Set(sv) => {
                            // Serialize set members as array
                            let members: Vec<String> = sv.members.into_iter().collect();
                            res.push(StoredEntry {
                                key: key.clone(),
                                store_type: StoreType::Set,
                                value: serde_json::to_value(members)?,
                                metadata: sv.metadata,
                            });
                        }
                        StoredValue::SortedSet(ssv) => {
                            // Convert to array of {score, data}
                            let mut arr: Vec<serde_json::Value> = Vec::new();
                            for (member, score) in ssv.members.into_iter() {
                                // try parse member as json, fallback to string
                                let data = serde_json::from_str::<serde_json::Value>(&member)
                                    .unwrap_or(serde_json::Value::String(member));
                                arr.push(serde_json::json!({"score": score, "data": data}));
                            }
                            res.push(StoredEntry {
                                key: key.clone(),
                                store_type: StoreType::SortedSet,
                                value: serde_json::Value::Array(arr),
                                metadata: ssv.metadata,
                            });
                        }
                        StoredValue::Json(jv) => {
                            res.push(StoredEntry {
                                key: key.clone(),
                                store_type: StoreType::Json,
                                value: jv.data,
                                metadata: jv.metadata,
                            });
                        }
                        StoredValue::Stream(sv) => {
                            // entries: Vec<(String, Vec<(String, String)>)>
                            let entries: Vec<serde_json::Value> = sv
                                .entries
                                .into_iter()
                                .map(|(id, fields)| {
                                    let map: serde_json::Map<String, serde_json::Value> = fields
                                        .into_iter()
                                        .map(|(k, v)| (k, serde_json::Value::String(v)))
                                        .collect();
                                    serde_json::json!({"id": id, "fields": serde_json::Value::Object(map)})
                                })
                                .collect();
                            res.push(StoredEntry {
                                key: key.clone(),
                                store_type: StoreType::Stream,
                                value: serde_json::Value::Array(entries),
                                metadata: sv.metadata,
                            });
                        }
                        StoredValue::TimeSeries(tsv) => {
                            let points: Vec<serde_json::Value> = tsv
                                .points
                                .into_iter()
                                .map(|(ts, val)| serde_json::json!({"timestamp": ts, "value": val}))
                                .collect();
                            res.push(StoredEntry {
                                key: key.clone(),
                                store_type: StoreType::TimeSeries,
                                value: serde_json::Value::Array(points),
                                metadata: tsv.metadata,
                            });
                        }
                        StoredValue::Geo(gv) => {
                            let locations: Vec<serde_json::Value> = gv
                                .locations
                                .into_iter()
                                .map(|(member, (lon, lat))| serde_json::json!({"member": member, "lon": lon, "lat": lat}))
                                .collect();
                            res.push(StoredEntry {
                                key: key.clone(),
                                store_type: StoreType::Geo,
                                value: serde_json::Value::Array(locations),
                                metadata: gv.metadata,
                            });
                        }
                    }
                }
            }
        }

        Ok(res)
    }

    // ============================================================================
    // TTL (Time-To-Live) Operations
    // ============================================================================

    /// Get TTL info for a key
    pub async fn get_ttl(&self, key: &str) -> Result<Option<TtlInfo>> {
        match self.get_value(key).await? {
            Some(value) => {
                if let Some(ttl) = Self::get_ttl_metadata(&value) {
                    Ok(Some(TtlInfo {
                        ttl_seconds: ttl.ttl_seconds,
                        expires_at: ttl.expires_at,
                        created_at: ttl.created_at,
                        remaining_seconds: ttl.remaining_ttl_seconds(),
                        has_ttl: ttl.ttl_seconds.is_some(),
                    }))
                } else {
                    Ok(Some(TtlInfo {
                        ttl_seconds: None,
                        expires_at: None,
                        created_at: 0,
                        remaining_seconds: None,
                        has_ttl: false,
                    }))
                }
            }
            None => Ok(None),
        }
    }

    /// Set/update TTL for an existing key (like Redis EXPIRE command)
    pub async fn set_key_ttl(&self, key: &str, ttl_seconds: u64) -> Result<bool> {
        let value = match self.get_value(key).await? {
            Some(v) => v,
            None => return Ok(false), // Key doesn't exist
        };

        let new_ttl = Some(TtlMetadata::new(Some(ttl_seconds)));

        // Create updated value with new TTL
        let updated_value = match value {
            StoredValue::String(mut v) => {
                v.ttl = new_ttl;
                StoredValue::String(v)
            }
            StoredValue::Hash(mut v) => {
                v.ttl = new_ttl;
                StoredValue::Hash(v)
            }
            StoredValue::List(mut v) => {
                v.ttl = new_ttl;
                StoredValue::List(v)
            }
            StoredValue::Set(mut v) => {
                v.ttl = new_ttl;
                StoredValue::Set(v)
            }
            StoredValue::SortedSet(mut v) => {
                v.ttl = new_ttl;
                StoredValue::SortedSet(v)
            }
            StoredValue::Json(mut v) => {
                v.ttl = new_ttl;
                StoredValue::Json(v)
            }
            StoredValue::Stream(mut v) => {
                v.ttl = new_ttl;
                StoredValue::Stream(v)
            }
            StoredValue::TimeSeries(mut v) => {
                v.ttl = new_ttl;
                StoredValue::TimeSeries(v)
            }
            StoredValue::Geo(mut v) => {
                v.ttl = new_ttl;
                StoredValue::Geo(v)
            }
        };

        let store_type = match &updated_value {
            StoredValue::String(_) => StoreType::String,
            StoredValue::Hash(_) => StoreType::Hash,
            StoredValue::List(_) => StoreType::List,
            StoredValue::Set(_) => StoreType::Set,
            StoredValue::SortedSet(_) => StoreType::SortedSet,
            StoredValue::Json(_) => StoreType::Json,
            StoredValue::Stream(_) => StoreType::Stream,
            StoredValue::TimeSeries(_) => StoreType::TimeSeries,
            StoredValue::Geo(_) => StoreType::Geo,
        };

        self.store_value(key, updated_value, store_type).await?;
        metrics::TTL_KEYS_TOTAL.inc();
        Ok(true)
    }

    /// Remove TTL from a key (like Redis PERSIST command)
    pub async fn persist_key(&self, key: &str) -> Result<bool> {
        let value = match self.get_value(key).await? {
            Some(v) => v,
            None => return Ok(false),
        };

        // Create updated value without TTL
        let updated_value = match value {
            StoredValue::String(mut v) => {
                if v.ttl.is_some() {
                    v.ttl = None;
                    metrics::TTL_KEYS_TOTAL.dec();
                }
                StoredValue::String(v)
            }
            StoredValue::Hash(mut v) => {
                if v.ttl.is_some() {
                    v.ttl = None;
                    metrics::TTL_KEYS_TOTAL.dec();
                }
                StoredValue::Hash(v)
            }
            StoredValue::List(mut v) => {
                if v.ttl.is_some() {
                    v.ttl = None;
                    metrics::TTL_KEYS_TOTAL.dec();
                }
                StoredValue::List(v)
            }
            StoredValue::Set(mut v) => {
                if v.ttl.is_some() {
                    v.ttl = None;
                    metrics::TTL_KEYS_TOTAL.dec();
                }
                StoredValue::Set(v)
            }
            StoredValue::SortedSet(mut v) => {
                if v.ttl.is_some() {
                    v.ttl = None;
                    metrics::TTL_KEYS_TOTAL.dec();
                }
                StoredValue::SortedSet(v)
            }
            StoredValue::Json(mut v) => {
                if v.ttl.is_some() {
                    v.ttl = None;
                    metrics::TTL_KEYS_TOTAL.dec();
                }
                StoredValue::Json(v)
            }
            StoredValue::Stream(mut v) => {
                if v.ttl.is_some() {
                    v.ttl = None;
                    metrics::TTL_KEYS_TOTAL.dec();
                }
                StoredValue::Stream(v)
            }
            StoredValue::TimeSeries(mut v) => {
                if v.ttl.is_some() {
                    v.ttl = None;
                    metrics::TTL_KEYS_TOTAL.dec();
                }
                StoredValue::TimeSeries(v)
            }
            StoredValue::Geo(mut v) => {
                if v.ttl.is_some() {
                    v.ttl = None;
                    metrics::TTL_KEYS_TOTAL.dec();
                }
                StoredValue::Geo(v)
            }
        };

        let store_type = match &updated_value {
            StoredValue::String(_) => StoreType::String,
            StoredValue::Hash(_) => StoreType::Hash,
            StoredValue::List(_) => StoreType::List,
            StoredValue::Set(_) => StoreType::Set,
            StoredValue::SortedSet(_) => StoreType::SortedSet,
            StoredValue::Json(_) => StoreType::Json,
            StoredValue::Stream(_) => StoreType::Stream,
            StoredValue::TimeSeries(_) => StoreType::TimeSeries,
            StoredValue::Geo(_) => StoreType::Geo,
        };

        self.store_value(key, updated_value, store_type).await?;
        Ok(true)
    }

    /// Delete a key (internal helper for TTL cleanup)
    async fn delete_key(&self, key: &str) -> Result<()> {
        self.delete(key).await
    }

    /// Run TTL cleanup - scans keys and removes expired ones
    /// Returns number of expired keys removed
    pub async fn cleanup_expired_keys(&self) -> Result<usize> {
        let timer = metrics::Timer::new();
        let mut expired_count = 0;
        let mut scanned_count = 0;

        // Get all keys from index
        let all_keys: Vec<String> = {
            let mut keys = Vec::new();
            for item in self.index_tree.iter() {
                if let Ok((k, _)) = item {
                    if let Ok(key_str) = String::from_utf8(k.to_vec()) {
                        keys.push(key_str);
                    }
                }
            }
            keys
        };

        scanned_count = all_keys.len();
        metrics::TTL_KEYS_SCANNED.set(scanned_count as i64);

        for key in all_keys {
            // Try to get the value - this will automatically check TTL and clean up
            // if expired (lazy cleanup on read)
            if let Ok(None) = self.get_value(&key).await {
                // Value was expired and cleaned up during get
                expired_count += 1;
            }
        }

        timer.observe_duration_seconds(&metrics::TTL_CLEANUP_DURATION);
        
        if expired_count > 0 {
            tracing::info!("TTL cleanup: scanned {} keys, expired {} keys", scanned_count, expired_count);
        }

        Ok(expired_count)
    }

    /// Start background TTL cleanup task
    /// Runs cleanup every `interval_seconds` (default: 60 seconds)
    pub fn start_ttl_cleanup_task(storage: BlobStorage, interval_seconds: Option<u64>) {
        let interval = std::time::Duration::from_secs(interval_seconds.unwrap_or(60));
        
        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            
            loop {
                interval_timer.tick().await;
                
                match storage.cleanup_expired_keys().await {
                    Ok(expired) => {
                        if expired > 0 {
                            tracing::debug!("TTL cleanup task: removed {} expired keys", expired);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("TTL cleanup task error: {}", e);
                    }
                }
            }
        });
        
        tracing::info!("TTL cleanup background task started (interval: {}s)", interval.as_secs());
    }
}

/// TTL information for a key
#[derive(Debug, Clone, Serialize, Deserialize, SimpleObject)]
pub struct TtlInfo {
    /// Original TTL in seconds (None if no TTL was set)
    pub ttl_seconds: Option<u64>,
    /// Absolute expiration timestamp in milliseconds (None if no TTL)
    pub expires_at: Option<i64>,
    /// Creation timestamp in milliseconds
    pub created_at: i64,
    /// Remaining TTL in seconds (None if expired or no TTL)
    pub remaining_seconds: Option<u64>,
    /// Whether this key has a TTL configured
    pub has_ttl: bool,
}

// Type alias for backward compatibility
pub type RedisStorage = BlobStorage;
