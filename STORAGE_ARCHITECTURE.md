# Storage Architecture - Sled + Iroh

## Overview

The Cyberfly Rust Node uses a **dual-storage architecture** combining:
1. **Sled** - Embedded key-value database for indexing and metadata
2. **Iroh Blobs** - Content-addressed storage for immutable data

This design provides the benefits of both:
- **Fast lookups** via Sled's B-tree index
- **Content-addressing** via Iroh's Blake3-based hashing
- **No external dependencies** - fully embedded
- **P2P replication** - Iroh handles distribution

## Architecture Diagram

```
┌─────────────────────────────────────────┐
│           BlobStorage                   │
│  (Type alias: RedisStorage for compat) │
└─────────────┬───────────────────────────┘
              │
      ┌───────┴──────┐
      │              │
┌─────▼─────┐  ┌────▼──────┐
│   Sled    │  │   Iroh    │
│   Index   │  │   Blobs   │
│           │  │           │
│ Key -> ID │  │ Hash->Data│
└───────────┘  └───────────┘
```

## Component Details

### Sled (Index Layer)

**Purpose:** Fast key lookups and metadata storage  
**Location:** `./data/sled_db/`  
**What it stores:**
- Key → (BlobHash, StoreType) mappings
- Metadata index for quick lookups
- Database structure information

**Key Features:**
- Embedded B-tree database
- ACID transactions
- Crash-safe (write-ahead log)
- Zero-copy reads
- Compression support

**Configuration:**
```rust
let sled_config = sled::Config::new()
    .path(&sled_path)
    .cache_capacity(1024 * 1024 * 1024) // 1GB cache
    .flush_every_ms(Some(1000))          // Durability vs performance
    .mode(sled::Mode::HighThroughput)    // Optimize for writes
    .use_compression(true);              // Save disk space

let sled_db = sled_config.open()?;
```

### Iroh Blobs (Content Layer)

**Purpose:** Immutable content storage and P2P distribution  
**Location:** `./data/iroh/blobs/`  
**What it stores:**
- Actual data values (serialized as JSON)
- Content-addressed by Blake3 hash
- SignedOperations for sync

**Key Features:**
- Content-addressed (hash-based)
- Immutable by design
- Deduplication (identical content = same hash)
- P2P replication ready
- Efficient streaming

**Usage:**
```rust
// Store data
let value_bytes = serde_json::to_vec(&value)?;
let blobs = self.store.blobs();
let tag = blobs.add_bytes(value_bytes).await?;
let hash = tag.hash; // Blake3 hash

// Retrieve data
let data = blobs.read_to_bytes(&hash).await?;
```

## Data Flow

### Write Operation

```
1. Client submits signed data via GraphQL
   ↓
2. Verify signature
   ↓
3. Serialize value to JSON bytes
   ↓
4. Store in Iroh Blobs → get Blake3 hash
   ↓
5. Index in Sled: key → (hash, store_type)
   ↓
6. Cache in memory (Moka cache)
   ↓
7. Return success
```

### Read Operation

```
1. Client queries by key via GraphQL
   ↓
2. Check memory cache (Moka)
   ↓
3. If miss, lookup in Sled index
   ↓
4. Get Blake3 hash from Sled
   ↓
5. Retrieve data from Iroh Blobs
   ↓
6. Deserialize and return
   ↓
7. Update cache
```

## Storage Types

All Redis-compatible types are supported:

| Type | Sled Index | Iroh Blob Content |
|------|-----------|-------------------|
| String | key → hash | `StringValue { value, metadata }` |
| Hash | key → hash | `HashValue { fields: HashMap, metadata }` |
| List | key → hash | `ListValue { items: Vec, metadata }` |
| Set | key → hash | `SetValue { members: HashSet, metadata }` |
| SortedSet | key → hash | `SortedSetValue { members: BTreeMap, metadata }` |
| JSON | key → hash | `JsonValue { data, id, metadata }` |
| Stream | key → hash | `StreamValue { entries, metadata }` |
| TimeSeries | key → hash | `TimeSeriesValue { points: BTreeMap, metadata }` |
| Geo | key → hash | `GeoValue { locations: HashMap, metadata }` |

## Caching Strategy

Three-tier caching:

```
1. Moka Cache (Memory)
   └─ Hot data, bounded to 10k entries
   
2. Sled (Disk - Indexed)
   └─ All keys with hash references
   
3. Iroh Blobs (Disk - Content)
   └─ All actual data values
```

## Performance Characteristics

### Sled
- **Read latency:** ~100 microseconds (cached)
- **Write latency:** ~1-5 milliseconds (with fsync)
- **Throughput:** 100k+ ops/sec (in-memory)
- **Storage:** ~100 bytes per key (overhead)

### Iroh Blobs
- **Write latency:** ~5-10 milliseconds
- **Read latency:** ~1 millisecond (small blobs)
- **Throughput:** Limited by disk I/O
- **Deduplication:** Automatic (same content = same hash)

### Combined
- **Write path:** Sled + Iroh = ~10-15ms
- **Read path:** Cache hit = ~1μs, Cache miss = ~5-10ms
- **Space efficiency:** No duplication of identical values

