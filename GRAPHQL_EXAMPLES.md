# GraphQL Query Examples

This document provides examples of GraphQL queries available in the Cyberfly Rust Node API.

## Stream Queries

### Get All Streams for a Database

Retrieves all stream keys and their entries for a specific database.

**Query:**
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

**Response:**
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
          },
          {
            "id": "1704067260000-0",
            "fields": [
              {
                "key": "temperature",
                "value": "23.1"
              },
              {
                "key": "humidity",
                "value": "47"
              }
            ]
          }
        ]
      },
      {
        "key": "user_events",
        "entries": [
          {
            "id": "1704067200000-0",
            "fields": [
              {
                "key": "event",
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

### Get Single Stream

Retrieves entries from a specific stream key within a database.

**Query:**
```graphql
query GetStream {
  getStream(
    dbName: "mydb"
    key: "sensor_data"
    start: "-"
    end: "+"
    count: 10
  ) {
    id
    fields {
      key
      value
    }
  }
}
```

### Filter Stream Entries

Filter stream entries by pattern matching on field values.

**Query:**
```graphql
query FilterStream {
  filterStream(
    dbName: "mydb"
    key: "sensor_data"
    start: "-"
    end: "+"
    pattern: "temperature"
  ) {
    id
    fields {
      key
      value
    }
  }
}
```

### Get Stream Length

Get the total number of entries in a stream.

**Query:**
```graphql
query GetStreamLength {
  getStreamLength(dbName: "mydb", key: "sensor_data")
}
```

**Response:**
```json
{
  "data": {
    "getStreamLength": 42
  }
}
```

## Other Query Examples

### Get Hash Data

```graphql
query GetHash {
  getHash(dbName: "mydb", key: "user:123", field: "name")
}
```

### Get All Hash Fields

```graphql
query GetAllHash {
  getAllHash(dbName: "mydb", key: "user:123") {
    key
    value
  }
}
```

### Get JSON Document

```graphql
query GetJson {
  getJson(dbName: "mydb", key: "config", path: "$.settings")
}
```

### Search Geo Radius

```graphql
query SearchGeoRadius {
  searchGeoRadius(
    dbName: "mydb"
    key: "locations"
    longitude: -122.4194
    latitude: 37.7749
    radius: 5.0
    unit: "km"
  ) {
    member
    longitude
    latitude
  }
}
```

### Get TimeSeries Data

```graphql
query GetTimeSeries {
  getTimeseries(
    dbName: "mydb"
    key: "metrics"
    fromTimestamp: "1704067200000"
    toTimestamp: "1704070800000"
  ) {
    timestamp
    value
  }
}
```

## Mutation Examples

### Submit Signed Data (Stream)

```graphql
mutation SubmitStreamData {
  submitData(
    data: {
      dbName: "mydb"
      key: "sensor_data"
      value: ""
      publicKey: "your_public_key_hex"
      signature: "your_signature_hex"
      storeType: "Stream"
      streamFields: [
        { key: "temperature", value: "22.5" }
        { key: "humidity", value: "45" }
      ]
    }
  ) {
    success
    message
  }
}
```

### Submit JSON Data

```graphql
mutation SubmitJsonData {
  submitData(
    data: {
      dbName: "mydb"
      key: "config"
      value: "{\"setting1\":\"value1\",\"setting2\":\"value2\"}"
      publicKey: "your_public_key_hex"
      signature: "your_signature_hex"
      storeType: "Json"
    }
  ) {
    success
    message
  }
}
```

### Submit Geospatial Data

```graphql
mutation SubmitGeoData {
  submitData(
    data: {
      dbName: "mydb"
      key: "locations"
      value: "office"
      publicKey: "your_public_key_hex"
      signature: "your_signature_hex"
      storeType: "Geo"
      longitude: -122.4194
      latitude: 37.7749
    }
  ) {
    success
    message
  }
}
```

## Subscription Examples

### Subscribe to Topic

```graphql
subscription SubscribeTopic {
  subscribeTopic(topic: "sensor_data") {
    topic
    payload
    timestamp
  }
}
```

### Subscribe to All Messages

```graphql
subscription SubscribeAll {
  subscribeAllMessages {
    topic
    payload
    timestamp
  }
}
```

## Notes

- All queries require proper Ed25519 signature verification for write operations
- The `dbName` parameter is used as a namespace prefix for keys
- Stream IDs follow the format `timestamp-sequence` (e.g., "1704067200000-0")
- Coordinates use WGS84 standard (longitude, latitude)
- Timestamps are in milliseconds since Unix epoch