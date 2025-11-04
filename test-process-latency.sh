#!/bin/bash

# Test script for process latency request feature
# This simulates publishing a process-latency-request via gossip

echo "🧪 Testing process latency request handler"
echo ""
echo "To test this feature:"
echo "1. Start the node: cargo run"
echo "2. In another terminal, use the client SDK to publish a test request:"
echo ""
echo "Example request payload:"
cat << 'EOF'
{
  "request_id": "test-123",
  "url": "https://api.github.com",
  "method": "GET",
  "headers": {
    "User-Agent": "CyberFly-Node"
  }
}
EOF
echo ""
echo "The node should:"
echo "  ✅ Receive the request on 'process-latency-request' topic"
echo "  ✅ Execute the HTTP request and measure latency"
echo "  ✅ Publish response to 'process-latency-response' topic"
echo ""
echo "Expected response format:"
cat << 'EOF'
{
  "request_id": "test-123",
  "status": 200,
  "statusText": "OK",
  "latency": 123.45,
  "nodeId": "<your-node-id>",
  "error": null
}
EOF
echo ""
echo "Check the logs for:"
echo "  - '⏱️  Received process-latency-request'"
echo "  - '⏱️  Processing latency request test-123 for URL: ...'"
echo "  - '✅ Latency request test-123 completed: X ms (status: 200)'"
echo "  - '📤 Published latency response for request test-123'"
