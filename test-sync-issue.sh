#!/bin/bash
# Test sync between nodes

echo "=== Testing Sync Issue ==="
echo ""

# Get bootstrap node data (assuming it's running on bootstrap peer)
echo "1. Querying local node for all data..."
curl -s -X POST http://localhost:8080/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "query { getAllData { key value storeType timestamp } }"
  }' | jq '.'

echo ""
echo "2. Checking node info (peer connections)..."
curl -s -X POST http://localhost:8080/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "query { getNodeInfo { nodeId discoveredPeers } }"
  }' | jq '.'

echo ""
echo "3. Testing manual sync request to bootstrap peer..."
BOOTSTRAP_PEER="80c014da61200a19a8362ab999915cc8e8970b1b068360199c353deb4948abcf"
curl -s -X POST http://localhost:8080/graphql \
  -H "Content-Type: application/json" \
  -d "{
    \"query\": \"mutation { requestSync(peerNodeId: \\\"$BOOTSTRAP_PEER\\\") }\"
  }" | jq '.'

echo ""
echo "=== Waiting 5 seconds for sync to complete ==="
sleep 5

echo ""
echo "4. Querying data again after sync..."
curl -s -X POST http://localhost:8080/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "query { getAllData { key value storeType timestamp } }"
  }' | jq '.'
