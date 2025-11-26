//! Network Resilience Module
//!
//! Provides network reliability features:
//! - **Circuit Breaker**: Prevents hammering failing peers
//! - **Peer Reputation**: Tracks peer reliability and prefers good peers
//! - **Bandwidth Throttling**: Rate limits network traffic
//!
//! ## Circuit Breaker States
//! ```text
//! ┌─────────┐  failure_threshold  ┌─────────┐
//! │  Closed │ ─────────────────▶ │  Open   │
//! │(normal) │                    │(blocked)│
//! └────┬────┘                    └────┬────┘
//!      │                              │
//!      │ success                      │ recovery_timeout
//!      │                              ▼
//!      │                        ┌──────────┐
//!      └──────────────────────  │Half-Open │
//!             test success      │ (testing)│
//!                               └──────────┘
//! ```

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use iroh::EndpointId;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::Instant;

// ============================================================================
// Circuit Breaker
// ============================================================================

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    /// Normal operation, requests are allowed
    Closed,
    /// Too many failures, requests are blocked
    Open,
    /// Testing if the service has recovered
    HalfOpen,
}

impl Default for CircuitState {
    fn default() -> Self {
        Self::Closed
    }
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening the circuit
    pub failure_threshold: u32,
    /// Time to wait before testing recovery (in Open state)
    pub recovery_timeout: Duration,
    /// Number of successful requests to close the circuit from HalfOpen
    pub success_threshold: u32,
    /// Time window for counting failures (rolling window)
    pub failure_window: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(30),
            success_threshold: 2,
            failure_window: Duration::from_secs(60),
        }
    }
}

/// Per-peer circuit breaker state
#[derive(Debug)]
struct PeerCircuitState {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    last_failure: Option<Instant>,
    opened_at: Option<Instant>,
    failure_timestamps: Vec<Instant>,
}

impl Default for PeerCircuitState {
    fn default() -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            last_failure: None,
            opened_at: None,
            failure_timestamps: Vec::new(),
        }
    }
}

