# Decentralized Database

A peer-to-peer database built with Rust featuring Iroh networking, CRDT sync, GraphQL API, and Ed25519 signature verification.

## Features

- **🔐 Cryptographic Security** - Ed25519 signatures for all data operations
- **🗄️ Embedded Storage** - Sled + Iroh Blobs (no external databases needed)
- **🔄 CRDT Sync** - Automatic conflict resolution with Automerge
- **🌐 P2P Network** - Iroh-based with automatic peer discovery
- **📡 GraphQL API** - Full CRUD with subscriptions
- **🤖 IoT Ready** - MQTT bridge for sensor integration
- **📊 Redis-Compatible** - String, Hash, List, Set, SortedSet, JSON, Stream, TimeSeries, Geo

## Quick Start

```bash
# Build
cargo build --release

# Run
cargo run --release

# Access GraphQL Playground
open http://127.0.0.1:8080
```

## Configuration

```bash
export API_HOST="127.0.0.1"
export API_PORT="8080"
export BOOTSTRAP_PEERS="NodeId@ip:port"  # Optional: connect to existing network
export MQTT_BROKER_HOST="localhost"       # Optional: for IoT integration
```

## Basic Usage

### Store Data (GraphQL)

```graphql
mutation {
  submitData(input: {
    dbName: "myapp-<your_public_key_hex>"
    key: "user:123"
    value: "John Doe"
    publicKey: "<hex_encoded_public_key>"
    signature: "<hex_encoded_signature>"
    storeType: "String"
  }) {
    success
    message
  }
}
```

### Query Data

```graphql
query {
  getString(dbName: "myapp-<public_key>", key: "user:123") {
    key
    value
  }
}
```

### Supported Store Types

| Type | Use Case |
|------|----------|
| `String` | Simple key-value |
| `Hash` | Object properties |
| `List` | Ordered collections |
| `Set` | Unique items |
| `SortedSet` | Ranked items |
| `Json` | Nested documents |
| `Stream` | Event logs |
| `TimeSeries` | Metrics/sensors |
| `Geo` | Location data |

## Architecture

```
Client (signs data) → GraphQL API → Signature Verify → Storage (Sled/Iroh) → CRDT Sync → P2P Network
```

## Documentation

| Doc | Description |
|-----|-------------|
| [GRAPHQL_EXAMPLES.md](./GRAPHQL_EXAMPLES.md) | Query examples |
| [EXAMPLES.md](./EXAMPLES.md) | Usage patterns |
| [docs/INDEX.md](./docs/INDEX.md) | Full documentation |

## Development

```bash
cargo test              # Run tests
cargo build --release   # Build optimized binary
RUST_LOG=debug cargo run  # Run with debug logging
```

## License

MIT License

