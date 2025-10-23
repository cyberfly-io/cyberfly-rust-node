use cyberfly_rust_node::storage::{RedisStorage, StorageError};
use tempfile::TempDir;
use tokio_test;
use serial_test::serial;
use std::collections::HashMap;

async fn create_test_storage() -> RedisStorage {
    let temp_dir = TempDir::new().unwrap();
    RedisStorage::new(temp_dir.path().to_str().unwrap()).await.unwrap()
}

#[tokio::test]
#[serial]
async fn test_string_operations() {
    let storage = create_test_storage().await;
    let db_name = "test_db";
    
    // Test set and get
    let result = storage.set(db_name, "test_key", "test_value").await;
    assert!(result.is_ok());
    
    let value = storage.get(db_name, "test_key").await.unwrap();
    assert_eq!(value, Some("test_value".to_string()));
    
    // Test get non-existent key
    let value = storage.get(db_name, "non_existent").await.unwrap();
    assert_eq!(value, None);
    
    // Test exists
    let exists = storage.exists(db_name, "test_key").await.unwrap();
    assert!(exists);
    
    let exists = storage.exists(db_name, "non_existent").await.unwrap();
    assert!(!exists);
    
    // Test delete
    let result = storage.del(db_name, "test_key").await;
    assert!(result.is_ok());
    
    let value = storage.get(db_name, "test_key").await.unwrap();
    assert_eq!(value, None);
}

#[tokio::test]
#[serial]
async fn test_hash_operations() {
    let storage = create_test_storage().await;
    let db_name = "test_db";
    
    // Test hset and hget
    let result = storage.hset(db_name, "test_hash", "field1", "value1").await;
    assert!(result.is_ok());
    
    let value = storage.hget(db_name, "test_hash", "field1").await.unwrap();
    assert_eq!(value, Some("value1".to_string()));
    
    // Test hget non-existent field
    let value = storage.hget(db_name, "test_hash", "non_existent").await.unwrap();
    assert_eq!(value, None);
    
    // Test hexists
    let exists = storage.hexists(db_name, "test_hash", "field1").await.unwrap();
    assert!(exists);
    
    let exists = storage.hexists(db_name, "test_hash", "non_existent").await.unwrap();
    assert!(!exists);
    
    // Test hgetall
    storage.hset(db_name, "test_hash", "field2", "value2").await.unwrap();
    let all_fields = storage.hgetall(db_name, "test_hash").await.unwrap();
    assert_eq!(all_fields.len(), 2);
    assert_eq!(all_fields.get("field1"), Some(&"value1".to_string()));
    assert_eq!(all_fields.get("field2"), Some(&"value2".to_string()));
    
    // Test hdel
    let result = storage.hdel(db_name, "test_hash", "field1").await;
    assert!(result.is_ok());
    
    let value = storage.hget(db_name, "test_hash", "field1").await.unwrap();
    assert_eq!(value, None);
    
    let exists = storage.hexists(db_name, "test_hash", "field1").await.unwrap();
    assert!(!exists);
}

#[tokio::test]
#[serial]
async fn test_list_operations() {
    let storage = create_test_storage().await;
    let db_name = "test_db";
    
    // Test lpush and lrange
    let result = storage.lpush(db_name, "test_list", "value1").await;
    assert!(result.is_ok());
    
    let result = storage.lpush(db_name, "test_list", "value2").await;
    assert!(result.is_ok());
    
    let values = storage.lrange(db_name, "test_list", 0, -1).await.unwrap();
    assert_eq!(values.len(), 2);
    assert_eq!(values[0], "value2"); // Last pushed is first
    assert_eq!(values[1], "value1");
    
    // Test rpush
    let result = storage.rpush(db_name, "test_list", "value3").await;
    assert!(result.is_ok());
    
    let values = storage.lrange(db_name, "test_list", 0, -1).await.unwrap();
    assert_eq!(values.len(), 3);
    assert_eq!(values[2], "value3"); // Right pushed is last
    
    // Test llen
    let length = storage.llen(db_name, "test_list").await.unwrap();
    assert_eq!(length, 3);
    
    // Test lpop
    let popped = storage.lpop(db_name, "test_list").await.unwrap();
    assert_eq!(popped, Some("value2".to_string()));
    
    let length = storage.llen(db_name, "test_list").await.unwrap();
    assert_eq!(length, 2);
    
    // Test rpop
    let popped = storage.rpop(db_name, "test_list").await.unwrap();
    assert_eq!(popped, Some("value3".to_string()));
    
    let length = storage.llen(db_name, "test_list").await.unwrap();
    assert_eq!(length, 1);
}

