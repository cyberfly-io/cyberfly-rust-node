use anyhow::Result;
use async_graphql::http::ALL_WEBSOCKET_PROTOCOLS;
use async_graphql::{Context, InputObject, Object, Schema, SimpleObject, Subscription};
use async_graphql_axum::{GraphQLProtocol, GraphQLRequest, GraphQLResponse, GraphQLWebSocket};
use axum::{
    extract::ws::WebSocketUpgrade,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_stream::{Stream, StreamExt};

use crate::{crypto, error::DbError, ipfs::IpfsStorage, storage::RedisStorage, sync::SyncManager};

#[derive(SimpleObject, Clone)]
pub struct StorageResult {
    pub success: bool,
    pub message: String,
}

#[derive(SimpleObject, Clone)]
pub struct QueryResult {
    pub key: String,
    pub value: Option<String>,
}

#[derive(SimpleObject, Clone)]
pub struct IpfsResult {
    pub success: bool,
    pub cid: Option<String>,
    pub message: String,
}

#[derive(SimpleObject, Clone)]
pub struct StreamEntry {
    pub id: String,
    pub fields: Vec<StreamField>,
}

#[derive(SimpleObject, Clone)]
pub struct StreamField {
    pub key: String,
    pub value: String,
}

#[derive(SimpleObject, Clone)]
pub struct SortedSetEntry {
    pub value: String,
    pub score: f64,
}

#[derive(SimpleObject, Clone)]
pub struct GeoResult {
    pub member: String,
    pub longitude: f64,
    pub latitude: f64,
}

#[derive(SimpleObject, Clone)]
pub struct TimeSeriesPoint {
    pub timestamp: String,
    pub value: f64,
}

#[derive(SimpleObject, Clone)]
pub struct GeoLocation {
    pub member: String,
    pub longitude: Option<f64>,
    pub latitude: Option<f64>,
}

#[derive(SimpleObject, Clone)]
pub struct IotMessage {
    pub topic: String,
    pub payload: String,
    pub timestamp: String,
}

#[derive(SimpleObject, Clone)]
pub struct IotPublishResult {
    pub success: bool,
    pub topic: String,
    pub message: String,
}

#[derive(SimpleObject, Clone)]
pub struct JsonWithMeta {
    pub key: String,
    pub data: async_graphql::Json<serde_json::Value>,
    pub public_key: Option<String>,
    pub signature: Option<String>,
    pub timestamp: Option<i64>,
}

#[derive(SimpleObject, Clone)]
pub struct StoredEntryGql {
    pub key: String,
    pub store_type: String,
    pub value: async_graphql::Json<serde_json::Value>,
    pub public_key: Option<String>,
    pub signature: Option<String>,
}

// Per-type GraphQL response types
#[derive(SimpleObject, Clone)]
pub struct StringEntryGql {
    pub key: String,
    pub value: String,
    pub public_key: Option<String>,
    pub signature: Option<String>,
}

#[derive(SimpleObject, Clone)]
pub struct HashEntryGql {
    pub key: String,
    pub fields: async_graphql::Json<serde_json::Value>,
    pub public_key: Option<String>,
    pub signature: Option<String>,
}

#[derive(SimpleObject, Clone)]
pub struct ListEntryGql {
    pub key: String,
    pub items: async_graphql::Json<serde_json::Value>,
    pub public_key: Option<String>,
    pub signature: Option<String>,
}

#[derive(SimpleObject, Clone)]
pub struct SetEntryGql {
    pub key: String,
    pub members: async_graphql::Json<serde_json::Value>,
    pub public_key: Option<String>,
    pub signature: Option<String>,
}

#[derive(SimpleObject, Clone)]
pub struct SortedSetEntryGql {
    pub key: String,
    pub members: async_graphql::Json<serde_json::Value>, // array of {member, score}
    pub public_key: Option<String>,
    pub signature: Option<String>,
}

#[derive(SimpleObject, Clone)]
pub struct StreamEntryGql {
    pub key: String,
    pub entries: async_graphql::Json<serde_json::Value>,
    pub public_key: Option<String>,
    pub signature: Option<String>,
}

#[derive(SimpleObject, Clone)]
pub struct TimeSeriesEntryGql {
    pub key: String,
    pub points: async_graphql::Json<serde_json::Value>,
    pub public_key: Option<String>,
    pub signature: Option<String>,
}

#[derive(SimpleObject, Clone)]
pub struct GeoEntryGql {
    pub key: String,
    pub locations: async_graphql::Json<serde_json::Value>,
    pub public_key: Option<String>,
    pub signature: Option<String>,
}

#[derive(SimpleObject, Clone)]
pub struct BlobOperation {
    pub op_id: String,
    pub timestamp: String,
    pub db_name: String,
    pub key: String,
    pub value: String,
    pub store_type: String,
    pub field: Option<String>,
    pub score: Option<f64>,
    pub json_path: Option<String>,
    pub stream_fields: Option<String>,
    pub ts_timestamp: Option<String>,
    pub longitude: Option<f64>,
    pub latitude: Option<f64>,
    pub public_key: String,
    pub signature: String,
}

#[derive(SimpleObject, Clone)]
pub struct NodeInfo {
    pub node_id: String,
    pub peer_id: String,
    pub health: String,
    pub connected_peers: i32,
    pub discovered_peers: i32,
    pub uptime_seconds: u64,
    pub relay_url: Option<String>,
}

#[derive(SimpleObject, Clone)]
pub struct PeerInfo {
    pub peer_id: String,
    pub connection_status: String,
    pub last_seen: String,
}

/// Real-time message event for subscriptions
#[derive(Clone, Debug)]
pub struct MessageEvent {
    pub topic: String,
    pub payload: Vec<u8>,
    pub timestamp: i64,
}

#[derive(SimpleObject, Clone)]
pub struct MessageUpdate {
    pub topic: String,
    pub payload: String,
    pub timestamp: String,
}

#[derive(InputObject)]
pub struct SignedData {
    /// Database name (must be in format: <name>-<public_key_hex>)
    pub db_name: String,
    /// The data key to store
    pub key: String,
    /// The data value (JSON string)
    pub value: String,
    /// Ed25519 public key (hex encoded)
    pub public_key: String,
    /// Ed25519 signature (hex encoded)
    pub signature: String,
    /// Store type: String, Hash, List, Set, SortedSet
    pub store_type: String,
    /// Optional field name for Hash store type
    pub field: Option<String>,
    /// Optional score for SortedSet store type
    pub score: Option<f64>,
    /// Optional JSON path for JSON store type (default: "$")
    pub json_path: Option<String>,
    /// Optional stream fields for Stream store type (JSON array of key-value pairs)
    pub stream_fields: Option<String>,
    /// Optional timestamp for TimeSeries store type (Unix timestamp in seconds)
    pub timestamp: Option<String>,
    /// Optional longitude for Geo store type
    pub longitude: Option<f64>,
    /// Optional latitude for Geo store type
    pub latitude: Option<f64>,
}

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Get a string value from storage
    async fn get_string(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        key: String,
    ) -> Result<QueryResult, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let full_key = format!("{}:{}", db_name, key);
        let value = storage.get_string(&full_key).await.map_err(DbError::from)?;

        Ok(QueryResult {
            key: full_key,
            value,
        })
    }

    /// Get all string entries for a database
    async fn get_all_strings(
        &self,
        ctx: &Context<'_>,
        db_name: String,
    ) -> Result<Vec<StringEntryGql>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let items = storage.get_all_strings(&db_name).await.map_err(DbError::from)?;
        Ok(items
            .into_iter()
            .map(|(k, v, meta)| StringEntryGql {
                key: k,
                value: v,
                public_key: meta.clone().map(|m| m.public_key),
                signature: meta.map(|m| m.signature),
            })
            .collect())
    }

    /// Get all hashes for a database
    async fn get_all_hashes(
        &self,
        ctx: &Context<'_>,
        db_name: String,
    ) -> Result<Vec<HashEntryGql>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let items = storage.get_all_hashes(&db_name).await.map_err(DbError::from)?;
        Ok(items
            .into_iter()
            .map(|(k, fields, meta)| HashEntryGql {
                key: k,
                fields: async_graphql::Json(serde_json::to_value(fields).unwrap_or(serde_json::Value::Null)),
                public_key: meta.clone().map(|m| m.public_key),
                signature: meta.map(|m| m.signature),
            })
            .collect())
    }

    /// Get all lists for a database
    async fn get_all_lists(
        &self,
        ctx: &Context<'_>,
        db_name: String,
    ) -> Result<Vec<ListEntryGql>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let items = storage.get_all_lists(&db_name).await.map_err(DbError::from)?;
        Ok(items
            .into_iter()
            .map(|(k, items_vec, meta)| ListEntryGql {
                key: k,
                items: async_graphql::Json(serde_json::to_value(items_vec).unwrap_or(serde_json::Value::Null)),
                public_key: meta.clone().map(|m| m.public_key),
                signature: meta.map(|m| m.signature),
            })
            .collect())
    }

    /// Get all sets for a database
    async fn get_all_sets(
        &self,
        ctx: &Context<'_>,
        db_name: String,
    ) -> Result<Vec<SetEntryGql>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let items = storage.get_all_sets(&db_name).await.map_err(DbError::from)?;
        Ok(items
            .into_iter()
            .map(|(k, members, meta)| SetEntryGql {
                key: k,
                members: async_graphql::Json(serde_json::to_value(members).unwrap_or(serde_json::Value::Null)),
                public_key: meta.clone().map(|m| m.public_key),
                signature: meta.map(|m| m.signature),
            })
            .collect())
    }

    /// Get all sorted sets for a database
    async fn get_all_sorted_sets(
        &self,
        ctx: &Context<'_>,
        db_name: String,
    ) -> Result<Vec<SortedSetEntryGql>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let items = storage.get_all_sorted_sets(&db_name).await.map_err(DbError::from)?;
        Ok(items
            .into_iter()
            .map(|(k, members, meta)| SortedSetEntryGql {
                key: k,
                members: async_graphql::Json(serde_json::to_value(members).unwrap_or(serde_json::Value::Null)),
                public_key: meta.clone().map(|m| m.public_key),
                signature: meta.map(|m| m.signature),
            })
            .collect())
    }

    /// Get all JSON docs for a database (alias uses existing get_all_json)
    async fn get_all_jsons(
        &self,
        ctx: &Context<'_>,
        db_name: String,
    ) -> Result<Vec<JsonWithMeta>, DbError> {
        // Reuse existing resolver implementation
        self.get_all_json(ctx, db_name).await
    }

    /// Get all stream entries for a database
    async fn get_all_streams(
        &self,
        ctx: &Context<'_>,
        db_name: String,
    ) -> Result<Vec<StreamEntryGql>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let items = storage.get_all_stream_entries(&db_name).await.map_err(DbError::from)?;
        Ok(items
            .into_iter()
            .map(|(k, entries, meta)| StreamEntryGql {
                key: k,
                entries: async_graphql::Json(serde_json::to_value(entries).unwrap_or(serde_json::Value::Null)),
                public_key: meta.clone().map(|m| m.public_key),
                signature: meta.map(|m| m.signature),
            })
            .collect())
    }

    /// Get all timeseries for a database
    async fn get_all_timeseries(
        &self,
        ctx: &Context<'_>,
        db_name: String,
    ) -> Result<Vec<TimeSeriesEntryGql>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let items = storage.get_all_timeseries(&db_name).await.map_err(DbError::from)?;
        Ok(items
            .into_iter()
            .map(|(k, points, meta)| TimeSeriesEntryGql {
                key: k,
                points: async_graphql::Json(serde_json::to_value(points).unwrap_or(serde_json::Value::Null)),
                public_key: meta.clone().map(|m| m.public_key),
                signature: meta.map(|m| m.signature),
            })
            .collect())
    }

    /// Get all geo entries for a database
    async fn get_all_geo(
        &self,
        ctx: &Context<'_>,
        db_name: String,
    ) -> Result<Vec<GeoEntryGql>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let items = storage.get_all_geo(&db_name).await.map_err(DbError::from)?;
        Ok(items
            .into_iter()
            .map(|(k, locs, meta)| GeoEntryGql {
                key: k,
                locations: async_graphql::Json(serde_json::to_value(locs).unwrap_or(serde_json::Value::Null)),
                public_key: meta.clone().map(|m| m.public_key),
                signature: meta.map(|m| m.signature),
            })
            .collect())
    }

    /// Get a hash field from storage
    async fn get_hash(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        key: String,
        field: String,
    ) -> Result<QueryResult, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let full_key = format!("{}:{}", db_name, key);
        let value = storage
            .get_hash(&full_key, &field)
            .await
            .map_err(DbError::from)?;

        Ok(QueryResult {
            key: format!("{}:{}", full_key, field),
            value,
        })
    }

    /// Get all hash fields from storage
    async fn get_all_hash(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        key: String,
    ) -> Result<Vec<QueryResult>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let full_key = format!("{}:{}", db_name, key);
        let fields = storage
            .get_all_hash(&full_key)
            .await
            .map_err(DbError::from)?;

        Ok(fields
            .into_iter()
            .map(|(field, value)| QueryResult {
                key: format!("{}:{}", full_key, field),
                value: Some(value),
            })
            .collect())
    }

    /// Get list items from storage
    async fn get_list(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        key: String,
        start: Option<i32>,
        stop: Option<i32>,
    ) -> Result<Vec<String>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let full_key = format!("{}:{}", db_name, key);
        let start = start.unwrap_or(0) as isize;
        let stop = stop.unwrap_or(-1) as isize;

        storage
            .get_list(&full_key, start, stop)
            .await
            .map_err(DbError::from)
    }

    /// Get set members from storage
    async fn get_set(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        key: String,
    ) -> Result<Vec<String>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let full_key = format!("{}:{}", db_name, key);
        storage.get_set(&full_key).await.map_err(DbError::from)
    }

    /// Get sorted set range from storage
    async fn get_sorted_set(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        key: String,
        start: Option<i32>,
        stop: Option<i32>,
    ) -> Result<Vec<SortedSetEntry>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let full_key = format!("{}:{}", db_name, key);
        let start = start.unwrap_or(0) as isize;
        let stop = stop.unwrap_or(-1) as isize;

        let results = storage
            .get_sorted_set_with_scores(&full_key, start, stop)
            .await
            .map_err(DbError::from)?;
        Ok(results
            .into_iter()
            .map(|(value, score)| SortedSetEntry { value, score })
            .collect())
    }

    /// Get file from IPFS by CID (hash)
    async fn get_ipfs_file(&self, ctx: &Context<'_>, cid: String) -> Result<String, DbError> {
        let ipfs = ctx
            .data::<IpfsStorage>()
            .map_err(|_| DbError::InternalError("IPFS storage not found".to_string()))?;

        // Use get_bytes (cid is actually a hash string in Iroh)
        let data = ipfs
            .get_bytes(&cid)
            .await
            .map_err(|e| DbError::InternalError(e.to_string()))?;

        // Convert bytes to base64 for transport
        use base64::Engine;
        Ok(base64::engine::general_purpose::STANDARD.encode(&data))
    }

    /// List all pinned CIDs
    /// Note: Iroh doesn't have traditional "pinning" - all added content is persistent
    async fn list_ipfs_pins(&self, ctx: &Context<'_>) -> Result<Vec<String>, DbError> {
        let _ipfs = ctx
            .data::<IpfsStorage>()
            .map_err(|_| DbError::InternalError("IPFS storage not found".to_string()))?;

        // TODO: Implement listing all stored blobs in Iroh
        // For now, return empty list
        Ok(vec![])
    }

    // ============ JSON Queries ============

    /// Get JSON document or specific path
    async fn get_json(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        key: String,
        path: Option<String>,
    ) -> Result<QueryResult, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let full_key = format!("{}:{}", db_name, key);
        let value = storage
            .get_json(&full_key, path.as_deref())
            .await
            .map_err(DbError::from)?;

        Ok(QueryResult {
            key: full_key,
            value,
        })
    }

    /// Filter JSON by JSONPath
    async fn filter_json(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        key: String,
        json_path: String,
    ) -> Result<QueryResult, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let full_key = format!("{}:{}", db_name, key);
        let value = storage
            .filter_json(&full_key, &json_path)
            .await
            .map_err(DbError::from)?;

        Ok(QueryResult {
            key: full_key,
            value,
        })
    }

    /// Get all JSON documents for a database prefix (with signature metadata)
    async fn get_all_json(
        &self,
        ctx: &Context<'_>,
        db_name: String,
    ) -> Result<Vec<JsonWithMeta>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let items = storage
            .get_all_json_with_meta(&db_name)
            .await
            .map_err(DbError::from)?;

        let mut out = Vec::new();
        for (key, data, meta) in items {
            let (pk, sig, ts) = match meta {
                Some(m) => (Some(m.public_key), Some(m.signature), Some(m.timestamp)),
                None => (None, None, None),
            };
            out.push(JsonWithMeta {
                key,
                data: async_graphql::Json(data),
                public_key: pk,
                signature: sig,
                timestamp: ts,
            });
        }

        Ok(out)
    }

    /// Get all entries across all store types for a database prefix
    async fn get_all(
        &self,
        ctx: &Context<'_>,
        db_name: String,
    ) -> Result<Vec<StoredEntryGql>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let items = storage.get_all(&db_name).await.map_err(DbError::from)?;

        let out: Vec<StoredEntryGql> = items
            .into_iter()
            .map(|entry| StoredEntryGql {
                key: entry.key,
                store_type: format!("{:?}", entry.store_type),
                value: async_graphql::Json(entry.value),
                public_key: entry.metadata.clone().map(|m| m.public_key),
                signature: entry.metadata.map(|m| m.signature),
            })
            .collect();

        Ok(out)
    }

    // ============ Stream Queries ============

    /// Get stream entries by range
    async fn get_stream(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        key: String,
        start: Option<String>,
        end: Option<String>,
        count: Option<i32>,
    ) -> Result<Vec<StreamEntry>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let full_key = format!("{}:{}", db_name, key);
        let start = start.as_deref().unwrap_or("-");
        let end = end.as_deref().unwrap_or("+");
        let count_val = count.map(|c| c as usize);

        let entries = storage
            .xrange(&full_key, start, end, count_val)
            .await
            .map_err(DbError::from)?;

        Ok(entries
            .into_iter()
            .map(|(id, fields)| StreamEntry {
                id,
                fields: fields
                    .into_iter()
                    .map(|(k, v)| StreamField { key: k, value: v })
                    .collect(),
            })
            .collect())
    }

    /// Filter stream entries by pattern
    async fn filter_stream(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        key: String,
        start: Option<String>,
        end: Option<String>,
        pattern: Option<String>,
    ) -> Result<Vec<StreamEntry>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let full_key = format!("{}:{}", db_name, key);
        let start = start.as_deref().unwrap_or("-");
        let end = end.as_deref().unwrap_or("+");

        let entries = storage
            .filter_stream(&full_key, start, end, pattern.as_deref())
            .await
            .map_err(DbError::from)?;

        Ok(entries
            .into_iter()
            .map(|(id, fields)| StreamEntry {
                id,
                fields: fields
                    .into_iter()
                    .map(|(k, v)| StreamField { key: k, value: v })
                    .collect(),
            })
            .collect())
    }

    /// Get stream length
    async fn get_stream_length(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        key: String,
    ) -> Result<i32, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let full_key = format!("{}:{}", db_name, key);
        let length = storage.xlen(&full_key).await.map_err(DbError::from)?;

        Ok(length as i32)
    }

    // ============ TimeSeries Queries ============

    /// Get time series data by time range
    async fn get_timeseries(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        key: String,
        from_timestamp: String,
        to_timestamp: String,
    ) -> Result<Vec<TimeSeriesPoint>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let full_key = format!("{}:{}", db_name, key);
        let from_ts = from_timestamp
            .parse::<i64>()
            .map_err(|_| DbError::InvalidData("Invalid from_timestamp".to_string()))?;
        let to_ts = to_timestamp
            .parse::<i64>()
            .map_err(|_| DbError::InvalidData("Invalid to_timestamp".to_string()))?;

        let data = storage
            .ts_range(&full_key, from_ts, to_ts)
            .await
            .map_err(DbError::from)?;

        Ok(data
            .into_iter()
            .map(|(ts, val)| TimeSeriesPoint {
                timestamp: ts.to_string(),
                value: val,
            })
            .collect())
    }

    /// Filter time series by value range
    async fn filter_timeseries(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        key: String,
        from_timestamp: String,
        to_timestamp: String,
        min_value: Option<f64>,
        max_value: Option<f64>,
    ) -> Result<Vec<TimeSeriesPoint>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let full_key = format!("{}:{}", db_name, key);
        let from_ts = from_timestamp
            .parse::<i64>()
            .map_err(|_| DbError::InvalidData("Invalid from_timestamp".to_string()))?;
        let to_ts = to_timestamp
            .parse::<i64>()
            .map_err(|_| DbError::InvalidData("Invalid to_timestamp".to_string()))?;

        let data = storage
            .filter_timeseries(&full_key, from_ts, to_ts, min_value, max_value)
            .await
            .map_err(DbError::from)?;

        Ok(data
            .into_iter()
            .map(|(ts, val)| TimeSeriesPoint {
                timestamp: ts.to_string(),
                value: val,
            })
            .collect())
    }

    /// Get latest time series value
    async fn get_latest_timeseries(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        key: String,
    ) -> Result<Option<TimeSeriesPoint>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let full_key = format!("{}:{}", db_name, key);
        let data = storage.ts_get(&full_key).await.map_err(DbError::from)?;

        Ok(data.map(|(ts, val)| TimeSeriesPoint {
            timestamp: ts.to_string(),
            value: val,
        }))
    }

    // ============ Geospatial Queries ============

    /// Get location of a member
    async fn get_geo_location(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        key: String,
        member: String,
    ) -> Result<Option<GeoLocation>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let full_key = format!("{}:{}", db_name, key);
        let pos = storage
            .geopos(&full_key, &member)
            .await
            .map_err(DbError::from)?;

        Ok(pos.map(|(lon, lat)| GeoLocation {
            member: member.clone(),
            longitude: Some(lon),
            latitude: Some(lat),
        }))
    }

    /// Search locations within radius
    async fn search_geo_radius(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        key: String,
        longitude: f64,
        latitude: f64,
        radius: f64,
        unit: Option<String>,
    ) -> Result<Vec<GeoResult>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let full_key = format!("{}:{}", db_name, key);
        let unit_str = unit.as_deref().unwrap_or("m");

        let results = storage
            .georadius_with_coords(&full_key, longitude, latitude, radius, unit_str)
            .await
            .map_err(DbError::from)?;
        Ok(results
            .into_iter()
            .map(|(member, lon, lat)| GeoResult {
                member,
                longitude: lon,
                latitude: lat,
            })
            .collect())
    }

    /// Search locations within radius from member
    async fn search_geo_radius_by_member(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        key: String,
        member: String,
        radius: f64,
        unit: Option<String>,
    ) -> Result<Vec<GeoResult>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let full_key = format!("{}:{}", db_name, key);
        let unit_str = unit.as_deref().unwrap_or("m");

        let results = storage
            .georadiusbymember_with_coords(&full_key, &member, radius, unit_str)
            .await
            .map_err(DbError::from)?;
        Ok(results
            .into_iter()
            .map(|(member, lon, lat)| GeoResult {
                member,
                longitude: lon,
                latitude: lat,
            })
            .collect())
    }

    /// Calculate distance between two members
    async fn get_geo_distance(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        key: String,
        member1: String,
        member2: String,
        unit: Option<String>,
    ) -> Result<Option<f64>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let full_key = format!("{}:{}", db_name, key);

        storage
            .geodist(&full_key, &member1, &member2, unit.as_deref())
            .await
            .map_err(DbError::from)
    }

    // ============ Filter Queries for Basic Types ============

    /// Filter hash fields by pattern
    async fn filter_hash(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        key: String,
        field_pattern: String,
    ) -> Result<Vec<QueryResult>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let full_key = format!("{}:{}", db_name, key);
        let fields = storage
            .filter_hash(&full_key, &field_pattern)
            .await
            .map_err(DbError::from)?;

        Ok(fields
            .into_iter()
            .map(|(field, value)| QueryResult {
                key: format!("{}:{}", full_key, field),
                value: Some(value),
            })
            .collect())
    }

    /// Filter list by value pattern
    async fn filter_list(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        key: String,
        value_pattern: String,
    ) -> Result<Vec<String>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let full_key = format!("{}:{}", db_name, key);
        storage
            .filter_list(&full_key, &value_pattern)
            .await
            .map_err(DbError::from)
    }

    /// Filter set by member pattern
    async fn filter_set(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        key: String,
        member_pattern: String,
    ) -> Result<Vec<String>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let full_key = format!("{}:{}", db_name, key);
        storage
            .filter_set(&full_key, &member_pattern)
            .await
            .map_err(DbError::from)
    }

    /// Filter sorted set by score range
    async fn filter_sorted_set(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        key: String,
        min_score: f64,
        max_score: f64,
    ) -> Result<Vec<SortedSetEntry>, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        let full_key = format!("{}:{}", db_name, key);
        let results = storage
            .filter_sorted_set_with_scores(&full_key, min_score, max_score)
            .await
            .map_err(DbError::from)?;
        Ok(results
            .into_iter()
            .map(|(value, score)| SortedSetEntry { value, score })
            .collect())
    }

    // ============ IoT Queries ============

    /// Get recent IoT messages
    async fn get_iot_messages(
        &self,
        ctx: &Context<'_>,
        topic_filter: Option<String>,
        limit: Option<i32>,
    ) -> Result<Vec<IotMessage>, DbError> {
        use crate::mqtt_bridge::MqttMessageStore;

        let store = ctx
            .data::<MqttMessageStore>()
            .map_err(|_| DbError::InternalError("MQTT message store not found".to_string()))?;

        let messages = store
            .get_messages(topic_filter, limit.map(|l| l as usize))
            .await;

        Ok(messages
            .into_iter()
            .map(|m| IotMessage {
                topic: m.topic,
                payload: String::from_utf8_lossy(&m.payload).to_string(),
                timestamp: m.timestamp.to_string(),
            })
            .collect())
    }

    // ============ Blob Operation Queries ============

    /// Get all blob operations for a specific database
    async fn get_blob_operations(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        limit: Option<i32>,
    ) -> Result<Vec<BlobOperation>, DbError> {
        let sync_manager = ctx
            .data::<SyncManager>()
            .map_err(|_| DbError::InternalError("SyncManager not found".to_string()))?;

        let all_operations = sync_manager.sync_store().get_all_operations().await;

        // Filter by database name
        let filtered_ops: Vec<BlobOperation> = all_operations
            .into_iter()
            .filter(|op| op.db_name == db_name)
            .take(limit.unwrap_or(100) as usize)
            .map(|op| BlobOperation {
                op_id: op.op_id,
                timestamp: op.timestamp.to_string(),
                db_name: op.db_name,
                key: op.key,
                value: op.value,
                store_type: op.store_type,
                field: op.field,
                score: op.score,
                json_path: op.json_path,
                stream_fields: op.stream_fields,
                ts_timestamp: op.ts_timestamp,
                longitude: op.longitude,
                latitude: op.latitude,
                public_key: op.public_key,
                signature: op.signature,
            })
            .collect();

        Ok(filtered_ops)
    }

    /// Get all blob operations (across all databases)
    async fn get_all_blob_operations(
        &self,
        ctx: &Context<'_>,
        limit: Option<i32>,
    ) -> Result<Vec<BlobOperation>, DbError> {
        let sync_manager = ctx
            .data::<SyncManager>()
            .map_err(|_| DbError::InternalError("SyncManager not found".to_string()))?;

        let all_operations = sync_manager.sync_store().get_all_operations().await;

        // Apply limit
        let limited_ops: Vec<BlobOperation> = all_operations
            .into_iter()
            .take(limit.unwrap_or(100) as usize)
            .map(|op| BlobOperation {
                op_id: op.op_id,
                timestamp: op.timestamp.to_string(),
                db_name: op.db_name,
                key: op.key,
                value: op.value,
                store_type: op.store_type,
                field: op.field,
                score: op.score,
                json_path: op.json_path,
                stream_fields: op.stream_fields,
                ts_timestamp: op.ts_timestamp,
                longitude: op.longitude,
                latitude: op.latitude,
                public_key: op.public_key,
                signature: op.signature,
            })
            .collect();

        Ok(limited_ops)
    }

    /// Get blob operations by database name since a specific timestamp
    async fn get_blob_operations_since(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        timestamp: String,
        limit: Option<i32>,
    ) -> Result<Vec<BlobOperation>, DbError> {
        let sync_manager = ctx
            .data::<SyncManager>()
            .map_err(|_| DbError::InternalError("SyncManager not found".to_string()))?;

        let ts = timestamp
            .parse::<i64>()
            .map_err(|_| DbError::InvalidData("Invalid timestamp".to_string()))?;

        let operations = sync_manager.sync_store().get_operations_since(ts).await;

        // Filter by database name
        let filtered_ops: Vec<BlobOperation> = operations
            .into_iter()
            .filter(|op| op.db_name == db_name)
            .take(limit.unwrap_or(100) as usize)
            .map(|op| BlobOperation {
                op_id: op.op_id,
                timestamp: op.timestamp.to_string(),
                db_name: op.db_name,
                key: op.key,
                value: op.value,
                store_type: op.store_type,
                field: op.field,
                score: op.score,
                json_path: op.json_path,
                stream_fields: op.stream_fields,
                ts_timestamp: op.ts_timestamp,
                longitude: op.longitude,
                latitude: op.latitude,
                public_key: op.public_key,
                signature: op.signature,
            })
            .collect();

        Ok(filtered_ops)
    }

    /// Get count of blob operations for a database
    async fn get_blob_operation_count(
        &self,
        ctx: &Context<'_>,
        db_name: Option<String>,
    ) -> Result<i32, DbError> {
        let sync_manager = ctx
            .data::<SyncManager>()
            .map_err(|_| DbError::InternalError("SyncManager not found".to_string()))?;

        if let Some(db) = db_name {
            // Count operations for specific database
            let all_operations = sync_manager.sync_store().get_all_operations().await;
            let count = all_operations.iter().filter(|op| op.db_name == db).count();
            Ok(count as i32)
        } else {
            // Count all operations
            let count = sync_manager.sync_store().operation_count().await;
            Ok(count as i32)
        }
    }

    // ============ Node Information Queries ============

    /// Get node information including peer connections and health
    async fn get_node_info(&self, ctx: &Context<'_>) -> Result<NodeInfo, DbError> {
        let endpoint = ctx
            .data::<iroh::Endpoint>()
            .map_err(|_| DbError::InternalError("Endpoint not found".to_string()))?;

        let node_id = endpoint.id().to_string();

        // Get basic stats from endpoint
        // Note: Iroh doesn't expose peer count directly, so we return placeholder values
        let connected_peers = 0; // TODO: Track peers from gossip events
        let discovered_peers = 0;
        let uptime_seconds = 0u64; // TODO: Track uptime

        // Determine health status
        let health = if connected_peers > 0 {
            "healthy"
        } else if discovered_peers > 0 {
            "discovering"
        } else {
            "isolated"
        };

        Ok(NodeInfo {
            node_id: node_id.clone(),
            peer_id: node_id,
            health: health.to_string(),
            connected_peers: connected_peers as i32,
            discovered_peers: discovered_peers as i32,
            uptime_seconds,
            relay_url: None, // TODO: Get from endpoint config
        })
    }

    /// Get list of connected peers
    async fn get_connected_peers(&self, ctx: &Context<'_>) -> Result<Vec<PeerInfo>, DbError> {
        let _endpoint = ctx
            .data::<iroh::Endpoint>()
            .map_err(|_| DbError::InternalError("Endpoint not found".to_string()))?;

        // TODO: Implement peer tracking from gossip events
        // For now, return empty list
        Ok(Vec::new())
    }

    /// Get list of discovered peers (not necessarily connected)
    async fn get_discovered_peers(&self, ctx: &Context<'_>) -> Result<Vec<PeerInfo>, DbError> {
        let peers_map = ctx
            .data::<std::sync::Arc<dashmap::DashMap<iroh::EndpointId, chrono::DateTime<chrono::Utc>>>>()
            .map_err(|_| DbError::InternalError("Discovered peers map not found".to_string()))?;

        let peers: Vec<(iroh::EndpointId, chrono::DateTime<chrono::Utc>)> = peers_map
            .iter()
            .map(|entry| (*entry.key(), *entry.value()))
            .collect();

        Ok(peers
            .into_iter()
            .map(|(node_id, last_seen)| PeerInfo {
                peer_id: node_id.to_string(),
                last_seen: last_seen.to_rfc3339(),
                connection_status: "connected".to_string(),
            })
            .collect())
    }
}

