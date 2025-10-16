# Get All Streams Feature

## Overview

The `getAllStream` query allows you to retrieve all stream keys and their entries for a specific database name. This is useful for bulk retrieval and monitoring of all stream data within a namespace.

## Implementation Details

### Storage Layer (`src/storage.rs`)

Added a new method `get_all_streams()` to the `BlobStorage` implementation:

```rust
pub async fn get_all_streams(&self, db_prefix: &str) -> Result<Vec<String>>
```

This method:
1. Reads the storage index
2. Filters keys by database prefix
3. Returns only keys with `StoreType::Stream`
4. Returns the full key names (including prefix)

### GraphQL Layer (`src/graphql.rs`)

Added a new struct `StreamData` to represent stream results:

```rust
pub struct StreamData {
    pub key: String,           // Stream key without db prefix
    pub entries: Vec<StreamEntry>,  // All entries in the stream
}
```

Added a new query method `get_all_stream()`:

```rust
async fn get_all_stream(
    &self,
    ctx: &Context<'_>,
    db_name: String,
) -> Result<Vec<StreamData>, DbError>
```

This query:
1. Retrieves all stream keys for the database
2. For each key, fetches all entries using `xrange`
3. Strips the database prefix from keys
4. Returns organized data with keys and their entries

## GraphQL Query

### Basic Usage

```graphql
query GetAllStreams {
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

### Response Structure

```json
{
  "data": {
    "getAllStream": [
      {
        "key": "sensor_data",
        "entries": [
          {
            "id": "1704067200000-0",
            "fields": [
              {
                "key": "temperature",
                "value": "22.5"
              },
              {
                "key": "humidity",
                "value": "45"
              }
            ]
          }
        ]
      },
      {
        "key": "events",
        "entries": [
          {
            "id": "1704067260000-0",
            "fields": [
              {
                "key": "event_type",
                "value": "login"
              },
              {
                "key": "user_id",
                "value": "user123"
              }
            ]
          }
        ]
      }
    ]
  }
}
```

## Use Cases

### 1. Dashboard Monitoring

Retrieve all streams in a database to display on a monitoring dashboard:

```graphql
query DashboardData {
  getAllStream(dbName: "production") {
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

### 2. Data Export

Export all stream data for backup or analysis:

```graphql
query ExportStreams {
  getAllStream(dbName: "analytics") {
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

### 3. Bulk Processing

Process all streams in a database for aggregation or transformation:

```javascript
const query = `
  query {
    getAllStream(dbName: "sensors") {
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
`;

// Process each stream
const response = await fetch('/graphql', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ query })
});

const data = await response.json();
data.data.getAllStream.forEach(stream => {
  console.log(`Processing stream: ${stream.key}`);
  console.log(`Total entries: ${stream.entries.length}`);
});
```

## Performance Considerations

1. **Memory Usage**: This query loads all entries from all streams into memory. For databases with many streams or large streams, this could be memory-intensive.

2. **Network Transfer**: The response size can be large if there are many streams with many entries.

3. **Use Case**: Best suited for:
   - Small to medium-sized databases
   - Administrative/monitoring tasks
   - Periodic bulk operations
   - Development and testing

4. **Alternatives**: For large databases, consider:
   - Using individual `getStream` queries with pagination
   - Implementing a separate query that returns only stream keys
   - Using `getStreamLength` to check sizes before fetching

## Comparison with Other Queries

| Query | Purpose | Returns |
|-------|---------|---------|
| `getStream` | Get entries from one stream | `Vec<StreamEntry>` |
| `getAllStream` | Get all streams in a database | `Vec<StreamData>` |
| `filterStream` | Filter one stream by pattern | `Vec<StreamEntry>` |
| `getStreamLength` | Get entry count for one stream | `i32` |

## Testing

To test the implementation:

1. Start the node:
```bash
cargo run
```

2. Submit some stream data:
```graphql
mutation {
  submitData(data: {
    dbName: "testdb"
    key: "stream1"
    value: ""
    publicKey: "your_public_key"
    signature: "your_signature"
    storeType: "Stream"
    streamFields: [
      { key: "field1", value: "value1" }
      { key: "field2", value: "value2" }
    ]
  }) {
    success
    message
  }
}
```

3. Query all streams:
```graphql
query {
  getAllStream(dbName: "testdb") {
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

## Future Enhancements

Potential improvements for future versions:

1. **Pagination**: Add `limit` and `offset` parameters
2. **Filtering**: Add pattern matching for stream keys
3. **Sorting**: Add options to sort by key name or entry count
4. **Metadata**: Include entry count and timestamp range per stream
5. **Streaming Response**: Use GraphQL subscriptions for large datasets

## Related Documentation

- [GRAPHQL_EXAMPLES.md](../GRAPHQL_EXAMPLES.md) - Full GraphQL API examples
- [Stream Operations](../README.md#streams) - Overview of stream data type
- [GraphQL API](../src/graphql.rs) - Implementation source code