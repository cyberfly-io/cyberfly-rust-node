# Storage and Filter Improvements

## Overview

This document summarizes the enhancements made to the Cyberfly Rust Node storage and filtering system to match and extend the TypeScript implementation's capabilities.

## Summary of Changes

### 1. Enhanced Storage Layer (`src/storage.rs`)

#### New Features

**Deduplication Support**
- Automatic deduplication by `_id` field for JSON documents
- Automatic deduplication by `_id` field for SortedSet entries
- Prevents duplicate data while maintaining update semantics

**SortedSet JSON Storage**
- `add_sorted_set_json()` - Store full JSON objects with scores
- `get_sorted_set_json()` - Retrieve parsed JSON objects with scores
- `delete_sorted_set_by_id()` - Internal deduplication helper
- `SortedSetEntry` struct with public `score` and `data` fields

**JSON Document Management**
- Enhanced `set_json()` with automatic `_id` deduplication
- `delete_json_by_id()` - Remove old documents with same `_id`
- `JsonValue` struct now tracks `_id` field

**Stream Enhancements**
- `xrevrange()` - Get stream entries in reverse order (latest first)
- Supports count parameter for limiting results
- Compatible with TypeScript `xRevRange` functionality

**Key Scanning and Filtering**
- `scan_keys()` - Pattern-based key scanning with wildcards
- `get_keys_by_type()` - Get all keys for a specific StoreType
- `get_all_streams()` - Get all stream keys for a database
- Regex-based pattern matching (* and ? wildcards)

#### Data Structure Changes

```rust
// Enhanced SortedSetEntry (now public)
pub struct SortedSetEntry {
    pub score: f64,
    pub data: serde_json::Value,
}

// Enhanced JsonValue (tracks _id)
struct JsonValue {
    data: serde_json::Value,
    id: Option<String>,  // New field for deduplication
}

// Enhanced SortedSetValue
struct SortedSetValue {
    members: BTreeMap<String, f64>,  // Stores JSON strings with scores
}
```

### 2. New Filter Module (`src/filters.rs`)

Comprehensive filtering system with five specialized filter types:

#### JsonFilter

**Capabilities:**
- Multi-key pattern matching across database
- Complex filter conditions (eq, ne, gt, gte, lt, lte, contains, in)
- Nested field access using dot notation
- Sorting by any field (ascending/descending)
- Pagination with offset and limit

**Key Methods:**
- `filter_across_keys()` - Main filtering method
- `matches_conditions()` - Condition evaluation
- `get_nested_field()` - Dot notation field access
- `compare_values()` - Sorting comparator

**Example:**
```rust
let filter = JsonFilter::new(&storage);
let mut conditions = JsonFilterConditions::new();
conditions.add_condition("status".to_string(), FilterCondition::Eq(json!("active")));
conditions.add_condition("age".to_string(), FilterCondition::Gte(json!(18)));

let options = FilterOptions {
    limit: Some(10),
    offset: Some(0),
    sort_by: Some("name".to_string()),
    sort_order: SortOrder::Asc,
};

let results = filter.filter_across_keys("users:*", &conditions, &options).await?;
```

#### StreamFilter

**Capabilities:**
- Range queries (xRange)
- Reverse range queries (xRevRange)
- Last N entries retrieval
- Pattern-based field filtering

**Key Methods:**
- `get_entries()` - Get entries in range
- `get_last_n_entries()` - Get most recent entries
- `filter_by_pattern()` - Filter by field pattern

**Example:**
```rust
let filter = StreamFilter::new(&storage);
let last_10 = filter.get_last_n_entries("mydb", "sensor_data", 10).await?;
```

#### SortedSetFilter

**Capabilities:**
- Score-based range filtering
- Index-based range queries
- Automatic JSON parsing
- Support for _id deduplication

**Key Methods:**
- `get_entries_by_score()` - Filter by score range
- `get_entries()` - Get by index range

**Example:**
```rust
let filter = SortedSetFilter::new(&storage);
let entries = filter.get_entries_by_score(
    "mydb:events",
    1704067200.0,
    1704070800.0
).await?;
```

#### TimeSeriesFilter

**Capabilities:**
- Time range queries
- Value filtering (min/max)
- Timestamp filtering
- Multiple aggregation types (avg, sum, min, max, count, first, last)
- Time bucketing for aggregation

