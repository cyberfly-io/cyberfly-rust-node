//! Secondary indexing system for Redis-style data types
//! Provides MongoDB-like query capabilities on top of the existing storage layer

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Secondary index for fast lookups by field values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecondaryIndex {
    /// Index name
    pub name: String,
    /// Database name this index belongs to
    pub db_name: String,
    /// Field being indexed (e.g., "email", "age", "status")
    pub field: String,
    /// Index type
    pub index_type: IndexType,
    /// Index data: field_value -> set of keys
    pub data: HashMap<String, HashSet<String>>,
    /// Maximum number of keys to index (0 = unlimited)
    pub max_keys: usize,
    /// Memory limit in bytes (0 = unlimited)
    pub memory_limit_bytes: usize,
}

/// Types of indexes supported
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IndexType {
    /// Exact match index (e.g., email = "user@example.com")
    Exact,
    /// Range index for numeric/timestamp values (e.g., age > 18)
    Range,
    /// Full-text search index (for string fields)
    FullText,
    /// Geospatial index (for location-based queries)
    Geo,
}

/// Query operators for indexed lookups
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueryOperator {
    /// Exact match: field = value
    Equals(String),
    /// Greater than: field > value (numeric)
    GreaterThan(f64),
    /// Less than: field < value (numeric)
    LessThan(f64),
    /// Range: min <= field <= max
    Between(f64, f64),
    /// In set: field in [val1, val2, ...]
    In(Vec<String>),
    /// Text contains (case-insensitive)
    Contains(String),
    /// Starts with prefix
    StartsWith(String),
}

/// Index query result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Keys matching the query
    pub keys: Vec<String>,
    /// Number of results
    pub count: usize,
    /// Query execution time in milliseconds
    pub execution_time_ms: u64,
}

impl SecondaryIndex {
    /// Create a new secondary index
    pub fn new(name: String, db_name: String, field: String, index_type: IndexType) -> Self {
        Self {
            name,
            db_name,
            field,
            index_type,
            data: HashMap::new(),
            max_keys: 0,  // unlimited by default
            memory_limit_bytes: 0,  // unlimited by default
        }
    }
    
    /// Create a new index with limits
    pub fn new_with_limits(
        name: String,
        db_name: String,
        field: String,
        index_type: IndexType,
        max_keys: usize,
        memory_limit_mb: usize,
    ) -> Self {
        Self {
            name,
            db_name,
            field,
            index_type,
            data: HashMap::new(),
            max_keys,
            memory_limit_bytes: memory_limit_mb * 1024 * 1024,
        }
    }

    /// Estimate current memory usage in bytes
    pub fn memory_usage_bytes(&self) -> usize {
        let mut total = std::mem::size_of::<Self>();
        
        for (key, values) in &self.data {
            // String overhead + capacity
            total += std::mem::size_of::<String>() + key.capacity();
            // HashSet overhead
            total += std::mem::size_of::<HashSet<String>>();
            
            for value in values {
                total += std::mem::size_of::<String>() + value.capacity();
            }
        }
        
        total
    }
    
    /// Check if adding a key would exceed limits
    fn would_exceed_limits(&self) -> bool {
        if self.max_keys > 0 && self.total_keys() >= self.max_keys {
            return true;
        }
        
        if self.memory_limit_bytes > 0 && self.memory_usage_bytes() >= self.memory_limit_bytes {
            return true;
        }
        
        false
    }

    /// Add a key to the index with the given field value
    pub fn insert(&mut self, field_value: String, key: String) -> Result<()> {
        if self.would_exceed_limits() {
            anyhow::bail!(
                "Index '{}' would exceed limits (max_keys: {}, memory: {} MB)",
                self.name,
                self.max_keys,
                self.memory_limit_bytes / 1024 / 1024
            );
        }
        
        self.data
            .entry(field_value)
            .or_insert_with(HashSet::new)
            .insert(key);
        
        Ok(())
    }

    /// Remove a key from the index
    pub fn remove(&mut self, field_value: &str, key: &str) {
        if let Some(keys) = self.data.get_mut(field_value) {
            keys.remove(key);
            if keys.is_empty() {
                self.data.remove(field_value);
            }
        }
    }

    /// Update index when a field value changes
    pub fn update(&mut self, old_value: &str, new_value: String, key: String) -> Result<()> {
        self.remove(old_value, &key);
        self.insert(new_value, key)
    }

