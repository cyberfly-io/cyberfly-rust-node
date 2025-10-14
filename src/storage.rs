use redis::{AsyncCommands, Client, streams::StreamReadReply};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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

#[derive(Clone)]
pub struct RedisStorage {
    client: Arc<Client>,
}

impl RedisStorage {
    pub async fn new(redis_url: &str) -> Result<Self> {
        let client = Client::open(redis_url)?;
        
        // Test connection
        let mut conn = client.get_multiplexed_async_connection().await?;
        redis::cmd("PING").query_async::<String>(&mut conn).await?;
        
        tracing::info!("Connected to Redis at {}", redis_url);
        
        Ok(Self {
            client: Arc::new(client),
        })
    }

    /// Store a string value
    pub async fn set_string(&self, key: &str, value: &str) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        conn.set::<_, _, ()>(key, value).await?;
        Ok(())
    }

    /// Get a string value
    pub async fn get_string(&self, key: &str) -> Result<Option<String>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: Option<String> = conn.get(key).await?;
        Ok(result)
    }

    /// Store a hash field
    pub async fn set_hash(&self, key: &str, field: &str, value: &str) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        conn.hset::<_, _, _, ()>(key, field, value).await?;
        Ok(())
    }

    /// Get a hash field
    pub async fn get_hash(&self, key: &str, field: &str) -> Result<Option<String>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: Option<String> = conn.hget(key, field).await?;
        Ok(result)
    }

    /// Get all hash fields
    pub async fn get_all_hash(&self, key: &str) -> Result<Vec<(String, String)>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: Vec<(String, String)> = conn.hgetall(key).await?;
        Ok(result)
    }

    /// Push to list
    pub async fn push_list(&self, key: &str, value: &str) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        conn.rpush::<_, _, ()>(key, value).await?;
        Ok(())
    }

    /// Get list range
    pub async fn get_list(&self, key: &str, start: isize, stop: isize) -> Result<Vec<String>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: Vec<String> = conn.lrange(key, start, stop).await?;
        Ok(result)
    }

    /// Add to set
    pub async fn add_set(&self, key: &str, member: &str) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        conn.sadd::<_, _, ()>(key, member).await?;
        Ok(())
    }

    /// Get set members
    pub async fn get_set(&self, key: &str) -> Result<Vec<String>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: Vec<String> = conn.smembers(key).await?;
        Ok(result)
    }

    /// Add to sorted set
    pub async fn add_sorted_set(&self, key: &str, score: f64, member: &str) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        conn.zadd::<_, _, _, ()>(key, member, score).await?;
        Ok(())
    }

    /// Get sorted set range
    pub async fn get_sorted_set(&self, key: &str, start: isize, stop: isize) -> Result<Vec<String>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: Vec<String> = conn.zrange(key, start, stop).await?;
        Ok(result)
    }

    /// Get sorted set range with scores
    pub async fn get_sorted_set_with_scores(&self, key: &str, start: isize, stop: isize) -> Result<Vec<(String, f64)>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: Vec<(String, f64)> = conn.zrange_withscores(key, start, stop).await?;
        Ok(result)
    }

    /// Check if key exists
    pub async fn exists(&self, key: &str) -> Result<bool> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: bool = conn.exists(key).await?;
        Ok(result)
    }

    /// Delete key
    pub async fn delete(&self, key: &str) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        conn.del::<_, ()>(key).await?;
        Ok(())
    }

    // ============ JSON Operations ============
    
    /// Set JSON document
    pub async fn set_json(&self, key: &str, path: &str, value: &str) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        redis::cmd("JSON.SET")
            .arg(key)
            .arg(path)
            .arg(value)
            .query_async::<()>(&mut conn)
            .await?;
        Ok(())
    }

    /// Get JSON document or path
    pub async fn get_json(&self, key: &str, path: Option<&str>) -> Result<Option<String>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: Option<String> = redis::cmd("JSON.GET")
            .arg(key)
            .arg(path.unwrap_or("$"))
            .query_async(&mut conn)
            .await?;
        Ok(result)
    }

    /// Filter JSON documents by path (uses JSON.GET with JSONPath)
    pub async fn filter_json(&self, key: &str, json_path: &str) -> Result<Option<String>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: Option<String> = redis::cmd("JSON.GET")
            .arg(key)
            .arg(json_path)
            .query_async(&mut conn)
            .await?;
        Ok(result)
    }

    /// Get JSON object type
    pub async fn json_type(&self, key: &str, path: Option<&str>) -> Result<Option<String>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: Option<String> = redis::cmd("JSON.TYPE")
            .arg(key)
            .arg(path.unwrap_or("$"))
            .query_async(&mut conn)
            .await?;
        Ok(result)
    }

    // ============ Stream Operations ============
    
    /// Add entry to stream
    pub async fn xadd(&self, key: &str, id: &str, items: &[(&str, &str)]) -> Result<String> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let mut cmd = redis::cmd("XADD");
        cmd.arg(key).arg(id);
        for (field, value) in items {
            cmd.arg(field).arg(value);
        }
        let result: String = cmd.query_async(&mut conn).await?;
        Ok(result)
    }

    /// Read from stream
    pub async fn xread(&self, keys: &[&str], ids: &[&str], count: Option<usize>) -> Result<StreamReadReply> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let mut cmd = redis::cmd("XREAD");
        if let Some(c) = count {
            cmd.arg("COUNT").arg(c);
        }
        cmd.arg("STREAMS");
        for key in keys {
            cmd.arg(key);
        }
        for id in ids {
            cmd.arg(id);
        }
        let result: StreamReadReply = cmd.query_async(&mut conn).await?;
        Ok(result)
    }

    /// Get stream range
    pub async fn xrange(&self, key: &str, start: &str, end: &str, count: Option<usize>) -> Result<Vec<(String, Vec<(String, String)>)>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let mut cmd = redis::cmd("XRANGE");
        cmd.arg(key).arg(start).arg(end);
        if let Some(c) = count {
            cmd.arg("COUNT").arg(c);
        }
        
        // Use redis streams feature types
        use redis::streams::StreamRangeReply;
        let result: StreamRangeReply = cmd.query_async(&mut conn).await?;
        
        let mut entries = Vec::new();
        for stream_id in result.ids {
            let mut field_pairs = Vec::new();
            for (k, v) in stream_id.map {
                if let redis::Value::BulkString(bytes) = v {
                    field_pairs.push((k, String::from_utf8_lossy(&bytes).to_string()));
                }
            }
            entries.push((stream_id.id, field_pairs));
        }
        
        Ok(entries)
    }

    /// Get stream length
    pub async fn xlen(&self, key: &str) -> Result<usize> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: usize = redis::cmd("XLEN")
            .arg(key)
            .query_async(&mut conn)
            .await?;
        Ok(result)
    }

    /// Filter stream entries by ID range
    pub async fn filter_stream(&self, key: &str, start: &str, end: &str, pattern: Option<&str>) -> Result<Vec<(String, Vec<(String, String)>)>> {
        let entries = self.xrange(key, start, end, None).await?;
        
        if let Some(pat) = pattern {
            // Filter entries where any field value contains the pattern
            Ok(entries.into_iter()
                .filter(|(_, fields)| {
                    fields.iter().any(|(_, v)| v.contains(pat))
                })
                .collect())
        } else {
            Ok(entries)
        }
    }

    // ============ TimeSeries Operations ============
    
    /// Add sample to time series (using Sorted Set as fallback if RedisTimeSeries not available)
    pub async fn ts_add(&self, key: &str, timestamp: i64, value: f64) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        // Try RedisTimeSeries first, fallback to sorted set
        let ts_result: Result<(), redis::RedisError> = redis::cmd("TS.ADD")
            .arg(key)
            .arg(timestamp)
            .arg(value)
            .query_async(&mut conn)
            .await;
        
        if ts_result.is_err() {
            // Fallback: use sorted set with timestamp as score
            let value_str = format!("{},{}", timestamp, value);
            conn.zadd::<_, _, _, ()>(key, value_str, timestamp as f64).await?;
        }
        Ok(())
    }

    /// Get time series range
    pub async fn ts_range(&self, key: &str, from_ts: i64, to_ts: i64) -> Result<Vec<(i64, f64)>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        
        // Try RedisTimeSeries first
        let ts_result: Result<Vec<(i64, f64)>, redis::RedisError> = redis::cmd("TS.RANGE")
            .arg(key)
            .arg(from_ts)
            .arg(to_ts)
            .query_async(&mut conn)
            .await;
        
        if let Ok(result) = ts_result {
            return Ok(result);
        }
        
        // Fallback: use sorted set range by score
        let values: Vec<String> = conn.zrangebyscore(key, from_ts as f64, to_ts as f64).await?;
        let mut result = Vec::new();
        
        for val in values {
            if let Some((ts_str, val_str)) = val.split_once(',') {
                if let (Ok(ts), Ok(v)) = (ts_str.parse::<i64>(), val_str.parse::<f64>()) {
                    result.push((ts, v));
                }
            }
        }
        
        Ok(result)
    }

    /// Get latest time series value
    pub async fn ts_get(&self, key: &str) -> Result<Option<(i64, f64)>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        
        // Try RedisTimeSeries first
        let ts_result: Result<Option<(i64, f64)>, redis::RedisError> = redis::cmd("TS.GET")
            .arg(key)
            .query_async(&mut conn)
            .await;
        
        if let Ok(result) = ts_result {
            return Ok(result);
        }
        
        // Fallback: get highest score from sorted set
        let values: Vec<String> = conn.zrevrange(key, 0, 0).await?;
        if let Some(val) = values.first() {
            if let Some((ts_str, val_str)) = val.split_once(',') {
                if let (Ok(ts), Ok(v)) = (ts_str.parse::<i64>(), val_str.parse::<f64>()) {
                    return Ok(Some((ts, v)));
                }
            }
        }
        
        Ok(None)
    }

    /// Filter time series by value range
    pub async fn filter_timeseries(&self, key: &str, from_ts: i64, to_ts: i64, min_value: Option<f64>, max_value: Option<f64>) -> Result<Vec<(i64, f64)>> {
        let mut data = self.ts_range(key, from_ts, to_ts).await?;
        
        if let Some(min) = min_value {
            data.retain(|(_, v)| *v >= min);
        }
        if let Some(max) = max_value {
            data.retain(|(_, v)| *v <= max);
        }
        
        Ok(data)
    }

    // ============ Geospatial Operations ============
    
    /// Add geospatial location
    pub async fn geoadd(&self, key: &str, longitude: f64, latitude: f64, member: &str) -> Result<usize> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: usize = redis::cmd("GEOADD")
            .arg(key)
            .arg(longitude)
            .arg(latitude)
            .arg(member)
            .query_async(&mut conn)
            .await?;
        Ok(result)
    }

    /// Search by radius (returns members within radius from point)
    pub async fn georadius(&self, key: &str, longitude: f64, latitude: f64, radius: f64, unit: &str) -> Result<Vec<String>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: Vec<String> = redis::cmd("GEORADIUS")
            .arg(key)
            .arg(longitude)
            .arg(latitude)
            .arg(radius)
            .arg(unit)
            .query_async(&mut conn)
            .await?;
        Ok(result)
    }

    /// Search by radius from member
    pub async fn georadiusbymember(&self, key: &str, member: &str, radius: f64, unit: &str) -> Result<Vec<String>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: Vec<String> = redis::cmd("GEORADIUSBYMEMBER")
            .arg(key)
            .arg(member)
            .arg(radius)
            .arg(unit)
            .query_async(&mut conn)
            .await?;
        Ok(result)
    }

    /// Get position of member
    pub async fn geopos(&self, key: &str, member: &str) -> Result<Option<(f64, f64)>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: Vec<Option<(f64, f64)>> = redis::cmd("GEOPOS")
            .arg(key)
            .arg(member)
            .query_async(&mut conn)
            .await?;
        Ok(result.into_iter().next().flatten())
    }

    /// Calculate distance between two members
    pub async fn geodist(&self, key: &str, member1: &str, member2: &str, unit: Option<&str>) -> Result<Option<f64>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let mut cmd = redis::cmd("GEODIST");
        cmd.arg(key).arg(member1).arg(member2);
        if let Some(u) = unit {
            cmd.arg(u);
        }
        let result: Option<f64> = cmd.query_async(&mut conn).await?;
        Ok(result)
    }

    /// Filter geospatial data by distance from point
    pub async fn filter_geo_by_radius(&self, key: &str, longitude: f64, latitude: f64, max_radius: f64, unit: &str) -> Result<Vec<String>> {
        self.georadius(key, longitude, latitude, max_radius, unit).await
    }

    /// Search by radius with coordinates
    pub async fn georadius_with_coords(&self, key: &str, longitude: f64, latitude: f64, radius: f64, unit: &str) -> Result<Vec<(String, f64, f64)>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let raw_result: Vec<Vec<redis::Value>> = redis::cmd("GEORADIUS")
            .arg(key)
            .arg(longitude)
            .arg(latitude)
            .arg(radius)
            .arg(unit)
            .arg("WITHCOORD")
            .query_async(&mut conn)
            .await?;
        
        let mut results = Vec::new();
        for item in raw_result {
            if let Some(member) = item.get(0).and_then(|v| redis::from_redis_value::<String>(v).ok()) {
                if let Some(coords_arr) = item.get(1) {
                    if let Ok(coords) = redis::from_redis_value::<Vec<f64>>(coords_arr) {
                        if coords.len() >= 2 {
                            results.push((member, coords[0], coords[1]));
                        }
                    }
                }
            }
        }
        Ok(results)
    }

    /// Search by radius from member with coordinates
    pub async fn georadiusbymember_with_coords(&self, key: &str, member: &str, radius: f64, unit: &str) -> Result<Vec<(String, f64, f64)>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let raw_result: Vec<Vec<redis::Value>> = redis::cmd("GEORADIUSBYMEMBER")
            .arg(key)
            .arg(member)
            .arg(radius)
            .arg(unit)
            .arg("WITHCOORD")
            .query_async(&mut conn)
            .await?;
        
        let mut results = Vec::new();
        for item in raw_result {
            if let Some(member_name) = item.get(0).and_then(|v| redis::from_redis_value::<String>(v).ok()) {
                if let Some(coords_arr) = item.get(1) {
                    if let Ok(coords) = redis::from_redis_value::<Vec<f64>>(coords_arr) {
                        if coords.len() >= 2 {
                            results.push((member_name, coords[0], coords[1]));
                        }
                    }
                }
            }
        }
        Ok(results)
    }

    // ============ General Filter Operations ============
    
    /// Filter hash by field pattern
    pub async fn filter_hash(&self, key: &str, field_pattern: &str) -> Result<Vec<(String, String)>> {
        let all_fields = self.get_all_hash(key).await?;
        Ok(all_fields.into_iter()
            .filter(|(field, _)| field.contains(field_pattern))
            .collect())
    }

    /// Filter list by value pattern
    pub async fn filter_list(&self, key: &str, value_pattern: &str) -> Result<Vec<String>> {
        let all_values = self.get_list(key, 0, -1).await?;
        Ok(all_values.into_iter()
            .filter(|v| v.contains(value_pattern))
            .collect())
    }

    /// Filter set by member pattern
    pub async fn filter_set(&self, key: &str, member_pattern: &str) -> Result<Vec<String>> {
        let all_members = self.get_set(key).await?;
        Ok(all_members.into_iter()
            .filter(|m| m.contains(member_pattern))
            .collect())
    }

    /// Filter sorted set by score range
    pub async fn filter_sorted_set(&self, key: &str, min_score: f64, max_score: f64) -> Result<Vec<String>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: Vec<String> = conn.zrangebyscore(key, min_score, max_score).await?;
        Ok(result)
    }

    /// Filter sorted set by score range with scores
    pub async fn filter_sorted_set_with_scores(&self, key: &str, min_score: f64, max_score: f64) -> Result<Vec<(String, f64)>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let result: Vec<(String, f64)> = conn.zrangebyscore_withscores(key, min_score, max_score).await?;
        Ok(result)
    }
}
