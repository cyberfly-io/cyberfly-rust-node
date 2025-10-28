# Architecture Improvements & Best Practices

## Current Architecture Assessment

### ✅ Strengths
1. **Strong cryptographic foundation** - Ed25519 signatures throughout
2. **CRDT-based sync** - Conflict-free replication
3. **Modern async runtime** - Tokio with excellent performance
4. **Content-addressed storage** - Iroh blobs for immutability
5. **Multiple protocols** - GraphQL, MQTT, Gossip
6. **Type safety** - Rust's compile-time guarantees

### ⚠️ Areas for Improvement
1. **Tight coupling** - Components could be more modular
2. **Error propagation** - Some errors lose context
3. **Resource management** - No explicit limits on growth
4. **State management** - Multiple sources of truth
5. **Testing strategy** - Limited integration tests

---

## Proposed Architecture Improvements

### 1. Hexagonal Architecture (Ports & Adapters)

**Current**: Direct dependencies between layers  
**Proposed**: Abstract interfaces with dependency injection

```rust
// Define ports (interfaces)
#[async_trait]
pub trait StoragePort: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<String>>;
    async fn set(&self, key: &str, value: &str) -> Result<()>;
}

#[async_trait]
pub trait NetworkPort: Send + Sync {
    async fn broadcast(&self, message: &[u8]) -> Result<()>;
    async fn subscribe(&self) -> Result<Receiver<Vec<u8>>>;
}

// Implement adapters
pub struct SledStorageAdapter {
    storage: BlobStorage,  // Sled + Iroh Blobs
}

#[async_trait]
impl StoragePort for SledStorageAdapter {
    async fn get(&self, key: &str) -> Result<Option<String>> {
        self.storage.get_string(key).await
    }
}

// Core business logic depends on ports, not concrete implementations
pub struct DataService {
    storage: Arc<dyn StoragePort>,
    network: Arc<dyn NetworkPort>,
}
```

**Benefits:**
- Easy to swap implementations (e.g., Sled → RocksDB, redb, etc.)
- Testable with mock implementations
- Clear separation of concerns

### 2. Event-Driven Architecture

**Current**: Direct method calls between components  
**Proposed**: Event bus for decoupled communication

```rust
// Define domain events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DomainEvent {
    DataWritten {
        db_name: String,
        key: String,
        timestamp: i64,
    },
    DataSynced {
        peer_id: String,
        operations_count: usize,
    },
    PeerConnected {
        peer_id: String,
    },
    PeerDisconnected {
        peer_id: String,
    },
}

// Event bus
pub struct EventBus {
    subscribers: Arc<RwLock<HashMap<TypeId, Vec<Sender<DomainEvent>>>>>,
}

impl EventBus {
    pub async fn publish(&self, event: DomainEvent) {
        // Send to all subscribers
    }
    
    pub async fn subscribe<T: 'static>(&self) -> Receiver<DomainEvent> {
        // Register subscriber
    }
}

// Subscribers react to events
pub struct AuditLogger {
    events: Receiver<DomainEvent>,
}

impl AuditLogger {
    pub async fn run(&mut self) {
        while let Some(event) = self.events.recv().await {
            match event {
                DomainEvent::DataWritten { .. } => self.log_write(event).await,
                _ => {}
            }
        }
    }
}
```

**Benefits:**
- Components don't need to know about each other
- Easy to add new features (just add subscribers)
- Natural fit for distributed systems

### 3. CQRS (Command Query Responsibility Segregation)

**Current**: Same interface for reads and writes  
**Proposed**: Separate read and write models

```rust
// Command side (writes)
pub struct CommandHandler {
    storage: Arc<BlobStorage>,  // Sled + Iroh
    event_bus: Arc<EventBus>,
}

impl CommandHandler {
    pub async fn handle(&self, command: Command) -> Result<CommandResult> {
        match command {
            Command::SubmitData(data) => {
                // Validate
                // Write to Sled index and Iroh blobs
                // Publish event
                self.event_bus.publish(DomainEvent::DataWritten { .. }).await;
            }
        }
    }
}

// Query side (reads)
pub struct QueryHandler {
    read_model: Arc<ReadModel>,
}

impl QueryHandler {
    pub async fn handle(&self, query: Query) -> Result<QueryResult> {
        match query {
            Query::GetAllData(db) => {
                // Read from optimized read model
                self.read_model.get_all(&db).await
            }
        }
    }
}

// Separate read model optimized for queries
pub struct ReadModel {
    cache: Arc<MokaCache<String, CachedData>>,
    projections: Arc<HashMap<String, Projection>>,
}
```