    /// Query the index with the given operator
    pub fn query(&self, operator: &QueryOperator) -> Vec<String> {
        let start = std::time::Instant::now();
        
        let keys = match operator {
            QueryOperator::Equals(value) => {
                self.data.get(value).cloned().unwrap_or_default()
            }
            QueryOperator::In(values) => {
                let mut result = HashSet::new();
                for value in values {
                    if let Some(keys) = self.data.get(value) {
                        result.extend(keys.clone());
                    }
                }
                result
            }
            QueryOperator::GreaterThan(threshold) => {
                self.range_query(|val| val > *threshold)
            }
            QueryOperator::LessThan(threshold) => {
                self.range_query(|val| val < *threshold)
            }
            QueryOperator::Between(min, max) => {
                self.range_query(|val| val >= *min && val <= *max)
            }
            QueryOperator::Contains(substring) => {
                self.text_query(|val| val.to_lowercase().contains(&substring.to_lowercase()))
            }
            QueryOperator::StartsWith(prefix) => {
                self.text_query(|val| val.to_lowercase().starts_with(&prefix.to_lowercase()))
            }
        };

        let elapsed = start.elapsed();
        tracing::debug!(
            "Index query on '{}' returned {} keys in {:?}",
            self.field,
            keys.len(),
            elapsed
        );

        keys.into_iter().collect()
    }

    /// Range query helper for numeric comparisons
    fn range_query<F>(&self, predicate: F) -> HashSet<String>
    where
        F: Fn(f64) -> bool,
    {
        let mut result = HashSet::new();
        for (value_str, keys) in &self.data {
            if let Ok(value) = value_str.parse::<f64>() {
                if predicate(value) {
                    result.extend(keys.clone());
                }
            }
        }
        result
    }

    /// Text query helper for string matching
    fn text_query<F>(&self, predicate: F) -> HashSet<String>
    where
        F: Fn(&str) -> bool,
    {
        let mut result = HashSet::new();
        for (value_str, keys) in &self.data {
            if predicate(value_str) {
                result.extend(keys.clone());
            }
        }
        result
    }

    /// Get all unique field values in the index
    pub fn get_all_values(&self) -> Vec<String> {
        self.data.keys().cloned().collect()
    }

    /// Get total number of indexed keys
    pub fn total_keys(&self) -> usize {
        self.data.values().map(|keys| keys.len()).sum()
    }
}

/// Index manager for managing multiple indexes
#[derive(Clone)]
pub struct IndexManager {
    /// All indexes: (db_name, index_name) -> Index
    indexes: Arc<RwLock<HashMap<(String, String), SecondaryIndex>>>,
}

