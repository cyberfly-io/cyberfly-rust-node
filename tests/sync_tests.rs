use cyberfly_rust_node::sync::{SignedOperation, SyncStore, SyncManager, SyncMessage};
use cyberfly_rust_node::storage::RedisStorage;
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use iroh::EndpointId;
use tempfile::TempDir;
use tokio_test;
use serial_test::serial;

fn create_test_operation(signing_key: &SigningKey, db_name: &str, key: &str, value: &str) -> SignedOperation {
    let verifying_key = signing_key.verifying_key();
    let public_key_hex = hex::encode(verifying_key.as_bytes());
    let op_id = uuid::Uuid::new_v4().to_string();
    let timestamp = chrono::Utc::now().timestamp_millis();
    
    let message = format!("{}:{}:{}:{}:{}", op_id, timestamp, db_name, key, value);
    let signature = signing_key.sign(message.as_bytes());
    
    SignedOperation {
        op_id,
        timestamp,
        db_name: db_name.to_string(),
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
    }
}

#[tokio::test]
async fn test_signed_operation_verify_valid() {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    let public_key_hex = hex::encode(verifying_key.as_bytes());
    let db_name = format!("testdb-{}", public_key_hex);
    
    let op = create_test_operation(&signing_key, &db_name, "test_key", "test_value");
    let result = op.verify();
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_signed_operation_verify_invalid_signature() {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    let public_key_hex = hex::encode(verifying_key.as_bytes());
    let db_name = format!("testdb-{}", public_key_hex);
    
    let mut op = create_test_operation(&signing_key, &db_name, "test_key", "test_value");
    // Corrupt the signature
    op.signature = "invalid_signature".to_string();
    
    let result = op.verify();
    assert!(result.is_err());
}

#[tokio::test]
async fn test_signed_operation_verify_wrong_db_name() {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    let public_key_hex = hex::encode(verifying_key.as_bytes());
    
    let op = create_test_operation(&signing_key, "wrong_db_name", "test_key", "test_value");
    let result = op.verify();
    assert!(result.is_err());
}

#[tokio::test]
async fn test_signed_operation_short_format() {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    let public_key_hex = hex::encode(verifying_key.as_bytes());
    let db_name = format!("testdb-{}", public_key_hex);
    
    // Create operation with short format signature (db_name:key:value)
    let op_id = uuid::Uuid::new_v4().to_string();
    let timestamp = chrono::Utc::now().timestamp_millis();
    let key = "test_key";
    let value = "test_value";
    
    let short_message = format!("{}:{}:{}", db_name, key, value);
    let signature = signing_key.sign(short_message.as_bytes());
    
    let op = SignedOperation {
        op_id,
        timestamp,
        db_name: db_name.clone(),
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
    
    let result = op.verify();
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_sync_store_add_operation() {
    let store = SyncStore::new();
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    let public_key_hex = hex::encode(verifying_key.as_bytes());
    let db_name = format!("testdb-{}", public_key_hex);
    
    let op = create_test_operation(&signing_key, &db_name, "test_key", "test_value");
    let op_id = op.op_id.clone();
    
    let result = store.add_operation(op).await;
    assert!(result.is_ok());
    assert!(result.unwrap()); // Should return true for new operation
    
    // Check if operation was added
    let operations = store.get_all_operations().await;
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].op_id, op_id);
}

#[tokio::test]
async fn test_sync_store_duplicate_operation() {
    let store = SyncStore::new();
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    let public_key_hex = hex::encode(verifying_key.as_bytes());
    let db_name = format!("testdb-{}", public_key_hex);
    
    let op = create_test_operation(&signing_key, &db_name, "test_key", "test_value");
    
    // Add operation first time
    let result1 = store.add_operation(op.clone()).await;
    assert!(result1.is_ok());
    assert!(result1.unwrap());
    
    // Add same operation again
    let result2 = store.add_operation(op).await;
    assert!(result2.is_ok());
    assert!(!result2.unwrap()); // Should return false for duplicate
    
    // Should still have only one operation
    let operations = store.get_all_operations().await;
    assert_eq!(operations.len(), 1);
}

#[tokio::test]
async fn test_sync_store_get_operations_for_db() {
    let store = SyncStore::new();
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    let public_key_hex = hex::encode(verifying_key.as_bytes());
    
    let db_name1 = format!("testdb1-{}", public_key_hex);
    let db_name2 = format!("testdb2-{}", public_key_hex);
    
    // Add operations for different databases
    let op1 = create_test_operation(&signing_key, &db_name1, "key1", "value1");
    let op2 = create_test_operation(&signing_key, &db_name2, "key2", "value2");
    let op3 = create_test_operation(&signing_key, &db_name1, "key3", "value3");
    
    store.add_operation(op1).await.unwrap();
    store.add_operation(op2).await.unwrap();
    store.add_operation(op3).await.unwrap();
    
    // Get operations for db1 only
    let db1_ops = store.get_operations_for_db_limited(&db_name1, 10).await;
    assert_eq!(db1_ops.len(), 2);
    
    // Get operations for db2 only
    let db2_ops = store.get_operations_for_db_limited(&db_name2, 10).await;
    assert_eq!(db2_ops.len(), 1);
}

#[tokio::test]
async fn test_sync_store_get_operations_since() {
    let store = SyncStore::new();
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    let public_key_hex = hex::encode(verifying_key.as_bytes());
    let db_name = format!("testdb-{}", public_key_hex);
    
    let base_timestamp = chrono::Utc::now().timestamp_millis();
    
    // Create operations with different timestamps
    let mut op1 = create_test_operation(&signing_key, &db_name, "key1", "value1");
    op1.timestamp = base_timestamp - 1000;
    
    let mut op2 = create_test_operation(&signing_key, &db_name, "key2", "value2");
    op2.timestamp = base_timestamp;
    
    let mut op3 = create_test_operation(&signing_key, &db_name, "key3", "value3");
    op3.timestamp = base_timestamp + 1000;
    
    store.add_operation(op1).await.unwrap();
    store.add_operation(op2).await.unwrap();
    store.add_operation(op3).await.unwrap();
    
    // Get operations since base_timestamp (should get op2 and op3)
    let recent_ops = store.get_operations_since_for_db_limited(base_timestamp, &db_name, 10).await;
    assert_eq!(recent_ops.len(), 2);
}

#[tokio::test]
async fn test_sync_store_operation_count() {
    let store = SyncStore::new();
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    let public_key_hex = hex::encode(verifying_key.as_bytes());
    let db_name = format!("testdb-{}", public_key_hex);
    
    // Initially should be 0
    assert_eq!(store.operation_count().await, 0);
    
    // Add operations
    let op1 = create_test_operation(&signing_key, &db_name, "key1", "value1");
    let op2 = create_test_operation(&signing_key, &db_name, "key2", "value2");
    
    store.add_operation(op1).await.unwrap();
    assert_eq!(store.operation_count().await, 1);
    
    store.add_operation(op2).await.unwrap();
    assert_eq!(store.operation_count().await, 2);
}

#[tokio::test]
async fn test_sync_store_get_operations_count_for_db() {
    let store = SyncStore::new();
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    let public_key_hex = hex::encode(verifying_key.as_bytes());
    
    let db_name1 = format!("testdb1-{}", public_key_hex);
    let db_name2 = format!("testdb2-{}", public_key_hex);
    
    // Add operations for different databases
    let op1 = create_test_operation(&signing_key, &db_name1, "key1", "value1");
    let op2 = create_test_operation(&signing_key, &db_name2, "key2", "value2");
    let op3 = create_test_operation(&signing_key, &db_name1, "key3", "value3");
    
    store.add_operation(op1).await.unwrap();
    store.add_operation(op2).await.unwrap();
    store.add_operation(op3).await.unwrap();
    
    // Check counts for each database
    assert_eq!(store.get_operations_count_for_db(&db_name1).await, 2);
    assert_eq!(store.get_operations_count_for_db(&db_name2).await, 1);
    assert_eq!(store.get_operations_count_for_db("nonexistent").await, 0);
}

#[tokio::test]
async fn test_sync_store_merge_operations() {
    let store = SyncStore::new();
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    let public_key_hex = hex::encode(verifying_key.as_bytes());
    let db_name = format!("testdb-{}", public_key_hex);
    
    let operations = vec![
        create_test_operation(&signing_key, &db_name, "key1", "value1"),
        create_test_operation(&signing_key, &db_name, "key2", "value2"),
        create_test_operation(&signing_key, &db_name, "key3", "value3"),
    ];
    
    let result = store.merge_operations(operations).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 3); // Should merge 3 operations
    
    assert_eq!(store.operation_count().await, 3);
}

