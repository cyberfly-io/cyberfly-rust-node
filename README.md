# Decentralized Database

A decentralized, peer-to-peer database built with Rust featuring embedded Sled storage, Iroh networking, CRDT-based conflict resolution, GraphQL API, and Ed25519 signature verification.

## Features

- **🔐 Ed25519 Signature Verification**: All data submissions must be cryptographically signed
- **🗝️ Database Naming with Public Key**: Databases are named as `<name>-<public_key_hex>` and verified on every operation
- **⚙️ Resource Management**: Explicit resource limits with automatic enforcement
  - **Semaphore-based Concurrency**: Configurable operation and connection limits
  - **RAII Guards**: Automatic cleanup on scope exit
  - **Real-time Monitoring**: Track utilization and pressure metrics
  - **Default Limits**: 1000 concurrent operations, 100 peer connections, 4GB memory
- **📊 State Management**: Centralized state coordination (single source of truth)
  - **Thread-safe State**: Arc<RwLock> for concurrent access
  - **Node Lifecycle**: Initialize → Running → Syncing → UnderPressure → ShuttingDown
  - **Peer Tracking**: Connection status, sync timestamps, operation counts
  - **Database Stats**: Entry counts, modification times, size tracking
  - **Immutable Snapshots**: Point-in-time state for observability
- **🗄️ Embedded Storage**: Dual-storage architecture with no external dependencies
  - **Sled**: Embedded B-tree database for fast key lookups and indexing
  - **Iroh Blobs**: Content-addressed storage with Blake3 hashing for immutable data
  - **Zero Dependencies**: No Redis or external databases required
  - **ACID Guarantees**: Durable, consistent, crash-safe storage
  - **Redis-Compatible Types**: String, Hash, List, Set, Sorted Set, JSON, Stream, TimeSeries, Geo
- **🔍 Advanced Filtering**: Filter data by patterns, ranges, JSONPath, time windows, and geographical radius
- **🔄 CRDT Sync with Persistence**: Conflict-free replicated data types using Automerge for distributed consensus
  - **Persistent Operation Log**: Operations stored in Iroh content-addressed blobs
  - **Survives Restarts**: Full sync works even after node restarts
  - **Immutable History**: Content-addressed storage with Blake3 hashing
  - **Distributed by Default**: IPFS-like decentralized storage
  - **LWW Conflict Resolution**: Last-Write-Wins with timestamp and operation ID ordering
- **📦 IPFS Integration**: File storage using Iroh (modern Rust IPFS implementation)
  - Store files with structured metadata (filename, size, MIME type, tags, description)
  - Two-CID system: separate content and metadata storage
  - **🔐 File Ownership Security**: Ed25519-based cryptographic ownership verification
  - Secure file deletion - only owners can delete their files
  - Signature-protected metadata updates
  - JSON storage for arbitrary data structures
  - Automatic MIME type detection
  - Searchable tags and descriptions
- **🌐 P2P Network**: Iroh-based networking with enhanced connectivity
  - **Transports**: QUIC support with efficient data transfer
  - **Gossip Protocol**: Decentralized message propagation
  - **Blob Storage**: Content-addressed storage for files and operation logs
  - **Automatic Peer Discovery**: Gossip-based full mesh topology (broadcasts every 10s)
  - **Self-Healing Network**: Nodes automatically discover and connect to all peers
  - **Connection Health**: Built-in connection monitoring
- **🤖 IoT Integration**: MQTT bridge for bidirectional communication with IoT devices
  - **Cross-machine MQTT broadcasting** - Messages propagate to all connected peers via gossip
  - Iroh Gossip ↔️ MQTT topic mapping with full topic preservation
  - **Full wildcard topic support** (`+` for single-level, `#` for multi-level)
  - **MQTT client ID automatically derived from Iroh node ID** for consistent node identification
  - Publish to IoT devices via GraphQL API
  - Subscribe to IoT sensor data and telemetry
  - Configurable QoS levels and topic patterns
  - Smart loop prevention - no message duplication
- **📡 GraphQL API**: Easy-to-use GraphQL interface for data submission, queries, and IoT control
- **⚡ Async Runtime**: Built on Tokio for high-performance async operations

