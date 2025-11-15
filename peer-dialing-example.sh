#!/bin/bash

# Example: How Peer Dialing Works With Only PeerId
# Demonstrates the peer discovery flow in your implementation

echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║         PRACTICAL EXAMPLE: PEER DIALING WITH PEER ID          ║"
echo "╚═══════════════════════════════════════════════════════════════╝"
echo ""

echo "📝 SCENARIO: Node A wants to connect to Node B"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

echo "Step 1: Node B Starts"
echo "  $ ./cyberfly-rust-node"
echo ""
echo "  Node B gets PeerId: b1234567...abcd"
echo "  ✅ Automatically publishes to DHT: b1234567 → 67.211.219.34:31001"
echo "  ✅ Broadcasts via mDNS on local network"
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

echo "Step 2: Node A Starts (Knows ONLY PeerId)"
echo "  Node A learns about Node B's PeerId through:"
echo ""
echo "  Option 1: Bootstrap Config"
echo "    BOOTSTRAP_PEERS='b1234567...abcd@67.211.219.34:31001'"
echo "    └─ First connection uses IP hint"
echo ""
echo "  Option 2: Gossip Discovery (Your New Feature!)"
echo "    Node C tells Node A: \"Hey, I know peer b1234567...\""
echo "    └─ No IP address shared!"
echo ""
echo "  Option 3: DHT Query"
echo "    Node A queries DHT: \"Where is b1234567?\""
echo "    └─ DHT responds with IP addresses"
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

echo "Step 3: Node A Dials Node B (Code Execution)"
echo ""
echo "Code Flow in src/iroh_network.rs:"
echo ""
cat << 'CODE'
// In handle_peer_discovery_event() - line ~1010
for peer_str in announcement.connected_peers {
    let peer_id = peer_str.parse::<EndpointId>()?;
    
    // Check if already connected
    if !self.discovered_peers.contains_key(&peer_id) {
        tracing::info!("Auto-connecting to discovered peer: {}", peer_id);
        
        // ✨ MAGIC HAPPENS HERE ✨
        // Connect using ONLY peer_id - no IP/port!
        if let Err(e) = self.dial_peer(peer_id).await {
            tracing::warn!("Failed to connect: {}", e);
        }
    }
}

// In dial_peer() - line 328
pub async fn dial_peer(&self, peer_id: EndpointId) -> Result<()> {
    // Iroh automatically:
    // 1. Queries DHT: peer_id → [IP addresses]
    // 2. Tries mDNS discovery
    // 3. Attempts each address
    // 4. Returns first successful connection
    
    let conn = self.endpoint
        .connect(peer_id, alpn)  // ← Only PeerId needed!
        .await?;
    
    tracing::info!("Successfully connected to peer: {}", peer_id);
    Ok(())
}
CODE
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

echo "Step 4: What Happens Inside endpoint.connect()"
echo ""
echo "  Iroh Endpoint Logic:"
echo ""
echo "  1️⃣  Check local cache for peer addresses"
echo "     └─ If found, try cached addresses first"
echo ""
echo "  2️⃣  Query DHT for peer_id"
echo "     └─ Send DHT_FIND_NODE(peer_id)"
echo "     └─ Receive: [67.211.219.34:31001, 192.168.1.42:31001]"
echo ""
echo "  3️⃣  Try mDNS discovery (if on local network)"
echo "     └─ Broadcast: \"Looking for peer_id b1234567...\""
echo "     └─ Node B responds: \"I'm at 192.168.1.42:31001\""
echo ""
echo "  4️⃣  Attempt connections in parallel"
echo "     ├─ Try: 67.211.219.34:31001"
echo "     ├─ Try: 192.168.1.42:31001"
echo "     └─ Return first successful connection"
echo ""
echo "  5️⃣  Store successful connection"
echo "     └─ Cache address for future use"
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

echo "Step 5: Result"
echo "  ✅ Node A connected to Node B"
echo "  ✅ Used only PeerId (b1234567...)"
echo "  ✅ No manual IP configuration needed"
echo "  ✅ Works across different networks"
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

echo "🎯 KEY TAKEAWAY"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "In your implementation:"
echo ""
echo "  ❌ NOT needed: endpoint.connect(peer_id, ip, port)"
echo "  ✅ Already works: endpoint.connect(peer_id, alpn)"
echo ""
echo "The DHT/mDNS discovery automatically resolves peer_id → addresses!"
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

echo "📊 DISCOVERY METHOD COMPARISON"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
printf "%-20s %-15s %-20s %s\n" "Method" "Scope" "Latency" "Notes"
echo "────────────────────────────────────────────────────────────────"
printf "%-20s %-15s %-20s %s\n" "mDNS" "Local Network" "~1-2 seconds" "Fast, automatic"
printf "%-20s %-15s %-20s %s\n" "DHT" "Global" "~3-10 seconds" "Reliable, decentralized"
printf "%-20s %-15s %-20s %s\n" "Gossip (yours!)" "Connected Peers" "Instant" "Fastest, social"
printf "%-20s %-15s %-20s %s\n" "Bootstrap Hint" "N/A" "Instant" "First connection only"
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

echo "🧪 TEST THIS YOURSELF"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

echo "Test 1: mDNS Discovery (Same Network)"
echo "  Terminal 1: PORT=8080 ./target/release/cyberfly-rust-node"
echo "  Terminal 2: PORT=8081 ./target/release/cyberfly-rust-node"
echo ""
echo "  Watch Terminal 2 logs:"
echo "    → Discovered peer via mDNS: abc123..."
echo "    → Auto-connecting to discovered peer: abc123..."
echo "    → Successfully connected!"
echo ""

echo "Test 2: Gossip Discovery (Your Feature!)"
echo "  Terminal 1 (Node A): PORT=8080 ./target/release/cyberfly-rust-node"
echo "  Terminal 2 (Node B): PORT=8081 BOOTSTRAP_PEERS='<Node_A_PeerId>' ..."
echo "  Terminal 3 (Node C): PORT=8082 BOOTSTRAP_PEERS='<Node_A_PeerId>' ..."
echo ""
echo "  After ~10 seconds (gossip broadcast):"
echo "    → Node B receives gossip: \"Node A knows Node C\""
echo "    → Node B auto-connects to Node C using only PeerId"
echo "    → Full mesh formed: A ↔ B ↔ C"
echo ""

echo "Test 3: DHT Discovery (Different Networks)"
echo "  Machine A: ./target/release/cyberfly-rust-node"
echo "  Machine B (different network): ./target/release/cyberfly-rust-node"
echo ""
echo "  After ~30 seconds (DHT propagation):"
echo "    → Both nodes published to DHT"
echo "    → Can connect using only PeerId"
echo "    → No bootstrap config needed!"
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

echo "✅ CONCLUSION"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "Your system ALREADY supports dialing with only PeerId! 🎉"
echo ""
echo "The peerId@ip:port format is ONLY for:"
echo "  • Initial bootstrap (faster first connection)"
echo "  • Direct connection hints"
echo ""
echo "After the first connection, everything uses PeerId only!"
echo ""
echo "Your gossip discovery makes it even better by:"
echo "  • Sharing PeerIds through social discovery"
echo "  • Faster than DHT queries"
echo "  • Building full mesh topology automatically"
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "📖 See PEER_DIALING_EXPLAINED.md for detailed explanation"
echo ""