**Key Methods:**
- `query()` - Main query method with options
- `aggregate_points()` - Apply aggregation

**Example:**
```rust
let filter = TimeSeriesFilter::new(&storage);
let options = TimeSeriesOptions {
    aggregation: Some(Aggregation {
        agg_type: AggregationType::Avg,
        time_bucket: 3600000,  // 1 hour
    }),
    min_value: Some(0.0),
    max_value: Some(100.0),
    ..Default::default()
};
let points = filter.query("mydb:temp", from_ts, to_ts, &options).await?;
```

#### GeospatialFilter

**Capabilities:**
- Distance calculations between members
- Radius search from coordinates
- Radius search from member location
- Results with or without coordinates
- Multiple unit support (m, km, mi, ft)

**Key Methods:**
- `get_distance()` - Calculate distance
- `search_radius()` - Search by coordinates
- `search_radius_with_coords()` - Search with coordinate results
- `search_by_member()` - Search from member location
- `search_by_member_with_coords()` - Search from member with coords

**Example:**
```rust
let filter = GeospatialFilter::new(&storage);
let nearby = filter.search_radius_with_coords(
    "mydb:locations",
    -122.4194,
    37.7749,
    5.0,
    "km"
).await?;
```

### 3. GraphQL API Enhancements (`src/graphql.rs`)

#### New Queries

**getAllStream**
- Retrieves all stream keys and entries for a database
- Returns `Vec<StreamData>` with key and entries
- Automatically strips database prefix from keys

**New Types:**
```rust
pub struct StreamData {
    pub key: String,
    pub entries: Vec<StreamEntry>,
}

pub struct SortedSetEntry {
    pub score: f64,
    pub data: serde_json::Value,
}
```

### 4. Dependencies Added

**Cargo.toml:**
```toml
regex = "1.10"  # For pattern matching in scan_keys
```

### 5. Documentation

#### New Documentation Files

1. **GRAPHQL_EXAMPLES.md** - Comprehensive GraphQL query examples
   - Stream queries (getAllStream, getStream, filterStream, getStreamLength)
   - Other data type queries
   - Mutation examples
   - Subscription examples

2. **docs/GET_ALL_STREAM.md** - Detailed getAllStream documentation
   - Implementation details
   - Usage examples
   - Use cases
   - Performance considerations
   - Testing instructions
   - Future enhancements

3. **docs/ADVANCED_FILTERS.md** - Complete filter documentation
   - Architecture overview
   - Usage examples for all filter types
   - Performance considerations
   - Best practices
   - Migration guide from TypeScript
   - Testing examples

## Comparison with TypeScript Implementation

### Feature Parity

| Feature | TypeScript | Rust | Status |
|---------|-----------|------|--------|
| JSON filtering with conditions | ✅ | ✅ | ✅ Complete |
| Sorting and pagination | ✅ | ✅ | ✅ Complete |
| Stream xRange | ✅ | ✅ | ✅ Complete |
| Stream xRevRange | ✅ | ✅ | ✅ Complete |
| SortedSet with JSON | ✅ | ✅ | ✅ Complete |
| _id deduplication | ✅ | ✅ | ✅ Complete |
| TimeSeries aggregation | ✅ | ✅ | ✅ Complete |
| Geo radius search | ✅ | ✅ | ✅ Complete |
| Pattern-based key scanning | ✅ | ✅ | ✅ Complete |

### Improvements Over TypeScript

1. **Type Safety**
   - Rust's strong type system prevents runtime errors
   - Compile-time guarantees for data structures
   - No `any` types

2. **Performance**
   - Zero-cost abstractions
   - No garbage collection pauses
   - Efficient memory usage

3. **Error Handling**
   - Result<T> types for explicit error handling
   - No silent failures or uncaught exceptions
   - Comprehensive error propagation

4. **Concurrency**
   - Safe concurrent access with Arc<RwLock<>>
   - No race conditions
   - Tokio async runtime for efficient I/O

5. **Storage Layer**
   - Iroh blob storage for content-addressed data
   - Built-in deduplication at storage level
   - Efficient caching layer

## Usage Examples

### Pattern Matching

