use cyberfly_rust_node::graphql::{create_schema, Context};
use cyberfly_rust_node::storage::RedisStorage;
use cyberfly_rust_node::sync::SyncStore;
use cyberfly_rust_node::crdt::CrdtStore;
use juniper::{execute, Variables, Value};
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use tempfile::TempDir;
use tokio_test;
use serial_test::serial;
use std::sync::Arc;

async fn create_test_context() -> Context {
    let temp_dir = TempDir::new().unwrap();
    let storage = RedisStorage::new(temp_dir.path().to_str().unwrap()).await.unwrap();
    let sync_store = SyncStore::new();
    let crdt_store = CrdtStore::new();
    
    Context {
        storage: Arc::new(storage),
        sync_store: Arc::new(sync_store),
        crdt_store: Arc::new(crdt_store),
    }
}

fn create_test_keypair() -> (SigningKey, String) {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    let public_key_hex = hex::encode(verifying_key.as_bytes());
    (signing_key, public_key_hex)
}

#[tokio::test]
async fn test_submit_data_valid() {
    let context = create_test_context().await;
    let schema = create_schema();
    let (signing_key, public_key_hex) = create_test_keypair();
    let db_name = format!("testdb-{}", public_key_hex);
    
    // Create a valid signature
    let message = format!("{}:test_key:test_value", db_name);
    let signature = signing_key.sign(message.as_bytes());
    let signature_hex = hex::encode(signature.to_bytes());
    
    let query = r#"
        mutation SubmitData($input: DataInput!) {
            submitData(input: $input) {
                success
                message
                operationId
            }
        }
    "#;
    
    let variables = Variables::from_json(&serde_json::json!({
        "input": {
            "dbName": db_name,
            "key": "test_key",
            "value": "test_value",
            "storeType": "String",
            "publicKey": public_key_hex,
            "signature": signature_hex
        }
    })).unwrap();
    
    let result = execute(query, None, &schema, &variables, &context).await;
    assert!(result.is_ok());
    
    let (value, errors) = result.unwrap();
    assert!(errors.is_empty());
    
    if let Value::Object(obj) = value {
        if let Some(Value::Object(submit_data)) = obj.get("submitData") {
            if let Some(Value::Scalar(success)) = submit_data.get("success") {
                assert_eq!(success.as_boolean(), Some(true));
            }
        }
    }
}

#[tokio::test]
async fn test_submit_data_invalid_signature() {
    let context = create_test_context().await;
    let schema = create_schema();
    let (_, public_key_hex) = create_test_keypair();
    let db_name = format!("testdb-{}", public_key_hex);
    
    let query = r#"
        mutation SubmitData($input: DataInput!) {
            submitData(input: $input) {
                success
                message
                operationId
            }
        }
    "#;
    
    let variables = Variables::from_json(&serde_json::json!({
        "input": {
            "dbName": db_name,
            "key": "test_key",
            "value": "test_value",
            "storeType": "String",
            "publicKey": public_key_hex,
            "signature": "invalid_signature"
        }
    })).unwrap();
    
    let result = execute(query, None, &schema, &variables, &context).await;
    assert!(result.is_ok());
    
    let (value, errors) = result.unwrap();
    assert!(errors.is_empty());
    
    if let Value::Object(obj) = value {
        if let Some(Value::Object(submit_data)) = obj.get("submitData") {
            if let Some(Value::Scalar(success)) = submit_data.get("success") {
                assert_eq!(success.as_boolean(), Some(false));
            }
        }
    }
}

#[tokio::test]
async fn test_submit_data_wrong_db_name() {
    let context = create_test_context().await;
    let schema = create_schema();
    let (signing_key, public_key_hex) = create_test_keypair();
    
    // Create signature for wrong db name
    let message = "wrong_db:test_key:test_value";
    let signature = signing_key.sign(message.as_bytes());
    let signature_hex = hex::encode(signature.to_bytes());
    
    let query = r#"
        mutation SubmitData($input: DataInput!) {
            submitData(input: $input) {
                success
                message
                operationId
            }
        }
    "#;
    
    let variables = Variables::from_json(&serde_json::json!({
        "input": {
            "dbName": "wrong_db",
            "key": "test_key",
            "value": "test_value",
            "storeType": "String",
            "publicKey": public_key_hex,
            "signature": signature_hex
        }
    })).unwrap();
    
    let result = execute(query, None, &schema, &variables, &context).await;
    assert!(result.is_ok());
    
    let (value, errors) = result.unwrap();
    assert!(errors.is_empty());
    
    if let Value::Object(obj) = value {
        if let Some(Value::Object(submit_data)) = obj.get("submitData") {
            if let Some(Value::Scalar(success)) = submit_data.get("success") {
                assert_eq!(success.as_boolean(), Some(false));
            }
        }
    }
}

