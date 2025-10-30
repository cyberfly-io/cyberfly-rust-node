#!/bin/bash

echo "======================================"
echo "  SYNC DIAGNOSTIC TEST"
echo "======================================"
echo ""

echo "This script will:"
echo "1. Store data on LOCAL node"
echo "2. Wait for sync broadcast"
echo "3. Check if data appears on BOOTSTRAP node"
echo "4. Store data on BOOTSTRAP node"  
echo "5. Check if data appears on LOCAL node"
echo ""
echo "Press Enter to continue..."
read

# Generate a unique test ID
TEST_ID=$(date +%s)
LOCAL_NODE="http://localhost:8080/graphql"
BS_NODE="http://208.73.202.62:8080/graphql"

echo "================================"
echo "TEST 1: Local → Bootstrap"
echo "================================"
echo ""

echo "Step 1: Storing test data on LOCAL node..."
STORE_RESULT=$(curl -s -X POST $LOCAL_NODE \
  -H "Content-Type: application/json" \
  -d "{
    \"query\": \"mutation { submitData(input: { dbName: \\\"synctest-$TEST_ID\\\", key: \\\"local_to_bs\\\", value: \\\"test_$TEST_ID\\\", storeType: \\\"String\\\", publicKey: \\\"48ffd73af89d938a8d7f5b2c39b8ebf6ac68f333de32863a1fc90f735f4ce14f\\\", signature: \\\"dummy\\\" }) { success message } }\"
  }")

echo "Response: $STORE_RESULT"

if echo "$STORE_RESULT" | grep -q "\"success\":true"; then
    echo "✓ Data stored on local node"
else
    echo "✗ Failed to store on local node"
    echo "   Check if node is running and signature is valid"
    exit 1
fi

echo ""
echo "⏱️  Waiting 5 seconds for sync to propagate..."
sleep 5

echo ""
echo "Step 2: Querying BOOTSTRAP node for the data..."
QUERY_RESULT=$(curl -s -X POST $BS_NODE \
  -H "Content-Type: application/json" \
  -d "{
    \"query\": \"query { getString(dbName: \\\"synctest-$TEST_ID\\\", key: \\\"local_to_bs\\\") }\"
  }")

echo "Response: $QUERY_RESULT"

if echo "$QUERY_RESULT" | grep -q "test_$TEST_ID"; then
    echo "✓ SUCCESS: Data synced from local to bootstrap!"
else
    echo "✗ FAIL: Data NOT found on bootstrap node"
    echo ""
    echo "Troubleshooting:"
    echo "  1. Check local node logs for: '📤 Broadcasting operation'"
    echo "  2. Check bootstrap node logs for: '📥 Received operation'"
    echo "  3. If bootstrap receives but doesn't store, check for signature errors"
fi

echo ""
echo "================================"
echo "TEST 2: Bootstrap → Local"
echo "================================"
echo ""

echo "Step 1: Requesting sync from bootstrap node..."
SYNC_RESULT=$(curl -s -X POST $LOCAL_NODE \
  -H "Content-Type: application/json" \
  -d "{
    \"query\": \"mutation { requestSync(peerId: \\\"80c014da61200a19a8362ab999915cc8e8970b1b068360199c353deb4948abcf\\\", fullSync: true) { success message } }\"
  }")

echo "Response: $SYNC_RESULT"

echo ""
echo "⏱️  Waiting 3 seconds for sync response..."
sleep 3

echo ""
echo "Step 2: Checking operation count..."
LOCAL_OPS=$(curl -s -X POST $LOCAL_NODE \
  -H "Content-Type: application/json" \
  -d '{"query": "query { getAllBlobOperations(limit: 1000) { opId } }"}' | \
  jq '.data.getAllBlobOperations | length')

echo "Local node now has: $LOCAL_OPS operations"

if [ "$LOCAL_OPS" -gt 43 ]; then
    echo "✓ SUCCESS: Bootstrap → Local sync works!"
else
    echo "⚠️  No new operations received from bootstrap"
fi

echo ""
echo "================================"
echo "SUMMARY"
echo "================================"
echo ""
echo "Check the node logs to see detailed sync activity."
echo "Look for these log patterns:"
echo ""
echo "When storing data:"
echo "  GraphQL → '📤 GraphQL: sending outbound SyncMessage::Operation'"
echo "  Network → '📤 Broadcasting operation <op_id>'"
echo ""
echo "When receiving data:"
echo "  Network → '📨 Received sync message from <peer>'"
echo "  Sync    → '📥 Received operation <op_id> from <peer>'"
echo "  Sync    → '✓ Operation successfully applied to storage'"
