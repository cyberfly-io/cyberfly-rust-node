# Iroh-Docs Research Summary

## Executive Summary

This document summarizes the research conducted on integrating **iroh-docs 0.93** into the cyberfly-rust-node sync module. After thorough investigation of the API and architecture, we recommend **keeping the current implementation** and planning migration when iroh-docs reaches version 1.0.

**Date:** January 2025  
**Status:** Research Complete, Migration Deferred  
**Researcher:** Development Team

---

## Current Sync Implementation

### Architecture (Working & Production-Ready)

```
GraphQL API
    ↓
SignedOperation (Ed25519 verification)
    ↓
SyncStore (Automerge CRDT + LWW)
    ↓
Iroh Blobs (persistent storage)
    ↓
Iroh Gossip (P2P replication)
    ↓
Redis (application storage)
```

### Key Features

✅ **Ed25519 Signature Verification**: All operations cryptographically signed  
✅ **CRDT Conflict Resolution**: Automerge with Last-Write-Wins semantics  
✅ **Persistent Storage**: Operations stored as blobs with index  
✅ **Automatic Replication**: Via Iroh gossip protocol  
✅ **Deduplication**: Tracks applied operations to prevent re-application  
✅ **Type Support**: All Redis types (String, Hash, List, Set, SortedSet, JSON, Stream, TimeSeries, Geo)  

### Performance Characteristics

- **Write**: O(1) per operation + blob write
- **Read**: O(n) for queries (indexable if needed)
- **Merge**: O(1) LWW timestamp comparison
- **Sync**: O(delta) - only new operations transmitted
- **Storage**: ~500 bytes per operation (JSON + signature)

---

## Iroh-Docs 0.93 API Research

### What is iroh-docs?

Iroh-docs is a **multi-dimensional key-value document store** with built-in CRDT synchronization. It provides:

- Native CRDT conflict resolution (no Automerge dependency needed)
- Automatic peer-to-peer synchronization
- Document sharing via tickets
- Real-time change subscriptions
- Multi-author support with Ed25519 signing

### Architecture

```
DocsApi (RPC interface)
    ↓
Docs Protocol
    ↓
Engine (CRDT logic)
    ↓
Store (redb persistence)
    ↓
Blobs (content storage)
```

### Verified API (Complete)

#### 1. Initialization

```rust
use iroh_docs::protocol::Docs;
use iroh_blobs::store::Store;
use iroh_gossip::net::Gossip;

// Create docs protocol
let docs = Docs::persistent(path)
    .spawn(endpoint, blobs_store, gossip)
    .await?;

// Get API handle
let api = docs.api();
```

#### 2. Author Management

```rust
// Get default author (created on first start)
let author_id = api.author_default().await?;

// Export author (WARNING: contains private key!)
let author = api.author_export(author_id).await?.unwrap();

// Create new author
let new_author = api.author_create().await?;

// List all authors
let mut authors = api.author_list().await?;
while let Some(author_id) = authors.next().await {
    println!("Author: {}", author_id?);
}
```

#### 3. Document Operations

```rust
use iroh_docs::api::Doc;

// Create new document
let doc: Doc = api.create().await?;

// Open existing document
let doc: Option<Doc> = api.open(namespace_id).await?;

// Import from ticket (with auto-sync)
let doc: Doc = api.import(ticket).await?;

// Get document ID
let namespace_id = doc.id();
```

#### 4. Reading/Writing Data

```rust
use bytes::Bytes;

// Write data
let hash = doc.set_bytes(
    author_id,
    Bytes::from("my-key".as_bytes()),
    Bytes::from(b"my-value".to_vec())
).await?;

// Read exact entry
let entry = doc.get_exact(
    author_id,
    "my-key".as_bytes(),
    false  // include_empty (deleted entries)
).await?;

// Query multiple entries
let mut stream = doc.get_many(Query::all()).await?;
tokio::pin!(stream);
while let Some(entry) = stream.next().await {
    let entry = entry?;
    let key = entry.key();
    let hash = entry.content_hash();
    // Note: Must fetch content from blobs separately!
}
```

#### 5. Content Retrieval (Important!)

**Entries only store metadata + content hash!**

```rust
// Step 1: Get entry
let entry = doc.get_exact(author_id, key, false).await?;

// Step 2: Extract content hash
let content_hash = entry.content_hash();

// Step 3: Fetch from blobs store
let content = blobs_store.get_bytes(content_hash).await?;
```