## Advantages vs. Redis

| Feature | Sled + Iroh | Redis |
|---------|-------------|-------|
| **Deployment** | Embedded, no daemon | Requires Redis server |
| **Dependencies** | None | Redis installation |
| **Persistence** | Built-in, durable | Requires AOF/RDB config |
| **Content Addressing** | Yes (Iroh) | No |
| **P2P Replication** | Native (Iroh) | Requires Redis Cluster |
| **Memory Usage** | Configurable cache | Primarily in-memory |
| **Crash Recovery** | Automatic | Requires snapshots |
| **Docker Size** | Smaller binary | Needs Redis image |

## Tuning Guidelines

### For Read-Heavy Workloads
```rust
sled::Config::new()
    .cache_capacity(2 * 1024 * 1024 * 1024) // 2GB cache
    .flush_every_ms(Some(5000))              // Less frequent flushes
    .mode(sled::Mode::HighThroughput)
```

### For Write-Heavy Workloads
```rust
sled::Config::new()
    .cache_capacity(512 * 1024 * 1024)      // 512MB cache
    .flush_every_ms(Some(100))               // More frequent flushes
    .mode(sled::Mode::HighThroughput)
```

### For Memory-Constrained Environments
```rust
sled::Config::new()
    .cache_capacity(128 * 1024 * 1024)      // 128MB cache
    .flush_every_ms(Some(1000))
    .use_compression(true)                   // Save disk space
```

## Backup Strategy

### Sled Backup
```rust
// Export entire Sled database
let export = sled_db.export()?;

// Save to file
std::fs::write("backup_sled.export", &export)?;

// Restore
let restored_db = sled::Config::new()
    .path("restored_db")
    .open()?;
restored_db.import(&export)?;
```

### Iroh Blobs Backup
```rust
// List all blob hashes
let hashes: Vec<Hash> = /* from Sled index */;

// Copy blob directory
// Or use Iroh's built-in replication
```

### Combined Backup
```bash
# 1. Export Sled index
# 2. Copy Iroh blob directory
# 3. Compress both
tar -czf backup.tar.gz data/sled_db/ data/iroh/blobs/

# Restore
tar -xzf backup.tar.gz -C /restore/location/
```

## Migration Path

### From Redis to Sled+Iroh

If migrating from a Redis-based system:

```rust
// 1. Export from Redis
let keys = redis.keys("*")?;
for key in keys {
    let value = redis.get(key)?;
    let store_type = redis.type(key)?;
    
    // 2. Import to Sled+Iroh
    blob_storage.migrate_value(key, value, store_type).await?;
}
```

### Type Compatibility

The `RedisStorage` type alias maintains API compatibility:
```rust
// Old code still works
pub type RedisStorage = BlobStorage;

// But consider renaming over time
pub use BlobStorage as Storage;
```

## Troubleshooting

### High Memory Usage
- Reduce Sled cache capacity
- Reduce Moka cache size
- Enable compression

### Slow Writes
- Increase flush interval (trade durability for speed)
- Use batch operations
- Check disk I/O

### Slow Reads
- Increase Sled cache
- Increase Moka cache
- Check for cache hit rate

### Database Corruption
- Sled has automatic recovery
- Keep recent backups
- Monitor Sled logs for warnings

## Monitoring Metrics

Key metrics to track:

```rust
pub struct StorageMetrics {
    // Sled metrics
    pub sled_cache_hit_rate: f64,
    pub sled_disk_usage_bytes: u64,
    pub sled_tree_height: u32,
    
    // Iroh metrics
    pub iroh_blob_count: u64,
    pub iroh_total_size_bytes: u64,
    pub iroh_dedup_ratio: f64,
    
    // Cache metrics
    pub moka_hit_rate: f64,
    pub moka_entry_count: u64,
    
    // Performance
    pub avg_read_latency_ms: f64,
    pub avg_write_latency_ms: f64,
}
```

## Future Improvements

### Possible Enhancements
1. **Tiered storage** - Hot data in Sled, cold data in Iroh only
2. **Compression** - Compress large values before storing
3. **Sharding** - Multiple Sled databases for horizontal scaling
4. **Read replicas** - Separate Sled DBs for read-only queries
5. **WAL optimization** - Tune write-ahead log for workload

### Alternative Storage Engines

If Sled doesn't meet needs, consider:
- **redb** - Simpler, more stable embedded DB
- **RocksDB** - Higher performance, more mature
- **SQLite** - Relational queries, broader adoption
- **LMDB** - Very fast reads, memory-mapped

All can work with the current architecture by implementing the `StoragePort` interface.

## Conclusion

The Sled + Iroh architecture provides:
- ✅ **Zero external dependencies** - fully embedded
- ✅ **ACID guarantees** - durable and consistent
- ✅ **Content addressing** - deduplication and integrity
- ✅ **P2P ready** - built-in distribution
- ✅ **Production ready** - crash-safe and performant

Perfect for a decentralized database that needs to be self-contained, portable, and resilient.