/// Circuit breaker for managing peer connections
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    peer_states: DashMap<EndpointId, PeerCircuitState>,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            peer_states: DashMap::new(),
        }
    }

    /// Check if a request to the peer should be allowed
    pub fn should_allow(&self, peer_id: EndpointId) -> bool {
        let mut entry = self.peer_states.entry(peer_id).or_default();
        let state = entry.value_mut();
        let now = Instant::now();

        match state.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if recovery timeout has passed
                if let Some(opened_at) = state.opened_at {
                    if now.duration_since(opened_at) >= self.config.recovery_timeout {
                        // Transition to HalfOpen
                        state.state = CircuitState::HalfOpen;
                        state.success_count = 0;
                        tracing::info!(
                            peer = %peer_id,
                            "Circuit breaker transitioning to HalfOpen"
                        );
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => {
                // Allow limited requests to test
                true
            }
        }
    }

    /// Record a successful request
    pub fn record_success(&self, peer_id: EndpointId) {
        let mut entry = self.peer_states.entry(peer_id).or_default();
        let state = entry.value_mut();

        match state.state {
            CircuitState::Closed => {
                // Reset failure count on success
                state.failure_count = 0;
                state.failure_timestamps.clear();
            }
            CircuitState::HalfOpen => {
                state.success_count += 1;
                if state.success_count >= self.config.success_threshold {
                    // Transition to Closed
                    state.state = CircuitState::Closed;
                    state.failure_count = 0;
                    state.success_count = 0;
                    state.opened_at = None;
                    state.failure_timestamps.clear();
                    tracing::info!(
                        peer = %peer_id,
                        "Circuit breaker closed (recovered)"
                    );
                    
                    // Update metrics
                    crate::metrics::CIRCUIT_BREAKER_STATE.with_label_values(&[&peer_id.to_string(), "closed"]).set(1);
                    crate::metrics::CIRCUIT_BREAKER_STATE.with_label_values(&[&peer_id.to_string(), "half_open"]).set(0);
                }
            }
            CircuitState::Open => {
                // Shouldn't happen, but handle gracefully
            }
        }
    }

    /// Record a failed request
    pub fn record_failure(&self, peer_id: EndpointId) {
        let mut entry = self.peer_states.entry(peer_id).or_default();
        let state = entry.value_mut();
        let now = Instant::now();

        // Clean old failures outside the window
        state.failure_timestamps.retain(|&t| {
            now.duration_since(t) < self.config.failure_window
        });

        state.failure_timestamps.push(now);
        state.failure_count = state.failure_timestamps.len() as u32;
        state.last_failure = Some(now);

        match state.state {
            CircuitState::Closed => {
                if state.failure_count >= self.config.failure_threshold {
                    // Open the circuit
                    state.state = CircuitState::Open;
                    state.opened_at = Some(now);
                    tracing::warn!(
                        peer = %peer_id,
                        failures = state.failure_count,
                        "Circuit breaker opened"
                    );
                    
                    // Update metrics
                    crate::metrics::CIRCUIT_BREAKER_STATE.with_label_values(&[&peer_id.to_string(), "open"]).set(1);
                    crate::metrics::CIRCUIT_BREAKER_STATE.with_label_values(&[&peer_id.to_string(), "closed"]).set(0);
                    crate::metrics::CIRCUIT_BREAKER_TRIPS.inc();
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in HalfOpen goes back to Open
                state.state = CircuitState::Open;
                state.opened_at = Some(now);
                state.success_count = 0;
                tracing::warn!(
                    peer = %peer_id,
                    "Circuit breaker re-opened from HalfOpen"
                );
                
                crate::metrics::CIRCUIT_BREAKER_STATE.with_label_values(&[&peer_id.to_string(), "open"]).set(1);
                crate::metrics::CIRCUIT_BREAKER_STATE.with_label_values(&[&peer_id.to_string(), "half_open"]).set(0);
            }
            CircuitState::Open => {
                // Already open, just update timestamp
                state.opened_at = Some(now);
            }
        }
    }

    /// Get the current state for a peer
    pub fn get_state(&self, peer_id: EndpointId) -> CircuitState {
        self.peer_states
            .get(&peer_id)
            .map(|s| s.state)
            .unwrap_or(CircuitState::Closed)
    }

    /// Get summary of all circuit breaker states
    pub fn get_summary(&self) -> CircuitBreakerSummary {
        let mut closed = 0;
        let mut open = 0;
        let mut half_open = 0;

        for entry in self.peer_states.iter() {
            match entry.state {
                CircuitState::Closed => closed += 1,
                CircuitState::Open => open += 1,
                CircuitState::HalfOpen => half_open += 1,
            }
        }

        CircuitBreakerSummary {
            total_peers: self.peer_states.len(),
            closed,
            open,
            half_open,
        }
    }

    /// Reset circuit breaker for a peer
    pub fn reset(&self, peer_id: EndpointId) {
        self.peer_states.remove(&peer_id);
    }

    /// Clear all circuit breaker states
    pub fn clear_all(&self) {
        self.peer_states.clear();
    }
}

/// Summary of circuit breaker states
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerSummary {
    pub total_peers: usize,
    pub closed: usize,
    pub open: usize,
    pub half_open: usize,
}

// ============================================================================
// Peer Reputation System
// ============================================================================

/// Peer reputation configuration
#[derive(Debug, Clone)]
pub struct ReputationConfig {
    /// Initial reputation score for new peers
    pub initial_score: f64,
    /// Maximum reputation score
    pub max_score: f64,
    /// Minimum reputation score (below this, peer is banned)
    pub min_score: f64,
    /// Score gain per successful sync
    pub sync_success_gain: f64,
    /// Score loss per failed sync
    pub sync_failure_loss: f64,
    /// Score gain per successful message delivery
    pub message_success_gain: f64,
    /// Score loss per failed message delivery
    pub message_failure_loss: f64,
    /// Decay factor per hour (reputation decays towards initial)
    pub hourly_decay_factor: f64,
    /// Minimum latency weight (lower latency = higher weight)
    pub latency_weight: f64,
}

impl Default for ReputationConfig {
    fn default() -> Self {
        Self {
            initial_score: 50.0,
            max_score: 100.0,
            min_score: 0.0,
            sync_success_gain: 2.0,
            sync_failure_loss: 5.0,
            message_success_gain: 0.5,
            message_failure_loss: 1.0,
            hourly_decay_factor: 0.99,
            latency_weight: 0.1,
        }
    }
}

/// Reputation data for a single peer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerReputation {
    /// Current reputation score
    pub score: f64,
    /// Total successful sync operations
    pub successful_syncs: u64,
    /// Total failed sync operations
    pub failed_syncs: u64,
    /// Total successful message deliveries
    pub successful_messages: u64,
    /// Total failed message deliveries
    pub failed_messages: u64,
    /// Average latency in milliseconds
    pub avg_latency_ms: f64,
    /// Latency sample count (for rolling average)
    pub latency_samples: u64,
    /// Total bytes received from peer
    pub bytes_received: u64,
    /// Total bytes sent to peer
    pub bytes_sent: u64,
    /// First interaction timestamp
    pub first_seen: DateTime<Utc>,
    /// Last interaction timestamp
    pub last_seen: DateTime<Utc>,
    /// Last decay calculation timestamp
    pub last_decay: DateTime<Utc>,
    /// Whether peer is currently banned
    pub is_banned: bool,
    /// Ban reason if banned
    pub ban_reason: Option<String>,
}

