# Cyberfly Rust Node - Feature Documentation Index

## Overview

This index provides quick navigation to all documentation for the advanced storage and filtering features implemented in the Cyberfly Rust Node.

## Quick Links

### Core Documentation

- **[README_FILTERS.md](../README_FILTERS.md)** - Quick start guide for new filtering features
- **[IMPLEMENTATION_SUMMARY.md](../IMPLEMENTATION_SUMMARY.md)** - Complete implementation overview
- **[GRAPHQL_EXAMPLES.md](../GRAPHQL_EXAMPLES.md)** - GraphQL API query examples

### Detailed Guides

- **[ADVANCED_FILTERS.md](ADVANCED_FILTERS.md)** - Comprehensive filtering system documentation
- **[STORAGE_IMPROVEMENTS.md](STORAGE_IMPROVEMENTS.md)** - Storage layer enhancements
- **[GET_ALL_STREAM.md](GET_ALL_STREAM.md)** - getAllStream feature documentation
- **[NETWORK_RESILIENCE.md](NETWORK_RESILIENCE.md)** - Circuit breaker, reputation, bandwidth throttling
- **[INDEXING.md](INDEXING.md)** - Secondary indexing system
- **[SIGNATURE_AND_FILTERS.md](SIGNATURE_AND_FILTERS.md)** - Signature verification and filters

## Features by Category

### Network Resilience

**Documentation:** [NETWORK_RESILIENCE.md](NETWORK_RESILIENCE.md)

**Features:**
- Circuit Breaker - Prevents hammering failing peers
- Peer Reputation - Tracks peer reliability over time
- Bandwidth Throttling - Rate limits network traffic
- Automatic peer banning and recovery

**Quick Example:**
```graphql
query {
  getNetworkResilienceSummary {
    circuitBreaker { totalPeers closed open halfOpen }
    reputation { totalPeers bannedPeers avgScore avgReliability }
    bandwidth { totalUploaded totalDownloaded }
  }
}
```

### JSON Filtering

