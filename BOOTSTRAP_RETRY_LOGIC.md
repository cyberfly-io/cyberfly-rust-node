# Bootstrap Peer Connection with Retry Logic

## Overview

The bootstrap peer connection system now includes **exponential backoff retry logic** to handle temporary network failures and improve connection reliability.

---

## 🚀 Features

### 1. **Exponential Backoff**
- Initial delay: **1 second**
- Maximum delay: **30 seconds**
- Delay doubles after each failed attempt
- Prevents overwhelming network with rapid retries

### 2. **Maximum Retry Attempts**
- Default: **5 attempts** per bootstrap peer
- Configurable via constants
- Logs attempt number for monitoring

### 3. **Parallel Connection Attempts**
- All bootstrap peers are contacted **simultaneously**
- Uses `tokio::spawn` for concurrent connections
- Faster bootstrap process when multiple peers configured

### 4. **Graceful Degradation**
- If all bootstrap connections fail, falls back to DHT/mDNS discovery
- Logs clear warnings about connection status
- Network still functions via alternative discovery methods

---

## 📊 Retry Schedule

| Attempt | Delay Before Retry | Cumulative Time |
|---------|-------------------|-----------------|
| 1       | 0s (immediate)    | 0s              |
| 2       | 1s                | 1s              |
| 3       | 2s                | 3s              |
| 4       | 4s                | 7s              |
| 5       | 8s                | 15s             |

**Total maximum time per peer:** ~15 seconds (if all 5 attempts fail)

---

## 🔧 Implementation Details

### Code Location
`src/iroh_network.rs` - Lines ~372-509

### Key Methods

#### 1. `add_bootstrap_addresses()`
- Parses bootstrap peer strings
- Spawns parallel retry tasks
- Waits for all attempts to complete
- Reports success/failure statistics

#### 2. `connect_bootstrap_peer_with_retry()`
- Handles retry logic for a single peer
- Implements exponential backoff
- Logs detailed connection progress
- Returns success or final error after max retries

---

## 📝 Configuration

### Constants (can be tuned)

```rust
const MAX_RETRIES: u32 = 5;           // Maximum number of attempts
const INITIAL_DELAY_MS: u64 = 1000;   // 1 second initial delay
const MAX_DELAY_MS: u64 = 30000;      // 30 seconds maximum delay
```

### Bootstrap Peer Format

```bash
# Environment variable format:
BOOTSTRAP_PEERS="peer_id@ip:port,peer_id2@ip2:port2"

# Example:
BOOTSTRAP_PEERS="04b754ba...9a7f8@67.211.219.34:31001"
```

---

## 📋 Log Output Examples

### Successful Connection (First Attempt)

```
🔄 Attempting to connect to bootstrap peer 04b75 at 67.211.219.34:31001 (attempt 1/5)
✅ Successfully connected to bootstrap peer 04b75 at 67.211.219.34:31001 (attempt 1)
✓ Successfully connected to 1/1 bootstrap peer(s)
```

### Retry Sequence

```
🔄 Attempting to connect to bootstrap peer 04b75 at 67.211.219.34:31001 (attempt 1/5)
⚠️  Connection attempt 1/5 failed for peer 04b75 at 67.211.219.34:31001: connection timeout
⏳ Retrying in 1000ms...

🔄 Attempting to connect to bootstrap peer 04b75 at 67.211.219.34:31001 (attempt 2/5)
⚠️  Connection attempt 2/5 failed for peer 04b75 at 67.211.219.34:31001: connection timeout
⏳ Retrying in 2000ms...

🔄 Attempting to connect to bootstrap peer 04b75 at 67.211.219.34:31001 (attempt 3/5)
✅ Successfully connected to bootstrap peer 04b75 at 67.211.219.34:31001 (attempt 3)
✓ Successfully connected to 1/1 bootstrap peer(s)
```

### All Attempts Failed

```
🔄 Attempting to connect to bootstrap peer 04b75 at 67.211.219.34:31001 (attempt 1/5)
⚠️  Connection attempt 1/5 failed for peer 04b75 at 67.211.219.34:31001: connection timeout
⏳ Retrying in 1000ms...

... (attempts 2-4 similar) ...

🔄 Attempting to connect to bootstrap peer 04b75 at 67.211.219.34:31001 (attempt 5/5)
❌ Failed to connect to bootstrap peer 04b75 at 67.211.219.34:31001 after 5 attempts: connection timeout
⚠️  Failed to connect to any bootstrap peers - will rely on DHT/mDNS discovery
```

---

## 🎯 Why Retry Logic?

### Problem: Transient Network Issues

Bootstrap nodes may be temporarily unavailable due to:
- Network congestion
- DNS resolution delays
- Firewall/NAT traversal issues
- Peer node restarts
- Temporary connectivity problems

### Solution: Exponential Backoff

Benefits:
1. **Resilience**: Automatically recovers from temporary failures
2. **Efficiency**: Avoids overwhelming network with rapid retries
3. **User Experience**: Reduces connection failures for end users
4. **Production Ready**: Industry-standard retry pattern

---

## 🔄 Connection Flow Diagram

