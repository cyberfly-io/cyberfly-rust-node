# Performance Metrics Guide

## Overview

The decentralized database now includes comprehensive Prometheus-compatible metrics for monitoring performance, cache efficiency, and system health.

## Metrics Endpoint

**URL**: `http://localhost:8080/metrics`

The `/metrics` endpoint exposes all metrics in Prometheus text format, ready for scraping by monitoring systems.

## Available Metrics

### Storage Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `storage_reads_total` | Counter | Total number of storage read operations |
| `storage_writes_total` | Counter | Total number of storage write operations |
| `storage_deletes_total` | Counter | Total number of storage delete operations |
| `storage_read_duration_seconds` | Histogram | Storage read operation latency (seconds) |
| `storage_write_duration_seconds` | Histogram | Storage write operation latency (seconds) |
| `storage_delete_duration_seconds` | Histogram | Storage delete operation latency (seconds) |

### Cache Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `cache_hits_total` | Counter | Total number of cache hits (both hot and warm tiers) |
| `cache_misses_total` | Counter | Total number of cache misses |
| `cache_hot_hits_total` | Counter | Number of cache hits in hot tier (most frequent) |
| `cache_warm_hits_total` | Counter | Number of cache hits in warm tier |
| `cache_size_hot` | Gauge | Current number of entries in hot cache tier |
| `cache_size_warm` | Gauge | Current number of entries in warm cache tier |

### GraphQL Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `graphql_requests_total` | Counter (labeled) | Total GraphQL requests by operation type |
| `graphql_errors_total` | Counter (labeled) | Total GraphQL errors by operation type |
| `graphql_duration_seconds` | Histogram (labeled) | GraphQL operation latency by operation type |

### Network Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `network_peers_connected` | Gauge | Current number of connected peers |
| `network_bytes_sent_total` | Counter | Total bytes sent over the network |
| `network_bytes_received_total` | Counter | Total bytes received from the network |

### Sync Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `sync_operations_total` | Counter | Total number of sync operations |
| `sync_conflicts_total` | Counter | Total number of sync conflicts resolved |
| `sync_merges_total` | Counter | Total number of CRDT merges |

## Key Performance Indicators (KPIs)

### Cache Hit Rate

Calculate cache hit rate:
```
cache_hit_rate = cache_hits_total / (cache_hits_total + cache_misses_total) * 100
```

**Target**: >80% hit rate for optimal performance

### Read Latency (P50, P95, P99)

Prometheus queries:
```promql
# P50 (median)
histogram_quantile(0.50, storage_read_duration_seconds)

# P95
histogram_quantile(0.95, storage_read_duration_seconds)

# P99
histogram_quantile(0.99, storage_read_duration_seconds)
```

**Targets**:
- P50: <10ms (cache hit)
- P95: <50ms (cache miss, Sled lookup)
- P99: <100ms

### Write Latency (P50, P95, P99)

```promql
histogram_quantile(0.50, storage_write_duration_seconds)
histogram_quantile(0.95, storage_write_duration_seconds)
histogram_quantile(0.99, storage_write_duration_seconds)
```

**Targets**:
- P50: <20ms
- P95: <100ms
- P99: <200ms

### Throughput

```promql
# Read throughput (ops/sec)
rate(storage_reads_total[1m])

# Write throughput (ops/sec)
rate(storage_writes_total[1m])

# Total throughput
rate(storage_reads_total[1m]) + rate(storage_writes_total[1m])
```

**Targets**:
- Read throughput: >1000 ops/sec
- Write throughput: >500 ops/sec

## Integration with Monitoring Systems

### Prometheus

Add to your `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'cyberfly-node'
    static_configs:
      - targets: ['localhost:8080']
    metrics_path: '/metrics'
    scrape_interval: 15s
```

### Grafana Dashboard

Create a Grafana dashboard with these panels:

#### 1. Cache Performance Panel
```promql
# Cache hit rate (%)
100 * (
  rate(cache_hits_total[5m]) / 
  (rate(cache_hits_total[5m]) + rate(cache_misses_total[5m]))
)
```

#### 2. Latency Panel (Heatmap)
```promql
storage_read_duration_seconds_bucket
storage_write_duration_seconds_bucket
```

#### 3. Throughput Panel
```promql
rate(storage_reads_total[1m])
rate(storage_writes_total[1m])
rate(storage_deletes_total[1m])
```

#### 4. Cache Size Panel
```promql
cache_size_hot
cache_size_warm
```

#### 5. Network Activity Panel
```promql
rate(network_bytes_sent_total[1m])
rate(network_bytes_received_total[1m])
network_peers_connected
```

## Alert Rules

Suggested Prometheus alert rules:

```yaml
groups:
  - name: cyberfly_alerts
    rules:
      # Low cache hit rate
      - alert: LowCacheHitRate
        expr: |
          100 * (
            rate(cache_hits_total[5m]) / 
            (rate(cache_hits_total[5m]) + rate(cache_misses_total[5m]))
          ) < 60
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "Cache hit rate below 60%"
          description: "Cache hit rate is {{ $value }}%, consider increasing cache size"
      
      # High read latency
      - alert: HighReadLatency
        expr: histogram_quantile(0.95, storage_read_duration_seconds) > 0.1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "P95 read latency above 100ms"
          description: "95th percentile read latency is {{ $value }}s"
      
      # High write latency
      - alert: HighWriteLatency
        expr: histogram_quantile(0.95, storage_write_duration_seconds) > 0.2
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "P95 write latency above 200ms"
          description: "95th percentile write latency is {{ $value }}s"
      
      # Low peer count
      - alert: LowPeerCount
        expr: network_peers_connected < 3
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "Less than 3 peers connected"
          description: "Only {{ $value }} peers connected"
      
      # High sync conflicts
      - alert: HighSyncConflicts
        expr: rate(sync_conflicts_total[5m]) > 10
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High rate of sync conflicts"
          description: "Sync conflict rate is {{ $value }}/sec"
```