pub struct MutationRoot;

#[Object]
impl MutationRoot {
    /// Submit signed data to the database
    async fn submit_data(
        &self,
        ctx: &Context<'_>,
        input: SignedData,
    ) -> Result<StorageResult, DbError> {
        let storage = ctx
            .data::<RedisStorage>()
            .map_err(|_| DbError::InternalError("Storage not found".to_string()))?;

        // Verify database name matches public key
        crypto::verify_db_name(&input.db_name, &input.public_key).map_err(|e| {
            DbError::SignatureError(format!("Database name verification failed: {}", e))
        })?;

        // Decode public key and signature from hex
        let public_key_bytes = hex::decode(&input.public_key)
            .map_err(|e| DbError::InvalidData(format!("Invalid public key hex: {}", e)))?;
        let signature_bytes = hex::decode(&input.signature)
            .map_err(|e| DbError::InvalidData(format!("Invalid signature hex: {}", e)))?;

        // Create message to verify (db_name:key:value)
        let message = format!("{}:{}:{}", input.db_name, input.key, input.value);

        // Verify signature
        crypto::verify_signature(&public_key_bytes, message.as_bytes(), &signature_bytes)
            .map_err(|e| DbError::SignatureError(e.to_string()))?;

        tracing::info!(
            "Signature verified for db: {}, key: {}",
            input.db_name,
            input.key
        );

        // Create full key with database namespace
        let full_key = format!("{}:{}", input.db_name, input.key);

        // Create signature metadata to store alongside values when possible
        let metadata_ts: i64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let sig_meta = Some(crate::storage::SignatureMetadata {
            public_key: input.public_key.clone(),
            signature: input.signature.clone(),
            timestamp: metadata_ts,
        });

        // Clone fields we'll need later for SignedOperation (before they're moved)
        let field_clone = input.field.clone();
        let score_clone = input.score;
        let json_path_clone = input.json_path.clone();
        let stream_fields_clone = input.stream_fields.clone();
        let ts_timestamp_clone = input.timestamp.clone();
        let longitude_clone = input.longitude;
        let latitude_clone = input.latitude;

        // Store data based on type
        match input.store_type.to_lowercase().as_str() {
            "string" => {
                storage
                    .set_string_with_metadata(&full_key, &input.value, sig_meta.clone())
                    .await
                    .map_err(DbError::from)?;
            }
            "hash" => {
                let field = input.field.ok_or_else(|| {
                    DbError::InvalidData("Field required for Hash type".to_string())
                })?;
                storage
                    .set_hash_with_metadata(&full_key, &field, &input.value, sig_meta.clone())
                    .await
                    .map_err(DbError::from)?;
            }
            "list" => {
                storage
                    .push_list_with_metadata(&full_key, &input.value, sig_meta.clone())
                    .await
                    .map_err(DbError::from)?;
            }
            "set" => {
                storage
                    .add_set_with_metadata(&full_key, &input.value, sig_meta.clone())
                    .await
                    .map_err(DbError::from)?;
            }
            "sortedset" => {
                let score = input.score.ok_or_else(|| {
                    DbError::InvalidData("Score required for SortedSet type".to_string())
                })?;
                storage
                    .add_sorted_set_with_metadata(&full_key, score, &input.value, sig_meta.clone())
                    .await
                    .map_err(DbError::from)?;
            }
            "json" => {
                let path = input.json_path.as_deref().unwrap_or("$");
                storage
                    .set_json_with_metadata(&full_key, path, &input.value, sig_meta.clone())
                    .await
                    .map_err(DbError::from)?;
            }
            "stream" => {
                // Parse stream_fields JSON array: [{"key": "field1", "value": "val1"}, ...]
                let fields_json = input.stream_fields.ok_or_else(|| {
                    DbError::InvalidData("stream_fields required for Stream type".to_string())
                })?;
                let fields: Vec<serde_json::Value> =
                    serde_json::from_str(&fields_json).map_err(|e| {
                        DbError::InvalidData(format!("Invalid stream_fields JSON: {}", e))
                    })?;

                let mut owned_fields: Vec<(String, String)> = Vec::new();

                for field_obj in fields {
                    if let (Some(k), Some(v)) = (
                        field_obj.get("key").and_then(|k| k.as_str()),
                        field_obj.get("value").and_then(|v| v.as_str()),
                    ) {
                        owned_fields.push((k.to_string(), v.to_string()));
                    }
                }
                let mut field_pairs: Vec<(String, String)> = Vec::new();

                for (k, v) in &owned_fields {
                    field_pairs.push((k.clone(), v.clone()));
                }

                storage
                    .xadd_with_metadata(&full_key, "*", &owned_fields, sig_meta.clone())
                    .await
                    .map_err(DbError::from)?;
            }
            "timeseries" => {
                let timestamp_str = input.timestamp.ok_or_else(|| {
                    DbError::InvalidData("timestamp required for TimeSeries type".to_string())
                })?;
                let timestamp = timestamp_str
                    .parse::<i64>()
                    .map_err(|_| DbError::InvalidData("Invalid timestamp format".to_string()))?;
                let value = input.value.parse::<f64>().map_err(|_| {
                    DbError::InvalidData("Value must be a number for TimeSeries type".to_string())
                })?;

                storage
                    .ts_add_with_metadata(&full_key, timestamp, value, sig_meta.clone())
                    .await
                    .map_err(DbError::from)?;
            }
            "geo" => {
                let longitude = input.longitude.ok_or_else(|| {
                    DbError::InvalidData("longitude required for Geo type".to_string())
                })?;
                let latitude = input.latitude.ok_or_else(|| {
                    DbError::InvalidData("latitude required for Geo type".to_string())
                })?;

                storage
                    .geoadd_with_metadata(&full_key, longitude, latitude, &input.value, sig_meta.clone())
                    .await
                    .map_err(DbError::from)?;
            }
            _ => {
                return Err(DbError::InvalidData(format!(
                    "Unknown store type: {}",
                    input.store_type
                )));
            }
        }

        // Create SignedOperation for the sync system
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let signed_operation = crate::sync::SignedOperation {
            op_id: uuid::Uuid::new_v4().to_string(),
            timestamp,
            db_name: input.db_name.clone(),
            key: input.key.clone(),
            value: input.value.clone(),
            store_type: input.store_type.clone(),
            field: field_clone,
            score: score_clone,
            json_path: json_path_clone,
            stream_fields: stream_fields_clone,
            ts_timestamp: ts_timestamp_clone,
            longitude: longitude_clone,
            latitude: latitude_clone,
            public_key: input.public_key.clone(),
            signature: input.signature.clone(),
        };

        // Add operation to SyncManager (stores in blob storage)
        if let Ok(sync_manager) = ctx.data::<SyncManager>() {
            match sync_manager
                .sync_store()
                .add_operation(signed_operation.clone())
                .await
            {
                Ok(added) => {
                    if added {
                        tracing::debug!(
                            "Operation added to blob storage: {}",
                            signed_operation.op_id
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to add operation to blob storage: {}", e);
                    // Continue - data is still in Redis
                }
            }
        }

        // If an outbound sync sender is available in the GraphQL context, broadcast the operation
        if let Ok(sync_out_tx) = ctx.data::<tokio::sync::mpsc::UnboundedSender<crate::sync::SyncMessage>>() {
            tracing::info!("GraphQL: sending outbound SyncMessage::Operation: {}", signed_operation.op_id);
            let res = sync_out_tx.send(crate::sync::SyncMessage::Operation { operation: signed_operation.clone() });
            if res.is_err() {
                tracing::warn!("GraphQL: failed to send outbound sync message (receiver gone)");
            }
        }

        // Broadcast message event to subscribers if broadcast channel is available
        if let Ok(broadcast_tx) = ctx.data::<broadcast::Sender<MessageEvent>>() {
            let event = MessageEvent {
                topic: format!("graphql/{}/{}", input.db_name, input.key),
                payload: input.value.as_bytes().to_vec(),
                timestamp,
            };

            // Ignore send errors (no active subscribers)
            let _ = broadcast_tx.send(event);
        }

        Ok(StorageResult {
            success: true,
            message: format!(
                "Data stored successfully in db: {}, key: {}",
                input.db_name, input.key
            ),
        })
    }

    /// Upload data to IPFS
    async fn add_to_ipfs(&self, ctx: &Context<'_>, data: String) -> Result<IpfsResult, DbError> {
        let ipfs = ctx
            .data::<IpfsStorage>()
            .map_err(|_| DbError::InternalError("IPFS storage not found".to_string()))?;

        let bytes = data.as_bytes();
        let cid = ipfs
            .add_bytes(bytes)
            .await
            .map_err(|e| DbError::InternalError(e.to_string()))?;

        Ok(IpfsResult {
            success: true,
            cid: Some(cid.clone()),
            message: format!("Data added to IPFS with CID: {}", cid),
        })
    }

    /// Pin a CID in IPFS
    /// Note: Iroh doesn't have traditional "pinning" - all added content is persistent by default
    async fn pin_ipfs(&self, ctx: &Context<'_>, cid: String) -> Result<IpfsResult, DbError> {
        let _ipfs = ctx
            .data::<IpfsStorage>()
            .map_err(|_| DbError::InternalError("IPFS storage not found".to_string()))?;

        // No-op in Iroh - content is already persistent
        Ok(IpfsResult {
            success: true,
            cid: Some(cid.clone()),
            message: format!(
                "CID already persistent (Iroh doesn't need explicit pinning): {}",
                cid
            ),
        })
    }

    /// Unpin a CID in IPFS
    /// Note: Iroh doesn't have traditional "unpinning" - garbage collection is handled differently
    async fn unpin_ipfs(&self, ctx: &Context<'_>, cid: String) -> Result<IpfsResult, DbError> {
        let _ipfs = ctx
            .data::<IpfsStorage>()
            .map_err(|_| DbError::InternalError("IPFS storage not found".to_string()))?;

        // No-op in Iroh
        Ok(IpfsResult {
            success: true,
            cid: Some(cid.clone()),
            message: format!("CID unpinned: {}", cid),
        })
    }

    // ============ IoT Mutations ============

    /// Publish message to IoT devices via MQTT
    async fn publish_iot_message(
        &self,
        ctx: &Context<'_>,
        topic: String,
        payload: String,
        qos: Option<i32>,
    ) -> Result<IotPublishResult, DbError> {
        use crate::mqtt_bridge::{GossipToMqttMessage, MessageOrigin};
        use rumqttc::QoS;
        use sha2::{Digest, Sha256};
        use tokio::sync::mpsc;

        let mqtt_tx = ctx
            .data::<mpsc::UnboundedSender<GossipToMqttMessage>>()
            .map_err(|_| DbError::InternalError("MQTT bridge not available".to_string()))?;

        let qos_level = match qos.unwrap_or(1) {
            0 => QoS::AtMostOnce,
            1 => QoS::AtLeastOnce,
            2 => QoS::ExactlyOnce,
            _ => QoS::AtLeastOnce,
        };

        let payload_bytes = payload.as_bytes().to_vec();

        // Generate message ID
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros();
        let mut hasher = Sha256::new();
        hasher.update(topic.as_bytes());
        hasher.update(&payload_bytes);
        hasher.update(timestamp.to_le_bytes());
        let message_id = format!("{:x}", hasher.finalize());

        let message = GossipToMqttMessage {
            topic: topic.clone(),
            payload: payload_bytes.clone(),
            qos: qos_level,
            message_id,
            origin: MessageOrigin::Gossip, // Originated from GraphQL API (gossip side)
        };

        mqtt_tx
            .send(message.clone())
            .map_err(|e| DbError::InternalError(format!("Failed to send to MQTT: {}", e)))?;

        // Also forward this publish into the gossip network so other peers receive it.
        if let Ok(mqtt_to_gossip) =
            ctx.data::<mpsc::UnboundedSender<crate::mqtt_bridge::MqttToGossipMessage>>()
        {
            let mg_msg = crate::mqtt_bridge::MqttToGossipMessage {
                topic: topic.clone(),
                payload: payload_bytes.clone(),
                message_id: message.message_id.clone(),
            };
            // Best-effort send; ignore failures to avoid failing the GraphQL mutation
            let _ = mqtt_to_gossip.send(mg_msg);
        }

        Ok(IotPublishResult {
            success: true,
            topic: topic.clone(),
            message: format!("Message published to topic: {}", topic),
        })
    }
}

pub struct SubscriptionRoot;

#[Subscription]
impl SubscriptionRoot {
    /// Subscribe to messages on a specific topic (supports wildcards)
    async fn subscribe_topic<'ctx>(
        &self,
        ctx: &Context<'ctx>,
        topic_filter: String,
    ) -> Result<impl Stream<Item = MessageUpdate> + 'ctx, DbError> {
        let rx = ctx
            .data::<broadcast::Sender<MessageEvent>>()
            .map_err(|_| DbError::InternalError("Message broadcast channel not found".to_string()))?
            .subscribe();

        Ok(
            tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(move |result| {
                let topic_filter = topic_filter.clone();
                match result {
                    Ok(event) => {
                        if topic_matches(&event.topic, &topic_filter) {
                            Some(MessageUpdate {
                                topic: event.topic,
                                payload: String::from_utf8_lossy(&event.payload).to_string(),
                                timestamp: event.timestamp.to_string(),
                            })
                        } else {
                            None
                        }
                    }
                    Err(_) => None,
                }
            }),
        )
    }

