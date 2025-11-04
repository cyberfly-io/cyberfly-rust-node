# Process Latency Request Feature

## Overview
The process latency request feature allows nodes in the CyberFly network to measure HTTP request latency in a distributed manner. Nodes can request other nodes to perform HTTP requests and report back the timing results.

## Implementation Details

### Gossip Topics
- **Request Topic**: `process-latency-request`
- **Response Topic**: `process-latency-response`

### Request Format
```json
{
  "request_id": "unique-identifier",
  "url": "https://example.com/api/endpoint",
  "method": "GET",
  "headers": {
    "User-Agent": "CyberFly-Node",
    "Content-Type": "application/json"
  },
  "body": "{\"optional\":\"payload\"}"
}
```

**Fields:**
- `request_id` (required): Unique identifier for tracking the request
- `url` (required): The HTTP endpoint to test
- `method` (optional): HTTP method (GET, POST, PUT, PATCH, DELETE). Default: GET
- `headers` (optional): HTTP headers to include
- `body` (optional): Request body for POST/PUT/PATCH requests

### Response Format
```json
{
  "request_id": "unique-identifier",
  "status": 200,
  "statusText": "OK",
  "latency": 123.45,
  "nodeRegion": "us-east-1",
  "nodeId": "node-peer-id",
  "error": null
}
```

**Fields:**
- `request_id`: Matches the request ID
- `status`: HTTP status code (0 if request failed)
- `statusText`: HTTP status text or "Error"
- `latency`: Request duration in milliseconds
- `nodeRegion`: AWS region mapped from IP geolocation (e.g. "us-east-1", "eu-west-1")
- `nodeId`: The node that performed the request
- `error`: Error message if request failed, null otherwise

## Architecture

### Rust Implementation (`src/iroh_network.rs`)

1. **Message Handler** (Line ~677):
   - Intercepts gossip messages on the `data` topic
   - Checks for `process-latency-request` topic in message metadata
   - Spawns async task to handle request without blocking

2. **Request Handler** (Line ~855):
   - `handle_process_latency_request()` function
   - Parses request from JSON payload
   - Builds HTTP request using `reqwest` client
   - Measures timing with `tokio::time::Instant`
   - Handles errors gracefully (timeouts, DNS failures, etc.)
   - Publishes response to `process-latency-response` topic

### Key Features
- **Non-blocking**: Requests are handled in separate async tasks
- **Timeout**: 30-second timeout for HTTP requests
- **Error Handling**: Network errors, DNS failures, and timeouts are captured
- **Timing Precision**: Microsecond precision using Tokio's Instant
- **Gossip-only**: Uses P2P gossip protocol, no GraphQL dependency

## Usage Examples

### Publishing a Request
Requests must be published to the gossip network on the `process-latency-request` topic. This can be done via:
- MQTT bridge (publish to MQTT, forwarded to gossip)
- Direct gossip broadcast
- Future GraphQL mutation (to be added)

### Example Request
```json
{
  "request_id": "test-api-latency",
  "url": "https://api.github.com",
  "method": "GET",
  "headers": {
    "User-Agent": "CyberFly-Latency-Test"
  }
}
```

### Subscribing to Responses
Subscribe to the `process-latency-response` topic to receive results from all nodes that processed requests.

## Log Messages

When processing latency requests, you'll see:
```
⏱️  Received process-latency-request
⏱️  Processing latency request test-api-latency for URL: https://api.github.com
✅ Latency request test-api-latency completed: 145.32 ms (status: 200)
📤 Published latency response for request test-api-latency
```

For failed requests:
```
⏱️  Received process-latency-request
⏱️  Processing latency request test-bad-url for URL: https://invalid-domain.com
❌ Latency request test-bad-url failed: dns error: failed to lookup address information
📤 Published latency response for request test-bad-url
```

## Testing

### Manual Test
1. Start the node:
   ```bash
   cargo run
   ```

2. In another terminal, publish a test request via MQTT:
   ```bash
   mosquitto_pub -t process-latency-request -m '{
     "request_id": "test-1",
     "url": "https://api.github.com",
     "method": "GET"
   }'
   ```

3. Subscribe to responses:
   ```bash
   mosquitto_sub -t process-latency-response
   ```

### Example Script
See `client-sdk/test-process-latency.ts` for request/response format examples.

Run with:
```bash
cd client-sdk
npx tsx test-process-latency.ts
```

## Comparison with JavaScript Implementation

This Rust implementation **exactly mirrors** the JavaScript pattern from `cyberfly-node`:

**Matching Features:**
- ✅ Subscribes to `process-latency-request` topic (JS uses `fetch-latency-request`)
- ✅ Performs HTTP request with timing measurement
- ✅ Publishes response to `process-latency-response` topic (JS uses `api-latency`)
- ✅ Includes error handling for network failures
- ✅ Reports `nodeId` in response
- ✅ Reports `nodeRegion` fetched from `http://ip-api.com/json/` on startup
- ✅ Maps country codes to AWS regions (same mapping as JS)
- ✅ Caches region globally (same as JS `cachedNodeRegion`)
- ✅ Returns "unknown" if region fetch fails
- ✅ Same response format with camelCase field names (`statusText`, `nodeId`, `nodeRegion`)

**Implementation Details:**
- **Region Detection**: Both implementations call `http://ip-api.com/json/` on startup
- **Region Mapping**: Identical AWS region mapping (US→us-east-1, GB→eu-west-2, etc.)
- **Fallback**: Both use `{countryCode}-region-1` format for unmapped countries
- **Timing**: JS uses `performance.now()`, Rust uses `tokio::time::Instant`
- **Response Topic**: Rust uses more descriptive name but same functionality

## Future Enhancements

1. **GraphQL Mutation**: Add `publishGossipMessage` mutation for easier testing
2. **Metrics**: Track latency measurement statistics
3. **Rate Limiting**: Prevent abuse by limiting requests per peer
4. **Request Validation**: Validate URLs, block internal IPs, etc.
5. **Custom Region Override**: Allow `NODE_REGION` environment variable to override auto-detection

## Files Modified
- `src/node_region.rs`: **NEW** - Node region detection module (mirrors JS `node-region.ts`)
  - Fetches location from `http://ip-api.com/json/` on startup
  - Maps country codes to AWS regions
  - Caches region globally using `once_cell::OnceCell`
- `src/lib.rs`: Added `node_region` module declaration
- `src/main.rs`: 
  - Added `node_region` module declaration
  - Calls `fetch_and_set_node_region()` on startup (line ~77)
- `src/iroh_network.rs`: Added process latency request handling
  - Message interception at line ~677
  - Handler function at line ~906
  - Uses `node_region::get_node_region()` for response
- `Cargo.toml`: Added `once_cell = "1.19"` dependency
- `client-sdk/test-process-latency.ts`: Example request/response formats
- `test-process-latency.sh`: Test instructions

## Dependencies
- `reqwest`: HTTP client for making requests
- `tokio`: Async runtime and timing
- `serde_json`: JSON parsing/serialization
- Existing: Iroh gossip protocol, GossipMessage struct
