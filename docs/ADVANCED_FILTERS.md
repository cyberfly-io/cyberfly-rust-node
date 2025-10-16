# Advanced Filters Documentation

## Overview

The Cyberfly Rust Node now includes advanced filtering capabilities that match and extend the TypeScript implementation. These filters provide powerful querying, sorting, pagination, and aggregation across all data types.

## Architecture

### Filter Modules

The filtering system is organized into specialized filters for each data type:

1. **JsonFilter** - Advanced JSON querying with conditions, sorting, and pagination
2. **StreamFilter** - Stream entry filtering and reverse queries
3. **SortedSetFilter** - Score-based and range queries with JSON data
4. **TimeSeriesFilter** - Temporal queries with aggregation
5. **GeospatialFilter** - Location-based queries with distance calculations

### Storage Enhancements

The storage layer has been enhanced to support:

- **Deduplication by `_id`**: Automatically removes old entries with the same `_id` field
- **JSON in SortedSets**: Stores full JSON objects with timestamps as scores
- **Pattern Matching**: SCAN-like key pattern matching with wildcards
- **Reverse Ranges**: xRevRange for getting latest stream entries
- **Multiple Key Queries**: Efficient batch operations across key patterns

## JsonFilter

### Features

- Multi-key pattern matching
- Complex filter conditions (eq, ne, gt, gte, lt, lte, contains, in)
- Nested field access with dot notation
- Sorting by any field (ascending/descending)
- Pagination with offset and limit

### Usage Example

```rust
use crate::filters::{JsonFilter, JsonFilterConditions, FilterCondition, FilterOptions, SortOrder};
use serde_json::json;

// Create filter
let filter = JsonFilter::new(&storage);

// Build conditions
let mut conditions = JsonFilterConditions::new();
conditions.add_condition(
    "age".to_string(),
    FilterCondition::Gte(json!(18))
);
conditions.add_condition(
    "status".to_string(),
    FilterCondition::Eq(json!("active"))
);

// Set options
let options = FilterOptions {
    limit: Some(10),
    offset: Some(0),
    sort_by: Some("name".to_string()),
    sort_order: SortOrder::Asc,
};

// Execute filter
let results = filter.filter_across_keys(
    "users:*",
    &conditions,
    &options
).await?;
```

### Filter Conditions

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

### Nested Field Access

Use dot notation to access nested fields:

```rust
conditions.add_condition(
    "address.city".to_string(),
    FilterCondition::Eq(json!("New York"))
);

conditions.add_condition(
    "profile.preferences.theme".to_string(),
    FilterCondition::Contains("dark".to_string())
);
```

## StreamFilter

### Features

- Range queries (xRange)
- Reverse range queries (xRevRange)
- Last N entries
- Pattern filtering on field values

### Usage Example

```rust
use crate::filters::StreamFilter;

let filter = StreamFilter::new(&storage);

// Get entries in range
let entries = filter.get_entries(
    "mydb",
    "sensor_data",
    "1704067200000-0",
    "1704070800000-0"
).await?;

// Get last 10 entries
let recent = filter.get_last_n_entries(
    "mydb",
    "sensor_data",
    10
).await?;

// Filter by pattern
let filtered = filter.filter_by_pattern(
    "mydb",
    "sensor_data",
    "temperature"
).await?;
```

### Stream Entry Format

```rust
// Each entry is (id, fields)
// id: "timestamp-sequence" (e.g., "1704067200000-0")
// fields: Vec<(key, value)>
let (id, fields) = &entries[0];
println!("Entry ID: {}", id);
for (key, value) in fields {
    println!("  {}: {}", key, value);
}
```

## SortedSetFilter

### Features

- Score-based filtering
- Index-based range queries
- JSON object storage with automatic parsing
- Deduplication by `_id` field

### Usage Example

```rust
use crate::filters::SortedSetFilter;

let filter = SortedSetFilter::new(&storage);

// Get entries by score range
let entries = filter.get_entries_by_score(
    "mydb:events",
    1704067200.0,
    1704070800.0
).await?;

// Get entries by index
let range = filter.get_entries(
    "mydb:events",
    0,   // start
    10   // stop
).await?;

// Process entries
for (json_data, score) in entries {
    println!("Score: {}", score);
    println!("Data: {}", json_data);
    
    // Access fields
    if let Some(event_type) = json_data.get("event_type") {
        println!("Event: {}", event_type);
    }
}
```

### Deduplication

When adding data with an `_id` field, old entries with the same `_id` are automatically removed:

```rust
// Add first entry
storage.add_sorted_set_json(
    "mydb:users",
    1704067200.0,
    r#"{"_id": "user123", "name": "John", "status": "online"}"#
).await?;

// This will replace the previous entry
storage.add_sorted_set_json(
    "mydb:users",
    1704067300.0,
    r#"{"_id": "user123", "name": "John", "status": "offline"}"#
).await?;

// Only the second entry exists now
```