    /// Subscribe to all messages (no filter)
    async fn subscribe_all_messages<'ctx>(
        &self,
        ctx: &Context<'ctx>,
    ) -> Result<impl Stream<Item = MessageUpdate> + 'ctx, DbError> {
        let rx = ctx
            .data::<broadcast::Sender<MessageEvent>>()
            .map_err(|_| DbError::InternalError("Message broadcast channel not found".to_string()))?
            .subscribe();

        Ok(
            tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|result| match result {
                Ok(event) => Some(MessageUpdate {
                    topic: event.topic,
                    payload: String::from_utf8_lossy(&event.payload).to_string(),
                    timestamp: event.timestamp.to_string(),
                }),
                Err(_) => None,
            }),
        )
    }

    /// Subscribe to IoT messages with optional topic filter
    async fn subscribe_iot_messages<'ctx>(
        &self,
        ctx: &Context<'ctx>,
        topic_filter: Option<String>,
    ) -> Result<impl Stream<Item = IotMessage> + 'ctx, DbError> {
        let rx = ctx
            .data::<broadcast::Sender<MessageEvent>>()
            .map_err(|_| DbError::InternalError("Message broadcast channel not found".to_string()))?
            .subscribe();

        Ok(
            tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(move |result| {
                let filter = topic_filter.clone();
                match result {
                    Ok(event) => {
                        if event.topic.starts_with("iot/") {
                            if let Some(f) = filter {
                                if !topic_matches(&event.topic, &f) {
                                    return None;
                                }
                            }
                            Some(IotMessage {
                                topic: event.topic,
                                payload: String::from_utf8_lossy(&event.payload).to_string(),
                                timestamp: event.timestamp.to_string(),
                            })
                        } else {
                            None
                        }
                    }
                    Err(_) => None,
                }
            }),
        )
    }
}