**Benefits:**
- Read and write paths optimized independently
- Read model can be denormalized for performance
- Easier to scale reads and writes separately

### 4. Domain-Driven Design (DDD) Structure

**Current**: Technical layers (storage, network, etc.)  
**Proposed**: Domain-centric organization

```
src/
├── domain/                 # Core business logic
│   ├── entities/
│   │   ├── database.rs
│   │   ├── operation.rs
│   │   └── peer.rs
│   ├── value_objects/
│   │   ├── signature.rs
│   │   ├── timestamp.rs
│   │   └── db_name.rs
│   ├── aggregates/
│   │   └── signed_data.rs
│   ├── events.rs
│   └── services/
│       ├── sync_service.rs
│       └── validation_service.rs
├── application/            # Use cases
│   ├── commands/
│   │   ├── submit_data.rs
│   │   └── request_sync.rs
│   └── queries/
│       ├── get_all_data.rs
│       └── get_node_info.rs
├── infrastructure/         # Technical implementations
│   ├── storage/
│   │   ├── redis.rs
│   │   └── sled.rs
│   ├── network/
│   │   └── iroh.rs
│   └── api/
│       ├── graphql.rs
│       └── mqtt.rs
└── main.rs                # Composition root
```

**Example domain entity:**
```rust
// src/domain/entities/database.rs
pub struct Database {
    name: DatabaseName,      // Value object
    owner: PublicKey,        // Value object
    entries: Vec<Entry>,
}

impl Database {
    pub fn new(name: &str, owner: PublicKey) -> Result<Self> {
        let db_name = DatabaseName::new(name, &owner)?;
        Ok(Self {
            name: db_name,
            owner,
            entries: Vec::new(),
        })
    }
    
    pub fn add_entry(&mut self, entry: Entry, signature: Signature) -> Result<()> {
        // Domain logic - validate, enforce rules
        self.validate_entry(&entry, &signature)?;
        self.entries.push(entry);
        Ok(())
    }
    
    fn validate_entry(&self, entry: &Entry, signature: &Signature) -> Result<()> {
        // Business rules enforcement
        if entry.key.is_empty() {
            return Err(DomainError::InvalidEntry("Key cannot be empty"));
        }
        signature.verify(&self.owner, entry)?;
        Ok(())
    }
}
```

### 5. Resource Management Pattern

**Current**: Unbounded growth potential  
**Proposed**: Explicit resource limits

```rust
pub struct ResourceManager {
    limits: ResourceLimits,
    metrics: Arc<ResourceMetrics>,
}

pub struct ResourceLimits {
    max_memory_bytes: usize,
    max_operations_in_flight: usize,
    max_connections: usize,
    max_db_size_bytes: usize,
}

impl ResourceManager {
    pub async fn acquire_operation_slot(&self) -> Result<OperationGuard> {
        if self.metrics.operations_in_flight.load(Ordering::Relaxed) >= self.limits.max_operations_in_flight {
            return Err(DbError::ResourceExhausted("Too many operations in flight"));
        }
        
        self.metrics.operations_in_flight.fetch_add(1, Ordering::Relaxed);
        Ok(OperationGuard { metrics: self.metrics.clone() })
    }
    
    pub async fn check_memory(&self) -> Result<()> {
        let current_memory = self.get_memory_usage();
        if current_memory > self.limits.max_memory_bytes {
            return Err(DbError::ResourceExhausted("Memory limit exceeded"));
        }
        Ok(())
    }
}

// RAII guard ensures resources are released
pub struct OperationGuard {
    metrics: Arc<ResourceMetrics>,
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        self.metrics.operations_in_flight.fetch_sub(1, Ordering::Relaxed);
    }
}
```

### 6. Observability Pattern

**Current**: Ad-hoc logging  
**Proposed**: Structured observability

