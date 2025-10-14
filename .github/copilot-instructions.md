# Decentralized Database Project

## Project Overview
This is a decentralized database built with Rust featuring:
- **Redis support** for all store types (basic + advanced)
- **libp2p** for peer-to-peer networking
- **CRDT merge** for conflict-free data synchronization
- **GraphQL API** for data submission with filtering
- **Ed25519 signature verification** for secure data storage
- **Advanced filtering** for all data types

## Architecture
- Users submit signed data via GraphQL API
- Nodes verify Ed25519 signatures before storing
- Data is stored in Redis with support for:
  - **Basic types**: String, Hash, List, Set, Sorted Set
  - **JSON**: Document storage with JSONPath queries (requires RedisJSON)
  - **Streams**: Event sourcing and log data (Redis 6.0+)
  - **TimeSeries**: Metric and sensor data (RedisTimeSeries or Sorted Set fallback)
  - **Geospatial**: Location data with radius search (Redis 6.0+)
- CRDT algorithms merge data across distributed nodes
- libp2p handles peer discovery and data propagation
- Advanced filtering on all data types via GraphQL

## Data Type Support

### Basic Types
- **String**: Key-value storage
- **Hash**: Nested key-value pairs
- **List**: Ordered collections
- **Set**: Unique unordered collections
- **Sorted Set**: Scored ordered collections

### Advanced Types
- **JSON**: Full JSON document storage with JSONPath filtering
- **Stream**: Redis Streams for event sourcing (xadd, xrange, xread)
- **TimeSeries**: Time-stamped numeric data with range queries
- **Geospatial**: Location data with georadius, geodist, geopos operations

### Filtering
- Pattern matching (strings, hash fields, list values, set members)
- Range queries (scores, timestamps, values)
- JSONPath queries for nested JSON
- Geospatial radius search
- Stream pattern filtering

## Development Guidelines
- Use async/await with Tokio runtime
- Implement proper error handling with Result types
- Follow Rust naming conventions (snake_case for functions/variables)
- Add unit tests for core functionality
- Document public APIs with rustdoc comments

## Key Dependencies
- `tokio`: Async runtime
- `libp2p`: P2P networking
- `redis`: Redis client
- `async-graphql`: GraphQL server
- `ed25519-dalek`: Signature verification
- `automerge`: CRDT implementation
- `serde`: Serialization/deserialization

## Running the Project
```bash
cargo build --release
cargo run
```
