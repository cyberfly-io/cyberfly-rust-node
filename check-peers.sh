#!/bin/bash

echo "🔍 Peer Discovery Troubleshooting"
echo "=================================="
echo ""

# Check if node is running
if ! pgrep -f "cyberfly-rust-node" > /dev/null; then
    echo "❌ Node is NOT running"
    echo ""
    echo "Start the node with:"
    echo "  cargo run --release"
    exit 1
fi

PID=$(pgrep -f "cyberfly-rust-node")
START_TIME=$(ps -p $PID -o lstart=)
BUILD_TIME=$(stat -f "%Sm" -t "%b %d %H:%M:%S %Y" ./target/release/cyberfly-rust-node)

echo "Node Status:"
echo "  PID: $PID"
echo "  Started: $START_TIME"
echo "  Binary built: $BUILD_TIME"
echo ""

# Check if node needs restart
NODE_START_EPOCH=$(date -j -f "%a %b %d %H:%M:%S %Y" "$START_TIME" "+%s" 2>/dev/null)
BUILD_EPOCH=$(stat -f "%m" ./target/release/cyberfly-rust-node 2>/dev/null)

if [ -n "$NODE_START_EPOCH" ] && [ -n "$BUILD_EPOCH" ]; then
    if [ $NODE_START_EPOCH -lt $BUILD_EPOCH ]; then
        echo "⚠️  WARNING: Node is running OLD code!"
        echo "   Binary was rebuilt AFTER the node started"
        echo ""
        echo "   ACTION REQUIRED:"
        echo "   1. Stop the current node (Ctrl+C in its terminal)"
        echo "   2. Run: cargo run --release"
        echo ""
        exit 1
    fi
fi

echo "✅ Node is running the latest code"
echo ""

# Check current status
echo "Current Peer Status:"
PEERS_JSON=$(curl -s -X POST http://localhost:8080/ -H "Content-Type: application/json" \
  -d '{"query":"query { getNodeInfo { nodeId connectedPeers discoveredPeers } }"}')

NODE_ID=$(echo "$PEERS_JSON" | jq -r '.data.getNodeInfo.nodeId')
CONNECTED=$(echo "$PEERS_JSON" | jq -r '.data.getNodeInfo.connectedPeers')
DISCOVERED=$(echo "$PEERS_JSON" | jq -r '.data.getNodeInfo.discoveredPeers')

echo "  NodeID: $NODE_ID"
echo "  Connected Peers: $CONNECTED"
echo "  Discovered Peers: $DISCOVERED"
echo ""

if [ "$DISCOVERED" = "0" ]; then
    echo "❌ No peers discovered yet"
    echo ""
    echo "Possible reasons:"
    echo ""
    echo "1. Machine 2 not connecting"
    echo "   - Check if Machine 2 is running"
    echo "   - Verify Machine 2 has correct BOOTSTRAP_PEERS config"
    echo "   - Expected format: export BOOTSTRAP_PEERS=\"$NODE_ID@10.48.44.225:<port>\""
    echo ""
    echo "2. Check network connectivity"
    echo "   - Can Machine 2 reach this machine?"
    echo "   - Are firewall rules blocking the connection?"
    echo ""
    echo "3. Check Iroh listening ports:"
    lsof -nP -iUDP | grep cyberfly | awk '{print "   " $9}' | head -2
    echo ""
    echo "4. Look for connection attempts in logs"
    echo "   - Look for: 'inserting new node in NodeMap'"
    echo "   - Look for: 'neighbor up'"
    echo ""
else
    echo "✅ $DISCOVERED peer(s) discovered!"
    echo ""
    echo "Peer details:"
    curl -s -X POST http://localhost:8080/ -H "Content-Type: application/json" \
      -d '{"query":"query { getDiscoveredPeers { peerId lastSeen } }"}' | jq '.data.getDiscoveredPeers'
fi

echo ""
echo "=================================="