#[tokio::test]
async fn test_get_data_existing() {
    let context = create_test_context().await;
    let schema = create_schema();
    let (signing_key, public_key_hex) = create_test_keypair();
    let db_name = format!("testdb-{}", public_key_hex);
    
    // First submit data
    let message = format!("{}:test_key:test_value", db_name);
    let signature = signing_key.sign(message.as_bytes());
    let signature_hex = hex::encode(signature.to_bytes());
    
    let submit_query = r#"
        mutation SubmitData($input: DataInput!) {
            submitData(input: $input) {
                success
            }
        }
    "#;
    
    let submit_variables = Variables::from_json(&serde_json::json!({
        "input": {
            "dbName": db_name,
            "key": "test_key",
            "value": "test_value",
            "storeType": "String",
            "publicKey": public_key_hex,
            "signature": signature_hex
        }
    })).unwrap();
    
    execute(submit_query, None, &schema, &submit_variables, &context).await.unwrap();
    
    // Now get the data
    let get_query = r#"
        query GetData($dbName: String!, $key: String!) {
            getData(dbName: $dbName, key: $key) {
                value
                exists
            }
        }
    "#;
    
    let get_variables = Variables::from_json(&serde_json::json!({
        "dbName": db_name,
        "key": "test_key"
    })).unwrap();
    
    let result = execute(get_query, None, &schema, &get_variables, &context).await;
    assert!(result.is_ok());
    
    let (value, errors) = result.unwrap();
    assert!(errors.is_empty());
    
    if let Value::Object(obj) = value {
        if let Some(Value::Object(get_data)) = obj.get("getData") {
            if let Some(Value::Scalar(exists)) = get_data.get("exists") {
                assert_eq!(exists.as_boolean(), Some(true));
            }
            if let Some(Value::Scalar(value)) = get_data.get("value") {
                assert_eq!(value.as_string(), Some("test_value"));
            }
        }
    }
}

#[tokio::test]
async fn test_get_data_nonexistent() {
    let context = create_test_context().await;
    let schema = create_schema();
    let (_, public_key_hex) = create_test_keypair();
    let db_name = format!("testdb-{}", public_key_hex);
    
    let query = r#"
        query GetData($dbName: String!, $key: String!) {
            getData(dbName: $dbName, key: $key) {
                value
                exists
            }
        }
    "#;
    
    let variables = Variables::from_json(&serde_json::json!({
        "dbName": db_name,
        "key": "nonexistent_key"
    })).unwrap();
    
    let result = execute(query, None, &schema, &variables, &context).await;
    assert!(result.is_ok());
    
    let (value, errors) = result.unwrap();
    assert!(errors.is_empty());
    
    if let Value::Object(obj) = value {
        if let Some(Value::Object(get_data)) = obj.get("getData") {
            if let Some(Value::Scalar(exists)) = get_data.get("exists") {
                assert_eq!(exists.as_boolean(), Some(false));
            }
        }
    }
}

