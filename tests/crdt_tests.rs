use cyberfly_rust_node::crdt::CrdtStore;
use serde_json::json;

#[test]
fn test_crdt_basic_operations() {
    let mut store = CrdtStore::new();
    
    // Test setting and getting a string value
    let value = json!("test_string");
    store.set("test_key", value.clone()).unwrap();
    
    let retrieved = store.get("test_key").unwrap();
    assert_eq!(retrieved, Some(value));
}

#[test]
fn test_crdt_nonexistent_key() {
    let store = CrdtStore::new();
    
    let retrieved = store.get("nonexistent_key").unwrap();
    assert_eq!(retrieved, None);
}

#[test]
fn test_crdt_number_values() {
    let mut store = CrdtStore::new();
    
    // Test integer
    let int_value = json!(42);
    store.set("int_key", int_value.clone()).unwrap();
    assert_eq!(store.get("int_key").unwrap(), Some(int_value));
    
    // Test float
    let float_value = json!(3.14);
    store.set("float_key", float_value.clone()).unwrap();
    assert_eq!(store.get("float_key").unwrap(), Some(float_value));
}

#[test]
fn test_crdt_boolean_values() {
    let mut store = CrdtStore::new();
    
    let bool_value = json!(true);
    store.set("bool_key", bool_value.clone()).unwrap();
    assert_eq!(store.get("bool_key").unwrap(), Some(bool_value));
}

#[test]
fn test_crdt_overwrite_value() {
    let mut store = CrdtStore::new();
    
    let value1 = json!("first_value");
    let value2 = json!("second_value");
    
    store.set("test_key", value1).unwrap();
    store.set("test_key", value2.clone()).unwrap();
    
    let retrieved = store.get("test_key").unwrap();
    assert_eq!(retrieved, Some(value2));
}

#[test]
fn test_crdt_merge_basic() {
    let mut store1 = CrdtStore::new();
    let mut store2 = CrdtStore::new();
    
    // Set a value in store1
    let value1 = json!("value_from_store1");
    store1.set("key1", value1.clone()).unwrap();
    
    // Set a value in store2
    let value2 = json!("value_from_store2");
    store2.set("key2", value2.clone()).unwrap();
    
    // Get bytes from store2 and merge into store1
    let store2_bytes = store2.to_bytes();
    store1.merge(&store2_bytes).unwrap();
    
    // Store1 should still have its original value
    assert_eq!(store1.get("key1").unwrap(), Some(value1));
}

#[test]
fn test_crdt_to_bytes() {
    let mut store = CrdtStore::new();
    
    store.set("test_key", json!("test_value")).unwrap();
    
    let bytes = store.to_bytes();
    assert!(!bytes.is_empty());
}

#[test]
fn test_crdt_get_changes() {
    let mut store = CrdtStore::new();
    
    store.set("test_key", json!("test_value")).unwrap();
    
    let changes = store.get_changes();
    assert!(!changes.is_empty());
}