## Architecture

```
┌─────────────┐         GraphQL API          ┌──────────────┐
│   Client    │────────────────────────────▶│     Node     │
│ (Signs data)│                              │              │
└─────────────┘                              │  ┌────────┐  │
                                             │  │Signature│  │
                                             │  │Verify   │  │
                                             │  └────┬───┘  │
                                             │       │      │
                                             │  ┌────▼───┐  │
                                             │  │  Sled  │  │
                                             │  │ Index  │  │
                                             │  └────┬───┘  │
                                             │       │      │
                                             │  ┌────▼───┐  │
                                             │  │  Iroh  │  │
                                             │  │ Blobs  │  │
                                             │  └────┬───┘  │
                                             │       │      │
                                             │  ┌────▼───┐  │
                                             │  │ CRDT   │  │
                                             │  │ Merge  │  │
                                             │  └────┬───┘  │
                                             │       │      │
                                             │  ┌────▼───┐  │
                                             │  │  Iroh  │  │
                                             │  │ P2P    │  │
                                             │  └────────┘  │
                                             └──────┬───────┘
                                                    │
                                          ┌─────────┴─────────┐
                                          │                   │
                                     ┌────▼─────┐      ┌─────▼────┐
                                     │  Node 2  │      │  Node 3  │
                                     └──────────┘      └──────────┘
```

## Prerequisites