impl PeerReputation {
    pub fn new(initial_score: f64) -> Self {
        let now = Utc::now();
        Self {
            score: initial_score,
            successful_syncs: 0,
            failed_syncs: 0,
            successful_messages: 0,
            failed_messages: 0,
            avg_latency_ms: 0.0,
            latency_samples: 0,
            bytes_received: 0,
            bytes_sent: 0,
            first_seen: now,
            last_seen: now,
            last_decay: now,
            is_banned: false,
            ban_reason: None,
        }
    }

    /// Calculate reliability ratio (0.0 to 1.0)
    pub fn reliability(&self) -> f64 {
        let total_syncs = self.successful_syncs + self.failed_syncs;
        let total_messages = self.successful_messages + self.failed_messages;
        let total = total_syncs + total_messages;
        
        if total == 0 {
            return 0.5; // Unknown reliability
        }
        
        let successes = self.successful_syncs + self.successful_messages;
        successes as f64 / total as f64
    }

    /// Calculate uptime (time since first seen vs time since last seen)
    pub fn uptime_hours(&self) -> f64 {
        let duration = self.last_seen.signed_duration_since(self.first_seen);
        duration.num_minutes() as f64 / 60.0
    }
}

/// Peer reputation manager
pub struct ReputationManager {
    config: ReputationConfig,
    reputations: Arc<DashMap<EndpointId, PeerReputation>>,
}

impl ReputationManager {
    pub fn new(config: ReputationConfig) -> Self {
        Self {
            config,
            reputations: Arc::new(DashMap::new()),
        }
    }

    /// Get or create reputation for a peer
    fn get_or_create(&self, peer_id: EndpointId) -> dashmap::mapref::one::RefMut<EndpointId, PeerReputation> {
        self.reputations
            .entry(peer_id)
            .or_insert_with(|| PeerReputation::new(self.config.initial_score))
    }

    /// Record a successful sync operation
    pub fn record_sync_success(&self, peer_id: EndpointId, latency_ms: Option<f64>) {
        let mut rep = self.get_or_create(peer_id);
        rep.successful_syncs += 1;
        rep.score = (rep.score + self.config.sync_success_gain).min(self.config.max_score);
        rep.last_seen = Utc::now();
        
        if let Some(latency) = latency_ms {
            self.update_latency(&mut rep, latency);
        }
        
        // Update metrics
        crate::metrics::PEER_REPUTATION_SCORE.with_label_values(&[&peer_id.to_string()]).set(rep.score);
    }

    /// Record a failed sync operation
    pub fn record_sync_failure(&self, peer_id: EndpointId) {
        let mut rep = self.get_or_create(peer_id);
        rep.failed_syncs += 1;
        rep.score = (rep.score - self.config.sync_failure_loss).max(self.config.min_score);
        rep.last_seen = Utc::now();
        
        // Check for ban
        if rep.score <= self.config.min_score {
            rep.is_banned = true;
            rep.ban_reason = Some("Reputation score too low".to_string());
            tracing::warn!(peer = %peer_id, "Peer banned due to low reputation");
        }
        
        crate::metrics::PEER_REPUTATION_SCORE.with_label_values(&[&peer_id.to_string()]).set(rep.score);
    }

