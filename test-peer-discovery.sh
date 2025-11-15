#!/usr/bin/env bash
# Test peer discovery protocol with 3 nodes

set -e

echo "🧪 Testing Peer Discovery Protocol"
echo "==================================="
echo ""

# Build the project
echo "📦 Building project..."
cargo build --release
echo ""

# Clean up any existing data directories
echo "🧹 Cleaning up old data..."
rm -rf ./data_node1 ./data_node2 ./data_node3
echo ""

# Start Node 1 (Bootstrap)
echo "🚀 Starting Node 1 (Bootstrap)..."
IROH_DATA_DIR=./data_node1 \
GRAPHQL_PORT=8001 \
METRICS_PORT=9001 \
cargo run --release > node1.log 2>&1 &
NODE1_PID=$!
echo "   PID: $NODE1_PID"
sleep 5

# Extract Node 1 ID from logs
NODE1_ID=$(grep -m 1 "Node ID:" node1.log | sed 's/.*Node ID: //' || echo "")
if [ -z "$NODE1_ID" ]; then
    echo "❌ Failed to extract Node 1 ID"
    kill $NODE1_PID
    exit 1
fi
echo "   Node ID: $NODE1_ID"
echo ""

# Start Node 2
echo "🚀 Starting Node 2..."
IROH_DATA_DIR=./data_node2 \
GRAPHQL_PORT=8002 \
METRICS_PORT=9002 \
BOOTSTRAP_PEERS="${NODE1_ID}@127.0.0.1:31001" \
cargo run --release > node2.log 2>&1 &
NODE2_PID=$!
echo "   PID: $NODE2_PID"
sleep 5

# Extract Node 2 ID
NODE2_ID=$(grep -m 1 "Node ID:" node2.log | sed 's/.*Node ID: //' || echo "")
echo "   Node ID: $NODE2_ID"
echo ""

# Start Node 3
echo "🚀 Starting Node 3..."
IROH_DATA_DIR=./data_node3 \
GRAPHQL_PORT=8003 \
METRICS_PORT=9003 \
BOOTSTRAP_PEERS="${NODE1_ID}@127.0.0.1:31001" \
cargo run --release > node3.log 2>&1 &
NODE3_PID=$!
echo "   PID: $NODE3_PID"
sleep 5

# Extract Node 3 ID
NODE3_ID=$(grep -m 1 "Node ID:" node3.log | sed 's/.*Node ID: //' || echo "")
echo "   Node ID: $NODE3_ID"
echo ""

echo "✅ All nodes started!"
echo ""
echo "📊 Node Information:"
echo "   Node 1: http://localhost:8001/graphql (Metrics: http://localhost:9001/metrics)"
echo "   Node 2: http://localhost:8002/graphql (Metrics: http://localhost:9002/metrics)"
echo "   Node 3: http://localhost:8003/graphql (Metrics: http://localhost:9003/metrics)"
echo ""
echo "🔍 Monitoring peer discovery..."
echo "   Watch for: '📡 Broadcasted peer list' and '✓ Established X new peer connection(s)'"
echo ""

# Monitor logs for peer discovery
echo "📝 Live logs (Ctrl+C to stop):"
echo ""

# Function to cleanup on exit
cleanup() {
    echo ""
    echo ""
    echo "🛑 Stopping all nodes..."
    kill $NODE1_PID $NODE2_PID $NODE3_PID 2>/dev/null || true
    echo ""
    echo "📋 Final peer counts:"
    echo ""
    
    echo "Node 1 discovered peers:"
    grep "Broadcasted peer list" node1.log | tail -1 || echo "  No broadcasts yet"
    
    echo ""
    echo "Node 2 discovered peers:"
    grep "Broadcasted peer list" node2.log | tail -1 || echo "  No broadcasts yet"
    
    echo ""
    echo "Node 3 discovered peers:"
    grep "Broadcasted peer list" node3.log | tail -1 || echo "  No broadcasts yet"
    
    echo ""
    echo "🔗 Connection events:"
    grep -h "Established.*new peer connection" node*.log | tail -10 || echo "  None found"
    
    echo ""
    echo "💾 Logs saved to: node1.log, node2.log, node3.log"
    echo ""
}

trap cleanup EXIT INT TERM

# Tail all logs with color coding
tail -f node1.log node2.log node3.log | grep --line-buffered -E "📡|📋|🔗|✓ Established|✓ Successfully connected"