- **Rust** (1.70+): [Install Rust](https://www.rust-lang.org/tools/install)
- **MQTT Broker** (Optional, for IoT integration): [Mosquitto](https://mosquitto.org/)

**That's it!** No external databases required. The node uses:
- **Sled** for embedded key-value storage (included)
- **Iroh** for content-addressed blob storage (included)

Both are fully embedded and require no separate installation or configuration.

## Installation

1. **Clone the repository**:
   ```bash
   git clone <repository-url>
   cd cyberfly-rust-node
   ```

2. **Build the project**:
   ```bash
   cargo build --release
   ```

3. **(Optional) Start MQTT broker for IoT integration**:
   ```bash
   mosquitto -c mosquitto.conf
   ```

3. **Build the project**:
   ```bash
   cargo build --release
   ```

## Configuration

Configure the node using environment variables:

```bash
# Redis connection
export REDIS_URL="redis://127.0.0.1:6379"

# API server
export API_HOST="127.0.0.1"
export API_PORT="8080"

# Iroh Networking (uses fixed port 11204 by default)
# Bootstrap peers format: "NodeId@ip:port,NodeId2@ip2:port2"
export BOOTSTRAP_PEERS="a8317a71f552fc1512c29dd8a743d0912bde10d18e858c8fced724bb9efb4581@10.48.44.105:11204"

# Note: Default port is 11204 for IPv4, 11205 for IPv6
# See PORT_CONFIGURATION.md for details on changing the default port

# MQTT/IoT Integration (enabled by default)
# Set MQTT_ENABLED="false" to disable MQTT bridge
export MQTT_BROKER_HOST="localhost"
export MQTT_BROKER_PORT="1883"
# MQTT_CLIENT_ID is optional - defaults to "cyberfly-{node_id}" using Iroh node ID
# export MQTT_CLIENT_ID="cyberfly-node-custom"
export MQTT_SUBSCRIPTIONS="iot/sensors/#,iot/actuators/#,iot/telemetry/#"
```

## Running

Start the node:

```bash
cargo run --release
```

### GraphQL API & Documentation

The project includes **GraphiQL**, a powerful interactive GraphQL IDE:

**Access GraphiQL:**
- Primary: `http://127.0.0.1:8080` (GraphiQL - recommended)
- Alternative: `http://127.0.0.1:8080/playground` (GraphQL Playground - legacy)
- Schema SDL: `http://127.0.0.1:8080/schema.graphql` (download schema)

**📚 Documentation & Query Templates:**
- **[GRAPHQL_USAGE.md](./GRAPHQL_USAGE.md)** - Complete guide to accessing the GraphQL API
- **[GRAPHQL_QUERY_TEMPLATES.md](./GRAPHQL_QUERY_TEMPLATES.md)** - Copy-paste ready query templates for all operations

**Quick Start:**
1. Open GraphiQL at `http://127.0.0.1:8080`
2. Browse available operations in the **Docs** panel (top-right corner)
3. **Copy queries from [GRAPHQL_QUERY_TEMPLATES.md](./GRAPHQL_QUERY_TEMPLATES.md)** and paste into the editor
4. Replace `YOUR_PUBLIC_KEY` and `YOUR_SIGNATURE_HEX` with actual values
5. Press `Ctrl+Enter` (or click ▶️) to execute

**GraphiQL Features:**
- **🎯 Auto-completion** - Press `Ctrl+Space` for field suggestions
- **📚 Schema Docs** - Click "Docs" to browse all queries/mutations/subscriptions
- **✨ Syntax Highlighting** - Instant error detection
- **🔌 WebSocket Support** - Test subscriptions in real-time

**⚠️ Note:** The Docs panel shows schema documentation, but queries cannot be clicked to auto-insert. Instead, **copy templates from [GRAPHQL_QUERY_TEMPLATES.md](./GRAPHQL_QUERY_TEMPLATES.md)**.

## Supported Data Types

### Basic Redis Types

| Type | Description | Use Cases |
|------|-------------|-----------|
| **String** | Simple key-value storage | Configuration, flags, counters |
| **Hash** | Key-value pairs within a key | User profiles, object properties |
| **List** | Ordered list of strings | Activity logs, queues, timelines |
| **Set** | Unordered collection of unique strings | Tags, unique visitors, relationships |
| **Sorted Set** | Set with scores for ordering | Leaderboards, priority queues, rankings |

### Advanced Types

| Type | Description | Use Cases | Requirements |
|------|-------------|-----------|--------------|
| **JSON** | JSON documents with JSONPath queries | Complex nested data, API responses, documents | RedisJSON module (optional) |
| **Stream** | Append-only log with consumer groups | Event sourcing, audit logs, message queues | Redis 6.0+ (built-in) |
| **TimeSeries** | Time-stamped numeric data | Metrics, sensor data, analytics | RedisTimeSeries (optional, falls back to Sorted Set) |
| **Geospatial** | Location data with radius search | Location tracking, proximity search, maps | Redis 6.0+ (built-in) |

### Filtering Capabilities

Each data type supports advanced filtering:

- **Pattern Matching**: Filter by field names, values, or member patterns
- **Range Queries**: Filter by score ranges (Sorted Set), value ranges (TimeSeries)
- **JSONPath**: Query nested JSON structures with JSONPath expressions
- **Time Windows**: Filter time-series data by timestamp ranges
- **Geo Radius**: Find locations within a radius from a point or member
- **Stream Patterns**: Filter stream entries by field value patterns

## Usage

### GraphQL API

#### Database Naming Convention

Databases are named using the format: `<name>-<public_key_hex>`

This ensures:
- Each user can only write to their own database
- Database ownership is cryptographically verified
- No conflicts between users with similar names

#### Submit Signed Data

To store data, you must sign it with an Ed25519 private key and provide a database name that includes your public key:

```graphql
mutation {
  submitData(input: {
    dbName: "myapp-8f4b3c2a1e5d7f9b..."  # <name>-<public_key_hex>
    key: "user:123"
    value: "John Doe"
    publicKey: "8f4b3c2a1e5d7f9b..." # hex encoded
    signature: "a7b3d8e9f2c4..." # hex encoded (sign: dbName:key:value)
    storeType: "String"
  }) {
    success
    message
  }
}
```

**Important**: The signature must be computed over the message: `dbName:key:value`

#### Store Types

**String**:
```graphql
mutation {
  submitData(input: {
    dbName: "myapp-8f4b3c..."
    key: "config:app"
    value: "production"
    publicKey: "8f4b3c..."
    signature: "..."
    storeType: "String"
  }) {
    success
    message
  }
}
```

**Hash**:
```graphql
mutation {
  submitData(input: {
    dbName: "myapp-8f4b3c..."
    key: "user:123"
    field: "name"
    value: "John Doe"
    publicKey: "8f4b3c..."
    signature: "..."
    storeType: "Hash"
  }) {
    success
    message
  }
}
```

**List**:
```graphql
mutation {
  submitData(input: {
    dbName: "myapp-8f4b3c..."
    key: "events:log"
    value: "User logged in"
    publicKey: "8f4b3c..."
    signature: "..."
    storeType: "List"
  }) {
    success
    message
  }
}
```

**Set**:
```graphql
mutation {
  submitData(input: {
    dbName: "myapp-8f4b3c..."
    key: "tags:article"
    value: "rust"
    publicKey: "8f4b3c..."
    signature: "..."
    storeType: "Set"
  }) {
    success
    message
  }
}
```

**Sorted Set**:
```graphql
mutation {
  submitData(input: {
    dbName: "myapp-8f4b3c..."
    key: "leaderboard"
    value: "player1"
    score: 1500.0
    publicKey: "8f4b3c..."
    signature: "..."
    storeType: "SortedSet"
  }) {
    success
    message
  }
}
```

**JSON**:
```graphql
mutation {
  submitData(input: {
    dbName: "myapp-8f4b3c..."
    key: "product:1001"
    value: "{\"name\":\"Laptop\",\"price\":999.99,\"stock\":50}"
    jsonPath: "$"
    publicKey: "8f4b3c..."
    signature: "..."
    storeType: "Json"
  }) {
    success
    message
  }
}
```

**Stream** (Event Sourcing):
```graphql
mutation {
  submitData(input: {
    dbName: "myapp-8f4b3c..."
    key: "events:orders"
    value: "order-123"
    streamFields: "[{\"key\":\"action\",\"value\":\"created\"},{\"key\":\"amount\",\"value\":\"299.99\"}]"
    publicKey: "8f4b3c..."
    signature: "..."
    storeType: "Stream"
  }) {
    success
    message
  }
}
```

**TimeSeries**:
```graphql
mutation {
  submitData(input: {
    dbName: "myapp-8f4b3c..."
    key: "metrics:cpu"
    value: "75.5"
    timestamp: "1697040000"
    publicKey: "8f4b3c..."
    signature: "..."
    storeType: "TimeSeries"
  }) {
    success
    message
  }
}
```

**Geospatial**:
```graphql
mutation {
  submitData(input: {
    dbName: "myapp-8f4b3c..."
    key: "locations:stores"
    value: "store-downtown"
    longitude: -122.4194
    latitude: 37.7749
    publicKey: "8f4b3c..."
    signature: "..."
    storeType: "Geo"
  }) {
    success
    message
  }
}
```

#### Query Data

**Get String**:
```graphql
query {
  getString(dbName: "myapp-8f4b3c...", key: "config:app") {
    key
    value
  }
}
```

**Get Hash Field**:
```graphql
query {
  getHash(dbName: "myapp-8f4b3c...", key: "user:123", field: "name") {
    key
    value
  }
}
```

**Get All Hash Fields**:
```graphql
query {
  getAllHash(dbName: "myapp-8f4b3c...", key: "user:123") {
    key
    value
  }
}
```

**Get List**:
```graphql
query {
  getList(dbName: "myapp-8f4b3c...", key: "events:log", start: 0, stop: 10) 
}
```

**Get Set**:
```graphql
query {
  getSet(dbName: "myapp-8f4b3c...", key: "tags:article")
}
```

**Get Sorted Set**:
```graphql
query {
  getSortedSet(dbName: "myapp-8f4b3c...", key: "leaderboard", start: 0, stop: 10)
}
```

**Get JSON Document**:
```graphql
query {
  getJson(dbName: "myapp-8f4b3c...", key: "product:1001", path: "$") {
    key
    value
  }
}
```

**Get Stream Entries**:
```graphql
query {
  getStream(
    dbName: "myapp-8f4b3c..."
    key: "events:orders"
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

**Get Stream Length**:
```graphql
query {
  getStreamLength(dbName: "myapp-8f4b3c...", key: "events:orders")
}
```

**Get TimeSeries Data**:
```graphql
query {
  getTimeseries(
    dbName: "myapp-8f4b3c..."
    key: "metrics:cpu"
    fromTimestamp: "1697040000"
    toTimestamp: "1697050000"
  ) {
    timestamp
    value
  }
}
```

**Get Latest TimeSeries Value**:
```graphql
query {
  getLatestTimeseries(dbName: "myapp-8f4b3c...", key: "metrics:cpu") {
    timestamp
    value
  }
}
```

**Get Geo Location**:
```graphql
query {
  getGeoLocation(dbName: "myapp-8f4b3c...", key: "locations:stores", member: "store-downtown") {
    member
    longitude
    latitude
  }
}
```

**Search by Geo Radius**:
```graphql
query {
  searchGeoRadius(
    dbName: "myapp-8f4b3c..."
    key: "locations:stores"
    longitude: -122.4194
    latitude: 37.7749
    radius: 5000
    unit: "m"
  )
}
```

**Get Geo Distance**:
```graphql
query {
  getGeoDistance(
    dbName: "myapp-8f4b3c..."
    key: "locations:stores"
    member1: "store-downtown"
    member2: "store-suburb"
    unit: "km"
  )
}
```

#### Advanced Filtering

**Filter Hash by Field Pattern**:
```graphql
query {
  filterHash(dbName: "myapp-8f4b3c...", key: "user:123", fieldPattern: "addr") {
    key
    value
  }
}
```

**Filter List by Value Pattern**:
```graphql
query {
  filterList(dbName: "myapp-8f4b3c...", key: "events:log", valuePattern: "error")
}
```

**Filter Set by Member Pattern**:
```graphql
query {
  filterSet(dbName: "myapp-8f4b3c...", key: "tags:article", memberPattern: "rust")
}
```

**Filter Sorted Set by Score Range**:
```graphql
query {
  filterSortedSet(
    dbName: "myapp-8f4b3c..."
    key: "leaderboard"
    minScore: 1000.0
    maxScore: 2000.0
  )
}
```

**Filter JSON by JSONPath**:
```graphql
query {
  filterJson(
    dbName: "myapp-8f4b3c..."
    key: "product:1001"
    jsonPath: "$.price"
  ) {
    key
    value
  }
}
```

**Filter Stream by Pattern**:
```graphql
query {
  filterStream(
    dbName: "myapp-8f4b3c..."
    key: "events:orders"
    start: "-"
    end: "+"
    pattern: "created"
  ) {
    id
    fields {
      key
      value
    }
  }
}
```

**Filter TimeSeries by Value Range**:
```graphql
query {
  filterTimeseries(
    dbName: "myapp-8f4b3c..."
    key: "metrics:cpu"
    fromTimestamp: "1697040000"
    toTimestamp: "1697050000"
    minValue: 70.0
    maxValue: 90.0
  ) {
    timestamp
    value
  }
}
```

**Search Geo by Radius from Member**:
```graphql
query {
  searchGeoRadiusByMember(
    dbName: "myapp-8f4b3c..."
    key: "locations:stores"
    member: "store-downtown"
    radius: 10
    unit: "km"
  )
}
```

#### IPFS Operations

**Upload to IPFS**:
```graphql
mutation {
  addToIpfs(data: "Hello, IPFS!") {
    success
    cid
    message
  }
}
```

**Pin CID**:
```graphql
mutation {
  pinIpfs(cid: "QmXXXXXXX") {
    success
    cid
    message
  }
}
```

**Unpin CID**:
```graphql
mutation {
  unpinIpfs(cid: "QmXXXXXXX") {
    success
    cid
    message
  }
}
```

**Get File from IPFS**:
```graphql
query {
  getIpfsFile(cid: "QmXXXXXXX")  # Returns base64 encoded data
}
```

**List Pinned CIDs**:
```graphql
query {
  listIpfsPins
}
```

### Generating Ed25519 Keys

Example using Rust:

```rust
use ed25519_dalek::{SigningKey, Signer};
use rand::rngs::OsRng;

let mut csprng = OsRng;
let signing_key = SigningKey::generate(&mut csprng);
let verifying_key = signing_key.verifying_key();

let message = b"user:123:John Doe";
let signature = signing_key.sign(message);

println!("Public Key: {}", hex::encode(verifying_key.as_bytes()));
println!("Signature: {}", hex::encode(signature.to_bytes()));
```

## Development

### Project Structure

```
src/
├── main.rs              # Application entry point
├── config.rs            # Configuration management
├── crypto.rs            # Ed25519 signature verification
├── storage.rs           # Sled + Iroh storage (Redis-like API)
├── crdt.rs              # CRDT merge logic using Automerge
├── iroh_network.rs      # Iroh-based P2P networking
├── ipfs.rs              # IPFS/Iroh file storage
├── graphql.rs           # GraphQL API schema and resolvers
├── mqtt_bridge.rs       # MQTT ↔ Gossip bridge
├── sync.rs              # Sync manager with CRDT operations
├── metrics.rs           # Prometheus metrics
├── resource_manager.rs  # Resource limits and concurrency control
├── state_manager.rs     # Centralized state coordination
├── error_context.rs     # Enhanced error types with context
├── error.rs             # Error types
└── filters.rs           # Advanced filtering (JSON, Geo, TimeSeries)
```

### Resource Management

ResourceManager and AppState are initialized in `main.rs` for centralized resource control:

```rust
// Initialize ResourceManager with default limits
let resource_manager = std::sync::Arc::new(
    cyberfly_rust_node::resource_manager::ResourceManager::new(
        cyberfly_rust_node::resource_manager::ResourceLimits::default(),
    )
);

// Initialize AppState (single source of truth)
let app_state = std::sync::Arc::new(
    cyberfly_rust_node::state_manager::AppState::new()
);
```

**Default Resource Limits:**
- Max concurrent operations: 1000
- Max peer connections: 100
- Max memory: 4GB
- Max database size: 10GB
- Max cache entries: 100,000
- Max value size: 10MB

**Usage in resolvers (future work):**
```rust
// Acquire operation slot with RAII guard
let _guard = resource_manager.acquire_operation_slot().await?;

// Update node state
app_state.set_node_state(NodeState::Running).await;

// Track peer connections
app_state.add_peer(peer_id, peer_state).await;

// Get snapshot for health checks
let snapshot = app_state.snapshot().await;
```

### Running Tests

```bash
cargo test
cargo test --test integration_tests  # Resource management integration tests
```

### Logging

Set log level with `RUST_LOG`:

```bash
RUST_LOG=debug cargo run
```

## Network Topology

The network uses Iroh's modern P2P stack with automatic peer discovery:

- **Gossip Protocol**: Four dedicated topics for different message types:
  - `data`: Application data propagation
  - `discovery`: Peer discovery beacons
  - `sync`: CRDT synchronization messages
  - `peer-discovery`: **Automatic full mesh connectivity** (NEW!)
- **Peer Discovery Protocol**: Every 10 seconds, nodes broadcast their connected peer lists
  - Enables automatic full mesh topology without manual configuration
  - Self-healing: new nodes quickly discover all existing peers
  - Region-aware announcements for potential optimization
  - See [PEER_DISCOVERY_PROTOCOL.md](PEER_DISCOVERY_PROTOCOL.md) for details
- **QUIC Transport**: Fast, reliable connections with built-in encryption
- **Content Addressing**: Blake3-based hashing for immutable data storage
- **Bootstrap Peers**: Initial connection points for joining the network

### Testing Full Mesh Peer Discovery

Run the automated test script to see peer discovery in action:

```bash
./test-peer-discovery.sh
```

This starts 3 nodes:
1. Node 1 acts as bootstrap
2. Node 2 connects to Node 1
3. Node 3 connects to Node 1
4. Within 10 seconds, Node 2 and Node 3 discover and connect to each other automatically
5. Result: Full mesh topology (all nodes connected to all other nodes)

## Security

- All data submissions require valid Ed25519 signatures
- Signatures are verified before data is stored
- Message format for signing: `key:value`
- Public keys and signatures must be hex-encoded

## CRDT Conflict Resolution

The system uses Automerge for automatic conflict resolution when the same key is updated by multiple nodes. Changes are merged using CRDT algorithms ensuring eventual consistency across the network.

## Additional Documentation

- **[PEER_DISCOVERY_PROTOCOL.md](PEER_DISCOVERY_PROTOCOL.md)**: Automatic full mesh peer discovery protocol
- **[EXAMPLES.md](EXAMPLES.md)**: Comprehensive usage examples for all data types and IoT integration
- **[IPFS_METADATA.md](IPFS_METADATA.md)**: Detailed guide for IPFS file metadata storage with helia-json
- **[FILE_OWNERSHIP.md](FILE_OWNERSHIP.md)**: Complete guide to file ownership verification and secure deletion
- **[LOOP_PREVENTION.md](LOOP_PREVENTION.md)**: Architecture and implementation of MQTT-libp2p loop prevention
- **[MQTT_PEER_ID.md](MQTT_PEER_ID.md)**: How libp2p peer IDs are used as MQTT client IDs for consistent node identification
- **[MQTT_WILDCARDS.md](MQTT_WILDCARDS.md)**: Complete guide to MQTT wildcard topic subscriptions (`+` and `#`)

## Roadmap

- [x] Basic Redis data types (String, Hash, List, Set, Sorted Set)
- [x] JSON document storage with JSONPath filtering
- [x] Redis Streams for event sourcing
- [x] TimeSeries data support
- [x] Geospatial location data with radius search
- [x] Advanced filtering for all data types
- [x] IPFS file storage with metadata
- [x] File ownership verification with Ed25519 signatures
- [x] Secure file deletion with cryptographic proof
- [x] MQTT bridge for IoT integration
- [x] Three-layer loop prevention system
- [x] Automatic peer discovery with full mesh topology
- [ ] Add authentication for GraphQL API
- [ ] GraphQL mutations for file upload/deletion
- [ ] Implement data replication policies using CRDT
- [ ] Add metrics and monitoring (Prometheus/Grafana)
- [ ] Support for more CRDT types
- [ ] Persistent peer storage for libp2p
- [ ] Complete rust-helia integration when crates mature
- [ ] Web UI for node management
- [ ] GraphQL subscriptions for real-time updates
- [ ] Multi-region data synchronization
- [ ] Multi-signature file deletion
- [ ] File sharing with access control lists

## Troubleshooting

### Common Issues

#### "peer doesn't support any known protocol" Warning
This warning is **normal and harmless**. It occurs when incompatible clients try to connect to your node. Your node properly rejects these connections. See `PROTOCOL_WARNING_EXPLANATION.md` for details.

#### Empty Peer Discovery Results
If `getDiscoveredPeers` returns empty:
- ✅ Your node is working correctly
- ⚠️ You need at least 2 nodes running to discover peers
- ⚠️ Configure `BOOTSTRAP_PEERS` environment variable
- See `EMPTY_PEERS_EXPLANATION.md` for complete guide
- Run `./test-bootstrap.sh` for automated 2-node testing

#### Bootstrap Peer Connection Issues
- Verify bootstrap peer is running and reachable
- Check NodeId format (64-character hex string)
- Confirm port is correct (usually 11204 for Iroh)
- See `BOOTSTRAP_PEERS.md` for configuration guide

### Documentation

- `PROTOCOL_WARNING_EXPLANATION.md` - Understanding Iroh protocol warnings
- `EMPTY_PEERS_EXPLANATION.md` - Why single nodes show no peers
- `BOOTSTRAP_PEERS.md` - Bootstrap peer configuration guide
- `PEER_DISCOVERY_GUIDE.md` - Peer discovery architecture
- `GRAPHIQL_GUIDE.md` - GraphQL API usage
- `NODE_INFO_QUERIES.md` - Node monitoring queries
- `EXAMPLES.md` - Example queries and operations

### Testing Peer Discovery

```bash
# Automated 2-node test
./test-bootstrap.sh

# Manual test with bootstrap peer
export BOOTSTRAP_PEERS="<node_id>@127.0.0.1:11204"
cargo run --release
```

### Log Control

```bash
# Hide protocol warnings
export RUST_LOG="cyberfly_rust_node=info,iroh::protocol=error"
cargo run --release

# Debug mode
export RUST_LOG="debug"
cargo run --release
```

## Contributing

Contributions are welcome! Please open an issue or submit a pull request.

## License

MIT License - see LICENSE file for details

## Support

For questions or issues, please open a GitHub issue.
