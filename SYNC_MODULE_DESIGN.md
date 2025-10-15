# Sync Module Design and Future Integration

## Current Implementation (Working)

The current sync module uses a proven architecture with:

1. **Iroh Blobs** for persistent storage
2. **Automerge** for CRDT conflict resolution
3. **Custom SignedOperation** struct for Ed25519 verification
4. **Last-Write-Wins (LWW)** merge semantics
5. **Iroh Gossip** for peer-to-peer message propagation

### Architecture

```
GraphQL API
    ↓
SignedOperation (verify signature)
    ↓
SyncStore (LWW + Automerge CRDT)
    ↓
Iroh Blobs (persistent storage)
    ↓
Iroh Gossip (P2P replication)
```

### Key Features

- **Signature Verification**: All operations are Ed25519-signed
- **Conflict Resolution**: LWW using timestamp + op_id tiebreaker
- **Persistence**: Operations stored as blobs with index
- **Replication**: Automatic via gossip protocol
- **Deduplication**: Applied operations tracked to prevent re-application

## Future: Iroh-Docs Integration

### Why Migrate to Iroh-Docs?

iroh-docs (version 0.93+) is a purpose-built CRDT document synchronization layer that provides:

1. **Native CRDT Support**: Built-in conflict resolution without Automerge
2. **Automatic Sync**: Handles replication automatically
3. **Document Model**: Key-value entries with built-in versioning
4. **Author Signing**: Native support for multiple authors
5. **Share Tickets**: Easy peer discovery and document sharing

### Migration Strategy

When iroh-docs API stabilizes (likely v1.0), migrate using this approach:

#### Phase 1: API Research
- Study iroh-docs 0.93+ API documentation
- Review examples in `n0-computer/iroh-docs` repository
- Test basic operations: create, insert, query, subscribe

#### Phase 2: Parallel Implementation
- Keep existing sync module working
- Create `sync_v2.rs` with iroh-docs implementation
- Add feature flag: `use-iroh-docs`
- Run both systems in parallel for testing

#### Phase 3: Data Model Mapping

Map current `SignedOperation` to iroh-docs entries:

```rust
// Current format
SignedOperation {
    op_id: String,
    timestamp: i64,
    db_name: String,
    key: String,
    value: String,
    // ... other fields
}

// iroh-docs format (estimated based on v0.93)
Entry {
    key: Vec<u8>,        // encode: "{db_name}:{key}:{field}"
    author: AuthorId,    // from public_key
    timestamp: u64,      // from timestamp
    content_hash: Hash,  // stores serialized SignedOperation
}
```

#### Phase 4: Key Methods to Implement

Based on iroh-docs patterns, implement these operations:

```rust
// Create/load document
async fn init_docs(
    store: Store,
    endpoint: Endpoint,
    blobs: BlobsProtocol,
    gossip: Gossip,
) -> Result<Docs> {
    let docs = Docs::new(store)
        .spawn(endpoint, blobs, gossip)
        .await?;
    Ok(docs)
}

// Insert operation
async fn insert_operation(
    docs: &Docs,
    author: &Author,
    namespace: NamespaceId,
    op: &SignedOperation,
) -> Result<()> {
    // Serialize operation
    let key = format!("{}:{}:{}", op.db_name, op.key, 
        op.field.as_deref().unwrap_or("")).into_bytes();
    let value = serde_json::to_vec(op)?;
    
    // Insert into document (API TBD - need to verify actual method)
    // Possible APIs based on docs:
    // - docs.set(namespace, author, key, value).await?;
    // - docs.insert(namespace, author, key, value).await?;
    // The exact method needs verification from source code
    
    Ok(())
}

// Query operations
async fn get_operations(
    docs: &Docs,
    namespace: NamespaceId,
) -> Result<Vec<SignedOperation>> {
    // Query all entries (API TBD)
    // Possible APIs:
    // - docs.query(namespace, Query::all()).await?;
    // - docs.get_all(namespace).await?;
    
    let mut operations = Vec::new();
    // for entry in entries {
    //     let op = serde_json::from_slice(entry.content())?;
    //     operations.push(op);
    // }
    Ok(operations)
}

// Subscribe to changes
async fn subscribe_changes(
    doc: &Doc,
) -> Result<impl Stream<Item = Result<LiveEvent>>> {
    // Subscribe to document events
    doc.subscribe().await
}

// Share document
async fn share_document(
    doc: &Doc,
    mode: ShareMode,
    addr_options: AddrInfoOptions,
) -> Result<DocTicket> {
    // Create share ticket for peer sync
    doc.share(mode, addr_options).await
}
```