    /// Record a successful message delivery
    pub fn record_message_success(&self, peer_id: EndpointId, bytes: u64) {
        let mut rep = self.get_or_create(peer_id);
        rep.successful_messages += 1;
        rep.bytes_received += bytes;
        rep.score = (rep.score + self.config.message_success_gain).min(self.config.max_score);
        rep.last_seen = Utc::now();
    }

    /// Record a failed message delivery
    pub fn record_message_failure(&self, peer_id: EndpointId) {
        let mut rep = self.get_or_create(peer_id);
        rep.failed_messages += 1;
        rep.score = (rep.score - self.config.message_failure_loss).max(self.config.min_score);
        rep.last_seen = Utc::now();
        
        if rep.score <= self.config.min_score {
            rep.is_banned = true;
            rep.ban_reason = Some("Reputation score too low".to_string());
        }
    }

    /// Record bytes sent to a peer
    pub fn record_bytes_sent(&self, peer_id: EndpointId, bytes: u64) {
        let mut rep = self.get_or_create(peer_id);
        rep.bytes_sent += bytes;
        rep.last_seen = Utc::now();
    }

    /// Update rolling average latency
    fn update_latency(&self, rep: &mut PeerReputation, latency_ms: f64) {
        // Exponential moving average
        if rep.latency_samples == 0 {
            rep.avg_latency_ms = latency_ms;
        } else {
            let alpha = 0.2; // Smoothing factor
            rep.avg_latency_ms = alpha * latency_ms + (1.0 - alpha) * rep.avg_latency_ms;
        }
        rep.latency_samples += 1;
    }

    /// Apply hourly decay to reputation scores
    pub fn apply_decay(&self) {
        let now = Utc::now();
        
        for mut entry in self.reputations.iter_mut() {
            let hours_since_decay = now
                .signed_duration_since(entry.last_decay)
                .num_minutes() as f64 / 60.0;
            
            if hours_since_decay >= 1.0 {
                // Decay towards initial score
                let decay_factor = self.config.hourly_decay_factor.powf(hours_since_decay);
                let diff = entry.score - self.config.initial_score;
                entry.score = self.config.initial_score + diff * decay_factor;
                entry.last_decay = now;
            }
        }
    }

    /// Get reputation for a peer
    pub fn get_reputation(&self, peer_id: EndpointId) -> Option<PeerReputation> {
        self.reputations.get(&peer_id).map(|r| r.clone())
    }

    /// Check if peer is banned
    pub fn is_banned(&self, peer_id: EndpointId) -> bool {
        self.reputations
            .get(&peer_id)
            .map(|r| r.is_banned)
            .unwrap_or(false)
    }

    /// Unban a peer
    pub fn unban(&self, peer_id: EndpointId) {
        if let Some(mut rep) = self.reputations.get_mut(&peer_id) {
            rep.is_banned = false;
            rep.ban_reason = None;
            rep.score = self.config.initial_score;
            tracing::info!(peer = %peer_id, "Peer unbanned");
        }
    }

    /// Get top peers by reputation score
    pub fn get_top_peers(&self, limit: usize) -> Vec<(EndpointId, PeerReputation)> {
        let mut peers: Vec<_> = self.reputations
            .iter()
            .filter(|r| !r.is_banned)
            .map(|r| (*r.key(), r.value().clone()))
            .collect();
        
        peers.sort_by(|a, b| b.1.score.partial_cmp(&a.1.score).unwrap_or(std::cmp::Ordering::Equal));
        peers.truncate(limit);
        peers
    }

    /// Get peers suitable for sync (high reputation, not banned, low latency)
    pub fn get_sync_candidates(&self, limit: usize) -> Vec<EndpointId> {
        let mut candidates: Vec<_> = self.reputations
            .iter()
            .filter(|r| !r.is_banned && r.score > self.config.initial_score * 0.5)
            .map(|r| {
                // Calculate composite score: reputation + latency bonus
                let latency_bonus = if r.avg_latency_ms > 0.0 {
                    self.config.latency_weight * (1000.0 / r.avg_latency_ms).min(10.0)
                } else {
                    0.0
                };
                let composite = r.score + latency_bonus;
                (*r.key(), composite)
            })
            .collect();
        
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        candidates.truncate(limit);
        candidates.into_iter().map(|(id, _)| id).collect()
    }