#### 6. Sharing & Syncing

```rust
use iroh_docs::api::protocol::{ShareMode, AddrInfoOptions};

// Create share ticket
let ticket = doc.share(
    ShareMode::Write,
    AddrInfoOptions::Addresses(vec![node_addr])
).await?;

// Share ticket as string
let ticket_str = ticket.to_string();

// Start syncing with peers
doc.start_sync(vec![peer1, peer2]).await?;

// Stop syncing
doc.leave().await?;
```

#### 7. Real-time Subscriptions

```rust
use iroh_docs::engine::LiveEvent;

let mut events = doc.subscribe().await?;

while let Some(event) = events.next().await {
    match event? {
        LiveEvent::InsertLocal { entry } => {
            println!("Local insert: {:?}", entry.key());
        }
        LiveEvent::InsertRemote { from, entry, .. } => {
            println!("Remote from {}: {:?}", from, entry.key());
        }
        LiveEvent::ContentReady { hash } => {
            println!("Content ready: {}", hash);
        }
        _ => {}
    }
}
```

---

## Comparison: Current vs iroh-docs

| Feature | Current (Automerge + Blobs) | iroh-docs 0.93 |
|---------|----------------------------|----------------|
| **CRDT Support** | ✅ Automerge | ✅ Built-in |
| **Signature Verification** | ✅ Custom SignedOperation | ✅ Built-in authors |
| **Persistence** | ✅ Blobs + Index | ✅ redb + Blobs |
| **P2P Sync** | ✅ Gossip + Custom | ✅ Automatic via tickets |
| **Content Storage** | ✅ Direct in blobs | ⚠️ Indirect (hash ref) |
| **API Stability** | ✅ Stable (our code) | ⚠️ 0.9x pre-release |
| **Documentation** | ✅ Well documented | ⚠️ Limited |
| **Team Knowledge** | ✅ Fully understood | ⚠️ Learning curve |
| **Custom Logic** | ✅ Full control | ⚠️ Framework constraints |
| **Dependencies** | Automerge | Built-in (removes Automerge) |

---

## Migration Path (When Ready)

### Prerequisites

- [ ] iroh-docs reaches version 1.0
- [ ] Stable API guarantees published
- [ ] Migration guide available
- [ ] Performance benchmarks validate benefits

### Phase 1: Preparation

1. **Create `sync_v2.rs`** with iroh-docs implementation
2. **Feature flag**: `cargo.toml` - `use-iroh-docs = ["iroh-docs"]`
3. **Parallel testing**: Run both systems side-by-side
4. **Performance benchmarks**: Compare throughput, latency, storage

### Phase 2: Implementation

```rust
// sync_v2.rs structure
pub struct SyncStoreV2 {
    docs_api: DocsApi,
    doc: Doc,
    author: Author,
    blobs_store: Store,  // For content retrieval
}

impl SyncStoreV2 {
    pub async fn add_operation(&self, op: SignedOperation) -> Result<()> {
        // 1. Verify signature
        op.verify()?;
        
        // 2. Serialize to bytes
        let bytes = serde_json::to_vec(&op)?;
        
        // 3. Write to document
        self.doc.set_bytes(
            self.author.id(),
            Bytes::from(op.doc_key().as_bytes()),
            Bytes::from(bytes)
        ).await?;
        
        Ok(())
    }
    
    pub async fn get_operation(&self, key: &str) -> Result<Option<SignedOperation>> {
        // 1. Get entry
        let entry = self.doc.get_exact(
            self.author.id(),
            key.as_bytes(),
            false
        ).await?;
        
        // 2. Fetch content from blobs
        if let Some(entry) = entry {
            let hash = entry.content_hash();
            let bytes = self.blobs_store.get_bytes(hash).await?;
            let op = serde_json::from_slice(&bytes)?;
            Ok(Some(op))
        } else {
            Ok(None)
        }
    }
}
```

### Phase 3: Migration

1. **Export existing data** from blob store
2. **Import into iroh-docs** documents
3. **Verify integrity** of all operations
4. **Gradual rollout** with feature flag
5. **Monitor performance** and stability

### Phase 4: Cleanup

