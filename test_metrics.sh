#!/bin/bash

echo "=== Testing Metrics Endpoint ==="
echo ""

# Check if metrics endpoint responds
echo "1. Testing /metrics endpoint availability..."
curl -s http://localhost:8080/metrics > /dev/null 2>&1
if [ $? -eq 0 ]; then
    echo "✅ Metrics endpoint is accessible"
else
    echo "❌ Metrics endpoint not available (node might not be running)"
    exit 1
fi

echo ""
echo "2. Fetching metrics..."
METRICS=$(curl -s http://localhost:8080/metrics)

# Check for key metrics
echo ""
echo "3. Checking for storage metrics..."
echo "$METRICS" | grep -q "storage_reads_total"
[ $? -eq 0 ] && echo "✅ storage_reads_total found" || echo "❌ storage_reads_total missing"

echo "$METRICS" | grep -q "storage_writes_total"
[ $? -eq 0 ] && echo "✅ storage_writes_total found" || echo "❌ storage_writes_total missing"

echo ""
echo "4. Checking for cache metrics..."
echo "$METRICS" | grep -q "cache_hits_total"
[ $? -eq 0 ] && echo "✅ cache_hits_total found" || echo "❌ cache_hits_total missing"

echo "$METRICS" | grep -q "cache_misses_total"
[ $? -eq 0 ] && echo "✅ cache_misses_total found" || echo "❌ cache_misses_total missing"

echo "$METRICS" | grep -q "cache_size_hot"
[ $? -eq 0 ] && echo "✅ cache_size_hot found" || echo "❌ cache_size_hot missing"

echo ""
echo "5. Calculating cache hit rate..."
HITS=$(echo "$METRICS" | grep "^cache_hits_total" | awk '{print $2}')
MISSES=$(echo "$METRICS" | grep "^cache_misses_total" | awk '{print $2}')

if [ -n "$HITS" ] && [ -n "$MISSES" ]; then
    TOTAL=$((HITS + MISSES))
    if [ $TOTAL -gt 0 ]; then
        HIT_RATE=$(awk "BEGIN {printf \"%.2f\", ($HITS/$TOTAL)*100}")
        echo "Cache Hits: $HITS"
        echo "Cache Misses: $MISSES"
        echo "Cache Hit Rate: ${HIT_RATE}%"
    else
        echo "No cache operations yet"
    fi
else
    echo "Unable to calculate hit rate"
fi

echo ""
echo "6. Checking latency histograms..."
echo "$METRICS" | grep -q "storage_read_duration_seconds"
[ $? -eq 0 ] && echo "✅ Read latency histogram found" || echo "❌ Read latency histogram missing"

echo "$METRICS" | grep -q "storage_write_duration_seconds"
[ $? -eq 0 ] && echo "✅ Write latency histogram found" || echo "❌ Write latency histogram missing"

echo ""
echo "=== Test Complete ==="