impl IndexManager {
    /// Create a new index manager
    pub fn new() -> Self {
        Self {
            indexes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new index
    pub async fn create_index(
        &self,
        db_name: String,
        index_name: String,
        field: String,
        index_type: IndexType,
    ) -> Result<()> {
        let mut indexes = self.indexes.write().await;
        let key = (db_name.clone(), index_name.clone());
        
        if indexes.contains_key(&key) {
            anyhow::bail!("Index '{}' already exists in database '{}'", index_name, db_name);
        }

        let index = SecondaryIndex::new(index_name.clone(), db_name.clone(), field.clone(), index_type);
        let field_name = index.field.clone();
        indexes.insert(key, index);
        
        tracing::info!("Created index '{}' for field '{}'", index_name, field_name);
        Ok(())
    }

    /// Drop an index
    pub async fn drop_index(&self, db_name: &str, index_name: &str) -> Result<()> {
        let mut indexes = self.indexes.write().await;
        let key = (db_name.to_string(), index_name.to_string());
        
        if indexes.remove(&key).is_some() {
            tracing::info!("Dropped index '{}' from database '{}'", index_name, db_name);
            Ok(())
        } else {
            anyhow::bail!("Index '{}' not found in database '{}'", index_name, db_name)
        }
    }

    /// Insert a key into an index
    pub async fn insert_into_index(
        &self,
        db_name: &str,
        index_name: &str,
        field_value: String,
        key: String,
    ) -> Result<()> {
        let mut indexes = self.indexes.write().await;
        let index_key = (db_name.to_string(), index_name.to_string());
        
        if let Some(index) = indexes.get_mut(&index_key) {
            index.insert(field_value, key);
            Ok(())
        } else {
            anyhow::bail!("Index '{}' not found in database '{}'", index_name, db_name)
        }
    }

    /// Remove a key from an index
    pub async fn remove_from_index(
        &self,
        db_name: &str,
        index_name: &str,
        field_value: &str,
        key: &str,
    ) -> Result<()> {
        let mut indexes = self.indexes.write().await;
        let index_key = (db_name.to_string(), index_name.to_string());
        
        if let Some(index) = indexes.get_mut(&index_key) {
            index.remove(field_value, key);
            Ok(())
        } else {
            anyhow::bail!("Index '{}' not found in database '{}'", index_name, db_name)
        }
    }

    /// Query an index
    pub async fn query_index(
        &self,
        db_name: &str,
        index_name: &str,
        operator: QueryOperator,
    ) -> Result<QueryResult> {
        let start = std::time::Instant::now();
        let indexes = self.indexes.read().await;
        let index_key = (db_name.to_string(), index_name.to_string());
        
        if let Some(index) = indexes.get(&index_key) {
            let keys = index.query(&operator);
            let count = keys.len();
            let execution_time_ms = start.elapsed().as_millis() as u64;
            
            Ok(QueryResult {
                keys,
                count,
                execution_time_ms,
            })
        } else {
            anyhow::bail!("Index '{}' not found in database '{}'", index_name, db_name)
        }
    }

    /// List all indexes in a database
    pub async fn list_indexes(&self, db_name: &str) -> Vec<String> {
        let indexes = self.indexes.read().await;
        indexes
            .keys()
            .filter(|(db, _)| db == db_name)
            .map(|(_, name)| name.clone())
            .collect()
    }

    /// Get index statistics
    pub async fn get_index_stats(&self, db_name: &str, index_name: &str) -> Result<IndexStats> {
        let indexes = self.indexes.read().await;
        let index_key = (db_name.to_string(), index_name.to_string());
        
        if let Some(index) = indexes.get(&index_key) {
            Ok(IndexStats {
                name: index.name.clone(),
                field: index.field.clone(),
                index_type: index.index_type.clone(),
                total_keys: index.total_keys(),
                unique_values: index.data.len(),
                memory_usage_bytes: index.memory_usage_bytes(),
                memory_usage_mb: (index.memory_usage_bytes() as f64) / (1024.0 * 1024.0),
                max_keys: index.max_keys,
                memory_limit_mb: if index.memory_limit_bytes > 0 {
                    Some(index.memory_limit_bytes / 1024 / 1024)
                } else {
                    None
                },
            })
        } else {
            anyhow::bail!("Index '{}' not found in database '{}'", index_name, db_name)
        }
    }
    
    /// Get total memory usage across all indexes
    pub async fn total_memory_usage(&self) -> usize {
        let indexes = self.indexes.read().await;
        indexes.values().map(|idx| idx.memory_usage_bytes()).sum()
    }
    
    /// Get total memory usage in megabytes
    pub async fn total_memory_usage_mb(&self) -> f64 {
        let bytes = self.total_memory_usage().await;
        (bytes as f64) / (1024.0 * 1024.0)
    }
}


impl Default for IndexManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Index statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub name: String,
    pub field: String,
    pub index_type: IndexType,
    pub total_keys: usize,
    pub unique_values: usize,
    pub memory_usage_bytes: usize,
    pub memory_usage_mb: f64,
    pub max_keys: usize,
    pub memory_limit_mb: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_exact_index() {
        let manager = IndexManager::new();
        
        // Create index on email field
        manager
            .create_index(
                "users".to_string(),
                "email_idx".to_string(),
                "email".to_string(),
                IndexType::Exact,
            )
            .await
            .unwrap();

        // Insert some data
        manager
            .insert_into_index("users", "email_idx", "alice@example.com".to_string(), "user:1".to_string())
            .await
            .unwrap();
        manager
            .insert_into_index("users", "email_idx", "bob@example.com".to_string(), "user:2".to_string())
            .await
            .unwrap();

        // Query by email
        let result = manager
            .query_index("users", "email_idx", QueryOperator::Equals("alice@example.com".to_string()))
            .await
            .unwrap();

        assert_eq!(result.count, 1);
        assert!(result.keys.contains(&"user:1".to_string()));
    }

    #[tokio::test]
    async fn test_range_index() {
        let manager = IndexManager::new();
        
        manager
            .create_index(
                "users".to_string(),
                "age_idx".to_string(),
                "age".to_string(),
                IndexType::Range,
            )
            .await
            .unwrap();

        // Insert users with ages
        manager.insert_into_index("users", "age_idx", "25".to_string(), "user:1".to_string()).await.unwrap();
        manager.insert_into_index("users", "age_idx", "30".to_string(), "user:2".to_string()).await.unwrap();
        manager.insert_into_index("users", "age_idx", "35".to_string(), "user:3".to_string()).await.unwrap();

        // Query users older than 28
        let result = manager
            .query_index("users", "age_idx", QueryOperator::GreaterThan(28.0))
            .await
            .unwrap();

        assert_eq!(result.count, 2);
    }

    #[tokio::test]
    async fn test_text_search() {
        let manager = IndexManager::new();
        
        manager
            .create_index(
                "products".to_string(),
                "name_idx".to_string(),
                "name".to_string(),
                IndexType::FullText,
            )
            .await
            .unwrap();

        manager.insert_into_index("products", "name_idx", "Laptop Computer".to_string(), "prod:1".to_string()).await.unwrap();
        manager.insert_into_index("products", "name_idx", "Desktop Computer".to_string(), "prod:2".to_string()).await.unwrap();
        manager.insert_into_index("products", "name_idx", "Mobile Phone".to_string(), "prod:3".to_string()).await.unwrap();

        // Search for products containing "computer"
        let result = manager
            .query_index("products", "name_idx", QueryOperator::Contains("computer".to_string()))
            .await
            .unwrap();

        assert_eq!(result.count, 2);
    }
}
