# Network Resilience

This document describes the network resilience features in the Cyberfly Rust Node that improve reliability and fault tolerance in peer-to-peer communication.

## Overview

The network resilience module provides three key features:

1. **Circuit Breaker** - Prevents hammering failing peers
2. **Peer Reputation** - Tracks peer reliability and prefers good peers
3. **Bandwidth Throttling** - Rate limits network traffic

## Circuit Breaker

The circuit breaker pattern prevents cascading failures by temporarily blocking requests to failing peers.

### States

```
┌─────────┐  failure_threshold  ┌─────────┐
│  Closed │ ─────────────────▶ │  Open   │
│(normal) │                    │(blocked)│
└────┬────┘                    └────┬────┘
     │                              │
     │ success                      │ recovery_timeout
     │                              ▼
     │                        ┌──────────┐
     └──────────────────────  │Half-Open │
            test success      │ (testing)│
                              └──────────┘
```

- **Closed**: Normal operation, requests are allowed
- **Open**: Too many failures, requests are blocked
- **Half-Open**: Testing if the service has recovered

### Configuration

```rust
CircuitBreakerConfig {
    failure_threshold: 5,          // Failures before opening
    recovery_timeout: 30 seconds,  // Time to wait before testing
    success_threshold: 2,          // Successes to close from HalfOpen
    failure_window: 60 seconds,    // Rolling window for counting failures
}
```

### GraphQL Queries

```graphql
# Get circuit breaker summary
query {
  getCircuitBreakerSummary {
    totalPeers
    closed
    open
    halfOpen
  }
}

# Get state for a specific peer
query {
  getCircuitBreakerState(peerId: "04b754ba2a3da0970d72d08b8740fb2ad96e63cf8f8bef6b7f1ab84e5b09a7f8") {
    peerId
    state
  }
}
```

## Peer Reputation

The reputation system tracks peer reliability over time and helps select the best peers for sync operations.

### Scoring

- **Initial score**: 50.0
- **Maximum score**: 100.0
- **Minimum score**: 0.0 (banned)

### Score Changes

| Event | Score Change |
|-------|-------------|
| Successful sync | +2.0 |
| Failed sync | -5.0 |
| Successful message | +0.5 |
| Failed message | -1.0 |

### Features

- **Automatic banning**: Peers with score ≤ 0 are banned
- **Latency tracking**: Rolling average of response times
- **Bytes tracking**: Total uploaded/downloaded per peer
- **Decay**: Reputation decays towards initial score over time
- **Reliability calculation**: `(successful_ops / total_ops)`

### GraphQL Queries

```graphql
# Get reputation summary
query {
  getReputationSummary {
    totalPeers
    bannedPeers
    avgScore
    avgReliability
  }
}

# Get reputation for a specific peer
query {
  getPeerReputation(peerId: "04b754ba2a3da0970d72d08b8740fb2ad96e63cf8f8bef6b7f1ab84e5b09a7f8") {
    peerId
    score
    successfulSyncs
    failedSyncs
    successfulMessages
    failedMessages
    avgLatencyMs
    bytesReceived
    bytesSent
    reliability
    isBanned
    banReason
    firstSeen
    lastSeen
  }
}

# Get top peers by reputation
query {
  getTopPeersByReputation(limit: 10) {
    peerId
    score
    reliability
    avgLatencyMs
  }
}
```

## Bandwidth Throttling

Token-bucket based rate limiting for upload and download traffic.

### Configuration

```rust
BandwidthConfig {
    upload_limit_bps: 0,        // 0 = unlimited
    download_limit_bps: 0,      // 0 = unlimited
    per_peer_upload_bps: 0,     // Per-peer limit
    per_peer_download_bps: 0,   // Per-peer limit
    refill_interval: 100ms,     // Token refill rate
}
```

### Features

- **Global limits**: Total bandwidth for all peers
- **Per-peer limits**: Individual peer bandwidth caps
- **Token bucket**: Allows burst traffic up to 10x refill amount
- **Graceful throttling**: Returns false if bandwidth unavailable

### GraphQL Queries

```graphql
# Get bandwidth statistics
query {
  getBandwidthStats {
    totalUploaded
    totalDownloaded
    uploadAvailable
    downloadAvailable
    uploadLimitBps
    downloadLimitBps
  }
}
```

## Combined Summary

Get all resilience metrics in a single query:

```graphql
query {
  getNetworkResilienceSummary {
    circuitBreaker {
      totalPeers
      closed
      open
      halfOpen
    }
    reputation {
      totalPeers
      bannedPeers
      avgScore
      avgReliability
    }
    bandwidth {
      totalUploaded
      totalDownloaded
      uploadLimitBps
      downloadLimitBps
    }
  }
}
```

## Prometheus Metrics

The following metrics are exported for monitoring:

### Circuit Breaker
- `circuit_breaker_state{peer, state}` - Circuit breaker state (1=active)
- `circuit_breaker_trips_total` - Total circuit breaker trips

### Reputation
- `peer_reputation_score{peer}` - Current reputation score
- `peers_banned_current` - Number of banned peers

### Bandwidth
- `bandwidth_bytes_total{direction}` - Total bytes (upload/download)
- `bandwidth_throttled_total{direction, scope}` - Throttled request count

## Integration

The resilience features are automatically integrated into the network layer:

```rust
// Check before communicating with a peer
if resilience.should_communicate(peer_id) {
    // Attempt operation
    match perform_sync(peer_id).await {
        Ok(_) => resilience.record_success(peer_id, Some(latency_ms)),
        Err(_) => resilience.record_failure(peer_id),
    }
}
```

## Best Practices

1. **Circuit Breaker Tuning**: Adjust `failure_threshold` based on expected error rates
2. **Reputation Decay**: Use `hourly_decay_factor` to prevent permanent reputation damage
3. **Bandwidth Limits**: Set limits based on available network capacity
4. **Monitoring**: Use Prometheus metrics to track network health
5. **Peer Selection**: Use `get_sync_candidates()` to prefer reliable peers

## Future Enhancements

- [ ] Persistent reputation storage across restarts
- [ ] Configurable reputation algorithms
- [ ] Dynamic bandwidth adjustment based on network conditions
- [ ] Peer grouping by reputation tier
- [ ] Automatic unban after cooldown period
