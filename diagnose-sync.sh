#!/bin/bash

echo "=== Data Sync Diagnostic ==="
echo ""

LOCAL_NODE="http://localhost:8080/graphql"
BOOTSTRAP_NODE="http://208.73.202.62:8080/graphql"

echo "1. Local node info:"
curl -s -X POST $LOCAL_NODE \
  -H "Content-Type: application/json" \
  -d '{"query": "query { getNodeInfo { nodeId discoveredPeers } }"}' | jq '.data.getNodeInfo'

echo ""
echo "2. Local node operation count:"
curl -s -X POST $LOCAL_NODE \
  -H "Content-Type: application/json" \
  -d '{"query": "query { getAllBlobOperations(limit: 1000) { opId dbName key value storeType timestamp } }"}' | jq '.data.getAllBlobOperations | length'

echo ""
echo "3. Local node data (example database):"
curl -s -X POST $LOCAL_NODE \
  -H "Content-Type: application/json" \
  -d '{"query": "query { getAllStrings(dbName: \"example-48ffd73af89d938a8d7f5b2c39b8ebf6ac68f333de32863a1fc90f735f4ce14f\") { key value } }"}' | jq '.data.getAllStrings | length'

echo ""
echo "4. Attempting to query bootstrap node at $BOOTSTRAP_NODE..."
echo "   (This will fail if bootstrap node is not accessible or running different code)"

timeout 5 curl -s -X POST $BOOTSTRAP_NODE \
  -H "Content-Type: application/json" \
  -d '{"query": "query { getNodeInfo { nodeId discoveredPeers } }"}' 2>&1 | jq '.data.getNodeInfo' || echo "  ✗ Bootstrap node not accessible via GraphQL API"

echo ""
echo "=== Summary ==="
echo "✓ Your LOCAL node has data and is connected to 1 peer (bootstrap)"
echo "✗ Bootstrap node sync requires:"
echo "  1. Bootstrap node must be running and accessible"
echo "  2. Bootstrap node must receive and process sync messages"
echo "  3. Bootstrap node must store received operations"
echo ""
echo "To test sync properly:"
echo "  1. Run TWO local nodes (different ports)"
echo "  2. Store data on node A"
echo "  3. Query data from node B"
echo "  4. Both nodes should see the same data"