1. **Remove Automerge** dependency
2. **Delete `sync.rs`** (old implementation)
3. **Rename `sync_v2.rs`** to `sync.rs`
4. **Update documentation**

---

## Key Findings

### Advantages of iroh-docs

✅ **Removes Automerge dependency** - one less external CRDT library  
✅ **Built-in sync protocol** - less custom code to maintain  
✅ **Document tickets** - elegant peer discovery  
✅ **Real-time subscriptions** - built-in change events  
✅ **Multi-author support** - native in the design  

### Disadvantages/Concerns

⚠️ **Content indirection** - entries only store hashes, not content  
⚠️ **API instability** - 0.9x is "canary series" before 1.0  
⚠️ **Learning curve** - team needs time to understand docs+blobs model  
⚠️ **Framework constraints** - less control over CRDT behavior  
⚠️ **Migration complexity** - non-trivial data migration required  

### Blockers for Immediate Migration

🛑 **API Stability**: Version 0.93 is pre-1.0 with potential breaking changes  
🛑 **Documentation Gap**: Limited examples and best practices  
🛑 **Content Retrieval**: Two-step process (entry → hash → blobs)  
🛑 **No Clear Benefit**: Current implementation is already efficient  
🛑 **Team Velocity**: Would slow down current development  

---

## Recommendations

### Short Term (Current)

✅ **Keep existing implementation**  
✅ **Document lessons learned** (this file)  
✅ **Monitor iroh-docs releases**  
✅ **Bookmark changelog**: https://github.com/n0-computer/iroh-docs/blob/main/CHANGELOG.md  

### Medium Term (Next 6 months)

📋 **Track iroh-docs 1.0 progress**  
📋 **Study production use cases** in the wild  
📋 **Create proof-of-concept** when API stabilizes  
📋 **Benchmark performance** against current implementation  

### Long Term (Post 1.0)

🎯 **Plan migration** when clear benefits proven  
🎯 **Gradual rollout** with feature flags  
🎯 **Maintain backwards compatibility** during transition  
🎯 **Document new patterns** for team  

---

## Resources

### Official Documentation
- **Iroh Docs**: https://iroh.computer/proto/iroh-docs
- **API Documentation**: https://docs.rs/iroh-docs/latest/iroh_docs/
- **Repository**: https://github.com/n0-computer/iroh-docs
- **Examples**: https://github.com/n0-computer/iroh-docs/tree/main/examples

### Source Code References
- **API Implementation**: `src/api.rs` - Complete RPC API
- **Protocol**: `src/protocol.rs` - Docs protocol handler
- **Engine**: `src/engine/` - Core CRDT logic
- **Setup Example**: `examples/setup.rs` - Basic initialization

### Community
- **Discord**: https://iroh.computer/discord
- **Blog**: https://iroh.computer/blog
- **GitHub Issues**: https://github.com/n0-computer/iroh-docs/issues

---

## Conclusion

After comprehensive research of the iroh-docs 0.93 API, we conclude that:

1. **The API is well-designed** and will be a good fit when mature
2. **Migration is feasible** but requires careful planning
3. **Current implementation is stable** and meets all requirements
4. **Timing is not right** - wait for 1.0 release

The research documented here provides a **complete blueprint** for future migration. When iroh-docs 1.0 releases with stable APIs, this document serves as the migration guide.

**Decision: Keep current sync implementation, revisit after iroh-docs 1.0 release.**

---

## Appendix: Quick Reference

### Current Implementation Files
- `src/sync.rs` - Main sync module with SyncStore and SyncManager
- `src/crdt.rs` - Automerge integration
- `src/storage.rs` - Redis backend
- `src/graphql.rs` - API layer for submissions

### Key Dependencies
```toml
[dependencies]
automerge = "0.6"  # CRDT library
iroh = "0.93.2"    # P2P networking
iroh-blobs = "0.95" # Blob storage
iroh-gossip = "0.93" # Message propagation
```

### Testing the Current Implementation
```bash
# Build and run
cargo build --release
cargo run

# Run tests
cargo test sync

# Check sync stats via GraphQL
curl -X POST http://localhost:4000 \
  -H "Content-Type: application/json" \
  -d '{"query": "{ getNodeInfo { nodeId connectedPeers } }"}'
```

---

**Last Updated:** January 2025  
**Next Review:** When iroh-docs 1.0 is announced  
**Maintained By:** Cyberfly Development Team