## Testing Metrics

### 1. Manual Test

```bash
# Start the node
./target/release/cyberfly-rust-node

# In another terminal, fetch metrics
curl http://localhost:8080/metrics

# You should see output like:
# storage_reads_total 1234
# cache_hits_total 987
# cache_misses_total 247
# ...
```

### 2. Load Test

```bash
# Generate load
cd client-sdk
npm install
node examples/publish-test-messages.ts --count 10000 --concurrent 100

# Watch metrics
watch -n 1 "curl -s http://localhost:8080/metrics | grep -E '(cache_hit|storage_read|storage_write)'"
```

### 3. Calculate Cache Hit Rate

```bash
curl -s http://localhost:8080/metrics | \
  awk '/^cache_hits_total/ {hits=$2} /^cache_misses_total/ {misses=$2} END {print "Cache hit rate:", (hits/(hits+misses)*100) "%"}'
```

## Performance Tuning Based on Metrics

### If Cache Hit Rate < 70%

1. **Increase cache size**:
   - Modify `src/storage.rs` TieredCache::new()
   - Increase hot tier from 5k to 10k
   - Increase warm tier from 50k to 100k

2. **Adjust TTL**:
   - Increase hot tier TTL from 5min to 10min
   - Increase warm tier TTL from 1hr to 2hr

3. **Check access patterns**:
   - Use `cache_hot_hits_total` vs `cache_warm_hits_total`
   - If warm tier dominates, rebalance tier sizes

### If P95 Read Latency > 50ms

1. **Check cache misses**: High latency often correlates with cache misses
2. **Verify Sled configuration**: Ensure 2GB cache is allocated
3. **Check disk I/O**: Use `iostat` to monitor disk performance
4. **Consider SSD**: NVMe SSDs dramatically reduce Sled latency

### If P95 Write Latency > 100ms

1. **Check Iroh blob store**: Writes go through Iroh blobs + Sled
2. **Verify flush interval**: Should be 1s (src/storage.rs)
3. **Check concurrent writes**: High concurrency may need batch writer (task 5)
4. **Monitor disk**: Use `iostat -x 1` to check write saturation

### If Throughput < 500 ops/sec

1. **Profile with flamegraph**:
   ```bash
   cargo install flamegraph
   sudo flamegraph --bin cyberfly-rust-node
   ```

2. **Check blocking operations**: Should all use spawn_blocking

3. **Implement batch writer**: Task 5 - 5-10x write improvement

## Code Examples

### Custom Metrics in Application Code

```rust
use crate::metrics::{self, Timer};

// Track custom operation
pub async fn my_operation() -> Result<()> {
    let timer = Timer::new();
    
    // Your code here
    
    timer.observe_duration_seconds(&metrics::READ_LATENCY);
    metrics::STORAGE_READS.inc();
    
    Ok(())
}

// Track GraphQL operation
pub async fn graphql_query(operation: &str) -> Result<Response> {
    let timer = Timer::new();
    metrics::GRAPHQL_REQUESTS.with_label_values(&[operation]).inc();
    
    match execute_query().await {
        Ok(response) => {
            timer.observe_duration_seconds(
                &metrics::GRAPHQL_LATENCY.with_label_values(&[operation])
            );
            Ok(response)
        }
        Err(e) => {
            metrics::GRAPHQL_ERRORS.with_label_values(&[operation]).inc();
            Err(e)
        }
    }
}
```

## Architecture Notes

### Why These Metrics?

1. **Storage metrics**: Core database operations - every system needs these
2. **Cache metrics**: Tiered cache is critical for performance - must monitor hit rate
3. **Latency histograms**: Percentiles (P50/P95/P99) reveal tail latency issues
4. **Network metrics**: P2P system - peer count and bandwidth are vital
5. **Sync metrics**: CRDT conflicts indicate data contention

### Histogram Buckets

Latency histograms use these buckets (in seconds):
```
[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]
```

This covers:
- **Fast operations** (1-10ms): Cache hits
- **Normal operations** (10-100ms): Sled lookups
- **Slow operations** (100ms-1s): Blob fetches, network I/O
- **Very slow operations** (>1s): Network timeouts, conflicts

### Timer Implementation

The `Timer` struct uses `std::time::Instant` for high-precision timing:
```rust
let timer = Timer::new();  // Records start time
// ... operation ...
timer.observe_duration_seconds(&histogram);  // Records duration
```

Instant::now() has nanosecond precision on Linux, making it suitable for sub-millisecond measurements.

## Troubleshooting

### Metrics endpoint returns 404

Check if the node started successfully and the HTTP server is listening:
```bash
netstat -tlnp | grep 8080
```

### Metrics show all zeros

Metrics are initialized but no operations have occurred. Generate some load:
```bash
cd client-sdk
node examples/publish-test-messages.ts --count 100
```

### Prometheus can't scrape metrics

1. Check firewall: `sudo ufw status`
2. Verify metrics endpoint: `curl http://localhost:8080/metrics`
3. Check Prometheus logs: `docker logs prometheus`

## Next Steps

After implementing metrics (Task 4), the final optimization is:

**Task 5: Batch Writer** - Parallel write processing with semaphore-based concurrency control for 5-10x write throughput improvement.

With metrics in place, you'll be able to measure the exact performance gains from the batch writer implementation!
