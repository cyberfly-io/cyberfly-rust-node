use automerge::{AutoCommit, transaction::Transactable, ReadDoc};
use anyhow::Result;
use serde_json::Value;

/// CRDT-based data structure for conflict-free merging
pub struct CrdtStore {
    doc: AutoCommit,
}

impl CrdtStore {
    pub fn new() -> Self {
        Self {
            doc: AutoCommit::new(),
        }
    }

    /// Set a value in the CRDT document
    pub fn set(&mut self, key: &str, value: Value) -> Result<()> {
        match value {
            Value::String(s) => {
                self.doc.put(automerge::ROOT, key, s)?;
            }
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    self.doc.put(automerge::ROOT, key, i)?;
                } else if let Some(f) = n.as_f64() {
                    self.doc.put(automerge::ROOT, key, f)?;
                }
            }
            Value::Bool(b) => {
                self.doc.put(automerge::ROOT, key, b)?;
            }
            Value::Null => {
                self.doc.put(automerge::ROOT, key, ())?;
            }
            _ => {
                // For complex types, store as JSON string
                let json_str = serde_json::to_string(&value)?;
                self.doc.put(automerge::ROOT, key, json_str)?;
            }
        }
        Ok(())
    }

    /// Get a value from the CRDT document
    pub fn get(&self, key: &str) -> Result<Option<Value>> {
        let val = self.doc.get(automerge::ROOT, key)?;
        
        if let Some((value, _)) = val {
            // Convert Automerge value to JSON value
            let json_value = match value {
                automerge::Value::Scalar(s) => {
                    match s.as_ref() {
                        automerge::ScalarValue::Str(s) => Value::String(s.to_string()),
                        automerge::ScalarValue::Int(i) => Value::Number((*i).into()),
                        automerge::ScalarValue::Uint(u) => Value::Number((*u).into()),
                        automerge::ScalarValue::F64(f) => {
                            serde_json::Number::from_f64(*f)
                                .map(Value::Number)
                                .unwrap_or(Value::Null)
                        }
                        automerge::ScalarValue::Boolean(b) => Value::Bool(*b),
                        automerge::ScalarValue::Null => Value::Null,
                        _ => Value::Null,
                    }
                }
                _ => Value::Null,
            };
            Ok(Some(json_value))
        } else {
            Ok(None)
        }
    }

    /// Merge changes from another CRDT document
    pub fn merge(&mut self, other_bytes: &[u8]) -> Result<()> {
        self.doc.load_incremental(other_bytes)?;
        Ok(())
    }

    /// Get the current state as bytes for sharing
    pub fn to_bytes(&mut self) -> Vec<u8> {
        self.doc.save()
    }

    /// Get all changes since a specific point
    pub fn get_changes(&mut self) -> Vec<u8> {
        self.doc.save()
    }
}

impl Default for CrdtStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_crdt_basic_operations() {
        let mut store = CrdtStore::new();
        
        // Set values
        store.set("key1", json!("value1")).unwrap();
        store.set("key2", json!(42)).unwrap();
        
        // Get values
        assert_eq!(store.get("key1").unwrap(), Some(json!("value1")));
        assert_eq!(store.get("key2").unwrap(), Some(json!(42)));
    }

    #[test]
    fn test_crdt_merge() {
        let mut store1 = CrdtStore::new();
        let mut store2 = CrdtStore::new();
        
        // Set different values in each store
        store1.set("key1", json!("value1")).unwrap();
        store2.set("key2", json!("value2")).unwrap();
        
        // Merge store2 into store1
        let store2_bytes = store2.to_bytes();
        store1.merge(&store2_bytes).unwrap();
        
        // Both keys should be present
        assert!(store1.get("key1").unwrap().is_some());
        assert!(store1.get("key2").unwrap().is_some());
    }
}