## TimeSeriesFilter

### Features

- Time range queries
- Value filtering (min/max)
- Timestamp filtering
- Aggregation (avg, sum, min, max, count, first, last)
- Time bucketing

### Usage Example

```rust
use crate::filters::{TimeSeriesFilter, TimeSeriesOptions, Aggregation, AggregationType};

let filter = TimeSeriesFilter::new(&storage);

// Basic query
let options = TimeSeriesOptions {
    min_value: Some(0.0),
    max_value: Some(100.0),
    count: Some(1000),
    ..Default::default()
};

let points = filter.query(
    "mydb:temperature",
    1704067200000,  // from timestamp
    1704070800000,  // to timestamp
    &options
).await?;

// With aggregation
let agg_options = TimeSeriesOptions {
    aggregation: Some(Aggregation {
        agg_type: AggregationType::Avg,
        time_bucket: 3600000,  // 1 hour buckets
    }),
    ..Default::default()
};

let aggregated = filter.query(
    "mydb:temperature",
    1704067200000,
    1704070800000,
    &agg_options
).await?;
```

### Aggregation Types

```rust
// Average
AggregationType::Avg

// Sum
AggregationType::Sum

// Minimum
AggregationType::Min

// Maximum
AggregationType::Max

// Count
AggregationType::Count

// First value in bucket
AggregationType::First

// Last value in bucket
AggregationType::Last
```

### Filtering Options

```rust
let options = TimeSeriesOptions {
    // Aggregate data
    aggregation: Some(Aggregation {
        agg_type: AggregationType::Avg,
        time_bucket: 60000,  // 1 minute
    }),
    
    // Limit results
    count: Some(100),
    
    // Filter by specific timestamps
    filter_by_ts: Some(vec![
        1704067200000,
        1704067260000,
        1704067320000,
    ]),
    
    // Filter by value range
    min_value: Some(20.0),
    max_value: Some(30.0),
};
```

## GeospatialFilter

### Features

- Distance calculations between members
- Radius search from coordinates
- Radius search from member location
- Results with coordinates
- Multiple unit support (m, km, mi, ft)

### Usage Example

```rust
use crate::filters::GeospatialFilter;

let filter = GeospatialFilter::new(&storage);

// Get distance between two locations
let distance = filter.get_distance(
    "mydb:locations",
    "office",
    "home",
    "km"
).await?;

// Search within radius
let nearby = filter.search_radius(
    "mydb:locations",
    -122.4194,  // longitude
    37.7749,    // latitude
    5.0,        // radius
    "km"        // unit
).await?;

// Search with coordinates returned
let with_coords = filter.search_radius_with_coords(
    "mydb:locations",
    -122.4194,
    37.7749,
    5.0,
    "km"
).await?;

for (member, lon, lat) in with_coords {
    println!("{}: ({}, {})", member, lon, lat);
}

// Search from member location
let near_office = filter.search_by_member(
    "mydb:locations",
    "office",
    2.0,
    "km"
).await?;
```

### Distance Units

- `"m"` - Meters
- `"km"` - Kilometers
- `"mi"` - Miles
- `"ft"` - Feet

## Storage Enhancements

### Pattern Matching

```rust
// Wildcard matching
let keys = storage.scan_keys("users:*").await?;

// Single character wildcard
let keys = storage.scan_keys("user:?").await?;

// Complex patterns
let keys = storage.scan_keys("db:*:stream:*").await?;
```

### Get Keys by Type

```rust
use crate::storage::StoreType;

// Get all streams
let streams = storage.get_keys_by_type("mydb", StoreType::Stream).await?;

// Get all JSON documents
let jsons = storage.get_keys_by_type("mydb", StoreType::Json).await?;

// Get all time series
let timeseries = storage.get_keys_by_type("mydb", StoreType::TimeSeries).await?;
```

### JSON with _id Deduplication

```rust
// First insert
storage.set_json(
    "mydb:doc1",
    ".",
    r#"{"_id": "doc123", "title": "First", "version": 1}"#
).await?;

// This will remove the old doc1 and create new entry
storage.set_json(
    "mydb:doc2",
    ".",
    r#"{"_id": "doc123", "title": "Updated", "version": 2}"#
).await?;

// Only doc2 exists now (doc1 was removed by _id match)
```

### Reverse Stream Range

```rust
// Get latest entries first
let latest = storage.xrevrange(
    "mydb:events",
    "+",     // start (most recent)
    "-",     // end (oldest)
    Some(10) // count
).await?;

// Get last 5 entries
let last_5 = storage.xrevrange(
    "mydb:logs",
    "+",
    "-",
    Some(5)
).await?;
```

## Performance Considerations

### Batch Processing

