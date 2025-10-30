# Indexing Memory Management

## Memory Usage

### Per-Key Memory Cost
Each indexed key uses approximately **100-150 bytes**:
- Field value (String): ~25-50 bytes
- Key reference (String): ~20-30 bytes
- HashSet overhead: ~24 bytes
- HashMap entry: ~40 bytes

### Scaling Table

| Records | Indexes | Memory Usage | Server Impact |
|---------|---------|--------------|---------------|
| 1K | 3 | ~450 KB | Negligible |
| 10K | 3 | ~4.5 MB | Negligible |
| 100K | 3 | ~45 MB | Low |
| 1M | 3 | ~450 MB | Acceptable |
| 10M | 3 | ~4.5 GB | High (use limits) |

## Memory Limits

### Setting Limits on Index Creation

```graphql
mutation {
  createIndexWithLimits(
    dbName: "sensors"
    indexName: "value_idx"
    field: "value"
    indexType: "range"
    maxKeys: 1000000        # Max 1M keys
    memoryLimitMB: 500      # Max 500 MB
  )
}
```

### Monitoring Memory Usage

```graphql
query {
  getIndexStats(dbName: "sensors", indexName: "value_idx") {
    name
    totalKeys
    uniqueValues
    memoryUsageBytes
    memoryUsageMb           # Human-readable MB
    maxKeys                 # Configured limit
    memoryLimitMb           # Configured limit
  }
}

# Get total memory across all indexes
query {
  getTotalIndexMemory {
    totalBytes
    totalMB
    indexCount
  }
}
```

## Memory Optimization Strategies

### 1. Selective Indexing
Only index fields you frequently query:

```graphql
# ✅ Good: Index frequently queried fields
createIndex(dbName: "users", indexName: "email_idx", field: "email", indexType: "exact")
createIndex(dbName: "users", indexName: "status_idx", field: "status", indexType: "exact")

# ❌ Avoid: Don't index every field
# createIndex(dbName: "users", indexName: "bio_idx", field: "bio", indexType: "fulltext")
```

### 2. Use Exact Indexes for Low-Cardinality Fields
Exact indexes are most efficient for fields with limited unique values:

```graphql
# ✅ Efficient: Few unique values (active/inactive/pending)
createIndex(dbName: "users", indexName: "status_idx", field: "status", indexType: "exact")

# ⚠️ Less efficient: Many unique values (timestamps, IDs)
# Consider if you really need to index these
```

### 3. Set Memory Limits for Large Datasets

```rust
// In Rust code
let index = SecondaryIndex::new_with_limits(
    "sensor_idx".to_string(),
    "sensors".to_string(),
    "sensor_id".to_string(),
    IndexType::Exact,
    10_000_000,  // Max 10M keys
    1024,        // Max 1GB memory
);
```

### 4. Use Time-Based Rotation
For time-series data, create new indexes periodically:

```graphql
# Hourly indexes for sensor data
createIndex(dbName: "sensors", indexName: "hour_12_idx", ...)
# Drop old indexes after retention period
dropIndex(dbName: "sensors", indexName: "hour_06_idx")
```

### 5. Compress Field Values
Store compressed/shortened values in indexes:

```rust
// Instead of storing full JSON in index:
// "{"name": "Long Sensor Name", "location": "Building A, Floor 3"}"

// Store just the key needed:
// "sensor_12345"
```

## Memory Monitoring

### Check Total Index Memory

```typescript
const stats = await client.request(`
  query {
    totalIndexMemory
  }
`);

console.log(`Total index memory: ${stats.totalIndexMemory} MB`);

// Alert if over threshold
if (stats.totalIndexMemory > 1000) { // 1GB
  console.warn('Index memory exceeds 1GB!');
}
```

### Per-Index Monitoring

```typescript
const indexes = await client.request(`
  query {
    listIndexes(dbName: "sensors")
  }
`);

for (const indexName of indexes.listIndexes) {
  const stats = await client.request(`
    query {
      getIndexStats(dbName: "sensors", indexName: "${indexName}") {
        memoryUsageMb
        totalKeys
      }
    }
  `);
  
  console.log(`${indexName}: ${stats.memoryUsageMb} MB (${stats.totalKeys} keys)`);
}
```

## Real-World Examples

### IoT Sensor Network (1M sensors, 100M readings)

```yaml
Scenario: Track sensor readings with time-based queries

Indexes:
  - sensor_id (exact): 1M unique values × 100 readings = 100M keys → ~10GB
  - timestamp (range): 100M unique timestamps → ~10GB
  
Solution: Use time-based partitioning
  - hour_current_idx: Last hour only → ~4M keys → ~400MB ✅
  - day_current_idx: Last 24 hours → ~100M keys → ~10GB ⚠️
  - Rotate hourly, keep last 24 indexes
```

### E-Commerce Platform (10M products)

```yaml
Scenario: Product catalog with search

Indexes:
  - category (exact): 100 categories × 100K products avg = 10M keys → ~1GB ✅
  - price (range): 10M products → ~1GB ✅
  - name (fulltext): 10M products → ~1.5GB ⚠️
  
Memory: ~3.5GB total (acceptable for 16GB+ server)
```

### User Management (100K users)

```yaml
Scenario: User authentication and search

Indexes:
  - email (exact): 100K unique → ~10MB ✅
  - username (exact): 100K unique → ~10MB ✅
  - age (range): 100K users → ~10MB ✅
  - name (fulltext): 100K users → ~15MB ✅
  
Memory: ~45MB total (negligible)
```

## When to Use Indexes

### ✅ Good Use Cases
- Exact lookups (email, username, ID)
- Range queries on numeric fields (age, price, timestamp)
- Status/category filtering (few unique values)
- Search on short text fields (< 100 chars)

### ❌ Avoid Indexing
- Fields rarely queried
- Very high cardinality (UUIDs, timestamps as exact match)
- Very long text fields (> 1KB)
- Binary data
- Frequently updated fields (index churn)

## Performance vs. Memory Trade-off

| Strategy | Query Speed | Memory Usage | Best For |
|----------|-------------|--------------|----------|
| No indexes | O(n) scan | 0 MB | Small datasets (< 1000 records) |
| Selective indexes | O(1) or O(log n) | Low (< 100 MB) | Targeted queries on key fields |
| Full indexes | O(1) | High (> 1 GB) | Large datasets, complex queries |
| Partitioned indexes | O(1) | Medium | Time-series, growing datasets |

## Summary

- **Expect ~100-150 bytes per indexed key**
- **1M keys ≈ 150 MB** (reasonable for modern servers)
- **Use limits for safety** (`maxKeys`, `memoryLimitMB`)
- **Monitor with `getIndexStats`** and `totalIndexMemory`
- **Index selectively** - not every field needs an index
- **Consider time-based rotation** for large, growing datasets

Your Redis-style database with indexing is **memory-efficient** compared to traditional databases while providing MongoDB-like query power! 🚀
