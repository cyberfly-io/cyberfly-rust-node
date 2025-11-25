//! Peer Registry Module
//!
//! Centralized peer lifecycle management extracted from iroh_network.rs
//! for better maintainability and testability.
//!
//! ## Responsibilities
//! - Track peer metadata (first_seen, last_seen, address, region, capabilities)
//! - Manage peer expiration with configurable timeouts
//! - Provide peer selection strategies (region diversity, load balancing)
//! - Expose metrics for observability

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use iroh::EndpointId;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

/// Peer capabilities advertised in discovery
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PeerCapabilities {
    pub supports_mqtt: bool,
    pub supports_streams: bool,
    pub supports_timeseries: bool,
    pub supports_geo: bool,
}

/// Connection status of a peer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeerStatus {
    /// Actively connected via gossip
    Connected,
    /// Recently seen but not currently connected
    Idle,
    /// Not seen for a while, may be stale
    Stale,
    /// Marked for removal
    Expired,
}

/// Rich metadata about a discovered peer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerMeta {
    /// When we first discovered this peer
    pub first_seen: DateTime<Utc>,
    /// Last activity timestamp (message received or connection)
    pub last_seen: DateTime<Utc>,
    /// Known socket address (if any)
    pub addr: Option<SocketAddr>,
    /// Last known region from peer announcements
    pub region: Option<String>,
    /// Advertised capabilities
    pub capabilities: PeerCapabilities,
    /// Connection failure count (for backoff)
    pub failure_count: u32,
    /// Last connection failure time
    pub last_failure: Option<DateTime<Utc>>,
    /// Whether this is a bootstrap peer
    pub is_bootstrap: bool,
}

impl PeerMeta {
    /// Create new peer metadata
    pub fn new(addr: Option<SocketAddr>, is_bootstrap: bool) -> Self {
        let now = Utc::now();
        Self {
            first_seen: now,
            last_seen: now,
            addr,
            region: None,
            capabilities: PeerCapabilities::default(),
            failure_count: 0,
            last_failure: None,
            is_bootstrap,
        }
    }

    /// Update last_seen timestamp
    pub fn touch(&mut self) {
        self.last_seen = Utc::now();
    }

    /// Record a connection failure
    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure = Some(Utc::now());
    }

    /// Reset failure count on successful connection
    pub fn reset_failures(&mut self) {
        self.failure_count = 0;
        self.last_failure = None;
    }

    /// Check if peer should be in backoff (too many recent failures)
    pub fn in_backoff(&self, max_failures: u32, backoff_duration: Duration) -> bool {
        if self.failure_count < max_failures {
            return false;
        }
        if let Some(last_failure) = self.last_failure {
            let elapsed = Utc::now().signed_duration_since(last_failure);
            elapsed.num_seconds() < backoff_duration.as_secs() as i64
        } else {
            false
        }
    }

    /// Get peer status based on timeouts
    pub fn status(&self, idle_timeout: Duration, stale_timeout: Duration) -> PeerStatus {
        let now = Utc::now();
        let since_last_seen = now.signed_duration_since(self.last_seen);
        let seconds = since_last_seen.num_seconds();

        if seconds < idle_timeout.as_secs() as i64 {
            PeerStatus::Connected
        } else if seconds < stale_timeout.as_secs() as i64 {
            PeerStatus::Idle
        } else {
            PeerStatus::Stale
        }
    }
}

/// Configuration for PeerRegistry
#[derive(Debug, Clone)]
pub struct PeerRegistryConfig {
    /// Duration after which idle peers are considered stale
    pub idle_timeout: Duration,
    /// Duration after which stale peers are expired/removed
    pub expiration_timeout: Duration,
    /// How often to run cleanup
    pub cleanup_interval: Duration,
    /// Maximum failures before backoff
    pub max_failures: u32,
    /// How long to wait after max failures before retrying
    pub backoff_duration: Duration,
    /// Target number of connected peers
    pub target_peer_count: usize,
}

impl Default for PeerRegistryConfig {
    fn default() -> Self {
        Self {
            idle_timeout: Duration::from_secs(15),
            expiration_timeout: Duration::from_secs(30),
            cleanup_interval: Duration::from_secs(10),
            max_failures: 5,
            backoff_duration: Duration::from_secs(300), // 5 minutes
            target_peer_count: 8,
        }
    }
}