/// Check if a topic matches a filter pattern (supports MQTT wildcards)
/// + = single level wildcard
/// # = multi-level wildcard
fn topic_matches(topic: &str, filter: &str) -> bool {
    // Exact match
    if topic == filter {
        return true;
    }

    let topic_parts: Vec<&str> = topic.split('/').collect();
    let filter_parts: Vec<&str> = filter.split('/').collect();

    let mut topic_idx = 0;
    let mut filter_idx = 0;

    while filter_idx < filter_parts.len() && topic_idx < topic_parts.len() {
        let filter_part = filter_parts[filter_idx];
        let topic_part = topic_parts[topic_idx];

        match filter_part {
            "#" => {
                // Multi-level wildcard matches everything remaining
                return true;
            }
            "+" => {
                // Single-level wildcard matches one level
                topic_idx += 1;
                filter_idx += 1;
            }
            _ => {
                // Exact match required
                if filter_part != topic_part {
                    return false;
                }
                topic_idx += 1;
                filter_idx += 1;
            }
        }
    }

    // Both should be exhausted for a match (unless filter ends with #)
    topic_idx == topic_parts.len() && filter_idx == filter_parts.len()
}

pub type ApiSchema = Schema<QueryRoot, MutationRoot, SubscriptionRoot>;

pub async fn create_server(
    storage: RedisStorage,
    ipfs: IpfsStorage, // Now passed in from main with shared network
    sync_manager: Option<SyncManager>,
    endpoint: Option<iroh::Endpoint>, // Pass Endpoint instead of wrapped IrohNetwork
    discovered_peers: Option<Arc<dashmap::DashMap<iroh::EndpointId, chrono::DateTime<chrono::Utc>>>>, // Discovered peers map
    mqtt_tx: Option<tokio::sync::mpsc::UnboundedSender<crate::mqtt_bridge::GossipToMqttMessage>>,
    mqtt_to_gossip_tx: Option<
        tokio::sync::mpsc::UnboundedSender<crate::mqtt_bridge::MqttToGossipMessage>,
    >,
    mqtt_store: Option<crate::mqtt_bridge::MqttMessageStore>,
    message_broadcast: Option<broadcast::Sender<MessageEvent>>,
    sync_outbound: Option<tokio::sync::mpsc::UnboundedSender<crate::sync::SyncMessage>>,
) -> Result<Router> {
    // Use provided broadcast channel or create a new one
    let broadcast_tx = message_broadcast.unwrap_or_else(|| {
        let (tx, _) = broadcast::channel::<MessageEvent>(1000);
        tx
    });

    let mut schema_builder = Schema::build(QueryRoot, MutationRoot, SubscriptionRoot)
        .enable_federation() // Enable GraphQL Federation
        .enable_subscription_in_federation() // Enable subscriptions in federation
        .data(storage)
        .data(ipfs)
        .data(broadcast_tx.clone());

    // Add SyncManager if available
    if let Some(sync_mgr) = sync_manager {
        schema_builder = schema_builder.data(sync_mgr);
    }

    // Add Endpoint if available
    if let Some(ep) = endpoint {
        schema_builder = schema_builder.data(ep);
    }

    // Add discovered peers map if available
    if let Some(peers_map) = discovered_peers {
        schema_builder = schema_builder.data(peers_map);
    }

    // Add MQTT components if available
    if let Some(tx) = mqtt_tx {
        schema_builder = schema_builder.data(tx);
    }
    if let Some(tx2) = mqtt_to_gossip_tx {
        schema_builder = schema_builder.data(tx2);
    }
    if let Some(store) = mqtt_store {
        schema_builder = schema_builder.data(store);
    }

    // Add outbound sync sender for broadcasting operations
    if let Some(tx) = sync_outbound {
        schema_builder = schema_builder.data(tx);
    }

    let schema = schema_builder.finish();

    let app = Router::new()
        .route("/", get(graphiql_handler))
        .route("/graphql", get(graphql_handler).post(graphql_handler))
        .route("/ws", get(graphql_subscription_handler))
        .route("/playground", get(graphql_playground))
        .route("/schema.graphql", get(graphql_schema_handler))
        .route("/graphql/schema.graphql", get(graphql_schema_handler))
        .with_state(schema);

    Ok(app)
}