### Verified API (iroh-docs 0.93)

After researching the actual source code, here are the **confirmed** API methods:

#### Creating Docs Protocol
```rust
use iroh_docs::protocol::Docs;

// In-memory storage
let docs = Docs::memory()
    .spawn(endpoint, blobs_store, gossip)
    .await?;

// Persistent storage
let docs = Docs::persistent(path_to_dir)
    .spawn(endpoint, blobs_store, gossip)
    .await?;

// Get API handle
let docs_api = docs.api(); // or use Deref: docs.create() works too
```

#### Author Management
```rust
// Get or create default author
let author_id = docs_api.author_default().await?;

// Export author (contains private key!)
let author = docs_api.author_export(author_id).await?;

// Create new author
let new_author_id = docs_api.author_create().await?;
```

#### Document Operations
```rust
// Create a new document
let doc: Doc = docs_api.create().await?;

// Open existing document
let doc: Option<Doc> = docs_api.open(namespace_id).await?;

// Import from ticket
let doc: Doc = docs_api.import(ticket).await?;

// Get document ID
let id = doc.id();
```

#### Reading/Writing Data
```rust
use bytes::Bytes;

// Write data
let hash = doc.set_bytes(
    author_id,
    Bytes::from(key.as_bytes().to_vec()),
    Bytes::from(value_bytes)
).await?;

// Read single entry
let entry: Option<Entry> = doc.get_exact(
    author_id,
    key.as_bytes(),
    false  // include_empty
).await?;

// Query multiple entries
let stream = doc.get_many(Query::all()).await?;
tokio::pin!(stream);
while let Some(entry_result) = stream.next().await {
    let entry = entry_result?;
    // entry.content_hash() gives the blob hash
    // Must fetch actual content from blobs separately!
}
```

#### Sharing and Syncing
```rust
use iroh_docs::api::protocol::{ShareMode, AddrInfoOptions};

// Share document
let ticket = doc.share(
    ShareMode::Write,
    AddrInfoOptions::Addresses(node_addrs)
).await?;

// Start sync with peers
doc.start_sync(vec![node_addr1, node_addr2]).await?;

// Stop sync
doc.leave().await?;
```

#### Subscribing to Changes
```rust
let mut events = doc.subscribe().await?;

while let Some(event_result) = events.next().await {
    match event_result? {
        LiveEvent::InsertLocal { entry } => {
            // Local insert
        }
        LiveEvent::InsertRemote { from, entry, .. } => {
            // Remote insert from peer
        }
        LiveEvent::ContentReady { hash } => {
            // Content blob is ready
        }
        _ => {}
    }
}
```

### Important Notes

1. **Entry Content Storage**: Entries in iroh-docs only store metadata and a `content_hash()`. The actual content bytes are stored in the blobs layer. To retrieve content:
   ```rust
   let entry = doc.get_exact(author_id, key, false).await?;
   if let Some(entry) = entry {
       let hash = entry.content_hash();
       // Fetch from blobs store using hash
       let content = blobs_store.get_bytes(hash).await?;
   }
   ```

2. **API Structure**: The `Docs` protocol implements `Deref` to `DocsApi`, so methods can be called directly on the `Docs` handle.

3. **RPC Based**: The API uses the `irpc` crate internally, allowing both in-process and cross-process usage with zero overhead for in-process calls.

### API Discovery Steps

To verify or extend this API knowledge:

1. **Clone the repository**:
   ```bash
   git clone https://github.com/n0-computer/iroh-docs.git
   cd iroh-docs
   git checkout v0.93.1  # latest 0.93.x tag
   ```

2. **Study the source**:
   - `src/protocol.rs` - Main Docs protocol implementation
   - `src/api.rs` - Complete RPC API with all methods
   - `examples/setup.rs` - Basic setup example
   - `tests/` - Integration tests

3. **Key Source Files**:
   ```rust
   // The Docs struct and protocol
   use iroh_docs::protocol::Docs;
   
   // The API methods (DocsApi and Doc)
   use iroh_docs::api::{DocsApi, Doc};
   
   // Entry type
   use iroh_docs::Entry;
   
   // Live events
   use iroh_docs::engine::LiveEvent;
   ```