/// Centralized peer registry for tracking discovered peers
pub struct PeerRegistry {
    /// Peer metadata indexed by EndpointId
    peers: Arc<DashMap<EndpointId, PeerMeta>>,
    /// Announcement deduplication cache (node_id -> last_timestamp)
    announcement_cache: Arc<DashMap<String, i64>>,
    /// Configuration
    config: PeerRegistryConfig,
    /// Local node ID (to filter self)
    local_node_id: EndpointId,
}

impl PeerRegistry {
    /// Create a new peer registry
    pub fn new(local_node_id: EndpointId, config: PeerRegistryConfig) -> Self {
        Self {
            peers: Arc::new(DashMap::new()),
            announcement_cache: Arc::new(DashMap::new()),
            config,
            local_node_id,
        }
    }

    /// Get a clone of the peers DashMap for external use
    pub fn peers_map(&self) -> Arc<DashMap<EndpointId, PeerMeta>> {
        Arc::clone(&self.peers)
    }

    /// Get announcement cache for deduplication
    pub fn announcement_cache(&self) -> Arc<DashMap<String, i64>> {
        Arc::clone(&self.announcement_cache)
    }

    /// Add or update a peer
    pub fn upsert_peer(
        &self,
        peer_id: EndpointId,
        addr: Option<SocketAddr>,
        is_bootstrap: bool,
    ) {
        // Don't track ourselves
        if peer_id == self.local_node_id {
            return;
        }

        self.peers
            .entry(peer_id)
            .and_modify(|meta| {
                meta.touch();
                if addr.is_some() {
                    meta.addr = addr;
                }
            })
            .or_insert_with(|| PeerMeta::new(addr, is_bootstrap));

        // Update metrics
        crate::metrics::NETWORK_PEERS.set(self.peers.len() as i64);
    }

    /// Update peer's region
    pub fn set_peer_region(&self, peer_id: EndpointId, region: String) {
        if let Some(mut entry) = self.peers.get_mut(&peer_id) {
            entry.region = Some(region);
        }
    }

    /// Update peer's capabilities
    pub fn set_peer_capabilities(&self, peer_id: EndpointId, capabilities: PeerCapabilities) {
        if let Some(mut entry) = self.peers.get_mut(&peer_id) {
            entry.capabilities = capabilities;
        }
    }

    /// Record a peer connection failure
    pub fn record_failure(&self, peer_id: EndpointId) {
        if let Some(mut entry) = self.peers.get_mut(&peer_id) {
            entry.record_failure();
        }
    }

    /// Record successful connection
    pub fn record_success(&self, peer_id: EndpointId) {
        if let Some(mut entry) = self.peers.get_mut(&peer_id) {
            entry.reset_failures();
            entry.touch();
        }
    }

    /// Touch peer (update last_seen)
    pub fn touch_peer(&self, peer_id: EndpointId) {
        if let Some(mut entry) = self.peers.get_mut(&peer_id) {
            entry.touch();
        }
    }

    /// Check if peer is in backoff
    pub fn is_in_backoff(&self, peer_id: EndpointId) -> bool {
        if let Some(entry) = self.peers.get(&peer_id) {
            entry.in_backoff(self.config.max_failures, self.config.backoff_duration)
        } else {
            false
        }
    }

    /// Get peer count
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Get count of peers by status
    pub fn peer_count_by_status(&self) -> (usize, usize, usize) {
        let mut connected = 0;
        let mut idle = 0;
        let mut stale = 0;

        for entry in self.peers.iter() {
            match entry.status(self.config.idle_timeout, self.config.expiration_timeout) {
                PeerStatus::Connected => connected += 1,
                PeerStatus::Idle => idle += 1,
                PeerStatus::Stale | PeerStatus::Expired => stale += 1,
            }
        }

        (connected, idle, stale)
    }