async fn graphql_handler(
    axum::extract::State(schema): axum::extract::State<ApiSchema>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
}

async fn graphql_subscription_handler(
    axum::extract::State(schema): axum::extract::State<ApiSchema>,
    protocol: GraphQLProtocol,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.protocols(ALL_WEBSOCKET_PROTOCOLS)
        .on_upgrade(move |socket| GraphQLWebSocket::new(socket, schema.clone(), protocol).serve())
}

async fn graphql_playground() -> impl axum::response::IntoResponse {
    axum::response::Html(async_graphql::http::playground_source(
        async_graphql::http::GraphQLPlaygroundConfig::new("/graphql").subscription_endpoint("/ws"),
    ))
}

async fn graphiql_handler() -> impl IntoResponse {
    let graphiql = async_graphql::http::GraphiQLSource::build()
        .endpoint("/graphql")
        .subscription_endpoint("/ws")
        .title("Cyberfly Decentralized Database - GraphiQL")
        .finish();

    let instructions = r#"
        <div class=\"cyberfly-graphiql-instructions\">
            <h1>Welcome to Cyberfly Decentralized Database GraphiQL</h1>
            <p>Find every query, mutation, and subscription from the built-in docs explorer:</p>
            <ol>
                <li>Look to the top-right corner and click the <strong>&lt; Docs</strong> button.</li>
                <li>Open <strong>Query</strong> to browse all read operations.</li>
                <li>Open <strong>Mutation</strong> for write operations.</li>
                <li>Open <strong>Subscription</strong> for real-time streams.</li>
                <li>Click any field to view arguments, return types, and example shapes.</li>
                <li>Copy the sample request into the editor (left pane) and press <code>Ctrl</code>/<code>Cmd</code> + <code>Enter</code> to run it.</li>
            </ol>
            <p>Docs pane missing? Press <strong>Ctrl/Cmd + Shift + D</strong> to toggle it.</p>
        </div>
        <style>
            .cyberfly-graphiql-instructions {
                position: fixed;
                top: 16px;
                right: 16px;
                max-width: 360px;
                background: rgba(14, 23, 38, 0.92);
                color: #f5f7fb;
                padding: 18px 22px;
                border-radius: 12px;
                box-shadow: 0 12px 32px rgba(0, 0, 0, 0.35);
                font-family: "Inter", "Segoe UI", sans-serif;
                z-index: 1000;
                line-height: 1.4;
            }
            .cyberfly-graphiql-instructions h1 {
                margin: 0 0 12px;
                font-size: 1.1rem;
                color: #8dd9ff;
            }
            .cyberfly-graphiql-instructions ol {
                margin: 0 0 12px 20px;
                padding: 0;
            }
            .cyberfly-graphiql-instructions li {
                margin-bottom: 6px;
            }
            .cyberfly-graphiql-instructions p {
                margin: 0 0 10px;
            }
            .cyberfly-graphiql-instructions code {
                background: rgba(255, 255, 255, 0.12);
                padding: 1px 6px;
                border-radius: 6px;
                font-size: 0.85em;
            }
            @media (max-width: 900px) {
                .cyberfly-graphiql-instructions {
                    position: static;
                    margin: 16px;
                }
            }
        </style>
    "#;

    let html = graphiql.replacen("</body>", &format!("{instructions}</body>"), 1);
    Html(html)
}

async fn graphql_schema_handler(
    axum::extract::State(schema): axum::extract::State<ApiSchema>,
) -> impl IntoResponse {
    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; charset=utf-8",
        )],
        schema.sdl(),
    )
}
