#!/bin/bash
# Sync Health Check and Manual Trigger

echo "=== Sync Health Check ==="
echo ""

LOCAL_OPS=$(curl -s -X POST http://localhost:8080/graphql -H "Content-Type: application/json" -d '{"query": "query { getAllBlobOperations(limit: 1000) { opId } }"}' | jq '.data.getAllBlobOperations | length')

BS_OPS=$(curl -s -X POST http://208.73.202.62:8080/graphql -H "Content-Type: application/json" -d '{"query": "query { getAllBlobOperations(limit: 1000) { opId } }"}' | jq '.data.getAllBlobOperations | length')

echo "Local node operations: $LOCAL_OPS"
echo "Bootstrap node operations: $BS_OPS"
echo ""

if [ "$LOCAL_OPS" -gt "$BS_OPS" ]; then
    echo "⚠️  Local node has MORE data than bootstrap"
    echo "   Bootstrap needs to pull from local node"
    echo ""
    echo "   Solution: Bootstrap node should run:"
    echo "   curl -X POST http://208.73.202.62:8080/graphql \\"
    echo "     -H 'Content-Type: application/json' \\"
    echo "     -d '{\"query\": \"mutation { requestSync(peerId: \\\"bdb4c73a758e0ec8e00a6cfdd35e31127a193669c32913cb4f4b58d300924bef\\\", fullSync: true) { success message } }\"}'"
elif [ "$BS_OPS" -gt "$LOCAL_OPS" ]; then
    echo "⚠️  Bootstrap has MORE data than local node"
    echo "   Local node needs to pull from bootstrap"
    echo ""
    echo "Triggering sync from bootstrap..."
    curl -s -X POST http://localhost:8080/graphql \
      -H "Content-Type: application/json" \
      -d '{"query": "mutation { requestSync(peerId: \"80c014da61200a19a8362ab999915cc8e8970b1b068360199c353deb4948abcf\", fullSync: true) { success message } }"}' | jq '.'
    
    echo ""
    echo "Waiting 3 seconds..."
    sleep 3
    
    NEW_LOCAL_OPS=$(curl -s -X POST http://localhost:8080/graphql -H "Content-Type: application/json" -d '{"query": "query { getAllBlobOperations(limit: 1000) { opId } }"}' | jq '.data.getAllBlobOperations | length')
    echo "Local operations after sync: $NEW_LOCAL_OPS"
else
    echo "✓ Both nodes have same operation count"
fi