#[tokio::test]
async fn test_list_keys() {
    let context = create_test_context().await;
    let schema = create_schema();
    let (signing_key, public_key_hex) = create_test_keypair();
    let db_name = format!("testdb-{}", public_key_hex);
    
    // Submit multiple keys
    for i in 1..=3 {
        let key = format!("test_key_{}", i);
        let value = format!("test_value_{}", i);
        let message = format!("{}:{}:{}", db_name, key, value);
        let signature = signing_key.sign(message.as_bytes());
        let signature_hex = hex::encode(signature.to_bytes());
        
        let submit_query = r#"
            mutation SubmitData($input: DataInput!) {
                submitData(input: $input) {
                    success
                }
            }
        "#;
        
        let submit_variables = Variables::from_json(&serde_json::json!({
            "input": {
                "dbName": db_name,
                "key": key,
                "value": value,
                "storeType": "String",
                "publicKey": public_key_hex,
                "signature": signature_hex
            }
        })).unwrap();
        
        execute(submit_query, None, &schema, &submit_variables, &context).await.unwrap();
    }
    
    // List keys
    let list_query = r#"
        query ListKeys($dbName: String!, $pattern: String) {
            listKeys(dbName: $dbName, pattern: $pattern) {
                keys
                count
            }
        }
    "#;
    
    let list_variables = Variables::from_json(&serde_json::json!({
        "dbName": db_name,
        "pattern": "test_key_*"
    })).unwrap();
    
    let result = execute(list_query, None, &schema, &list_variables, &context).await;
    assert!(result.is_ok());
    
    let (value, errors) = result.unwrap();
    assert!(errors.is_empty());
    
    if let Value::Object(obj) = value {
        if let Some(Value::Object(list_keys)) = obj.get("listKeys") {
            if let Some(Value::Scalar(count)) = list_keys.get("count") {
                assert_eq!(count.as_int(), Some(3));
            }
            if let Some(Value::List(keys)) = list_keys.get("keys") {
                assert_eq!(keys.len(), 3);
            }
        }
    }
}

