# Advanced Filtering and Storage Features

## Quick Start

The Cyberfly Rust Node now includes advanced filtering capabilities adapted from the TypeScript implementation, with enhanced type safety and performance.

## New Features

### 1. getAllStream Query

Retrieve all streams for a database in one query:

```graphql
query {
  getAllStream(dbName: "mydb") {
    key
    entries {
      id
      fields {
        key
        value
      }
    }
  }
}
```

### 2. Advanced JSON Filtering

Filter JSON documents across multiple keys with complex conditions:

```rust
use crate::filters::{JsonFilter, JsonFilterConditions, FilterCondition, FilterOptions, SortOrder};

let filter = JsonFilter::new(&storage);
let mut conditions = JsonFilterConditions::new();

// Add conditions
conditions.add_condition("status".to_string(), FilterCondition::Eq(json!("active")));
conditions.add_condition("age".to_string(), FilterCondition::Gte(json!(18)));
conditions.add_condition("profile.verified".to_string(), FilterCondition::Eq(json!(true)));

// Configure options
let options = FilterOptions {
    limit: Some(10),
    offset: Some(0),
    sort_by: Some("name".to_string()),
    sort_order: SortOrder::Asc,
};

// Execute filter
let results = filter.filter_across_keys("users:*", &conditions, &options).await?;
```

### 3. Stream Filtering

Query stream data with range and pattern filters:

```rust
use crate::filters::StreamFilter;

let filter = StreamFilter::new(&storage);

// Get last 10 entries
let recent = filter.get_last_n_entries("mydb", "sensor_data", 10).await?;

// Get entries in range
let range = filter.get_entries("mydb", "logs", "1704067200000-0", "+").await?;

// Filter by pattern
let filtered = filter.filter_by_pattern("mydb", "events", "temperature").await?;
```

### 4. Time Series with Aggregation

Query time series data with powerful aggregation:

```rust
use crate::filters::{TimeSeriesFilter, TimeSeriesOptions, Aggregation, AggregationType};

let filter = TimeSeriesFilter::new(&storage);
let options = TimeSeriesOptions {
    aggregation: Some(Aggregation {
        agg_type: AggregationType::Avg,
        time_bucket: 3600000,  // 1 hour buckets
    }),
    min_value: Some(0.0),
    max_value: Some(100.0),
    count: Some(24),  // Last 24 hours
    ..Default::default()
};

let hourly_avg = filter.query("mydb:cpu_usage", from_ts, to_ts, &options).await?;
```

### 5. Geospatial Queries

Search locations with radius and distance calculations:

```rust
use crate::filters::GeospatialFilter;

let filter = GeospatialFilter::new(&storage);

// Search within radius with coordinates
let nearby = filter.search_radius_with_coords(
    "mydb:locations",
    -122.4194,  // longitude
    37.7749,    // latitude
    5.0,        // radius
    "km"        // unit
).await?;

// Get distance between locations
let distance = filter.get_distance("mydb:locations", "office", "home", "km").await?;
```

### 6. Deduplication by _id

Automatic deduplication for JSON and SortedSet:

```rust
// First insert
storage.set_json("mydb:doc1", ".", r#"{"_id": "user123", "name": "John"}"#).await?;

// This automatically removes doc1 (same _id)
storage.set_json("mydb:doc2", ".", r#"{"_id": "user123", "name": "John Updated"}"#).await?;

// Only doc2 exists now
```

### 7. Pattern-Based Key Scanning

Scan keys with wildcards:

```rust
// Wildcard matching
let keys = storage.scan_keys("users:*").await?;

// Multiple wildcards
let keys = storage.scan_keys("db:*:stream:*").await?;

// Get keys by type
let streams = storage.get_keys_by_type("mydb", StoreType::Stream).await?;
```

## Filter Conditions

All supported filter conditions:

```rust
// Equality
FilterCondition::Eq(json!("value"))

// Not equal
FilterCondition::Ne(json!("value"))

// Greater than
FilterCondition::Gt(json!(100))

// Greater than or equal
FilterCondition::Gte(json!(18))

// Less than
FilterCondition::Lt(json!(1000))

// Less than or equal
FilterCondition::Lte(json!(99))

// Contains (string)
FilterCondition::Contains("search_term".to_string())

// In array
FilterCondition::In(vec![json!("value1"), json!("value2")])
```

## Aggregation Types

