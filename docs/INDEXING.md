# Secondary Indexing System

MongoDB-like query capabilities on top of Redis-style data types.

## Features

- **Exact Match Indexes** - Fast lookups by field value (e.g., email, username)
- **Range Indexes** - Numeric/timestamp range queries (e.g., age > 18, price between 10-100)
- **Full-Text Search** - Text substring and prefix matching
- **Geospatial Indexes** - Location-based queries (planned)
- **P2P Compatible** - Indexes sync across nodes automatically

## Quick Start

### 1. Create an Index

```graphql
mutation {
  createIndex(
    dbName: "users"
    indexName: "email_idx"
    field: "email"
    indexType: "exact"
  )
}
```

### 2. Store Data (Indexes Updated Automatically)

```graphql
mutation {
  submitData(input: {
    dbName: "users"
    key: "user:alice"
    value: "{\\"name\\": \\"Alice\\", \\"email\\": \\"alice@example.com\\", \\"age\\": 25}"
    storeType: "JSON"
    publicKey: "..."
    signature: "..."
  }) {
    success
  }
}
```

### 3. Query by Index

```graphql
query {
  queryIndex(
    dbName: "users"
    indexName: "email_idx"
    operator: "equals"
    value: "alice@example.com"
  ) {
    keys
    count
    executionTimeMs
  }
}
```

## Index Types

### Exact Match Index
Perfect for unique identifiers, emails, usernames, status fields.

```graphql
createIndex(
  dbName: "users"
  indexName: "email_idx"
  field: "email"
  indexType: "exact"
)

# Query
queryIndex(
  dbName: "users"
  indexName: "email_idx"
  operator: "equals"
  value: "alice@example.com"
)
```

### Range Index
For numeric fields like age, price, timestamps.

```graphql
createIndex(
  dbName: "users"
  indexName: "age_idx"
  field: "age"
  indexType: "range"
)

# Query: age > 18
queryIndex(
  dbName: "users"
  indexName: "age_idx"
  operator: "gt"
  min: 18
)

# Query: age between 18 and 65
queryIndex(
  dbName: "users"
  indexName: "age_idx"
  operator: "between"
  min: 18
  max: 65
)
```

### Full-Text Search Index
For text fields with substring/prefix search.

```graphql
createIndex(
  dbName: "products"
  indexName: "name_idx"
  field: "name"
  indexType: "fulltext"
)

# Query: name contains "laptop"
queryIndex(
  dbName: "products"
  indexName: "name_idx"
  operator: "contains"
  value: "laptop"
)

# Query: name starts with "Mac"
queryIndex(
  dbName: "products"
  indexName: "name_idx"
  operator: "startswith"
  value: "Mac"
)
```

## Query Operators

| Operator | Index Type | Description | Parameters |
|----------|------------|-------------|------------|
| `equals` | Exact | Exact match | `value` |
| `in` | Exact | Match any of values | `values: [String]` |
| `gt` | Range | Greater than | `min` |
| `lt` | Range | Less than | `max` |
| `between` | Range | Between min and max | `min`, `max` |
| `contains` | FullText | Substring match (case-insensitive) | `value` |
| `startswith` | FullText | Prefix match (case-insensitive) | `value` |

## Advanced Usage

### Compound Queries
Use multiple indexes and intersect results in your application:

```typescript
// Find users who are:
// - Between 25-35 years old
// - AND have email ending in @example.com

const ageResult = await client.queryIndex({
  dbName: 'users',
  indexName: 'age_idx',
  operator: 'between',
  min: 25,
  max: 35
});

const emailResult = await client.queryIndex({
  dbName: 'users',
  indexName: 'email_idx',
  operator: 'contains',
  value: '@example.com'
});

// Intersect results
const matching = ageResult.keys.filter(key => 
  emailResult.keys.includes(key)
);
```

### Index Management

```graphql
# List all indexes in a database
query {
  listIndexes(dbName: "users")
}

# Get index statistics
query {
  getIndexStats(dbName: "users", indexName: "email_idx") {
    name
    field
    indexType
    totalKeys
    uniqueValues
  }
}

# Drop an index
mutation {
  dropIndex(dbName: "users", indexName: "email_idx")
}
```

## Automatic Index Updates

When you update data, indexes are automatically maintained:

```rust
// In storage.rs (pseudo-code)
pub async fn set_hash_field(&mut self, key: &str, field: &str, value: &str) -> Result<()> {
    // Store the data
    self.storage.insert(key, field, value)?;
    
    // Update any indexes on this field
    self.index_manager.update_indexes(key, field, value).await?;
    
    Ok(())
}
```

## Performance

- **Exact Match**: O(1) - Hash table lookup
- **Range Query**: O(n) - Scans all values (optimized with sorted structures)
- **Text Search**: O(n*m) - Scans values, matches substring
- **Memory**: ~100 bytes per indexed key

## Use Cases

### User Management
```graphql
# Index by email for login
createIndex(dbName: "users", indexName: "email_idx", field: "email", indexType: "exact")

# Index by age for demographic queries
createIndex(dbName: "users", indexName: "age_idx", field: "age", indexType: "range")

# Index by username for search
createIndex(dbName: "users", indexName: "username_idx", field: "username", indexType: "fulltext")
```

### E-Commerce
```graphql
# Index products by category
createIndex(dbName: "products", indexName: "category_idx", field: "category", indexType: "exact")

# Index by price for filtering
createIndex(dbName: "products", indexName: "price_idx", field: "price", indexType: "range")

# Index by name for search
createIndex(dbName: "products", indexName: "name_idx", field: "name", indexType: "fulltext")
```

### IoT Sensor Data
```graphql
# Index by sensor ID
createIndex(dbName: "sensors", indexName: "sensor_idx", field: "sensor_id", indexType: "exact")

# Index by value range for alerts
createIndex(dbName: "sensors", indexName: "value_idx", field: "value", indexType: "range")

# Index by timestamp for time-series queries
createIndex(dbName: "sensors", indexName: "time_idx", field: "timestamp", indexType: "range")
```

## Limitations

1. **Single Field Indexes** - Each index covers one field only
2. **No Joins** - Indexes don't support cross-key relationships
3. **In-Memory** - Indexes stored in memory (persisted to blob storage)
4. **Case-Insensitive Text** - Text search normalizes to lowercase

## Future Enhancements

- [ ] Composite indexes (multiple fields)
- [ ] Geospatial radius queries
- [ ] Index persistence/recovery
- [ ] Index compression
- [ ] Async index building for large datasets
- [ ] TTL support (auto-expire indexed keys)
