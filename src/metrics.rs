use lazy_static::lazy_static;
use prometheus::{
    Counter, CounterVec, Histogram, HistogramOpts, HistogramVec, IntCounter, IntCounterVec,
    IntGauge, Opts, Registry,
};
use std::time::Instant;

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();
    
    // Storage metrics
    pub static ref STORAGE_READS: IntCounter = IntCounter::new(
        "storage_reads_total",
        "Total number of storage read operations"
    ).unwrap();
    
    pub static ref STORAGE_WRITES: IntCounter = IntCounter::new(
        "storage_writes_total",
        "Total number of storage write operations"
    ).unwrap();
    
    pub static ref STORAGE_DELETES: IntCounter = IntCounter::new(
        "storage_deletes_total",
        "Total number of storage delete operations"
    ).unwrap();
    
    // Cache metrics
    pub static ref CACHE_HITS: IntCounter = IntCounter::new(
        "cache_hits_total",
        "Total number of cache hits"
    ).unwrap();
    
    pub static ref CACHE_MISSES: IntCounter = IntCounter::new(
        "cache_misses_total",
        "Total number of cache misses"
    ).unwrap();
    
    pub static ref CACHE_HOT_HITS: IntCounter = IntCounter::new(
        "cache_hot_hits_total",
        "Total number of hot tier cache hits"
    ).unwrap();
    
    pub static ref CACHE_WARM_HITS: IntCounter = IntCounter::new(
        "cache_warm_hits_total",
        "Total number of warm tier cache hits"
    ).unwrap();
    
    pub static ref CACHE_SIZE_HOT: IntGauge = IntGauge::new(
        "cache_size_hot",
        "Current number of entries in hot cache tier"
    ).unwrap();
    
    pub static ref CACHE_SIZE_WARM: IntGauge = IntGauge::new(
        "cache_size_warm",
        "Current number of entries in warm cache tier"
    ).unwrap();
    
    // Latency metrics (in seconds)
    pub static ref READ_LATENCY: Histogram = Histogram::with_opts(
        HistogramOpts::new(
            "storage_read_duration_seconds",
            "Storage read operation latency in seconds"
        )
        .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0])
    ).unwrap();
    
    pub static ref WRITE_LATENCY: Histogram = Histogram::with_opts(
        HistogramOpts::new(
            "storage_write_duration_seconds",
            "Storage write operation latency in seconds"
        )
        .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0])
    ).unwrap();
    
    pub static ref DELETE_LATENCY: Histogram = Histogram::with_opts(
        HistogramOpts::new(
            "storage_delete_duration_seconds",
            "Storage delete operation latency in seconds"
        )
        .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0])
    ).unwrap();
    
    // GraphQL metrics
    pub static ref GRAPHQL_REQUESTS: IntCounterVec = IntCounterVec::new(
        Opts::new("graphql_requests_total", "Total number of GraphQL requests"),
        &["operation"]
    ).unwrap();
    
    pub static ref GRAPHQL_ERRORS: IntCounterVec = IntCounterVec::new(
        Opts::new("graphql_errors_total", "Total number of GraphQL errors"),
        &["operation"]
    ).unwrap();
    
    pub static ref GRAPHQL_LATENCY: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "graphql_duration_seconds",
            "GraphQL operation latency in seconds"
        )
        .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]),
        &["operation"]
    ).unwrap();
    
    // Network metrics
    pub static ref NETWORK_PEERS: IntGauge = IntGauge::new(
        "network_peers_connected",
        "Current number of connected peers"
    ).unwrap();
    
    pub static ref NETWORK_BYTES_SENT: Counter = Counter::new(
        "network_bytes_sent_total",
        "Total bytes sent over the network"
    ).unwrap();
    
    pub static ref NETWORK_BYTES_RECEIVED: Counter = Counter::new(
        "network_bytes_received_total",
        "Total bytes received from the network"
    ).unwrap();
    
    // Sync metrics
    pub static ref SYNC_OPERATIONS: IntCounter = IntCounter::new(
        "sync_operations_total",
        "Total number of sync operations"
    ).unwrap();
    
    pub static ref SYNC_CONFLICTS: IntCounter = IntCounter::new(
        "sync_conflicts_total",
        "Total number of sync conflicts resolved"
    ).unwrap();
    
    pub static ref SYNC_MERGES: IntCounter = IntCounter::new(
        "sync_merges_total",
        "Total number of CRDT merges"
    ).unwrap();
    
    // Extended peer metrics
    pub static ref PEER_CONNECTIONS_TOTAL: IntCounter = IntCounter::new(
        "peer_connections_total",
        "Total number of peer connection attempts"
    ).unwrap();
    
    pub static ref PEER_CONNECTION_FAILURES: IntCounter = IntCounter::new(
        "peer_connection_failures_total",
        "Total number of peer connection failures"
    ).unwrap();
    
    pub static ref PEER_EXPIRATIONS: IntCounter = IntCounter::new(
        "peer_expirations_total",
        "Total number of peers expired due to inactivity"
    ).unwrap();
    
    pub static ref PEER_ANNOUNCEMENTS_RECEIVED: IntCounter = IntCounter::new(
        "peer_announcements_received_total",
        "Total number of peer discovery announcements received"
    ).unwrap();
    
    pub static ref PEER_ANNOUNCEMENTS_SENT: IntCounter = IntCounter::new(
        "peer_announcements_sent_total",
        "Total number of peer discovery announcements sent"
    ).unwrap();
    
    pub static ref GOSSIP_MESSAGES_RECEIVED: IntCounterVec = IntCounterVec::new(
        Opts::new("gossip_messages_received_total", "Total gossip messages received by topic"),
        &["topic"]
    ).unwrap();
    
    pub static ref GOSSIP_MESSAGES_SENT: IntCounterVec = IntCounterVec::new(
        Opts::new("gossip_messages_sent_total", "Total gossip messages sent by topic"),
        &["topic"]
    ).unwrap();
}