    /// Get peers suitable for connection (not in backoff, not stale)
    pub fn get_connectable_peers(&self) -> Vec<(EndpointId, Option<SocketAddr>)> {
        self.peers
            .iter()
            .filter(|entry| {
                let status = entry.status(self.config.idle_timeout, self.config.expiration_timeout);
                status != PeerStatus::Expired
                    && !entry.in_backoff(self.config.max_failures, self.config.backoff_duration)
            })
            .map(|entry| (*entry.key(), entry.addr))
            .collect()
    }

    /// Get peers from different regions for diversity
    pub fn get_diverse_peers(&self, exclude_region: Option<&str>, limit: usize) -> Vec<EndpointId> {
        let mut by_region: std::collections::HashMap<String, Vec<EndpointId>> =
            std::collections::HashMap::new();

        for entry in self.peers.iter() {
            let region = entry.region.clone().unwrap_or_else(|| "unknown".to_string());
            if exclude_region.map_or(true, |r| r != region) {
                by_region.entry(region).or_default().push(*entry.key());
            }
        }

        // Take one peer from each region in round-robin
        let mut result = Vec::with_capacity(limit);
        let mut regions: Vec<_> = by_region.into_iter().collect();
        regions.sort_by_key(|(region, _)| region.clone());

        let mut idx = 0;
        while result.len() < limit && !regions.is_empty() {
            let region_idx = idx % regions.len();
            if let Some(peer_id) = regions[region_idx].1.pop() {
                result.push(peer_id);
            }
            if regions[region_idx].1.is_empty() {
                regions.remove(region_idx);
            } else {
                idx += 1;
            }
        }

        result
    }

    /// Run expiration cleanup, returns removed peer IDs
    pub fn cleanup_expired(&self) -> Vec<EndpointId> {
        let mut expired = Vec::new();

        self.peers.retain(|peer_id, meta| {
            let status = meta.status(self.config.idle_timeout, self.config.expiration_timeout);
            if status == PeerStatus::Stale || status == PeerStatus::Expired {
                // Keep bootstrap peers longer
                if meta.is_bootstrap {
                    let since = Utc::now().signed_duration_since(meta.last_seen);
                    // Give bootstrap peers 5x longer before expiring
                    if since.num_seconds() < (self.config.expiration_timeout.as_secs() * 5) as i64 {
                        return true;
                    }
                }
                expired.push(*peer_id);
                false
            } else {
                true
            }
        });

        // Update metrics
        crate::metrics::NETWORK_PEERS.set(self.peers.len() as i64);

        expired
    }

    /// Get all peers as a list for broadcasting
    pub fn get_peer_list(&self) -> Vec<String> {
        self.peers
            .iter()
            .map(|entry| {
                let peer_id = entry.key();
                if let Some(addr) = entry.addr {
                    format!("{}@{}", peer_id, addr)
                } else {
                    peer_id.to_string()
                }
            })
            .collect()
    }

    /// Get peer info for GraphQL
    pub fn get_peer_info(&self, peer_id: EndpointId) -> Option<PeerMeta> {
        self.peers.get(&peer_id).map(|entry| entry.clone())
    }

    /// Get all peer info for GraphQL
    pub fn get_all_peer_info(&self) -> Vec<(EndpointId, PeerMeta)> {
        self.peers
            .iter()
            .map(|entry| (*entry.key(), entry.clone()))
            .collect()
    }

    /// Check if we should dial more peers
    pub fn needs_more_peers(&self) -> bool {
        let (connected, _, _) = self.peer_count_by_status();
        connected < self.config.target_peer_count
    }

    /// Check if an announcement is newer than cached
    pub fn is_newer_announcement(&self, node_id: &str, timestamp: i64) -> bool {
        if let Some(cached) = self.announcement_cache.get(node_id) {
            timestamp > *cached
        } else {
            true
        }
    }

    /// Update announcement cache
    pub fn cache_announcement(&self, node_id: String, timestamp: i64) {
        self.announcement_cache.insert(node_id, timestamp);
    }

    /// Cleanup old announcement cache entries to prevent unbounded growth
    /// Removes entries older than the given max_age_seconds
    pub fn cleanup_announcement_cache(&self, max_age_seconds: i64) {
        let now = Utc::now().timestamp();
        let cutoff = now - max_age_seconds;
        
        self.announcement_cache.retain(|_node_id, timestamp| {
            *timestamp > cutoff
        });
    }