4. **Run examples**:
   ```bash
   cd iroh-docs
   cargo run --example setup
   ```

### Integration Checklist

- [ ] Study iroh-docs source code for v0.93
- [ ] Create prototype with basic insert/query
- [ ] Test CRDT merge behavior
- [ ] Benchmark performance vs current implementation
- [ ] Implement migration tool for existing data
- [ ] Add feature flag for gradual rollout
- [ ] Update documentation
- [ ] Deploy to test environment
- [ ] Monitor sync performance
- [ ] Full production deployment

## Recommended Approach (Current)

**Keep the existing implementation** because:

1. ✅ **It works reliably** - proven in testing
2. ✅ **Well-understood** - clear code and logic
3. ✅ **Flexible** - custom SignedOperation struct
4. ✅ **Stable API** - no breaking changes expected
5. ✅ **Good performance** - efficient blob storage

**Consider migration when**:
- iroh-docs reaches v1.0 with stable API
- Clear documentation and examples available
- Significant performance or feature benefits
- Community adoption and support mature

## Current Implementation Details

### File Structure

```
src/
├── sync.rs              # Main sync module
│   ├── SyncMessage      # P2P sync messages
│   ├── SignedOperation  # Verified data operations
│   ├── SyncStore        # CRDT + blob storage
│   └── SyncManager      # Coordination layer
├── crdt.rs              # Automerge integration
├── storage.rs           # Redis storage backend
└── graphql.rs           # API layer
```

### Data Flow

1. **Submit**: GraphQL → verify signature → SyncStore
2. **Store**: SyncStore → LWW merge → Blob storage
3. **Replicate**: Gossip protocol → remote peers
4. **Receive**: Remote → verify → merge → apply to Redis
5. **Query**: GraphQL → SyncStore → operations

### Performance Characteristics

- **Write**: O(1) per operation + blob write
- **Read**: O(n) for queries (can add indexing)
- **Merge**: O(1) LWW comparison
- **Sync**: O(delta) only new operations transmitted
- **Storage**: ~500 bytes per operation (JSON + signature)

## Current Status (January 2025)

### iroh-docs 0.93 API Research Complete ✅

The iroh-docs 0.93 API has been fully researched and documented above. Key findings:

**Pros:**
- Clean, well-designed API with `DocsApi` and `Doc` handles
- Automatic CRDT merge with built-in conflict resolution
- Native document sharing via tickets
- Real-time sync events via subscriptions
- RPC-based design allows in-process and cross-process use

**Cons:**
- Entries only store hash references; must fetch content from blobs separately
- Additional complexity in content retrieval workflow
- API still evolving (0.9x is "canary series" before 1.0)
- Less documentation compared to stable releases

### Migration Blockers

1. **Content Retrieval Complexity**: Need to maintain blob store reference alongside docs for content fetching
2. **API Stability**: Version 0.9x is pre-1.0 "canary" - breaking changes expected
3. **Learning Curve**: Team needs time to understand docs+blobs interaction model
4. **No Clear Performance Benefit**: Current blob-based sync is already efficient

### Recommendation

**Keep current implementation** until:
- [ ] iroh-docs reaches 1.0 with stable API
- [ ] Clear migration guide and best practices published
- [ ] Performance benchmarks show significant improvement
- [ ] Production use cases validate stability

The current sync module using Automerge + iroh-blobs is:
- ✅ Production-ready and well-tested
- ✅ Fully documented and understood
- ✅ Efficient with LWW merge semantics
- ✅ No external API dependencies

### Next Steps

1. Monitor iroh-docs changelog for 1.0 announcement
2. Bookmark: https://github.com/n0-computer/iroh-docs/blob/main/CHANGELOG.md
3. When ready, use this document as migration guide
4. Consider creating `sync_v2.rs` as experimental parallel implementation

## Conclusion

The current sync implementation is production-ready and should be maintained. 
The iroh-docs API (0.93) is now fully understood and documented for future migration.
Monitor iroh-docs development and plan migration when version 1.0 releases with 
stable API guarantees and clear benefits.

For questions or updates, refer to:
- iroh-docs repository: https://github.com/n0-computer/iroh-docs
- iroh-docs API source: https://github.com/n0-computer/iroh-docs/blob/main/src/api.rs
- Iroh documentation: https://iroh.computer/docs
- Project guidelines: `.github/copilot-instructions.md`
