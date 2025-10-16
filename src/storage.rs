use anyhow::Result;
use async_graphql::SimpleObject;
use iroh_blobs::store::fs::FsStore;
use iroh_blobs::Hash;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

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

// Data structures for different Redis-like types
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StringValue {
    value: String,
    metadata: Option<SignatureMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HashValue {
    fields: HashMap<String, String>,
    metadata: Option<SignatureMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ListValue {
    items: Vec<String>,
    metadata: Option<SignatureMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SetValue {
    members: HashSet<String>,
    metadata: Option<SignatureMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SortedSetValue {
    // Store JSON objects with scores for sorting
    // Key is the serialized JSON, value is the score (timestamp)
    members: BTreeMap<String, f64>,
    metadata: Option<SignatureMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortedSetEntry {
    pub score: f64,
    pub data: serde_json::Value,
    pub metadata: Option<SignatureMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonValue {
    data: serde_json::Value,
    // Track _id for deduplication if present
    id: Option<String>,
    metadata: Option<SignatureMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StreamValue {
    entries: Vec<(String, Vec<(String, String)>)>,
    metadata: Option<SignatureMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TimeSeriesValue {
    points: BTreeMap<i64, f64>,
    metadata: Option<SignatureMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GeoValue {
    locations: HashMap<String, (f64, f64)>,
    metadata: Option<SignatureMetadata>,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct StorageIndex {
    mappings: HashMap<String, (String, StoreType)>,
}

pub struct BlobStorage {
    store: FsStore,
    index: Arc<RwLock<StorageIndex>>,
    cache: Arc<RwLock<HashMap<String, StoredValue>>>,
}

impl Clone for BlobStorage {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            index: Arc::clone(&self.index),
            cache: Arc::clone(&self.cache),
        }
    }
}

impl BlobStorage {
    pub async fn new(store: FsStore) -> Result<Self> {
        tracing::info!("Initializing BlobStorage with FsStore");

        let storage = Self {
            store,
            index: Arc::new(RwLock::new(StorageIndex::default())),
            cache: Arc::new(RwLock::new(HashMap::new())),
        };

        if let Err(e) = storage.load_index().await {
            tracing::warn!(
                "Could not load existing index: {}. Starting with empty index.",
                e
            );
        }

        tracing::info!("BlobStorage initialized successfully");
        Ok(storage)
    }

    async fn load_index(&self) -> Result<()> {
        // Try to load index from a known blob hash stored in memory
        // For now, start with empty index (production would persist this)
        Ok(())
    }

    async fn save_index(&self) -> Result<()> {
        let index = self.index.read().await;
        let index_json = serde_json::to_vec(&*index)?;
        let blobs = self.store.blobs();
        let _tag = blobs.add_bytes(index_json).await?;
        tracing::debug!("Index saved with {} entries", index.mappings.len());
        Ok(())
    }

    async fn store_value(
        &self,
        key: &str,
        value: StoredValue,
        store_type: StoreType,
    ) -> Result<()> {
        let value_bytes = serde_json::to_vec(&value)?;
        let blobs = self.store.blobs();
        let tag = blobs.add_bytes(value_bytes).await?;
        let hash_str = tag.hash.to_string();

        {
            let mut index = self.index.write().await;
            index
                .mappings
                .insert(key.to_string(), (hash_str, store_type));
        }

        {
            let mut cache = self.cache.write().await;
            cache.insert(key.to_string(), value);
        }

        if self.index.read().await.mappings.len() % 10 == 0 {
            let _ = self.save_index().await;
        }

        Ok(())
    }

    async fn get_value(&self, key: &str) -> Result<Option<StoredValue>> {
        {
            let cache = self.cache.read().await;
            if let Some(value) = cache.get(key) {
                return Ok(Some(value.clone()));
            }
        }

        let (hash_str, _store_type) = {
            let index = self.index.read().await;
            match index.mappings.get(key) {
                Some(entry) => entry.clone(),
                None => return Ok(None),
            }
        };

        let hash: Hash = hash_str.parse()?;
        let blobs = self.store.blobs();
        let value_bytes = blobs.get_bytes(hash).await?.to_vec();
        let value: StoredValue = serde_json::from_slice(&value_bytes)?;

        {
            let mut cache = self.cache.write().await;
            cache.insert(key.to_string(), value.clone());
        }

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
        let stored_value = StoredValue::String(StringValue {
            value: value.to_string(),
            metadata,
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
        let mut hash_value = match self.get_value(key).await? {
            Some(StoredValue::Hash(hv)) => hv,
            None => HashValue {
                fields: HashMap::new(),
                metadata: metadata.clone(),
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
        let mut list_value = match self.get_value(key).await? {
            Some(StoredValue::List(lv)) => lv,
            None => ListValue {
                items: Vec::new(),
                metadata: metadata.clone(),
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
        let mut set_value = match self.get_value(key).await? {
            Some(StoredValue::Set(sv)) => sv,
            None => SetValue {
                members: HashSet::new(),
                metadata: metadata.clone(),
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
        let mut sorted_set_value = match self.get_value(key).await? {
            Some(StoredValue::SortedSet(ssv)) => ssv,
            None => SortedSetValue {
                members: BTreeMap::new(),
                metadata: metadata.clone(),
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
        let index = self.index.read().await;
        Ok(index.mappings.contains_key(key))
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        {
            let mut index = self.index.write().await;
            index.mappings.remove(key);
        }

        {
            let mut cache = self.cache.write().await;
            cache.remove(key);
        }

        self.save_index().await?;
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

        let json_value = JsonValue {
            data: json_data,
            id,
            metadata,
        };
        self.store_value(key, StoredValue::Json(json_value), StoreType::Json)
            .await
    }

    // Delete JSON documents with matching _id
    async fn delete_json_by_id(&self, key_prefix: &str, target_id: &str) -> Result<()> {
        let index = self.index.read().await;
        let keys_to_check: Vec<String> = index
            .mappings
            .keys()
            .filter(|k| k.starts_with(key_prefix))
            .cloned()
            .collect();
        drop(index);

        for key in keys_to_check {
            if let Ok(Some(StoredValue::Json(jv))) = self.get_value(&key).await {
                if jv.id.as_deref() == Some(target_id) {
                    // Remove from cache and index
                    let mut cache = self.cache.write().await;
                    cache.remove(&key);
                    drop(cache);

                    let mut index = self.index.write().await;
                    index.mappings.remove(&key);
                    drop(index);
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
        self.get_json(key, None).await
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
        let mut stream_value = match self.get_value(key).await? {
            Some(StoredValue::Stream(sv)) => sv,
            None => StreamValue {
                entries: Vec::new(),
                metadata: metadata.clone(),
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
        let mut ts_value = match self.get_value(key).await? {
            Some(StoredValue::TimeSeries(tsv)) => tsv,
            None => TimeSeriesValue {
                points: BTreeMap::new(),
                metadata: metadata.clone(),
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
                        let above_min = min_value.map_or(true, |min| **val >= min);
                        let below_max = max_value.map_or(true, |max| **val <= max);
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
        let mut geo_value = match self.get_value(key).await? {
            Some(StoredValue::Geo(gv)) => gv,
            None => GeoValue {
                locations: HashMap::new(),
                metadata: metadata.clone(),
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
        let index = self.index.read().await;
        let prefix = format!("{}:", db_prefix);

        let stream_keys: Vec<String> = index
            .mappings
            .iter()
            .filter(|(key, (_hash, store_type))| {
                key.starts_with(&prefix) && matches!(store_type, StoreType::Stream)
            })
            .map(|(key, _)| key.clone())
            .collect();

        Ok(stream_keys)
    }

    // Scan keys by pattern (similar to Redis SCAN)
    pub async fn scan_keys(&self, pattern: &str) -> Result<Vec<String>> {
        let index = self.index.read().await;

        // Simple pattern matching: * for wildcard
        let regex_pattern = pattern.replace("*", ".*").replace("?", ".");

        let re = regex::Regex::new(&regex_pattern)?;

        let matching_keys: Vec<String> = index
            .mappings
            .keys()
            .filter(|key| re.is_match(key))
            .cloned()
            .collect();

        Ok(matching_keys)
    }

    // Get keys by store type
    pub async fn get_keys_by_type(
        &self,
        db_prefix: &str,
        store_type: StoreType,
    ) -> Result<Vec<String>> {
        let index = self.index.read().await;
        let prefix = format!("{}:", db_prefix);

        let keys: Vec<String> = index
            .mappings
            .iter()
            .filter(|(key, (_hash, stype))| {
                key.starts_with(&prefix)
                    && std::mem::discriminant(stype) == std::mem::discriminant(&store_type)
            })
            .map(|(key, _)| key.clone())
            .collect();

        Ok(keys)
    }
}

// Type alias for backward compatibility
pub type RedisStorage = BlobStorage;
