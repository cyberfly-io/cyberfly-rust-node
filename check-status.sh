#!/bin/bash

# Quick status check for peer connection

echo "=================================="
echo "Cyberfly Node Connection Status"
echo "=================================="
echo ""

# Check if node is running
if lsof -i :11204 > /dev/null 2>&1; then
    echo "✅ Node is running on port 11204"
else
    echo "❌ Node is NOT running on port 11204"
    exit 1
fi

echo ""
echo "Testing peer discovery via GraphQL..."
echo ""

# Query for discovered peers
RESPONSE=$(curl -s -X POST http://localhost:8080 \
  -H "Content-Type: application/json" \
  -d '{"query":"query { getDiscoveredPeers { nodeId lastSeen } }"}')

# Check if we got peers
PEER_COUNT=$(echo "$RESPONSE" | grep -o '"nodeId"' | wc -l)

if [ "$PEER_COUNT" -gt 0 ]; then
    echo "✅ Discovered $PEER_COUNT peer(s):"
    echo "$RESPONSE" | jq '.data.getDiscoveredPeers' 2>/dev/null || echo "$RESPONSE"
else
    echo "⚠️  No peers discovered yet"
    echo "Response: $RESPONSE"
fi

echo ""
echo "=================================="
echo "To monitor logs in real-time:"
echo "  tail -f <log_file>"
echo "Or grep for gossip events:"
echo "  cargo run --release 2>&1 | grep neighbor"
echo "=================================="
