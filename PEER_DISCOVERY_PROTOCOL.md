# Peer Discovery Protocol

## Overview

The peer discovery protocol enables automatic full-mesh connectivity across the decentralized network. Each node broadcasts its list of connected peers every 10 seconds, allowing other nodes to discover and connect to peers they don't know about yet.

**Inspired by**: [iroh-gossip-discovery](https://github.com/therishidesai/iroh-gossip-discovery)

## Features

- **🔐 Cryptographic Signatures**: Ed25519 signatures prevent spoofing and ensure authenticity
- **🌐 Full Mesh Topology**: Automatic peer discovery creates complete network connectivity
- **🔄 Self-Healing**: Nodes automatically reconnect via other peers
- **🧹 Auto-Cleanup**: Inactive peers are removed after 30 seconds of inactivity
- **📍 Region Awareness**: Nodes share their geographic region for optimization
- **🚫 Spoofing Prevention**: Node ID verification prevents impersonation attacks

## How It Works

### 1. **Signed Peer List Broadcasting**
- Every 10 seconds, each node broadcasts a cryptographically signed `PeerDiscoveryAnnouncement` containing:
  - Node ID of the sender
  - List of currently connected peer IDs
  - Timestamp of the announcement
  - Region of the announcing node
  - Ed25519 signature for authenticity

### 2. **Signature Verification**
When a node receives an announcement:
1. Verifies the node ID matches the sender (prevents impersonation)
2. Verifies the Ed25519 signature (ensures authenticity)
3. Only processes announcements with valid signatures
4. Rejects tampered or spoofed messages

### 3. **Automatic Connection**
After verification:
1. Checks each peer ID in the received list
2. If a peer is unknown (not in local discovered peers map), attempts to connect
3. Successful connections are tracked with timestamps
4. Failed connections are logged but don't block the process

### 4. **Deduplication & Loop Prevention**
- Announcements are cached with `node_id:timestamp` keys
- Only newer announcements from the same node are processed
- Nodes skip connecting to themselves
- Nodes skip peers they're already connected to

### 5. **Node Expiration**
- Background cleanup task runs every 10 seconds
- Peers inactive for >30 seconds are automatically removed
- Prevents stale peer lists and memory leaks
- Self-healing: nodes rejoin when they reconnect

## Message Format

```json
{
  "node_id": "04b754ba2a3da0970d72d08b8740fb2ad96e63cf8f8bef6b7f1ab84e5b09a7f8",
  "connected_peers": [
    "8921781873f3b664e020c4fe1c5b9796e70adccbaa26d12a39de9b317d9e9269",
    "a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6q7r8s9t0u1v2w3x4y5z6a7b8c9d0e1f2"
  ],
  "timestamp": 1700000000,
  "region": "us-east-1",
  "signature": "3a7c8b5d2e1f9a4c6b8d0e2f4a6c8e0b2d4f6a8c0e2b4d6f8a0c2e4b6d8f0a2c"
}
```

### Signature Generation

```
message = "{node_id}:{timestamp}:{sorted_peer_ids_comma_separated}"
signature = Ed25519_Sign(secret_key, message)
```

### Signature Verification

```
announced_node_id == sender_node_id  // Prevent impersonation
Ed25519_Verify(public_key, message, signature)  // Authenticate
```

## Gossip Topic

- **Topic ID**: `decentralized-peer-list-v1-iroh!` (exactly 32 bytes)
- **Protocol**: Iroh Gossip
- **Broadcast Interval**: 10 seconds
- **Initial Delay**: 5 seconds (allows gossip network to stabilize)

## Network Topology Evolution

### Example Scenario

**Initial State:**
```
Node A ←→ Node B
Node C ←→ Node D
```

**After Node B broadcasts its peer list:**
```
Node A ←→ Node B
Node C connects to Node A (discovered via B)
Node D ←→ Node C
```

**After all nodes broadcast:**
```
Full mesh:
Node A ←→ Node B
Node A ←→ Node C
Node A ←→ Node D
Node B ←→ Node C
Node B ←→ Node D
Node C ←→ Node D
```

## Benefits

1. **Automatic Full Mesh**: No manual peer configuration needed beyond bootstrap
2. **Network Resilience**: New nodes quickly discover all existing peers
3. **Region Awareness**: Nodes share their region for potential optimization
4. **Scalable Discovery**: Gossip protocol scales to large networks
5. **Self-Healing**: Disconnected nodes automatically reconnect via other peers
6. **Security**: Cryptographic signatures prevent spoofing and impersonation
7. **Auto-Cleanup**: Inactive peers are automatically removed
8. **Tamper-Proof**: Ed25519 signatures ensure announcement integrity

## Security Features

### Ed25519 Signature Verification
- Every announcement is signed with the node's secret key
- Recipients verify signatures before processing
- Prevents malicious actors from spoofing peer lists

### Node ID Verification
- Announced node ID must match the sender's EndpointId
- Derived from the same public key used for signature verification
- Prevents impersonation attacks

### Timestamp-Based Deduplication
- Announcements include monotonically increasing timestamps
- Older announcements are ignored
- Prevents replay attacks

## Logging

The protocol provides detailed logging:

```
📡 Broadcasted signed peer list: 3 connected peers from region us-east-1
📋 Received verified peer list from <node_id> (region: eu-west-1): 5 peers
🔗 Attempting to connect to peer <peer_id> (discovered via <announcing_node>)
✓ Successfully connected to peer <peer_id>
✓ Established 2 new peer connection(s) via discovery
🕒 Removed expired peer: <peer_id>
🧹 Cleaned up 1 expired peer(s)
⚠️  Invalid signature from <node_id> - ignoring announcement
⚠️  Node ID mismatch: announced <id1> but message from <id2>
```

## Configuration

No configuration needed! The protocol is automatically enabled when the node starts.

## Testing Multi-Node Discovery

### Local Test (3 Nodes)

**Terminal 1 (Bootstrap Node):**
```bash
cargo run --release
# Note the Node ID from logs
```

**Terminal 2 (Node 2):**
```bash
export BOOTSTRAP_PEERS="<node1_id>@127.0.0.1:31001"
cargo run --release
# Watch logs for peer discovery
```

**Terminal 3 (Node 3):**
```bash
export BOOTSTRAP_PEERS="<node1_id>@127.0.0.1:31001"
cargo run --release
# Should discover both Node 1 and Node 2
```

### Expected Behavior

1. Node 2 connects to Node 1 (bootstrap)
2. Node 3 connects to Node 1 (bootstrap)
3. Within 10 seconds:
   - Node 1 broadcasts: "I'm connected to Node 2 and Node 3"
   - Node 2 receives list and connects to Node 3
   - Node 3 receives list and connects to Node 2
4. All nodes are now fully connected (full mesh)

## GraphQL Query

Check connected peers via GraphQL:

```graphql
query {
  getConnectedPeers {
    peerId
    connectionStatus
    lastSeen
  }
}
```

## Monitoring

Track peer discovery via metrics:
- Prometheus endpoint: `http://localhost:31003/metrics`
- Look for `peer_discovery_announcements_sent`
- Look for `peer_discovery_connections_established`

## Troubleshooting

### No Peers Discovered
- Ensure at least one node is configured as bootstrap peer
- Check firewall rules allow UDP/QUIC traffic on port 31001
- Verify nodes are on the same network or have public IPs

### Connection Failures
- Check logs for "Failed to connect to peer" messages
- Verify peer IDs are correct (64-character hex strings)
- Ensure QUIC/UDP port 31001 is reachable

### Duplicate Connection Attempts
- The cache prevents this, but check `peer_announcement_cache` is working
- Look for "Already processed this or a newer announcement" in debug logs

## Performance Considerations

- **Bandwidth**: ~100 bytes per announcement × nodes in network × 0.1 Hz
- **Memory**: O(N) where N = number of unique peers seen
- **CPU**: Minimal, dominated by JSON serialization/deserialization

## Future Enhancements

- [ ] Peer scoring based on region proximity
- [ ] Adaptive broadcast frequency based on network stability
- [ ] Signed announcements for security
- [ ] DHT integration for larger networks
- [ ] Relay peer recommendations