    /// Run full cleanup: expired peers + old announcements
    pub fn run_full_cleanup(&self) -> (Vec<EndpointId>, usize) {
        let expired_peers = self.cleanup_expired();
        let cache_size_before = self.announcement_cache.len();
        
        // Clean announcements older than 1 hour
        self.cleanup_announcement_cache(3600);
        
        let announcements_removed = cache_size_before - self.announcement_cache.len();
        (expired_peers, announcements_removed)
    }
}

/// Summary statistics for GraphQL/metrics
#[derive(Debug, Clone, Serialize)]
pub struct PeerSummary {
    pub total_peers: usize,
    pub connected_peers: usize,
    pub idle_peers: usize,
    pub stale_peers: usize,
    pub bootstrap_peers: usize,
    pub peers_by_region: std::collections::HashMap<String, usize>,
    pub peers_with_address: usize,
}

impl PeerRegistry {
    /// Get summary statistics
    pub fn summary(&self) -> PeerSummary {
        let mut connected = 0;
        let mut idle = 0;
        let mut stale = 0;
        let mut bootstrap = 0;
        let mut with_address = 0;
        let mut by_region: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        for entry in self.peers.iter() {
            let status = entry.status(self.config.idle_timeout, self.config.expiration_timeout);
            match status {
                PeerStatus::Connected => connected += 1,
                PeerStatus::Idle => idle += 1,
                PeerStatus::Stale | PeerStatus::Expired => stale += 1,
            }

            if entry.is_bootstrap {
                bootstrap += 1;
            }
            if entry.addr.is_some() {
                with_address += 1;
            }

            let region = entry.region.clone().unwrap_or_else(|| "unknown".to_string());
            *by_region.entry(region).or_insert(0) += 1;
        }

        PeerSummary {
            total_peers: self.peers.len(),
            connected_peers: connected,
            idle_peers: idle,
            stale_peers: stale,
            bootstrap_peers: bootstrap,
            peers_by_region: by_region,
            peers_with_address: with_address,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_node_id() -> EndpointId {
        // Use thread_rng which provides CryptoRng
        let mut rng = rand::thread_rng();
        let secret = iroh::SecretKey::generate(&mut rng);
        secret.public()
    }

    fn other_node_id() -> EndpointId {
        let mut rng = rand::thread_rng();
        let secret = iroh::SecretKey::generate(&mut rng);
        secret.public()
    }

    #[test]
    fn test_peer_meta_status() {
        let mut meta = PeerMeta::new(None, false);
        let idle_timeout = Duration::from_secs(10);
        let stale_timeout = Duration::from_secs(30);

        assert_eq!(meta.status(idle_timeout, stale_timeout), PeerStatus::Connected);

        // Simulate time passing
        meta.last_seen = Utc::now() - chrono::Duration::seconds(15);
        assert_eq!(meta.status(idle_timeout, stale_timeout), PeerStatus::Idle);

        meta.last_seen = Utc::now() - chrono::Duration::seconds(35);
        assert_eq!(meta.status(idle_timeout, stale_timeout), PeerStatus::Stale);
    }

    #[test]
    fn test_peer_registry_upsert() {
        let local_id = test_node_id();
        let registry = PeerRegistry::new(local_id, PeerRegistryConfig::default());

        let peer_id = other_node_id();
        registry.upsert_peer(peer_id, None, false);
        assert_eq!(registry.peer_count(), 1);

        // Upsert same peer updates it
        registry.upsert_peer(peer_id, Some("127.0.0.1:8080".parse().unwrap()), false);
        assert_eq!(registry.peer_count(), 1);

        // Don't track self
        registry.upsert_peer(local_id, None, false);
        assert_eq!(registry.peer_count(), 1);
    }

    #[test]
    fn test_peer_backoff() {
        let mut meta = PeerMeta::new(None, false);
        let max_failures = 3;
        let backoff = Duration::from_secs(60);

        assert!(!meta.in_backoff(max_failures, backoff));

        meta.record_failure();
        meta.record_failure();
        meta.record_failure();
        assert!(meta.in_backoff(max_failures, backoff));

        meta.reset_failures();
        assert!(!meta.in_backoff(max_failures, backoff));
    }
}