```rust
// Wildcard matching
let keys = storage.scan_keys("users:*").await?;

// Multiple wildcards
let keys = storage.scan_keys("db:*:stream:*").await?;

// Single character wildcard
let keys = storage.scan_keys("user:?").await?;
```

### Deduplication in Action

```rust
// First insert
storage.set_json(
    "mydb:doc1",
    ".",
    r#"{"_id": "doc123", "title": "First", "version": 1}"#
).await?;

// Second insert with same _id (removes doc1)
storage.set_json(
    "mydb:doc2",
    ".",
    r#"{"_id": "doc123", "title": "Updated", "version": 2}"#
).await?;

// Only doc2 exists - doc1 was automatically removed
```

### Complex JSON Filtering

```rust
let filter = JsonFilter::new(&storage);

let mut conditions = JsonFilterConditions::new();
conditions.add_condition("status".to_string(), FilterCondition::Eq(json!("active")));
conditions.add_condition("age".to_string(), FilterCondition::Gte(json!(18)));
conditions.add_condition("tier".to_string(), FilterCondition::In(vec![
    json!("premium"),
    json!("enterprise")
]));
conditions.add_condition("profile.verified".to_string(), FilterCondition::Eq(json!(true)));

let options = FilterOptions {
    limit: Some(20),
    offset: Some(0),
    sort_by: Some("created_at".to_string()),
    sort_order: SortOrder::Desc,
};

let results = filter.filter_across_keys("users:*", &conditions, &options).await?;
```

### Time Series with Aggregation

```rust
let filter = TimeSeriesFilter::new(&storage);

let options = TimeSeriesOptions {
    aggregation: Some(Aggregation {
        agg_type: AggregationType::Avg,
        time_bucket: 3600000,  // 1 hour buckets
    }),
    count: Some(24),  // Last 24 hours
    min_value: Some(0.0),
    max_value: Some(100.0),
    ..Default::default()
};

let hourly_avg = filter.query(
    "mydb:cpu_usage",
    start_ts,
    end_ts,
    &options
).await?;
```

## Performance Considerations

### Memory Usage

- Filters process data in batches where possible
- Pagination helps limit memory consumption
- Caching layer reduces redundant storage reads

### Optimization Tips

1. **Use specific patterns**: `"mydb:users:*"` vs `"*:*:*"`
2. **Enable pagination**: Always set `limit` for large datasets
3. **Aggregate time series**: Use larger time buckets for long ranges
4. **Index by type**: Use `get_keys_by_type()` instead of scanning all keys
5. **Batch operations**: Process multiple keys in batches

## Testing

Build and test:

```bash
# Check compilation
cargo check

# Build release
cargo build --release

# Run with logging
RUST_LOG=debug cargo run
```

## Future Enhancements

1. **GraphQL Integration**: Add filter parameters to existing queries
2. **Streaming Results**: Implement pagination for GraphQL subscriptions
3. **Query Optimization**: Add query planning and optimization
4. **Indexing**: Create secondary indexes for common queries
5. **Caching**: Implement query result caching
6. **Metrics**: Add performance metrics and monitoring

## Migration Guide

For users migrating from the TypeScript implementation:

### Code Changes

**TypeScript:**
```typescript
const filter = new RedisJSONFilter(redis);
const results = await filter.filterAcrossKeys(
    'users:*',
    '.',
    { status: 'active' },
    { limit: 10 }
);
```

**Rust:**
```rust
let filter = JsonFilter::new(&storage);
let mut conditions = JsonFilterConditions::new();
conditions.add_condition("status".to_string(), FilterCondition::Eq(json!("active")));

let options = FilterOptions {
    limit: Some(10),
    ..Default::default()
};

let results = filter.filter_across_keys("users:*", &conditions, &options).await?;
```

### Key Differences

1. Conditions are built explicitly, not passed as objects
2. Error handling uses `Result<T>` instead of exceptions
3. Async/await uses Tokio instead of Node.js
4. Storage layer is different (Iroh vs Redis)

## Conclusion

The Rust implementation now has feature parity with the TypeScript version while providing:

- ✅ Better type safety
- ✅ Improved performance
- ✅ Enhanced error handling
- ✅ Safer concurrency
- ✅ More flexible storage layer

All filtering capabilities from the TypeScript implementation are now available in Rust with additional improvements and optimizations.