**Documentation:** [ADVANCED_FILTERS.md](ADVANCED_FILTERS.md#jsonfilter)

**Features:**
- Multi-key pattern matching
- Complex filter conditions (eq, ne, gt, gte, lt, lte, contains, in)
- Nested field access with dot notation
- Sorting (ascending/descending)
- Pagination (offset/limit)

**Quick Example:**
```rust
let filter = JsonFilter::new(&storage);
let mut conditions = JsonFilterConditions::new();
conditions.add_condition("status".to_string(), FilterCondition::Eq(json!("active")));
conditions.add_condition("age".to_string(), FilterCondition::Gte(json!(18)));

let options = FilterOptions {
    limit: Some(10),
    sort_by: Some("name".to_string()),
    sort_order: SortOrder::Asc,
    ..Default::default()
};

let results = filter.filter_across_keys("users:*", &conditions, &options).await?;
```

### Stream Filtering

**Documentation:** 
- [ADVANCED_FILTERS.md](ADVANCED_FILTERS.md#streamfilter)
- [GET_ALL_STREAM.md](GET_ALL_STREAM.md)

**Features:**
- Range queries (xRange)
- Reverse range queries (xRevRange)
- Last N entries retrieval
- Pattern-based filtering
- getAllStream GraphQL query

**Quick Example:**
```rust
let filter = StreamFilter::new(&storage);
let last_10 = filter.get_last_n_entries("mydb", "sensor_data", 10).await?;
```

**GraphQL Example:**
```graphql
query {
  getAllStream(dbName: "mydb") {
    key
    entries {
      id
      fields { key value }
    }
  }
}
```

### SortedSet with JSON

**Documentation:** [ADVANCED_FILTERS.md](ADVANCED_FILTERS.md#sortedsetfilter)

**Features:**
- Store full JSON objects with scores
- Automatic JSON parsing
- Score-based range queries
- Index-based range queries
- Deduplication by `_id` field

**Quick Example:**
```rust
storage.add_sorted_set_json(
    "mydb:events",
    1704067200.0,
    r#"{"_id": "event123", "type": "login"}"#
).await?;

let filter = SortedSetFilter::new(&storage);
let entries = filter.get_entries_by_score("mydb:events", min, max).await?;
```

### Time Series

**Documentation:** [ADVANCED_FILTERS.md](ADVANCED_FILTERS.md#timeseriesfilter)

**Features:**
- Time range queries
- Value filtering (min/max)
- Timestamp filtering
- Aggregation types: avg, sum, min, max, count, first, last
- Time bucketing

**Quick Example:**
```rust
let filter = TimeSeriesFilter::new(&storage);
let options = TimeSeriesOptions {
    aggregation: Some(Aggregation {
        agg_type: AggregationType::Avg,
        time_bucket: 3600000,  // 1 hour
    }),
    ..Default::default()
};
let data = filter.query("mydb:cpu", from_ts, to_ts, &options).await?;
```

### Geospatial

**Documentation:** [ADVANCED_FILTERS.md](ADVANCED_FILTERS.md#geospatialfilter)

**Features:**
- Distance calculations between members
- Radius search from coordinates
- Radius search from member location
- Results with coordinates
- Multiple unit support (m, km, mi, ft)

**Quick Example:**
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

### Storage Enhancements

**Documentation:** [STORAGE_IMPROVEMENTS.md](STORAGE_IMPROVEMENTS.md)

**Features:**
- Deduplication by `_id` field (JSON and SortedSet)
- Pattern-based key scanning with wildcards
- Get keys by store type
- Reverse stream range (xRevRange)
- JSON in SortedSet with automatic parsing

**Quick Examples:**
```rust
// Pattern matching
let keys = storage.scan_keys("users:*").await?;

// Get all streams
let streams = storage.get_keys_by_type("mydb", StoreType::Stream).await?;

// Reverse range
let latest = storage.xrevrange("mydb:logs", "+", "-", Some(10)).await?;

// Deduplication
storage.set_json("key1", ".", r#"{"_id": "doc123", "data": "v1"}"#).await?;
storage.set_json("key2", ".", r#"{"_id": "doc123", "data": "v2"}"#).await?;
// key1 is automatically removed
```

## GraphQL API

**Documentation:** [GRAPHQL_EXAMPLES.md](../GRAPHQL_EXAMPLES.md)

### New Queries

#### getAllStream
```graphql
query {
  getAllStream(dbName: "mydb") {
    key
    entries {
      id
      fields { key value }
    }
  }
}
```

### Existing Queries (Enhanced)

- `getStream` - Get stream with range
- `filterStream` - Filter stream by pattern
- `getStreamLength` - Get entry count
- `getJson` - Get JSON document
- `filterJson` - Filter JSON with conditions
- `getTimeseries` - Get time series data
- `filterTimeseries` - Filter with value range
- `searchGeoRadius` - Search locations by radius
- `searchGeoRadiusByMember` - Search from member location

## Implementation Details

### Source Files

- **`src/storage.rs`** - Enhanced storage layer
  - New methods: `add_sorted_set_json`, `get_sorted_set_json`, `xrevrange`, `scan_keys`, `get_keys_by_type`, `get_all_streams`
  - Deduplication: `delete_json_by_id`, `delete_sorted_set_by_id`

- **`src/filters.rs`** - New filter module
  - `JsonFilter` - JSON filtering
  - `StreamFilter` - Stream queries
  - `SortedSetFilter` - SortedSet queries
  - `TimeSeriesFilter` - Time series with aggregation
  - `GeospatialFilter` - Location queries

- **`src/graphql.rs`** - GraphQL API
  - New query: `get_all_stream`
  - New type: `StreamData`
  - Enhanced: `SortedSetEntry` (public fields)

- **`src/main.rs`** - Module registration
  - Added `mod filters;`

### Data Structures

```rust
// SortedSet entry with JSON
pub struct SortedSetEntry {
    pub score: f64,
    pub data: serde_json::Value,
}

// Stream data for getAllStream
pub struct StreamData {
    pub key: String,
    pub entries: Vec<StreamEntry>,
}

// Filter conditions
pub enum FilterCondition {
    Eq(JsonValue),
    Ne(JsonValue),
    Gt(JsonValue),
    Gte(JsonValue),
    Lt(JsonValue),
    Lte(JsonValue),
    Contains(String),
    In(Vec<JsonValue>),
}

// Aggregation types
pub enum AggregationType {
    Avg, Sum, Min, Max, Count, First, Last,
}
```

## Migration from TypeScript

**Documentation:** [ADVANCED_FILTERS.md](ADVANCED_FILTERS.md#migration-from-typescript)

### Key Differences

1. **Type Safety**: Strongly typed vs `any`
2. **Error Handling**: `Result<T>` vs exceptions
3. **Async**: Tokio vs Node.js
4. **Storage**: Iroh blobs vs Redis directly

### Equivalent Operations

| TypeScript | Rust |
|------------|------|
| `filterAcrossKeys()` | `filter.filter_across_keys()` |
| `getLastNEntries()` | `filter.get_last_n_entries()` |
| `geoSearchWith()` | `filter.search_radius_with_coords()` |
| `ts.range()` with agg | `filter.query()` with options |
| `zRangeWithScores()` | `filter.get_entries_by_score()` |

## Performance

**Documentation:** [ADVANCED_FILTERS.md](ADVANCED_FILTERS.md#performance-considerations)

### Best Practices

1. **Use specific patterns**: `"mydb:users:*"` not `"*"`
2. **Enable pagination**: Always set `limit` for large datasets
3. **Aggregate time series**: Use larger buckets for long ranges
4. **Use type queries**: `get_keys_by_type()` vs scanning all
5. **Batch operations**: Process multiple keys together

### Optimization Examples

```rust
// Good: Specific pattern with pagination
let options = FilterOptions {
    limit: Some(20),
    offset: Some(page * 20),
    ..Default::default()
};
let results = filter.filter_across_keys("users:active:*", &conditions, &options).await?;

// Good: Aggregated time series
let options = TimeSeriesOptions {
    aggregation: Some(Aggregation {
        agg_type: AggregationType::Avg,
        time_bucket: 3600000,  // Hourly for day-long queries
    }),
    ..Default::default()
};
```

## Testing

### Build and Run

```bash
# Check compilation
cargo check

# Build release
cargo build --release

# Run node
cargo run

# With debug logging
RUST_LOG=debug cargo run
```

### Test Filters

```rust
#[tokio::test]
async fn test_json_filter() {
    let storage = create_test_storage().await;
    let filter = JsonFilter::new(&storage);
    
    // Add test data
    storage.set_json("test:user1", ".", r#"{"name":"Alice","age":25}"#).await.unwrap();
    
    // Filter
    let mut conditions = JsonFilterConditions::new();
    conditions.add_condition("age".to_string(), FilterCondition::Gte(json!(25)));
    
    let results = filter.filter_across_keys("test:*", &conditions, &FilterOptions::default()).await.unwrap();
    assert_eq!(results.len(), 1);
}
```

## Common Use Cases

### Dashboard Monitoring
- Use `getAllStream` to fetch all streams
- Use aggregated time series for metrics
- Use geospatial queries for location tracking

### Data Export
- Use pattern scanning to find keys
- Use filters with pagination for large datasets
- Use JSON filtering for selective export

### Real-time Analytics
- Use stream filtering with patterns
- Use time series with aggregation
- Use sorted sets for leaderboards

### Search and Discovery
- Use JSON filters with multiple conditions
- Use geospatial radius search
- Use pattern matching for key discovery

## Support

### Documentation

- Read the guides in `docs/` directory
- Check examples in `GRAPHQL_EXAMPLES.md`
- Review implementation in source files

### Issues

- Check compilation errors with `cargo check`
- Enable debug logging with `RUST_LOG=debug`
- Review error messages for detailed information

## Changelog

### v0.1.0 - Advanced Filtering Implementation

**Added:**
- JsonFilter with complex conditions and pagination
- StreamFilter with xRevRange support
- SortedSetFilter with JSON storage
- TimeSeriesFilter with aggregation
- GeospatialFilter with radius search
- getAllStream GraphQL query
- Pattern-based key scanning
- Deduplication by `_id` field
- Comprehensive documentation suite

**Enhanced:**
- Storage layer with new methods
- SortedSet to support JSON objects
- JSON documents with `_id` tracking
- Stream queries with reverse range

**Documentation:**
- GRAPHQL_EXAMPLES.md
- docs/GET_ALL_STREAM.md
- docs/ADVANCED_FILTERS.md
- docs/STORAGE_IMPROVEMENTS.md
- IMPLEMENTATION_SUMMARY.md
- README_FILTERS.md

## Future Roadmap

1. **GraphQL Integration**: Add filter parameters to all queries
2. **Query Optimization**: Implement query planning and caching
3. **Secondary Indexes**: Create indexes for common patterns
4. **Metrics**: Add performance monitoring
5. **Streaming Results**: Implement cursor-based pagination

## License

See main project LICENSE file.

---

**Last Updated**: 2024
**Version**: 0.1.0
**Status**: Production Ready ✅