    /// Get reputation summary statistics
    pub fn get_summary(&self) -> ReputationSummary {
        let mut total = 0;
        let mut banned = 0;
        let mut sum_score = 0.0;
        let mut sum_reliability = 0.0;

        for entry in self.reputations.iter() {
            total += 1;
            if entry.is_banned {
                banned += 1;
            }
            sum_score += entry.score;
            sum_reliability += entry.reliability();
        }

        ReputationSummary {
            total_peers: total,
            banned_peers: banned,
            avg_score: if total > 0 { sum_score / total as f64 } else { 0.0 },
            avg_reliability: if total > 0 { sum_reliability / total as f64 } else { 0.0 },
        }
    }
}

/// Reputation summary statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationSummary {
    pub total_peers: usize,
    pub banned_peers: usize,
    pub avg_score: f64,
    pub avg_reliability: f64,
}

// ============================================================================
// Bandwidth Throttling
// ============================================================================

/// Bandwidth throttle configuration
#[derive(Debug, Clone)]
pub struct BandwidthConfig {
    /// Maximum upload bytes per second (0 = unlimited)
    pub upload_limit_bps: u64,
    /// Maximum download bytes per second (0 = unlimited)
    pub download_limit_bps: u64,
    /// Per-peer upload limit (0 = use global limit)
    pub per_peer_upload_bps: u64,
    /// Per-peer download limit (0 = use global limit)
    pub per_peer_download_bps: u64,
    /// Token bucket refill interval
    pub refill_interval: Duration,
}

impl Default for BandwidthConfig {
    fn default() -> Self {
        Self {
            upload_limit_bps: 0,      // Unlimited by default
            download_limit_bps: 0,    // Unlimited by default
            per_peer_upload_bps: 0,
            per_peer_download_bps: 0,
            refill_interval: Duration::from_millis(100),
        }
    }
}

/// Token bucket for rate limiting
#[derive(Debug)]
struct TokenBucket {
    /// Current tokens available
    tokens: AtomicU64,
    /// Maximum tokens (bucket capacity)
    max_tokens: u64,
    /// Tokens added per refill
    refill_amount: u64,
    /// Last refill time
    last_refill: RwLock<Instant>,
    /// Refill interval
    refill_interval: Duration,
}

impl TokenBucket {
    fn new(rate_bps: u64, refill_interval: Duration) -> Self {
        // Calculate tokens per refill based on rate and interval
        let refill_amount = (rate_bps as f64 * refill_interval.as_secs_f64()) as u64;
        let max_tokens = refill_amount * 10; // Allow burst up to 10x refill amount
        
        Self {
            tokens: AtomicU64::new(max_tokens),
            max_tokens,
            refill_amount,
            last_refill: RwLock::new(Instant::now()),
            refill_interval,
        }
    }

    async fn refill(&self) {
        let mut last = self.last_refill.write().await;
        let now = Instant::now();
        let elapsed = now.duration_since(*last);
        
        if elapsed >= self.refill_interval {
            let refills = (elapsed.as_millis() / self.refill_interval.as_millis()) as u64;
            let tokens_to_add = refills * self.refill_amount;
            
            let current = self.tokens.load(Ordering::Relaxed);
            let new_tokens = (current + tokens_to_add).min(self.max_tokens);
            self.tokens.store(new_tokens, Ordering::Relaxed);
            
            *last = now;
        }
    }

    fn try_consume(&self, amount: u64) -> bool {
        loop {
            let current = self.tokens.load(Ordering::Relaxed);
            if current < amount {
                return false;
            }
            
            if self.tokens.compare_exchange(
                current,
                current - amount,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ).is_ok() {
                return true;
            }
        }
    }

    fn available(&self) -> u64 {
        self.tokens.load(Ordering::Relaxed)
    }
}