```rust
use tracing::{instrument, Span};

pub struct ObservabilityContext {
    span: Span,
    metrics: Arc<Metrics>,
}

impl ObservabilityContext {
    pub fn new(operation: &str) -> Self {
        let span = tracing::info_span!(
            "operation",
            op = operation,
            request_id = %Uuid::new_v4(),
        );
        Self {
            span,
            metrics: Arc::new(Metrics::default()),
        }
    }
    
    pub fn record_success(&self, duration: Duration) {
        self.metrics.operation_duration.observe(duration.as_secs_f64());
        self.metrics.operation_success.inc();
    }
    
    pub fn record_error(&self, error: &DbError) {
        self.metrics.operation_errors.inc();
        tracing::error!(
            parent: &self.span,
            error = %error,
            "Operation failed"
        );
    }
}

// Use in handlers
#[instrument(skip(storage))]
pub async fn submit_data(
    storage: &RedisStorage,
    data: SignedData,
) -> Result<StorageResult> {
    let obs = ObservabilityContext::new("submit_data");
    let start = Instant::now();
    
    match execute_submit(storage, data).await {
        Ok(result) => {
            obs.record_success(start.elapsed());
            Ok(result)
        }
        Err(e) => {
            obs.record_error(&e);
            Err(e)
        }
    }
}
```

### 7. State Machine Pattern for Sync

**Current**: Implicit states in sync logic  
**Proposed**: Explicit state machine

```rust
pub enum SyncState {
    Idle,
    Connecting { peer_id: String },
    Syncing { peer_id: String, progress: SyncProgress },
    Complete,
    Failed { error: String },
}

pub struct SyncStateMachine {
    state: Arc<RwLock<SyncState>>,
}

impl SyncStateMachine {
    pub async fn transition(&self, event: SyncEvent) -> Result<()> {
        let mut state = self.state.write().await;
        let new_state = match (&*state, event) {
            (SyncState::Idle, SyncEvent::StartSync { peer_id }) => {
                SyncState::Connecting { peer_id }
            }
            (SyncState::Connecting { peer_id }, SyncEvent::Connected) => {
                SyncState::Syncing {
                    peer_id: peer_id.clone(),
                    progress: SyncProgress::default(),
                }
            }
            (SyncState::Syncing { .. }, SyncEvent::Progress { ops_synced }) => {
                // Update progress
                return Ok(());
            }
            (SyncState::Syncing { .. }, SyncEvent::Complete) => {
                SyncState::Complete
            }
            (_, SyncEvent::Error { error }) => {
                SyncState::Failed { error }
            }
            _ => return Err(anyhow!("Invalid state transition")),
        };
        
        *state = new_state;
        Ok(())
    }
}
```

### 8. Repository Pattern

**Current**: Direct storage access throughout  
**Proposed**: Repository abstraction

```rust
#[async_trait]
pub trait DatabaseRepository: Send + Sync {
    async fn save(&self, db: &Database) -> Result<()>;
    async fn find_by_name(&self, name: &str) -> Result<Option<Database>>;
    async fn find_all(&self) -> Result<Vec<Database>>;
    async fn delete(&self, name: &str) -> Result<()>;
}

pub struct RedisRepository {
    storage: Arc<RedisStorage>,
}

#[async_trait]
impl DatabaseRepository for RedisRepository {
    async fn save(&self, db: &Database) -> Result<()> {
        // Convert domain model to storage format
        // Handle persistence
    }
    
    async fn find_by_name(&self, name: &str) -> Result<Option<Database>> {
        // Load from storage
        // Convert to domain model
    }
}
```

### 9. Factory Pattern for Complex Creation

**Current**: Complex initialization in main.rs  
**Proposed**: Builder/Factory pattern