#[tokio::test]
async fn test_get_sync_operations() {
    let context = create_test_context().await;
    let schema = create_schema();
    let (signing_key, public_key_hex) = create_test_keypair();
    let db_name = format!("testdb-{}", public_key_hex);
    
    // Submit data to create sync operations
    let message = format!("{}:test_key:test_value", db_name);
    let signature = signing_key.sign(message.as_bytes());
    let signature_hex = hex::encode(signature.to_bytes());
    
    let submit_query = r#"
        mutation SubmitData($input: DataInput!) {
            submitData(input: $input) {
                success
            }
        }
    "#;
    
    let submit_variables = Variables::from_json(&serde_json::json!({
        "input": {
            "dbName": db_name,
            "key": "test_key",
            "value": "test_value",
            "storeType": "String",
            "publicKey": public_key_hex,
            "signature": signature_hex
        }
    })).unwrap();
    
    execute(submit_query, None, &schema, &submit_variables, &context).await.unwrap();
    
    // Get sync operations
    let sync_query = r#"
        query GetSyncOperations($dbName: String!, $limit: Int) {
            getSyncOperations(dbName: $dbName, limit: $limit) {
                operations {
                    opId
                    dbName
                    key
                    value
                    storeType
                    publicKey
                    signature
                }
                count
            }
        }
    "#;
    
    let sync_variables = Variables::from_json(&serde_json::json!({
        "dbName": db_name,
        "limit": 10
    })).unwrap();
    
    let result = execute(sync_query, None, &schema, &sync_variables, &context).await;
    assert!(result.is_ok());
    
    let (value, errors) = result.unwrap();
    assert!(errors.is_empty());
    
    if let Value::Object(obj) = value {
        if let Some(Value::Object(sync_ops)) = obj.get("getSyncOperations") {
            if let Some(Value::Scalar(count)) = sync_ops.get("count") {
                assert_eq!(count.as_int(), Some(1));
            }
            if let Some(Value::List(operations)) = sync_ops.get("operations") {
                assert_eq!(operations.len(), 1);
                
                if let Some(Value::Object(op)) = operations.get(0) {
                    if let Some(Value::Scalar(key)) = op.get("key") {
                        assert_eq!(key.as_string(), Some("test_key"));
                    }
                    if let Some(Value::Scalar(value)) = op.get("value") {
                        assert_eq!(value.as_string(), Some("test_value"));
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn test_submit_json_data() {
    let context = create_test_context().await;
    let schema = create_schema();
    let (signing_key, public_key_hex) = create_test_keypair();
    let db_name = format!("testdb-{}", public_key_hex);
    
    let json_value = r#"{"name": "test", "age": 30}"#;
    let message = format!("{}:test_json_key:{}", db_name, json_value);
    let signature = signing_key.sign(message.as_bytes());
    let signature_hex = hex::encode(signature.to_bytes());
    
    let query = r#"
        mutation SubmitData($input: DataInput!) {
            submitData(input: $input) {
                success
                message
                operationId
            }
        }
    "#;
    
    let variables = Variables::from_json(&serde_json::json!({
        "input": {
            "dbName": db_name,
            "key": "test_json_key",
            "value": json_value,
            "storeType": "JSON",
            "publicKey": public_key_hex,
            "signature": signature_hex
        }
    })).unwrap();
    
    let result = execute(query, None, &schema, &variables, &context).await;
    assert!(result.is_ok());
    
    let (value, errors) = result.unwrap();
    assert!(errors.is_empty());
    
    if let Value::Object(obj) = value {
        if let Some(Value::Object(submit_data)) = obj.get("submitData") {
            if let Some(Value::Scalar(success)) = submit_data.get("success") {
                assert_eq!(success.as_boolean(), Some(true));
            }
        }
    }
}

#[tokio::test]
async fn test_submit_hash_data() {
    let context = create_test_context().await;
    let schema = create_schema();
    let (signing_key, public_key_hex) = create_test_keypair();
    let db_name = format!("testdb-{}", public_key_hex);
    
    let message = format!("{}:test_hash_key:test_value", db_name);
    let signature = signing_key.sign(message.as_bytes());
    let signature_hex = hex::encode(signature.to_bytes());
    
    let query = r#"
        mutation SubmitData($input: DataInput!) {
            submitData(input: $input) {
                success
                message
                operationId
            }
        }
    "#;
    
    let variables = Variables::from_json(&serde_json::json!({
        "input": {
            "dbName": db_name,
            "key": "test_hash_key",
            "value": "test_value",
            "storeType": "Hash",
            "field": "test_field",
            "publicKey": public_key_hex,
            "signature": signature_hex
        }
    })).unwrap();
    
    let result = execute(query, None, &schema, &variables, &context).await;
    assert!(result.is_ok());
    
    let (value, errors) = result.unwrap();
    assert!(errors.is_empty());
    
    if let Value::Object(obj) = value {
        if let Some(Value::Object(submit_data)) = obj.get("submitData") {
            if let Some(Value::Scalar(success)) = submit_data.get("success") {
                assert_eq!(success.as_boolean(), Some(true));
            }
        }
    }
}

#[tokio::test]
async fn test_database_stats() {
    let context = create_test_context().await;
    let schema = create_schema();
    let (signing_key, public_key_hex) = create_test_keypair();
    let db_name = format!("testdb-{}", public_key_hex);
    
    // Submit some data first
    let message = format!("{}:test_key:test_value", db_name);
    let signature = signing_key.sign(message.as_bytes());
    let signature_hex = hex::encode(signature.to_bytes());
    
    let submit_query = r#"
        mutation SubmitData($input: DataInput!) {
            submitData(input: $input) {
                success
            }
        }
    "#;
    
    let submit_variables = Variables::from_json(&serde_json::json!({
        "input": {
            "dbName": db_name,
            "key": "test_key",
            "value": "test_value",
            "storeType": "String",
            "publicKey": public_key_hex,
            "signature": signature_hex
        }
    })).unwrap();
    
    execute(submit_query, None, &schema, &submit_variables, &context).await.unwrap();
    
    // Get database stats
    let stats_query = r#"
        query GetDatabaseStats($dbName: String!) {
            getDatabaseStats(dbName: $dbName) {
                keyCount
                syncOperationCount
                lastModified
            }
        }
    "#;
    
    let stats_variables = Variables::from_json(&serde_json::json!({
        "dbName": db_name
    })).unwrap();
    
    let result = execute(stats_query, None, &schema, &stats_variables, &context).await;
    assert!(result.is_ok());
    
    let (value, errors) = result.unwrap();
    assert!(errors.is_empty());
    
    if let Value::Object(obj) = value {
        if let Some(Value::Object(stats)) = obj.get("getDatabaseStats") {
            if let Some(Value::Scalar(key_count)) = stats.get("keyCount") {
                assert_eq!(key_count.as_int(), Some(1));
            }
            if let Some(Value::Scalar(sync_count)) = stats.get("syncOperationCount") {
                assert_eq!(sync_count.as_int(), Some(1));
            }
        }
    }
}