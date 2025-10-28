# Quick Improvements - Action Items

## Immediate Actions (Can be implemented today)

### 1. Add Input Validation Constants
```rust
// Add to src/crypto.rs or new src/validation.rs
pub const MAX_KEY_LENGTH: usize = 256;
pub const MAX_VALUE_LENGTH: usize = 10 * 1024 * 1024; // 10MB
pub const MAX_DB_NAME_LENGTH: usize = 128;
pub const MAX_FIELD_LENGTH: usize = 256;
```

### 2. Implement Request Timeout
```rust
// In src/graphql.rs create_server()
let app = Router::new()
    .route("/", get(graphiql_handler))
    .route("/graphql", get(graphql_handler).post(graphql_handler))
    .layer(tower::timeout::TimeoutLayer::new(Duration::from_secs(30)))
    .with_state(schema);
```

### 3. Add Environment Variable Validation
```rust
// In src/config.rs
impl Config {
    pub fn validate(&self) -> Result<()> {
        if self.api_port == 0 {
            return Err(anyhow!("API_PORT must be > 0"));
        }
        if self.mqtt_config.enabled && self.mqtt_config.broker_host.is_empty() {
            return Err(anyhow!("MQTT_BROKER_HOST required when MQTT enabled"));
        }
        Ok(())
    }
}
```

### 4. Improve Error Messages
```rust
// Make DbError more user-friendly
impl DbError {
    pub fn user_message(&self) -> String {
        match self {
            DbError::SignatureError(_) => "Invalid signature - data verification failed".to_string(),
            DbError::StorageError(_) => "Database operation failed - please try again".to_string(),
            DbError::NetworkError(_) => "Network connection issue - retrying...".to_string(),
            _ => self.to_string(),
        }
    }
}
```

### 5. Add Request ID Tracing
```rust
use uuid::Uuid;

// Add middleware to inject request IDs
pub async fn add_request_id<B>(
    req: Request<B>,
    next: Next<B>,
) -> Response {
    let request_id = Uuid::new_v4().to_string();
    tracing::info!(request_id = %request_id, "Processing request");
    next.run(req).await
}
```

## This Week (High ROI, Low Effort)

### 6. Add Prometheus Metrics Endpoint
```toml
# Cargo.toml
prometheus = "0.13"
axum-prometheus = "0.6"
```

```rust
use axum_prometheus::PrometheusMetricLayer;

let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();

let app = Router::new()
    .route("/metrics", get(|| async move { metric_handle.render() }))
    .layer(prometheus_layer);
```

### 7. Optimize Sled Configuration
```rust
// Tune Sled performance settings
let sled_config = sled::Config::new()
    .path(&sled_path)
    .cache_capacity(1024 * 1024 * 1024) // 1GB cache
    .flush_every_ms(Some(1000)) // Flush every second
    .mode(sled::Mode::HighThroughput);

let sled_db = sled_config.open()?;
```

### 8. Add Health Check Endpoint
```rust
#[derive(Serialize)]
pub struct HealthCheck {
    status: String,
    version: String,
    uptime: u64,
    sled_ok: bool,
    iroh_ok: bool,
    peers_connected: u32,
}

async fn health_check(State(storage): State<BlobStorage>) -> Json<HealthCheck> {
    let sled_ok = storage.sled_db.was_recovered(); // Check Sled health
    let iroh_ok = true; // Check Iroh blob store health
    Json(HealthCheck {
        status: if sled_ok && iroh_ok { "healthy" } else { "degraded" }.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime: /* calculate */,
        sled_ok,
        iroh_ok,
        peers_connected: /* get from sync manager */,
    })
}
```

### 9. Add Rate Limiting
```toml
# Cargo.toml
tower-governor = "0.4"
```

```rust
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};

let governor_conf = GovernorConfigBuilder::default()
    .per_second(10)
    .burst_size(20)
    .finish()
    .unwrap();

let app = Router::new()
    .layer(GovernorLayer { config: governor_conf });
```

### 10. Improve Logging
```rust
// Add structured logging
tracing::info!(
    event = "data_submitted",
    db_name = %input.db_name,
    key = %input.key,
    store_type = %input.store_type,
    public_key = %input.public_key[..16], // Log prefix only
    "Data submitted successfully"
);
```

## Next 2 Weeks