```rust
// Process in batches for large datasets
let pattern = "mydb:users:*";
let keys = storage.scan_keys(pattern).await?;

const BATCH_SIZE: usize = 100;
for batch in keys.chunks(BATCH_SIZE) {
    // Process batch
    for key in batch {
        let data = storage.get_json(key, None).await?;
        // Process data...
    }
}
```

### Pagination

```rust
// Paginate through results
let page_size = 20;
for page in 0..10 {
    let options = FilterOptions {
        limit: Some(page_size),
        offset: Some(page * page_size),
        ..Default::default()
    };
    
    let results = filter.filter_across_keys(
        "users:*",
        &conditions,
        &options
    ).await?;
    
    if results.is_empty() {
        break;
    }
    
    // Process page...
}
```

### Aggregation Optimization

```rust
// Use larger time buckets for long time ranges
let options = TimeSeriesOptions {
    aggregation: Some(Aggregation {
        agg_type: AggregationType::Avg,
        time_bucket: 3600000,  // 1 hour for day-long queries
    }),
    ..Default::default()
};

// Or use count to limit results
let options = TimeSeriesOptions {
    count: Some(100),  // Only return 100 points
    ..Default::default()
};
```

## Error Handling

```rust
use anyhow::Result;

async fn query_data() -> Result<()> {
    let filter = JsonFilter::new(&storage);
    
    match filter.filter_across_keys("pattern:*", &conditions, &options).await {
        Ok(results) => {
            println!("Found {} results", results.len());
        }
        Err(e) => {
            eprintln!("Filter error: {}", e);
            return Err(e);
        }
    }
    
    Ok(())
}
```

## Best Practices

1. **Use Specific Patterns**: Narrow your key patterns to reduce scanning
   ```rust
   // Good
   "mydb:users:active:*"
   
   // Less efficient
   "*:*:*"
   ```

2. **Index Important Fields**: Structure your data with filterable fields at the top level
   ```json
   {
     "_id": "user123",
     "status": "active",
     "tier": "premium",
     "details": { ... }
   }
   ```

3. **Use Aggregation**: For time series data, aggregate when possible
   ```rust
   // Instead of 86400 points for a day of second-resolution data
   // Aggregate to hourly (24 points)
   time_bucket: 3600000
   ```

4. **Leverage _id Deduplication**: Use `_id` for natural deduplication
   ```json
   {
     "_id": "sensor:ABC:reading:123",
     "value": 42,
     "timestamp": 1704067200000
   }
   ```

5. **Combine Filters**: Use multiple conditions to narrow results
   ```rust
   conditions.add_condition("status", FilterCondition::Eq(json!("active")));
   conditions.add_condition("age", FilterCondition::Gte(json!(18)));
   conditions.add_condition("country", FilterCondition::In(vec![json!("US"), json!("CA")]));
   ```

## Migration from TypeScript

### Key Differences

1. **Type Safety**: Rust version is strongly typed, TypeScript used `any`
2. **Error Handling**: Uses `Result<T>` instead of exceptions
3. **Async**: Uses Tokio async runtime instead of Node.js
4. **Storage**: Uses Iroh blob storage instead of Redis directly

### Equivalent Operations

| TypeScript | Rust |
|------------|------|
| `filterAcrossKeys()` | `filter.filter_across_keys()` |
| `getLastNEntries()` | `filter.get_last_n_entries()` |
| `geoSearchWith()` | `filter.search_radius_with_coords()` |
| `ts.range()` with aggregation | `filter.query()` with `TimeSeriesOptions` |
| `zRangeWithScores()` | `filter.get_entries()` or `get_entries_by_score()` |

### Example Migration

**TypeScript:**
```typescript
const results = await jsonFilter.filterAcrossKeys(
  'users:*',
  '.',
  { status: 'active', age: { gte: 18 } },
  { limit: 10, sortBy: 'name', sortOrder: 'asc' }
);
```

**Rust:**
```rust
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

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_json_filter() {
        let storage = create_test_storage().await;
        let filter = JsonFilter::new(&storage);
        
        // Add test data
        storage.set_json("test:user1", ".", r#"{"name":"Alice","age":25}"#).await.unwrap();
        storage.set_json("test:user2", ".", r#"{"name":"Bob","age":30}"#).await.unwrap();
        
        // Filter
        let mut conditions = JsonFilterConditions::new();
        conditions.add_condition("age".to_string(), FilterCondition::Gte(json!(25)));
        
        let results = filter.filter_across_keys("test:*", &conditions, &FilterOptions::default()).await.unwrap();
        
        assert_eq!(results.len(), 2);
    }
}
```

## See Also

- [GRAPHQL_EXAMPLES.md](../GRAPHQL_EXAMPLES.md) - GraphQL query examples
- [GET_ALL_STREAM.md](GET_ALL_STREAM.md) - getAllStream feature documentation
- [Storage Implementation](../src/storage.rs) - Storage layer code
- [Filter Implementation](../src/filters.rs) - Filter module code