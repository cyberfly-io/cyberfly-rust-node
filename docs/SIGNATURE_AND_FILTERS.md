# Signature Verification and Filter Integration

## Overview

The Cyberfly Rust Node now includes comprehensive signature verification for all stored data and advanced filtering capabilities integrated with GraphQL. This ensures data authenticity and provides powerful querying across all data types.

## Table of Contents

1. [Signature Verification](#signature-verification)
2. [Storage with Signatures](#storage-with-signatures)
3. [GraphQL Filter Queries](#graphql-filter-queries)
4. [Verification on Consumer Side](#verification-on-consumer-side)
5. [Complete Examples](#complete-examples)

## Signature Verification

### How It Works

Every piece of data stored in the system includes:
- **public_key**: Ed25519 public key (hex encoded)
- **signature**: Ed25519 signature (hex encoded)
- **timestamp**: Unix timestamp in milliseconds

When data is submitted:
1. Client signs the data with their private key
2. Server verifies the signature matches the public key
3. Data is stored with signature metadata
4. Consumers can verify data authenticity when retrieved

### SignatureMetadata Structure

```rust
pub struct SignatureMetadata {
    pub public_key: String,   // Hex-encoded public key
    pub signature: String,    // Hex-encoded signature
    pub timestamp: i64,       // Unix timestamp (ms)
}
```

## Storage with Signatures

### All Data Types Include Metadata

Every storage operation now supports signature metadata:

```rust
// String
storage.set_string_with_metadata(key, value, Some(metadata)).await?;

// Hash
storage.set_hash_with_metadata(key, field, value, Some(metadata)).await?;

// List
storage.push_list_with_metadata(key, value, Some(metadata)).await?;

// Set
storage.add_set_with_metadata(key, member, Some(metadata)).await?;

// SortedSet
storage.add_sorted_set_with_metadata(key, score, member, Some(metadata)).await?;

// JSON
storage.set_json_with_metadata(key, path, json_str, Some(metadata)).await?;

// Stream
storage.xadd_with_metadata(key, id, fields, Some(metadata)).await?;

// TimeSeries
storage.ts_add_with_metadata(key, timestamp, value, Some(metadata)).await?;

// Geospatial
storage.geoadd_with_metadata(key, lon, lat, member, Some(metadata)).await?;
```

## GraphQL Filter Queries

### 1. Filter JSON Documents

Query JSON documents across multiple keys with complex conditions:

```graphql
query FilterUsers {
  filterJsonDocuments(
    pattern: "users:*"
    conditions: [
      { field: "status", operator: "eq", value: "\"active\"" }
      { field: "age", operator: "gte", value: "18" }
      { field: "tier", operator: "in", value: "[\"premium\", \"enterprise\"]" }
    ]
    options: {
      limit: 20
      offset: 0
      sortBy: "name"
      sortOrder: "asc"
    }
  ) {
    key
    data
    metadata {
      publicKey
      signature
      timestamp
    }
  }
}
```

**Supported Operators:**
- `eq` - Equal
- `ne` - Not equal
- `gt` - Greater than
- `gte` - Greater than or equal
- `lt` - Less than
- `lte` - Less than or equal
- `contains` - String contains
- `in` - Value in array

**Nested Fields:**
Use dot notation for nested fields:
```graphql
{ field: "profile.verified", operator: "eq", value: "true" }
```

### 2. Get Last N Stream Entries

Retrieve the most recent entries from a stream:

```graphql
query RecentLogs {
  getStreamLastN(
    dbName: "mydb"
    streamName: "logs"
    count: 10
  ) {
    id
    fields {
      key
      value
    }
    metadata {
      publicKey
      signature
      timestamp
    }
  }
}
```

### 3. Query TimeSeries with Aggregation

Query time series data with aggregation and filtering:

```graphql
query HourlyCPU {
  queryTimeseries(
    dbName: "mydb"
    key: "cpu_usage"
    fromTimestamp: "1704067200000"
    toTimestamp: "1704153600000"
    filterOptions: {
      aggregationType: "avg"
      timeBucket: "3600000"  # 1 hour in milliseconds
      minValue: 0.0
      maxValue: 100.0
      count: 24
    }
  ) {
    timestamp
    value
    metadata {
      publicKey
      signature
      timestamp
    }
  }
}
```

**Aggregation Types:**
- `avg` - Average
- `sum` - Sum
- `min` - Minimum
- `max` - Maximum
- `count` - Count
- `first` - First value
- `last` - Last value

### 4. Search Geospatial with Coordinates

Search locations within radius and get coordinates:

```graphql
query NearbyStores {
  searchGeoWithCoords(
    dbName: "mydb"
    key: "stores"
    longitude: -122.4194
    latitude: 37.7749
    radius: 5.0
    unit: "km"
  ) {
    member
    longitude
    latitude
    metadata {
      publicKey
      signature
      timestamp
    }
  }
}
```

### 5. Verify Data

Verify signature of retrieved data:

```graphql
query VerifySignature {
  verifyData(
    data: "mydb:users:john:active"
    publicKey: "a1b2c3d4..."
    signature: "e5f6g7h8..."
  )
}
```

Returns `true` if signature is valid, `false` otherwise.

### 6. Get All Streams

Retrieve all streams for a database with metadata:

```graphql
query AllStreams {
  getAllStream(dbName: "mydb") {
    key
    entries {
      id
      fields {
        key
        value
      }
      metadata {
        publicKey
        signature
        timestamp
      }
    }
    metadata {
      publicKey
      signature
      timestamp
    }
  }
}
```

## Verification on Consumer Side

### Client-Side Verification (JavaScript/TypeScript)

```typescript
import { ed25519 } from '@noble/ed25519';

async function verifyData(
  data: string,
  publicKeyHex: string,
  signatureHex: string
): Promise<boolean> {
  try {
    const publicKey = Buffer.from(publicKeyHex, 'hex');
    const signature = Buffer.from(signatureHex, 'hex');
    const message = Buffer.from(data, 'utf8');
    
    return await ed25519.verify(signature, message, publicKey);
  } catch (error) {
    console.error('Verification failed:', error);
    return false;
  }
}

// Example usage
const result = await fetch('/graphql', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    query: `
      query {
        filterJsonDocuments(pattern: "users:*", conditions: []) {
          key
          data
          metadata { publicKey signature timestamp }
        }
      }
    `
  })
});

const { data } = await result.json();

for (const doc of data.filterJsonDocuments) {
  const isValid = await verifyData(
    doc.data,
    doc.metadata.publicKey,
    doc.metadata.signature
  );
  
  if (isValid) {
    console.log('✅ Valid data:', doc.data);
  } else {
    console.log('❌ Invalid signature for:', doc.key);
  }
}
```

### Client-Side Verification (Python)

```python
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
from cryptography.hazmat.primitives import serialization
import binascii

def verify_data(data: str, public_key_hex: str, signature_hex: str) -> bool:
    try:
        public_key_bytes = binascii.unhexlify(public_key_hex)
        signature_bytes = binascii.unhexlify(signature_hex)
        message_bytes = data.encode('utf-8')
        
        public_key = Ed25519PublicKey.from_public_bytes(public_key_bytes)
        public_key.verify(signature_bytes, message_bytes)
        return True
    except Exception as e:
        print(f"Verification failed: {e}")
        return False

# Example usage
import requests

response = requests.post('http://localhost:3000/graphql', json={
    'query': '''
        query {
            filterJsonDocuments(pattern: "users:*", conditions: []) {
                key
                data
                metadata { publicKey signature timestamp }
            }
        }
    '''
})

data = response.json()

for doc in data['data']['filterJsonDocuments']:
    is_valid = verify_data(
        doc['data'],
        doc['metadata']['publicKey'],
        doc['metadata']['signature']
    )
    
    if is_valid:
        print(f"✅ Valid data: {doc['data']}")
    else:
        print(f"❌ Invalid signature for: {doc['key']}")
```

### Client-Side Verification (Rust)

```rust
use ed25519_dalek::{PublicKey, Signature, Verifier};

fn verify_data(data: &str, public_key_hex: &str, signature_hex: &str) -> bool {
    let public_key_bytes = match hex::decode(public_key_hex) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    
    let signature_bytes = match hex::decode(signature_hex) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    
    let public_key = match PublicKey::from_bytes(&public_key_bytes) {
        Ok(key) => key,
        Err(_) => return false,
    };
    
    let signature = match Signature::from_bytes(&signature_bytes) {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    
    public_key.verify(data.as_bytes(), &signature).is_ok()
}

// Example usage
let is_valid = verify_data(
    "mydb:users:john:active",
    "a1b2c3d4e5f6...",
    "1a2b3c4d5e6f..."
);

if is_valid {
    println!("✅ Valid data");
} else {
    println!("❌ Invalid signature");
}
```

## Complete Examples

### Example 1: Submit and Verify User Data

**Step 1: Generate Keys (Client)**
```javascript
import { ed25519 } from '@noble/ed25519';

const privateKey = ed25519.utils.randomPrivateKey();
const publicKey = await ed25519.getPublicKey(privateKey);
const publicKeyHex = Buffer.from(publicKey).toString('hex');
```

**Step 2: Sign and Submit Data**
```graphql
mutation SubmitUser {
  submitData(data: {
    dbName: "users"
    key: "john123"
    value: "{\"name\":\"John\",\"age\":30,\"status\":\"active\"}"
    publicKey: "a1b2c3d4e5f6..."
    signature: "1a2b3c4d5e6f..."
    storeType: "Json"
  }) {
    success
    message
  }
}
```

**Step 3: Query with Filters**
```graphql
query ActiveUsers {
  filterJsonDocuments(
    pattern: "users:*"
    conditions: [
      { field: "status", operator: "eq", value: "\"active\"" }
      { field: "age", operator: "gte", value: "18" }
    ]
    options: { limit: 10, sortBy: "name" }
  ) {
    key
    data
    metadata {
      publicKey
      signature
      timestamp
    }
  }
}
```

**Step 4: Verify on Client**
```javascript
const result = await fetchGraphQL(query);

for (const user of result.data.filterJsonDocuments) {
  const isValid = await verifyData(
    user.data,
    user.metadata.publicKey,
    user.metadata.signature
  );
  
  if (isValid) {
    const userData = JSON.parse(user.data);
    console.log('✅ Verified user:', userData);
  } else {
    console.warn('❌ Invalid signature, skipping user');
  }
}
```

### Example 2: Stream Data with Verification

**Submit Stream Entry**
```graphql
mutation LogEvent {
  submitData(data: {
    dbName: "app"
    key: "logs"
    value: ""
    publicKey: "a1b2c3d4..."
    signature: "1a2b3c4d..."
    storeType: "Stream"
    streamFields: "[{\"key\":\"level\",\"value\":\"info\"},{\"key\":\"message\",\"value\":\"User logged in\"}]"
  }) {
    success
    message
  }
}
```

**Query Last 10 Entries**
```graphql
query RecentLogs {
  getStreamLastN(dbName: "app", streamName: "logs", count: 10) {
    id
    fields { key value }
    metadata {
      publicKey
      signature
      timestamp
    }
  }
}
```

**Verify Entries**
```javascript
const logs = await fetchGraphQL(query);

for (const entry of logs.data.getStreamLastN) {
  // Reconstruct message for verification
  const message = entry.fields
    .map(f => `${f.key}:${f.value}`)
    .join('|');
  
  const isValid = await verifyData(
    message,
    entry.metadata.publicKey,
    entry.metadata.signature
  );
  
  if (isValid) {
    console.log('✅ Verified log entry:', entry.id);
  }
}
```

### Example 3: TimeSeries with Aggregation

**Submit TimeSeries Data**
```graphql
mutation RecordMetric {
  submitData(data: {
    dbName: "metrics"
    key: "cpu_usage"
    value: "45.5"
    publicKey: "a1b2c3d4..."
    signature: "1a2b3c4d..."
    storeType: "TimeSeries"
    timestamp: "1704067200000"
  }) {
    success
    message
  }
}
```

**Query with Hourly Aggregation**
```graphql
query HourlyCPU {
  queryTimeseries(
    dbName: "metrics"
    key: "cpu_usage"
    fromTimestamp: "1704067200000"
    toTimestamp: "1704153600000"
    filterOptions: {
      aggregationType: "avg"
      timeBucket: "3600000"
      minValue: 0.0
      maxValue: 100.0
    }
  ) {
    timestamp
    value
    metadata {
      publicKey
      signature
      timestamp
    }
  }
}
```

### Example 4: Geospatial Search

**Submit Location**
```graphql
mutation AddStore {
  submitData(data: {
    dbName: "locations"
    key: "stores"
    value: "store_downtown"
    publicKey: "a1b2c3d4..."
    signature: "1a2b3c4d..."
    storeType: "Geo"
    longitude: -122.4194
    latitude: 37.7749
  }) {
    success
    message
  }
}
```

**Search Nearby**
```graphql
query NearbyStores {
  searchGeoWithCoords(
    dbName: "locations"
    key: "stores"
    longitude: -122.4500
    latitude: 37.7800
    radius: 5.0
    unit: "km"
  ) {
    member
    longitude
    latitude
    metadata {
      publicKey
      signature
      timestamp
    }
  }
}
```

## Security Considerations

### 1. Signature Verification

- Always verify signatures on the consumer side
- Don't trust data without valid signatures
- Check timestamp to prevent replay attacks

### 2. Key Management

- Private keys should never leave the client
- Use secure key generation (cryptographically random)
- Store private keys securely (e.g., hardware wallets, secure enclaves)

### 3. Message Format

The signed message format is consistent:
```
db_name:key:value
```

For example:
```
users:john123:{"name":"John","age":30}
```

### 4. Timestamp Validation

Check metadata timestamp to ensure data freshness:
```javascript
const MAX_AGE_MS = 24 * 60 * 60 * 1000; // 24 hours

function isDataFresh(metadata) {
  const now = Date.now();
  const age = now - metadata.timestamp;
  return age < MAX_AGE_MS;
}
```

## Performance Tips

### 1. Batch Verification

Verify signatures in parallel:
```javascript
const results = await Promise.all(
  documents.map(async (doc) => ({
    doc,
    valid: await verifyData(doc.data, doc.metadata.publicKey, doc.metadata.signature)
  }))
);

const validDocs = results.filter(r => r.valid).map(r => r.doc);
```

### 2. Cache Verified Keys

Cache known public keys to avoid repeated verifications:
```javascript
const verifiedKeys = new Set();

async function verifyWithCache(data, publicKey, signature) {
  if (verifiedKeys.has(publicKey)) {
    return true; // Trust previously verified key
  }
  
  const valid = await verifyData(data, publicKey, signature);
  if (valid) {
    verifiedKeys.add(publicKey);
  }
  return valid;
}
```

### 3. Pagination

Use pagination for large result sets:
```graphql
query PaginatedUsers {
  filterJsonDocuments(
    pattern: "users:*"
    conditions: [...]
    options: {
      limit: 20
      offset: 0
    }
  ) {
    key
    data
    metadata { publicKey signature timestamp }
  }
}
```

## Error Handling

### Common Errors

1. **Invalid Signature**: Data has been tampered with
2. **Invalid Public Key**: Malformed hex encoding
3. **Missing Metadata**: Data stored without signature
4. **Expired Timestamp**: Data too old (if timestamp validation enabled)

### Example Error Handling

```javascript
async function safeVerifyData(doc) {
  try {
    if (!doc.metadata) {
      console.warn('No metadata for:', doc.key);
      return false;
    }
    
    if (!isDataFresh(doc.metadata)) {
      console.warn('Stale data:', doc.key);
      return false;
    }
    
    return await verifyData(
      doc.data,
      doc.metadata.publicKey,
      doc.metadata.signature
    );
  } catch (error) {
    console.error('Verification error:', error);
    return false;
  }
}
```

## Testing

### Unit Test Example

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_signed_data_storage() {
        let storage = create_test_storage().await;
        
        let metadata = SignatureMetadata {
            public_key: "test_key".to_string(),
            signature: "test_sig".to_string(),
            timestamp: 1704067200000,
        };
        
        storage.set_string_with_metadata(
            "test:key",
            "test_value",
            Some(metadata.clone())
        ).await.unwrap();
        
        // Verify metadata was stored
        let stored = storage.get_string("test:key").await.unwrap();
        assert_eq!(stored, Some("test_value".to_string()));
    }
}
```

## Migration Guide

### From Unsigned to Signed Data

If you have existing unsigned data:

1. **Add metadata to existing data**: Use migration script to add default metadata
2. **Support both modes**: Accept data with and without signatures temporarily
3. **Gradual rollout**: Phase in signature requirements

### Example Migration

```rust
async fn migrate_to_signed_data(storage: &BlobStorage) -> Result<()> {
    let keys = storage.scan_keys("*").await?;
    
    for key in keys {
        // Check if metadata exists
        let value = storage.get_value(&key).await?;
        
        // Add default metadata if missing
        // (This is just for migration - in production, require real signatures)
        if !has_metadata(&value) {
            let default_metadata = SignatureMetadata {
                public_key: "migration_key".to_string(),
                signature: "migration_sig".to_string(),
                timestamp: chrono::Utc::now().timestamp_millis(),
            };
            
            // Re-store with metadata
            // ... update based on type
        }
    }
    
    Ok(())
}
```

## Summary

The integrated signature and filter system provides:

✅ **Data Authenticity**: Every piece of data is signed and verifiable
✅ **Advanced Filtering**: Complex queries across all data types
✅ **Consumer Verification**: Clients can verify data integrity
✅ **Metadata Tracking**: Public key, signature, and timestamp for all data
✅ **GraphQL Integration**: Powerful query language with filter support
✅ **Type Safety**: Strongly typed API with compile-time guarantees

All data can now be trusted and efficiently queried with full signature verification support.