```rust
pub struct NodeBuilder {
    config: Config,
    storage: Option<Arc<RedisStorage>>,
    network: Option<Arc<dyn NetworkPort>>,
}

impl NodeBuilder {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            storage: None,
            network: None,
        }
    }
    
    pub fn with_storage(mut self, storage: Arc<RedisStorage>) -> Self {
        self.storage = Some(storage);
        self
    }
    
    pub fn with_network(mut self, network: Arc<dyn NetworkPort>) -> Self {
        self.network = Some(network);
        self
    }
    
    pub async fn build(self) -> Result<Node> {
        let storage = self.storage.ok_or(anyhow!("Storage required"))?;
        let network = self.network.ok_or(anyhow!("Network required"))?;
        
        // Create all components
        let sync_manager = SyncManager::new(storage.clone(), network.clone());
        let event_bus = Arc::new(EventBus::new());
        
        Ok(Node {
            storage,
            network,
            sync_manager,
            event_bus,
        })
    }
}

// Usage
let node = NodeBuilder::new(config)
    .with_storage(redis_storage)
    .with_network(iroh_network)
    .build()
    .await?;
```

---

## Implementation Strategy

### Phase 1: Extract Interfaces (2 weeks)
1. Define port interfaces for storage, network, crypto
2. Create adapter implementations
3. Update dependency injection in main.rs
4. Add unit tests with mocks

### Phase 2: Event Bus (2 weeks)
1. Implement event bus
2. Define domain events
3. Refactor existing tight coupling to events
4. Add event-based logging and metrics

### Phase 3: CQRS (3 weeks)
1. Separate command and query handlers
2. Create read models for common queries
3. Add projections for complex queries
4. Optimize read path with caching

### Phase 4: DDD Structure (3 weeks)
1. Reorganize code by domain
2. Extract value objects
3. Define aggregates
4. Implement domain services

### Phase 5: Resource Management (1 week)
1. Add resource limits configuration
2. Implement resource guards
3. Add memory/connection monitoring
4. Test under load

---

## Testing Strategy

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use mockall::mock;
    
    mock! {
        pub StoragePort {}
        #[async_trait]
        impl StoragePort for StoragePort {
            async fn get(&self, key: &str) -> Result<Option<String>>;
            async fn set(&self, key: &str, value: &str) -> Result<()>;
        }
    }
    
    #[tokio::test]
    async fn test_submit_data_success() {
        let mut mock_storage = MockStoragePort::new();
        mock_storage
            .expect_set()
            .returning(|_, _| Ok(()));
            
        let service = DataService::new(Arc::new(mock_storage));
        let result = service.submit_data(data).await;
        assert!(result.is_ok());
    }
}
```

### Integration Tests
```rust
#[tokio::test]
async fn test_full_workflow() {
    // Start test node
    let node = create_test_node().await;
    
    // Submit data
    let result = node.submit_data(signed_data).await?;
    assert!(result.success);
    
    // Verify stored
    let stored = node.query_data(&db_name, &key).await?;
    assert_eq!(stored.value, expected_value);
    
    // Cleanup
    node.shutdown().await?;
}
```

### Load Tests
```rust
use criterion::{criterion_group, criterion_main, Criterion};

fn benchmark_submit_1000_ops(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let node = rt.block_on(create_test_node());
    
    c.bench_function("submit_1000_operations", |b| {
        b.iter(|| {
            rt.block_on(async {
                for i in 0..1000 {
                    node.submit_data(create_signed_data(i)).await.unwrap();
                }
            });
        });
    });
}
```

---

## Migration Path

1. **Start with new features** - Implement new functionality using new patterns
2. **Gradual refactoring** - Refactor existing code module by module
3. **Maintain compatibility** - Keep old interfaces working during transition
4. **Comprehensive testing** - Test each change thoroughly
5. **Documentation** - Update docs as patterns change

---

## Benefits Summary

| Pattern | Benefit | Effort | Priority |
|---------|---------|--------|----------|
| Hexagonal Architecture | Testability, Flexibility | High | High |
| Event Bus | Decoupling, Extensibility | Medium | High |
| CQRS | Performance, Scalability | High | Medium |
| DDD Structure | Maintainability | Medium | Medium |
| Resource Management | Stability | Low | High |
| Observability | Debuggability | Low | High |
| State Machine | Correctness | Medium | Medium |
| Repository Pattern | Abstraction | Medium | Low |

---

## Conclusion

These architecture improvements will:
- Make the codebase more maintainable
- Enable easier testing
- Improve performance and scalability
- Reduce coupling between components
- Make it easier to add new features

**Start small**: Pick one pattern, implement it for one module, evaluate, and expand.