#[tokio::test]
#[serial]
async fn test_set_operations() {
    let storage = create_test_storage().await;
    let db_name = "test_db";
    
    // Test sadd and smembers
    let result = storage.sadd(db_name, "test_set", "member1").await;
    assert!(result.is_ok());
    
    let result = storage.sadd(db_name, "test_set", "member2").await;
    assert!(result.is_ok());
    
    let members = storage.smembers(db_name, "test_set").await.unwrap();
    assert_eq!(members.len(), 2);
    assert!(members.contains(&"member1".to_string()));
    assert!(members.contains(&"member2".to_string()));
    
    // Test sismember
    let is_member = storage.sismember(db_name, "test_set", "member1").await.unwrap();
    assert!(is_member);
    
    let is_member = storage.sismember(db_name, "test_set", "non_member").await.unwrap();
    assert!(!is_member);
    
    // Test scard
    let count = storage.scard(db_name, "test_set").await.unwrap();
    assert_eq!(count, 2);
    
    // Test srem
    let result = storage.srem(db_name, "test_set", "member1").await;
    assert!(result.is_ok());
    
    let is_member = storage.sismember(db_name, "test_set", "member1").await.unwrap();
    assert!(!is_member);
    
    let count = storage.scard(db_name, "test_set").await.unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
#[serial]
async fn test_sorted_set_operations() {
    let storage = create_test_storage().await;
    let db_name = "test_db";
    
    // Test zadd and zrange
    let result = storage.zadd(db_name, "test_zset", 1.0, "member1").await;
    assert!(result.is_ok());
    
    let result = storage.zadd(db_name, "test_zset", 2.0, "member2").await;
    assert!(result.is_ok());
    
    let result = storage.zadd(db_name, "test_zset", 1.5, "member3").await;
    assert!(result.is_ok());
    
    let members = storage.zrange(db_name, "test_zset", 0, -1).await.unwrap();
    assert_eq!(members.len(), 3);
    assert_eq!(members[0], "member1"); // Lowest score first
    assert_eq!(members[1], "member3");
    assert_eq!(members[2], "member2"); // Highest score last
    
    // Test zscore
    let score = storage.zscore(db_name, "test_zset", "member2").await.unwrap();
    assert_eq!(score, Some(2.0));
    
    let score = storage.zscore(db_name, "test_zset", "non_member").await.unwrap();
    assert_eq!(score, None);
    
    // Test zcard
    let count = storage.zcard(db_name, "test_zset").await.unwrap();
    assert_eq!(count, 3);
    
    // Test zrem
    let result = storage.zrem(db_name, "test_zset", "member2").await;
    assert!(result.is_ok());
    
    let count = storage.zcard(db_name, "test_zset").await.unwrap();
    assert_eq!(count, 2);
    
    let score = storage.zscore(db_name, "test_zset", "member2").await.unwrap();
    assert_eq!(score, None);
}

#[tokio::test]
#[serial]
async fn test_json_operations() {
    let storage = create_test_storage().await;
    let db_name = "test_db";
    
    let json_value = r#"{"name": "test", "age": 30, "active": true}"#;
    
    // Test json_set and json_get
    let result = storage.json_set(db_name, "test_json", ".", json_value).await;
    assert!(result.is_ok());
    
    let value = storage.json_get(db_name, "test_json", ".").await.unwrap();
    assert!(value.is_some());
    
    // Test json_get with path
    let name = storage.json_get(db_name, "test_json", ".name").await.unwrap();
    assert_eq!(name, Some("\"test\"".to_string()));
    
    let age = storage.json_get(db_name, "test_json", ".age").await.unwrap();
    assert_eq!(age, Some("30".to_string()));
    
    // Test json_del
    let result = storage.json_del(db_name, "test_json", ".age").await;
    assert!(result.is_ok());
    
    let age = storage.json_get(db_name, "test_json", ".age").await.unwrap();
    assert_eq!(age, None);
    
    // Test json_type
    let json_type = storage.json_type(db_name, "test_json", ".name").await.unwrap();
    assert_eq!(json_type, Some("string".to_string()));
    
    let json_type = storage.json_type(db_name, "test_json", ".active").await.unwrap();
    assert_eq!(json_type, Some("boolean".to_string()));
}

#[tokio::test]
#[serial]
async fn test_time_series_operations() {
    let storage = create_test_storage().await;
    let db_name = "test_db";
    
    let timestamp1 = chrono::Utc::now().timestamp_millis() as u64;
    let timestamp2 = timestamp1 + 1000;
    let timestamp3 = timestamp2 + 1000;
    
    // Test ts_add
    let result = storage.ts_add(db_name, "test_ts", timestamp1, 10.5).await;
    assert!(result.is_ok());
    
    let result = storage.ts_add(db_name, "test_ts", timestamp2, 20.5).await;
    assert!(result.is_ok());
    
    let result = storage.ts_add(db_name, "test_ts", timestamp3, 30.5).await;
    assert!(result.is_ok());
    
    // Test ts_range
    let values = storage.ts_range(db_name, "test_ts", timestamp1, timestamp3).await.unwrap();
    assert_eq!(values.len(), 3);
    assert_eq!(values[0].0, timestamp1);
    assert_eq!(values[0].1, 10.5);
    assert_eq!(values[2].0, timestamp3);
    assert_eq!(values[2].1, 30.5);
    
    // Test ts_get
    let latest = storage.ts_get(db_name, "test_ts").await.unwrap();
    assert!(latest.is_some());
    let (ts, value) = latest.unwrap();
    assert_eq!(ts, timestamp3);
    assert_eq!(value, 30.5);
}

#[tokio::test]
#[serial]
async fn test_geospatial_operations() {
    let storage = create_test_storage().await;
    let db_name = "test_db";
    
    // Test geoadd
    let result = storage.geoadd(db_name, "test_geo", 13.361389, 38.115556, "Palermo").await;
    assert!(result.is_ok());
    
    let result = storage.geoadd(db_name, "test_geo", 15.087269, 37.502669, "Catania").await;
    assert!(result.is_ok());
    
    // Test geopos
    let positions = storage.geopos(db_name, "test_geo", &["Palermo", "Catania"]).await.unwrap();
    assert_eq!(positions.len(), 2);
    assert!(positions[0].is_some());
    assert!(positions[1].is_some());
    
    let palermo_pos = positions[0].as_ref().unwrap();
    assert!((palermo_pos.0 - 13.361389).abs() < 0.001);
    assert!((palermo_pos.1 - 38.115556).abs() < 0.001);
    
    // Test geodist
    let distance = storage.geodist(db_name, "test_geo", "Palermo", "Catania", "km").await.unwrap();
    assert!(distance.is_some());
    let dist = distance.unwrap();
    assert!(dist > 150.0 && dist < 200.0); // Approximate distance between cities
}

#[tokio::test]
#[serial]
async fn test_key_operations() {
    let storage = create_test_storage().await;
    let db_name = "test_db";
    
    // Set up test data
    storage.set(db_name, "key1", "value1").await.unwrap();
    storage.set(db_name, "key2", "value2").await.unwrap();
    storage.set(db_name, "test_key", "test_value").await.unwrap();
    
    // Test keys with pattern
    let keys = storage.keys(db_name, "key*").await.unwrap();
    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&"key1".to_string()));
    assert!(keys.contains(&"key2".to_string()));
    
    let keys = storage.keys(db_name, "test_*").await.unwrap();
    assert_eq!(keys.len(), 1);
    assert!(keys.contains(&"test_key".to_string()));
    
    // Test scan
    let (cursor, keys) = storage.scan(db_name, 0, Some("key*"), Some(10)).await.unwrap();
    assert!(keys.len() <= 2);
    
    // Test type
    let key_type = storage.key_type(db_name, "key1").await.unwrap();
    assert_eq!(key_type, Some("string".to_string()));
    
    // Test ttl (should be -1 for keys without expiration)
    let ttl = storage.ttl(db_name, "key1").await.unwrap();
    assert_eq!(ttl, -1);
    
    // Test expire and ttl
    let result = storage.expire(db_name, "key1", 60).await;
    assert!(result.is_ok());
    
    let ttl = storage.ttl(db_name, "key1").await.unwrap();
    assert!(ttl > 0 && ttl <= 60);
}