/// Bandwidth throttle manager
pub struct BandwidthThrottle {
    config: BandwidthConfig,
    /// Global upload bucket
    upload_bucket: Option<TokenBucket>,
    /// Global download bucket
    download_bucket: Option<TokenBucket>,
    /// Per-peer upload buckets
    peer_upload_buckets: DashMap<EndpointId, TokenBucket>,
    /// Per-peer download buckets
    peer_download_buckets: DashMap<EndpointId, TokenBucket>,
    /// Total bytes uploaded
    total_uploaded: AtomicU64,
    /// Total bytes downloaded
    total_downloaded: AtomicU64,
}

impl BandwidthThrottle {
    pub fn new(config: BandwidthConfig) -> Self {
        let upload_bucket = if config.upload_limit_bps > 0 {
            Some(TokenBucket::new(config.upload_limit_bps, config.refill_interval))
        } else {
            None
        };
        
        let download_bucket = if config.download_limit_bps > 0 {
            Some(TokenBucket::new(config.download_limit_bps, config.refill_interval))
        } else {
            None
        };
        
        Self {
            config,
            upload_bucket,
            download_bucket,
            peer_upload_buckets: DashMap::new(),
            peer_download_buckets: DashMap::new(),
            total_uploaded: AtomicU64::new(0),
            total_downloaded: AtomicU64::new(0),
        }
    }

    /// Check if upload is allowed and consume tokens
    pub async fn try_upload(&self, peer_id: EndpointId, bytes: u64) -> bool {
        // Refill buckets
        if let Some(ref bucket) = self.upload_bucket {
            bucket.refill().await;
        }
        
        // Check global limit
        if let Some(ref bucket) = self.upload_bucket {
            if !bucket.try_consume(bytes) {
                crate::metrics::BANDWIDTH_THROTTLED.with_label_values(&["upload", "global"]).inc();
                return false;
            }
        }
        
        // Check per-peer limit
        if self.config.per_peer_upload_bps > 0 {
            let bucket = self.peer_upload_buckets
                .entry(peer_id)
                .or_insert_with(|| {
                    TokenBucket::new(self.config.per_peer_upload_bps, self.config.refill_interval)
                });
            
            bucket.refill().await;
            if !bucket.try_consume(bytes) {
                crate::metrics::BANDWIDTH_THROTTLED.with_label_values(&["upload", "peer"]).inc();
                return false;
            }
        }
        
        self.total_uploaded.fetch_add(bytes, Ordering::Relaxed);
        crate::metrics::BANDWIDTH_BYTES.with_label_values(&["upload"]).inc_by(bytes as f64);
        true
    }

    /// Check if download is allowed and consume tokens
    pub async fn try_download(&self, peer_id: EndpointId, bytes: u64) -> bool {
        // Refill buckets
        if let Some(ref bucket) = self.download_bucket {
            bucket.refill().await;
        }
        
        // Check global limit
        if let Some(ref bucket) = self.download_bucket {
            if !bucket.try_consume(bytes) {
                crate::metrics::BANDWIDTH_THROTTLED.with_label_values(&["download", "global"]).inc();
                return false;
            }
        }
        
        // Check per-peer limit
        if self.config.per_peer_download_bps > 0 {
            let bucket = self.peer_download_buckets
                .entry(peer_id)
                .or_insert_with(|| {
                    TokenBucket::new(self.config.per_peer_download_bps, self.config.refill_interval)
                });
            
            bucket.refill().await;
            if !bucket.try_consume(bytes) {
                crate::metrics::BANDWIDTH_THROTTLED.with_label_values(&["download", "peer"]).inc();
                return false;
            }
        }
        
        self.total_downloaded.fetch_add(bytes, Ordering::Relaxed);
        crate::metrics::BANDWIDTH_BYTES.with_label_values(&["download"]).inc_by(bytes as f64);
        true
    }

    /// Get current bandwidth statistics
    pub fn get_stats(&self) -> BandwidthStats {
        BandwidthStats {
            total_uploaded: self.total_uploaded.load(Ordering::Relaxed),
            total_downloaded: self.total_downloaded.load(Ordering::Relaxed),
            upload_available: self.upload_bucket.as_ref().map(|b| b.available()),
            download_available: self.download_bucket.as_ref().map(|b| b.available()),
            upload_limit_bps: self.config.upload_limit_bps,
            download_limit_bps: self.config.download_limit_bps,
        }
    }