```
┌─────────────────────────────────────────────────────────────┐
│  Node Starts                                                │
│  Parse bootstrap peer strings                               │
└────────────────────┬────────────────────────────────────────┘
                     │
                     ▼
         ┌───────────────────────┐
         │ For each bootstrap    │
         │ peer, spawn async     │
         │ retry task            │
         └───┬───────────┬───────┘
             │           │
     ┌───────┴─────┐     └──────────┐
     ▼             ▼                ▼
┌─────────┐  ┌─────────┐      ┌─────────┐
│ Peer A  │  │ Peer B  │      │ Peer C  │
│ Retry   │  │ Retry   │  ... │ Retry   │
│ Task    │  │ Task    │      │ Task    │
└────┬────┘  └────┬────┘      └────┬────┘
     │            │                │
     ▼            ▼                ▼
┌─────────────────────────────────────────┐
│ Attempt 1: Connect                      │
├─────────────────────────────────────────┤
│ Failed? → Wait 1s                       │
│ Attempt 2: Connect                      │
├─────────────────────────────────────────┤
│ Failed? → Wait 2s                       │
│ Attempt 3: Connect                      │
├─────────────────────────────────────────┤
│ Failed? → Wait 4s                       │
│ Attempt 4: Connect                      │
├─────────────────────────────────────────┤
│ Failed? → Wait 8s                       │
│ Attempt 5: Connect                      │
├─────────────────────────────────────────┤
│ Success! OR Final failure               │
└─────────────────────────────────────────┘
     │            │                │
     └────────────┴────────────────┘
                  │
                  ▼
     ┌────────────────────────────┐
     │ Report connection results  │
     │ "Connected to X/Y peers"   │
     └────────────────────────────┘
                  │
                  ▼
     ┌────────────────────────────┐
     │ Continue with gossip       │
     │ network initialization     │
     └────────────────────────────┘
```

---

## 🧪 Testing the Retry Logic

### Test 1: Simulate Network Failure

```bash
# Terminal 1: Start node with invalid bootstrap peer
BOOTSTRAP_PEERS="04b754ba...9a7f8@192.168.1.99:31001" \
  ./target/release/cyberfly-rust-node

# Expected output:
# - 5 retry attempts with increasing delays
# - Final warning: "Failed to connect to any bootstrap peers"
# - Node continues with DHT/mDNS discovery
```

### Test 2: Valid Bootstrap Peer

```bash
# Terminal 1: Start bootstrap node
PORT=31001 ./target/release/cyberfly-rust-node

# Terminal 2: Connect with retry logic
BOOTSTRAP_PEERS="<node1_peer_id>@127.0.0.1:31001" \
  ./target/release/cyberfly-rust-node

# Expected output:
# - Successful connection on first attempt
# - "✅ Successfully connected to bootstrap peer"
```

### Test 3: Delayed Success

```bash
# Start bootstrap node AFTER client attempts connection
# Client will retry and eventually succeed when bootstrap comes online
```

---

## ⚙️ Tuning the Retry Logic

### For Development (Fast Failure)

```rust
const MAX_RETRIES: u32 = 2;           // Fewer retries
const INITIAL_DELAY_MS: u64 = 500;    // Shorter delays
const MAX_DELAY_MS: u64 = 2000;
```

### For Production (High Reliability)

```rust
const MAX_RETRIES: u32 = 10;          // More retries
const INITIAL_DELAY_MS: u64 = 1000;
const MAX_DELAY_MS: u64 = 60000;      // 1 minute max delay
```

### For Low-Latency Networks

```rust
const MAX_RETRIES: u32 = 3;
const INITIAL_DELAY_MS: u64 = 100;    // Very short delays
const MAX_DELAY_MS: u64 = 1000;
```

---

## 📈 Benefits

### Before (No Retry Logic)

- ❌ Single connection attempt
- ❌ Fails on temporary network issues
- ❌ Requires manual restart
- ❌ Poor user experience

### After (With Retry Logic)

- ✅ Multiple connection attempts
- ✅ Automatic recovery from transient failures
- ✅ Exponential backoff prevents network spam
- ✅ Parallel attempts speed up bootstrap
- ✅ Production-ready reliability

---

## 🔍 Monitoring

### Metrics to Track

1. **Connection Success Rate**
   - Percentage of bootstrap peers connected successfully
   - Track via logs: `"Successfully connected to X/Y bootstrap peer(s)"`

2. **Average Retry Count**
   - How many attempts typically needed
   - Log format: `"(attempt N)"`

3. **Connection Time**
   - Time from start to successful connection
   - Use log timestamps

### Health Indicators

| Indicator | Status | Action |
|-----------|--------|--------|
| 100% success on attempt 1 | ✅ Excellent | None needed |
| Success after 2-3 retries | ⚠️ Acceptable | Monitor network |
| Frequent failures after 5 retries | ❌ Problem | Check bootstrap peers |

---

## 🎯 Fallback Behavior

If all bootstrap connections fail:

1. **DHT Discovery** continues working
   - Nodes publish to DHT automatically
   - Can discover peers via DHT queries

2. **mDNS Discovery** continues working
   - Local network peer discovery
   - Automatic on same LAN/WiFi

3. **Gossip Discovery** continues working
   - Once any peer is found (via DHT/mDNS)
   - Gossip protocol shares more peers

**Result:** Network remains functional even without bootstrap peers!

---

## ✅ Summary

The retry logic ensures:

- 🔄 **5 automatic retry attempts** with exponential backoff
- ⚡ **Parallel connections** to all bootstrap peers
- 📊 **Clear logging** of connection progress
- 🛡️ **Graceful degradation** to DHT/mDNS if bootstrap fails
- 🚀 **Production-ready** reliability

**Total impact:** Dramatically improved connection success rate while maintaining network efficiency!