#[tokio::test]
async fn test_sync_message_serialization() {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    let public_key_hex = hex::encode(verifying_key.as_bytes());
    let db_name = format!("testdb-{}", public_key_hex);
    
    let operation = create_test_operation(&signing_key, &db_name, "test_key", "test_value");
    
    let sync_request = SyncMessage::SyncRequest {
        requester: "test_requester".to_string(),
        since_timestamp: Some(1234567890),
    };
    
    let sync_response = SyncMessage::SyncResponse {
        requester: "test_requester".to_string(),
        operations: vec![operation.clone()],
        has_more: false,
        continuation_token: None,
    };
    
    let operation_message = SyncMessage::Operation { operation };
    
    // Test serialization/deserialization
    let request_json = serde_json::to_string(&sync_request).unwrap();
    let deserialized_request: SyncMessage = serde_json::from_str(&request_json).unwrap();
    
    let response_json = serde_json::to_string(&sync_response).unwrap();
    let deserialized_response: SyncMessage = serde_json::from_str(&response_json).unwrap();
    
    let op_json = serde_json::to_string(&operation_message).unwrap();
    let deserialized_op: SyncMessage = serde_json::from_str(&op_json).unwrap();
    
    // Verify the messages are correctly deserialized
    match deserialized_request {
        SyncMessage::SyncRequest { requester, since_timestamp } => {
            assert_eq!(requester, "test_requester");
            assert_eq!(since_timestamp, Some(1234567890));
        }
        _ => panic!("Wrong message type"),
    }
    
    match deserialized_response {
        SyncMessage::SyncResponse { operations, .. } => {
            assert_eq!(operations.len(), 1);
        }
        _ => panic!("Wrong message type"),
    }
    
    match deserialized_op {
        SyncMessage::Operation { operation } => {
            assert_eq!(operation.key, "test_key");
        }
        _ => panic!("Wrong message type"),
    }
}