### 11. Add CLI Tool
```toml
# Cargo.toml
clap = { version = "4.4", features = ["derive"] }
```

Create `src/bin/cyberfly-cli.rs`:
```rust
use clap::Parser;

#[derive(Parser)]
#[command(name = "cyberfly")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

enum Commands {
    Start,
    Status,
    Query { db_name: String, key: String },
}
```

### 12. Implement Backup System
```rust
pub struct BackupManager {
    backup_dir: PathBuf,
}

impl BackupManager {
    pub async fn create_snapshot(&self) -> Result<String> {
        // Export all data to JSON
        // Compress with zstd
        // Save with timestamp
    }
}
```

### 13. Add Batch Operations
```rust
pub async fn batch_submit_data(
    &self,
    ctx: &Context<'_>,
    operations: Vec<SignedData>,
) -> Result<BatchResult, DbError> {
    // Validate all signatures first
    // Use Sled batch writes for atomic operations
    let batch = self.sled_db.apply_batch(/* ... */)?;
    // Store values in Iroh blobs
    // Return batch results
}
```

### 14. Add TTL Support
```rust
pub async fn set_string_with_ttl(
    &self,
    key: &str,
    value: &str,
    ttl_seconds: u64,
    metadata: Option<SignatureMetadata>,
) -> Result<()> {
    // Store value
    // Set expiration
}
```

### 15. Create Development Docker Compose
```yaml
# docker-compose.dev.yml
version: '3.8'
services:
  mosquitto:
    image: eclipse-mosquitto:latest
    ports:
      - "1883:1883"
  
  cyberfly:
    build: .
    environment:
      MQTT_BROKER_HOST: mosquitto
    volumes:
      - ./data:/app/data  # Persist Sled DB and Iroh blobs
    depends_on:
      - mosquitto
```

## Month 1

### 16. Audit Logging System
```rust
pub struct AuditLogger {
    log_file: Arc<RwLock<File>>,
}

impl AuditLogger {
    pub async fn log_event(&self, event: AuditEvent) {
        let json = serde_json::to_string(&event).unwrap();
        self.log_file.write().await.write_all(json.as_bytes()).await.unwrap();
    }
}
```

### 17. Add Pagination
```rust
pub async fn get_all_with_pagination(
    &self,
    db_name: String,
    page: u32,
    page_size: u32,
) -> Result<PaginatedResult, DbError>
```

### 18. Implement Circuit Breaker
```rust
pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitState>>,
    failure_threshold: u32,
    timeout: Duration,
}
```

### 19. Add Query Result Streaming
```rust
pub async fn stream_all_entries(
    &self,
    db_name: String,
) -> impl Stream<Item = Result<StoredEntry, DbError>>
```

### 20. Create Comprehensive Tests
```rust
#[cfg(test)]
mod integration_tests {
    #[tokio::test]
    async fn test_full_sync_workflow() {
        // Test complete sync between 3 nodes
    }
    
    #[tokio::test]
    async fn test_concurrent_writes() {
        // Test race conditions
    }
}
```

## Implementation Priority

1. ✅ **Security** (Items 1, 3, 9) - Prevent vulnerabilities
2. ✅ **Reliability** (Items 2, 7, 18) - Prevent crashes (Note: Item 7 is Sled tuning, not connection pooling)
3. ✅ **Observability** (Items 5, 6, 8, 10) - Debug issues
4. ✅ **Performance** (Items 7, 13, 19) - Handle scale (Sled optimization, batch operations)
5. ✅ **Developer Experience** (Items 11, 15, 20) - Ease development

## Measurement & Success Criteria

- [ ] Request timeout prevents hung connections
- [ ] Rate limiting stops abuse (measure req/sec)
- [ ] Health check shows all components (Sled + Iroh + MQTT)
- [ ] Metrics exported to Prometheus
- [ ] Sled configuration optimized for workload
- [ ] CLI tool simplifies operations
- [ ] Tests achieve 70%+ coverage
- [ ] Zero security vulnerabilities in audit
- [ ] Backup/restore works reliably (Sled snapshots + Iroh blobs)
- [ ] Error rates < 0.1%

## Getting Started

1. Pick one item from "Immediate Actions"
2. Create a branch: `git checkout -b feature/rate-limiting`
3. Implement the change
4. Add tests
5. Update documentation
6. Create PR with description and test results

**Remember**: Small, incremental improvements are better than large, risky changes!