#[tokio::test]
#[serial]
async fn test_database_operations() {
    let storage = create_test_storage().await;
    let db_name = "test_db";
    
    // Add some data
    storage.set(db_name, "key1", "value1").await.unwrap();
    storage.set(db_name, "key2", "value2").await.unwrap();
    
    // Test dbsize
    let size = storage.dbsize(db_name).await.unwrap();
    assert_eq!(size, 2);
    
    // Test flushdb
    let result = storage.flushdb(db_name).await;
    assert!(result.is_ok());
    
    let size = storage.dbsize(db_name).await.unwrap();
    assert_eq!(size, 0);
    
    let value = storage.get(db_name, "key1").await.unwrap();
    assert_eq!(value, None);
}

#[tokio::test]
#[serial]
async fn test_error_handling() {
    let storage = create_test_storage().await;
    let db_name = "test_db";
    
    // Test operations on non-existent keys
    let value = storage.get(db_name, "non_existent").await.unwrap();
    assert_eq!(value, None);
    
    let exists = storage.exists(db_name, "non_existent").await.unwrap();
    assert!(!exists);
    
    let length = storage.llen(db_name, "non_existent_list").await.unwrap();
    assert_eq!(length, 0);
    
    let count = storage.scard(db_name, "non_existent_set").await.unwrap();
    assert_eq!(count, 0);
    
    let count = storage.zcard(db_name, "non_existent_zset").await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
#[serial]
async fn test_concurrent_operations() {
    let storage = create_test_storage().await;
    let db_name = "test_db";
    
    // Test concurrent writes
    let handles: Vec<_> = (0..10).map(|i| {
        let storage = storage.clone();
        let db_name = db_name.to_string();
        tokio::spawn(async move {
            let key = format!("concurrent_key_{}", i);
            let value = format!("concurrent_value_{}", i);
            storage.set(&db_name, &key, &value).await
        })
    }).collect();
    
    // Wait for all operations to complete
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }
    
    // Verify all keys were set
    for i in 0..10 {
        let key = format!("concurrent_key_{}", i);
        let expected_value = format!("concurrent_value_{}", i);
        let value = storage.get(db_name, &key).await.unwrap();
        assert_eq!(value, Some(expected_value));
    }
}

#[tokio::test]
#[serial]
async fn test_large_data_operations() {
    let storage = create_test_storage().await;
    let db_name = "test_db";
    
    // Test large string value
    let large_value = "x".repeat(1024 * 1024); // 1MB string
    let result = storage.set(db_name, "large_key", &large_value).await;
    assert!(result.is_ok());
    
    let retrieved_value = storage.get(db_name, "large_key").await.unwrap();
    assert_eq!(retrieved_value, Some(large_value));
    
    // Test many small keys
    for i in 0..1000 {
        let key = format!("small_key_{}", i);
        let value = format!("small_value_{}", i);
        let result = storage.set(db_name, &key, &value).await;
        assert!(result.is_ok());
    }
    
    let size = storage.dbsize(db_name).await.unwrap();
    assert_eq!(size, 1001); // 1000 small keys + 1 large key
}