Time series aggregation options:

```rust
AggregationType::Avg    // Average
AggregationType::Sum    // Sum
AggregationType::Min    // Minimum
AggregationType::Max    // Maximum
AggregationType::Count  // Count
AggregationType::First  // First value in bucket
AggregationType::Last   // Last value in bucket
```

## Storage Enhancements

### SortedSet with JSON

Store full JSON objects in sorted sets:

```rust
// Add JSON object with score (timestamp)
storage.add_sorted_set_json(
    "mydb:events",
    1704067200.0,
    r#"{"_id": "event123", "type": "login", "user": "john"}"#
).await?;

// Retrieve with automatic JSON parsing
let entries = storage.get_sorted_set_json("mydb:events", 0, -1).await?;
for entry in entries {
    println!("Score: {}", entry.score);
    println!("Data: {}", entry.data);
}
```

### Reverse Stream Range

Get latest stream entries first:

```rust
// Get last 5 entries (newest first)
let latest = storage.xrevrange("mydb:logs", "+", "-", Some(5)).await?;
```

## Documentation

Comprehensive documentation available:

- **[GRAPHQL_EXAMPLES.md](GRAPHQL_EXAMPLES.md)** - Complete GraphQL query examples
- **[docs/GET_ALL_STREAM.md](docs/GET_ALL_STREAM.md)** - getAllStream feature documentation
- **[docs/ADVANCED_FILTERS.md](docs/ADVANCED_FILTERS.md)** - Complete filter system guide
- **[docs/STORAGE_IMPROVEMENTS.md](docs/STORAGE_IMPROVEMENTS.md)** - Storage enhancement details
- **[IMPLEMENTATION_SUMMARY.md](IMPLEMENTATION_SUMMARY.md)** - Implementation overview

## Performance Tips

1. **Use specific patterns**: `"mydb:users:*"` instead of `"*"`
2. **Enable pagination**: Always set `limit` for large datasets
3. **Aggregate time series**: Use larger time buckets for long ranges
4. **Batch operations**: Process multiple keys together
5. **Use type-specific queries**: `get_keys_by_type()` instead of scanning all

## Examples

### Complex User Query

```rust
let filter = JsonFilter::new(&storage);
let mut conditions = JsonFilterConditions::new();

// Active users, age 18+, premium tier, verified
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

let users = filter.filter_across_keys("users:*", &conditions, &options).await?;
```

### Sensor Data Aggregation

```rust
let filter = TimeSeriesFilter::new(&storage);
let options = TimeSeriesOptions {
    aggregation: Some(Aggregation {
        agg_type: AggregationType::Avg,
        time_bucket: 3600000,  // Hourly average
    }),
    min_value: Some(0.0),    // Filter out negative values
    max_value: Some(100.0),  // Filter out outliers
    count: Some(168),        // Last week (168 hours)
    ..Default::default()
};

let weekly_avg = filter.query(
    "sensors:temperature",
    start_timestamp,
    end_timestamp,
    &options
).await?;
```

### Location-Based Search

```rust
let filter = GeospatialFilter::new(&storage);

// Find all stores within 5km of user location
let nearby_stores = filter.search_radius_with_coords(
    "stores:locations",
    user_longitude,
    user_latitude,
    5.0,
    "km"
).await?;

for (store_id, lon, lat) in nearby_stores {
    println!("Store: {} at ({}, {})", store_id, lon, lat);
}
```

## Building and Running

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

## Feature Parity

All features from the TypeScript implementation are now available:

- ✅ JSON filtering with conditions
- ✅ Nested field access
- ✅ Sorting and pagination
- ✅ Stream xRange and xRevRange
- ✅ SortedSet with JSON storage
- ✅ _id deduplication
- ✅ Time series aggregation
- ✅ Geospatial queries
- ✅ Pattern-based key scanning

## Advantages Over TypeScript

1. **Type Safety**: Compile-time type checking
2. **Performance**: Zero-cost abstractions, no GC pauses
3. **Memory Safety**: No memory leaks or buffer overflows
4. **Concurrency**: Safe concurrent access with Arc<RwLock<>>
5. **Error Handling**: Explicit Result<T> types

## Contributing

When adding new filters or storage methods:

1. Add implementation to appropriate module
2. Add tests
3. Update documentation
4. Add examples to this README
5. Ensure clean compilation with `cargo check`

## License

See main project LICENSE file.