# Bootstrap Peer Automatic Reconnection

## Overview
Automatic reconnection monitoring ensures persistent connections to bootstrap peers. When a bootstrap peer connection drops, the system automatically detects and reconnects.

## Features

### 1. Connection Monitoring
- **Check Interval**: Every 30 seconds
- **Status Detection**: Uses Iroh's `conn_type()` API to check connection health
- **Connection Types Monitored**:
  - `ConnectionType::Direct` - Direct UDP connection ✓
  - `ConnectionType::Relay` - Connection via relay server ✓
  - `ConnectionType::Mixed` - Both direct and relay ✓
  - `ConnectionType::None` - No connection ✗ (triggers reconnection)

### 2. Reconnection Logic
- **Detection**: Monitors each bootstrap peer's connection status
- **Delay**: Waits 5 seconds after detecting disconnection before reconnecting
- **Retry Strategy**: Uses existing exponential backoff (5 attempts, 1s→30s)
- **Parallel Monitoring**: Each peer monitored independently

## Implementation Details

### Connection Status Check
```rust
let conn_watcher = endpoint.conn_type(node_id);
let is_connected = if let Some(mut watcher) = conn_watcher {
    use iroh::endpoint::ConnectionType;
    !matches!(watcher.get(), ConnectionType::None)
} else {
    false  // No watcher means no connection info
};
```

### Reconnection Flow
1. **Monitor Task**: Spawned after successful initial connections
2. **Periodic Check**: Every 30 seconds, check each bootstrap peer
3. **Detect Disconnection**: If `ConnectionType::None`, peer is disconnected
4. **Wait Period**: Sleep 5 seconds to avoid rapid reconnection attempts
5. **Reconnect**: Spawn task with `connect_bootstrap_peer_with_retry()`
6. **Retry Logic**: Up to 5 attempts with exponential backoff

### Key Functions

#### `monitor_bootstrap_connections()`
- Runs as background task
- Checks connection status every 30 seconds
- Spawns reconnection tasks when needed
- Logs connection status changes

#### `connect_bootstrap_peer_with_retry()`
- Called for reconnection attempts
- Maximum 5 retries
- Exponential backoff: 1s, 2s, 4s, 8s, 16s (capped at 30s)
- Same logic used for initial connections and reconnections

## Configuration

### Tunable Parameters
```rust
const CHECK_INTERVAL_SECS: u64 = 30;  // How often to check connections
const RECONNECT_DELAY_SECS: u64 = 5;  // Wait before reconnecting
const MAX_RETRIES: u32 = 5;            // Maximum reconnection attempts
const INITIAL_RETRY_DELAY_SECS: u64 = 1;  // First retry delay
const MAX_RETRY_DELAY_SECS: u64 = 30;     // Maximum retry delay
```

### Recommended Settings
- **Stable Networks**: Increase `CHECK_INTERVAL_SECS` to 60-120 seconds
- **Unstable Networks**: Keep at 30 seconds or reduce to 15 seconds
- **High Latency**: Increase `RECONNECT_DELAY_SECS` to 10-15 seconds
- **Critical Bootstrap**: Increase `MAX_RETRIES` to 10

## Testing Reconnection

### Test Scenario 1: Graceful Disconnect
```bash
# Terminal 1: Start bootstrap node
cargo run --release -- --bootstrap

# Terminal 2: Start client node with bootstrap
cargo run --release -- --bootstrap-peers "BOOTSTRAP_NODE_ID@127.0.0.1:11204"

# Terminal 1: Stop bootstrap (Ctrl+C)
# Wait 30 seconds - client should detect disconnection
# Terminal 1: Restart bootstrap node
# Client should automatically reconnect within 5-35 seconds
```

### Test Scenario 2: Network Interruption
```bash
# Simulate network failure
sudo pfctl -e  # Enable packet filter (macOS)
sudo pfctl -f /etc/pf.conf  # Reset rules

# Add rule to block traffic
echo "block drop proto tcp from any to 127.0.0.1 port 11204" | sudo pfctl -f -

# Wait for monitor to detect (30s check interval)
# Remove block
sudo pfctl -f /etc/pf.conf

# Monitor should detect and reconnect
```

### Expected Log Output

#### Successful Monitoring
```
🔍 Started bootstrap connection monitor (checks every 30s)
[Every 30s] Checking 2 bootstrap peer(s)...
✓ Bootstrap peer abc123 at 127.0.0.1:11204 still connected (Direct)
✓ Bootstrap peer def456 at 192.168.1.100:11204 still connected (Relay)
```