    /// Update bandwidth limits at runtime
    pub fn update_limits(&mut self, upload_bps: u64, download_bps: u64) {
        self.config.upload_limit_bps = upload_bps;
        self.config.download_limit_bps = download_bps;
        
        self.upload_bucket = if upload_bps > 0 {
            Some(TokenBucket::new(upload_bps, self.config.refill_interval))
        } else {
            None
        };
        
        self.download_bucket = if download_bps > 0 {
            Some(TokenBucket::new(download_bps, self.config.refill_interval))
        } else {
            None
        };
    }
}

/// Bandwidth usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthStats {
    pub total_uploaded: u64,
    pub total_downloaded: u64,
    pub upload_available: Option<u64>,
    pub download_available: Option<u64>,
    pub upload_limit_bps: u64,
    pub download_limit_bps: u64,
}

// ============================================================================
// Combined Network Resilience Manager
// ============================================================================

/// Combined network resilience manager
pub struct NetworkResilience {
    pub circuit_breaker: Arc<CircuitBreaker>,
    pub reputation: Arc<ReputationManager>,
    pub bandwidth: Arc<RwLock<BandwidthThrottle>>,
}

impl NetworkResilience {
    pub fn new(
        circuit_config: CircuitBreakerConfig,
        reputation_config: ReputationConfig,
        bandwidth_config: BandwidthConfig,
    ) -> Self {
        Self {
            circuit_breaker: Arc::new(CircuitBreaker::new(circuit_config)),
            reputation: Arc::new(ReputationManager::new(reputation_config)),
            bandwidth: Arc::new(RwLock::new(BandwidthThrottle::new(bandwidth_config))),
        }
    }

    /// Check if we should communicate with a peer
    pub fn should_communicate(&self, peer_id: EndpointId) -> bool {
        // Check circuit breaker first
        if !self.circuit_breaker.should_allow(peer_id) {
            return false;
        }
        
        // Check if peer is banned
        if self.reputation.is_banned(peer_id) {
            return false;
        }
        
        true
    }

    /// Record successful operation
    pub fn record_success(&self, peer_id: EndpointId, latency_ms: Option<f64>) {
        self.circuit_breaker.record_success(peer_id);
        self.reputation.record_sync_success(peer_id, latency_ms);
    }

    /// Record failed operation
    pub fn record_failure(&self, peer_id: EndpointId) {
        self.circuit_breaker.record_failure(peer_id);
        self.reputation.record_sync_failure(peer_id);
    }

    /// Get combined resilience summary
    pub fn get_summary(&self) -> ResilienceSummary {
        ResilienceSummary {
            circuit_breaker: self.circuit_breaker.get_summary(),
            reputation: self.reputation.get_summary(),
        }
    }

    /// Start background tasks (decay, cleanup)
    pub fn start_background_tasks(self: Arc<Self>) {
        // Reputation decay task
        let resilience = Arc::clone(&self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3600)); // Every hour
            loop {
                interval.tick().await;
                resilience.reputation.apply_decay();
                tracing::debug!("Applied reputation decay");
            }
        });
    }
}