/// Initialize metrics registry
pub fn init_metrics() {
    // Register storage metrics
    REGISTRY.register(Box::new(STORAGE_READS.clone())).unwrap();
    REGISTRY.register(Box::new(STORAGE_WRITES.clone())).unwrap();
    REGISTRY.register(Box::new(STORAGE_DELETES.clone())).unwrap();
    
    // Register cache metrics
    REGISTRY.register(Box::new(CACHE_HITS.clone())).unwrap();
    REGISTRY.register(Box::new(CACHE_MISSES.clone())).unwrap();
    REGISTRY.register(Box::new(CACHE_HOT_HITS.clone())).unwrap();
    REGISTRY.register(Box::new(CACHE_WARM_HITS.clone())).unwrap();
    REGISTRY.register(Box::new(CACHE_SIZE_HOT.clone())).unwrap();
    REGISTRY.register(Box::new(CACHE_SIZE_WARM.clone())).unwrap();
    
    // Register latency metrics
    REGISTRY.register(Box::new(READ_LATENCY.clone())).unwrap();
    REGISTRY.register(Box::new(WRITE_LATENCY.clone())).unwrap();
    REGISTRY.register(Box::new(DELETE_LATENCY.clone())).unwrap();
    
    // Register GraphQL metrics
    REGISTRY.register(Box::new(GRAPHQL_REQUESTS.clone())).unwrap();
    REGISTRY.register(Box::new(GRAPHQL_ERRORS.clone())).unwrap();
    REGISTRY.register(Box::new(GRAPHQL_LATENCY.clone())).unwrap();
    
    // Register network metrics
    REGISTRY.register(Box::new(NETWORK_PEERS.clone())).unwrap();
    REGISTRY.register(Box::new(NETWORK_BYTES_SENT.clone())).unwrap();
    REGISTRY.register(Box::new(NETWORK_BYTES_RECEIVED.clone())).unwrap();
    
    // Register sync metrics
    REGISTRY.register(Box::new(SYNC_OPERATIONS.clone())).unwrap();
    REGISTRY.register(Box::new(SYNC_CONFLICTS.clone())).unwrap();
    REGISTRY.register(Box::new(SYNC_MERGES.clone())).unwrap();
    
    // Register peer metrics
    REGISTRY.register(Box::new(PEER_CONNECTIONS_TOTAL.clone())).unwrap();
    REGISTRY.register(Box::new(PEER_CONNECTION_FAILURES.clone())).unwrap();
    REGISTRY.register(Box::new(PEER_EXPIRATIONS.clone())).unwrap();
    REGISTRY.register(Box::new(PEER_ANNOUNCEMENTS_RECEIVED.clone())).unwrap();
    REGISTRY.register(Box::new(PEER_ANNOUNCEMENTS_SENT.clone())).unwrap();
    REGISTRY.register(Box::new(GOSSIP_MESSAGES_RECEIVED.clone())).unwrap();
    REGISTRY.register(Box::new(GOSSIP_MESSAGES_SENT.clone())).unwrap();
    
    tracing::info!("Metrics registry initialized with {} collectors", REGISTRY.gather().len());
}

/// Get cache hit rate as a percentage
pub fn cache_hit_rate() -> f64 {
    let hits = CACHE_HITS.get() as f64;
    let misses = CACHE_MISSES.get() as f64;
    let total = hits + misses;
    
    if total == 0.0 {
        0.0
    } else {
        (hits / total) * 100.0
    }
}

/// Helper struct for timing operations
pub struct Timer {
    start: Instant,
}

impl Timer {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
    
    pub fn observe_duration_seconds(&self, histogram: &Histogram) {
        let duration = self.start.elapsed();
        histogram.observe(duration.as_secs_f64());
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

/// Export metrics in Prometheus format
pub fn export_metrics() -> String {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}