#### Disconnect Detection
```
⚠️  Bootstrap peer abc123 at 127.0.0.1:11204 disconnected - attempting reconnection
[Attempt 1/5] Connecting to bootstrap peer abc123 at 127.0.0.1:11204
✅ Successfully reconnected to bootstrap peer abc123 at 127.0.0.1:11204
```

#### Reconnection Failure
```
⚠️  Bootstrap peer abc123 at 127.0.0.1:11204 disconnected - attempting reconnection
[Attempt 1/5] Failed: connection timeout (retrying in 1s)
[Attempt 2/5] Failed: connection timeout (retrying in 2s)
...
❌ Failed to reconnect to bootstrap peer abc123 at 127.0.0.1:11204: max retries exceeded
```

## Why This Matters

### Network Stability
Bootstrap peers are the initial entry points to the gossip network:
- **Discovery Hub**: Bootstrap peers share information about other peers
- **Gossip Relay**: Messages are propagated through bootstrap connections
- **DHT Seeding**: Bootstrap peers help populate the routing table

### Connection Persistence Benefits
1. **Faster Peer Discovery**: Always connected to known stable peers
2. **Message Reliability**: Consistent gossip propagation paths
3. **Network Resilience**: Automatic recovery from temporary failures
4. **Reduced Isolation**: Prevents nodes from becoming network islands

### Use Cases
- **IoT Devices**: Unreliable network connections (WiFi/cellular)
- **Mobile Nodes**: Frequent network switches and interruptions
- **Cloud Deployments**: Instance restarts and migrations
- **Development**: Frequent node restarts during testing

## Architecture Integration

### Startup Flow
1. Parse bootstrap peer addresses from CLI
2. Add addresses to Iroh endpoint
3. Attempt parallel connections with retry logic
4. Collect successfully connected peers
5. **Start monitoring task for connected peers**

### Runtime Behavior
- Monitor runs continuously as background task
- Independent of gossip protocol and message handling
- Logs connection status for observability
- Spawns reconnection tasks as needed (non-blocking)

### Shutdown
- Monitor task terminates when endpoint closes
- No explicit cleanup needed (Tokio task drops)
- Connection state managed by Iroh endpoint

## API Reference

### Iroh Connection APIs Used

#### `Endpoint::conn_type(endpoint_id: EndpointId)`
Returns: `Option<n0_watcher::Direct<ConnectionType>>`
- `Some(watcher)` if peer is known (may be connected or disconnected)
- `None` if peer has never been contacted

#### `ConnectionType` Enum
- `Direct(SocketAddr)` - Direct UDP connection
- `Relay(RelayUrl)` - Relayed connection
- `Mixed(SocketAddr, RelayUrl)` - Both direct and relay
- `None` - No active connection

#### `Watcher::get()`
Returns current value of watched state
- Must be called on mutable reference
- Returns `ConnectionType` for connection status

## Future Enhancements

### Metrics
- Track reconnection frequency per peer
- Measure downtime duration
- Alert on persistent connection failures

### Adaptive Monitoring
- Adjust check interval based on connection stability
- Faster checks when instability detected
- Slower checks for consistently stable connections

### Connection Health Scores
- Track connection quality over time
- Prefer stable peers for message routing
- Demote or remove consistently failing peers

### Circuit Breaker Pattern
- Temporarily stop reconnection attempts after repeated failures
- Exponentially increase backoff for persistently unavailable peers
- Resume attempts after cooling period

## Troubleshooting

### Monitor Not Detecting Disconnections
- **Symptom**: Peer disconnects but monitor shows "still connected"
- **Cause**: Check interval too long or connection lingering in state
- **Fix**: Reduce `CHECK_INTERVAL_SECS` or add manual disconnect detection

### Rapid Reconnection Attempts
- **Symptom**: Reconnection attempts happening too frequently
- **Cause**: `RECONNECT_DELAY_SECS` too short
- **Fix**: Increase delay to 10-15 seconds

### Failed Reconnections
- **Symptom**: All 5 retry attempts fail
- **Cause**: Bootstrap peer permanently unavailable or network issue
- **Fix**: Check bootstrap peer status, verify network connectivity, increase retry count

### Memory Leaks
- **Symptom**: Memory usage grows over time
- **Cause**: Reconnection tasks not terminating
- **Fix**: Ensure tasks complete (success or failure) and drop properly

## Related Documentation
- [BOOTSTRAP_RETRY_LOGIC.md](BOOTSTRAP_RETRY_LOGIC.md) - Initial connection retry
- [PEER_DISCOVERY_PROTOCOL.md](PEER_DISCOVERY_PROTOCOL.md) - Gossip-based discovery
- [PEER_DIALING_EXPLAINED.md](PEER_DIALING_EXPLAINED.md) - DHT and mDNS discovery