/// Combined resilience summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResilienceSummary {
    pub circuit_breaker: CircuitBreakerSummary,
    pub reputation: ReputationSummary,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_peer_id() -> EndpointId {
        // Use thread_rng to generate a valid ed25519 key
        let mut rng = rand::thread_rng();
        let secret = iroh::SecretKey::generate(&mut rng);
        secret.public()
    }

    fn test_peer_id_2() -> EndpointId {
        let mut rng = rand::thread_rng();
        let secret = iroh::SecretKey::generate(&mut rng);
        secret.public()
    }

    #[test]
    fn test_circuit_breaker_closed_state() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::default());
        let peer = test_peer_id();
        
        // Should be allowed initially
        assert!(cb.should_allow(peer));
        assert_eq!(cb.get_state(peer), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_opens_after_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            failure_window: Duration::from_secs(60),
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);
        let peer = test_peer_id();
        
        // Record failures
        cb.record_failure(peer);
        cb.record_failure(peer);
        assert!(cb.should_allow(peer)); // Still allowed
        
        cb.record_failure(peer);
        assert!(!cb.should_allow(peer)); // Now blocked
        assert_eq!(cb.get_state(peer), CircuitState::Open);
    }

    #[test]
    fn test_circuit_breaker_success_resets() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);
        let peer = test_peer_id();
        
        cb.record_failure(peer);
        cb.record_failure(peer);
        cb.record_success(peer); // Should reset
        cb.record_failure(peer);
        cb.record_failure(peer);
        
        // Should still be allowed (only 2 failures after reset)
        assert!(cb.should_allow(peer));
    }

    #[test]
    fn test_reputation_initial_score() {
        let rm = ReputationManager::new(ReputationConfig::default());
        let peer = test_peer_id();
        
        rm.record_sync_success(peer, None);
        let rep = rm.get_reputation(peer).unwrap();
        
        assert!(rep.score > 50.0); // Initial + gain
        assert_eq!(rep.successful_syncs, 1);
    }

    #[test]
    fn test_reputation_failure_reduces_score() {
        let rm = ReputationManager::new(ReputationConfig::default());
        let peer = test_peer_id();
        
        rm.record_sync_success(peer, None); // Start with some score
        let initial_score = rm.get_reputation(peer).unwrap().score;
        
        rm.record_sync_failure(peer);
        let after_failure = rm.get_reputation(peer).unwrap().score;
        
        assert!(after_failure < initial_score);
    }

    #[test]
    fn test_reputation_ban_at_zero() {
        let config = ReputationConfig {
            initial_score: 10.0,
            sync_failure_loss: 5.0,
            min_score: 0.0,
            ..Default::default()
        };
        let rm = ReputationManager::new(config);
        let peer = test_peer_id();
        
        rm.record_sync_failure(peer);
        rm.record_sync_failure(peer);
        rm.record_sync_failure(peer);
        
        assert!(rm.is_banned(peer));
    }

    #[test]
    fn test_reputation_unban() {
        let config = ReputationConfig {
            initial_score: 10.0,
            sync_failure_loss: 5.0,
            min_score: 0.0,
            ..Default::default()
        };
        let rm = ReputationManager::new(config);
        let peer = test_peer_id();
        
        rm.record_sync_failure(peer);
        rm.record_sync_failure(peer);
        rm.record_sync_failure(peer);
        assert!(rm.is_banned(peer));
        
        rm.unban(peer);
        assert!(!rm.is_banned(peer));
    }

    #[test]
    fn test_get_top_peers() {
        let rm = ReputationManager::new(ReputationConfig::default());
        let peer1 = test_peer_id();
        let peer2 = test_peer_id_2();
        
        // Give peer1 more successes
        rm.record_sync_success(peer1, None);
        rm.record_sync_success(peer1, None);
        rm.record_sync_success(peer1, None);
        rm.record_sync_success(peer2, None);
        
        let top = rm.get_top_peers(2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, peer1); // peer1 should be first
    }

    #[tokio::test]
    async fn test_bandwidth_unlimited() {
        let throttle = BandwidthThrottle::new(BandwidthConfig::default());
        let peer = test_peer_id();
        
        // Should always allow with unlimited
        assert!(throttle.try_upload(peer, 1_000_000).await);
        assert!(throttle.try_download(peer, 1_000_000).await);
    }

    #[tokio::test]
    async fn test_bandwidth_limited() {
        let config = BandwidthConfig {
            upload_limit_bps: 1000, // 1KB/s
            ..Default::default()
        };
        let throttle = BandwidthThrottle::new(config);
        let peer = test_peer_id();
        
        // First request should succeed
        assert!(throttle.try_upload(peer, 500).await);
        
        // Trying to send more than available should fail
        assert!(!throttle.try_upload(peer, 100_000).await);
    }

    #[test]
    fn test_network_resilience_combined() {
        let nr = NetworkResilience::new(
            CircuitBreakerConfig::default(),
            ReputationConfig::default(),
            BandwidthConfig::default(),
        );
        let peer = test_peer_id();
        
        // Should allow initially
        assert!(nr.should_communicate(peer));
        
        // Record some successes
        nr.record_success(peer, Some(100.0));
        
        let summary = nr.get_summary();
        assert_eq!(summary.circuit_breaker.closed, 1);
    }
}
