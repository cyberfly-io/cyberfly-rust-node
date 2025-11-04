/**
 * Example: Test Process Latency Request Feature
 * 
 * This demonstrates how to:
 * 1. Publish a process-latency-request via the data submission
 * 2. Subscribe to process-latency-response to receive results
 * 
 * Note: Since the client SDK uses GraphQL for data submission,
 * you would need to publish the request directly via MQTT or
 * by adding a GraphQL mutation for gossip messages.
 * 
 * For now, this shows the expected message format.
 */

interface ProcessLatencyRequest {
  request_id: string;
  url: string;
  method?: string;
  headers?: Record<string, string>;
  body?: string;
}

interface ProcessLatencyResponse {
  request_id: string;
  status: number;
  statusText: string;
  latency: number;
  nodeRegion: string;
  nodeId: string;
  error?: string;
}

console.log('🧪 Process Latency Request Test Examples\n');

// Example 1: Simple GET request
const request1: ProcessLatencyRequest = {
  request_id: 'test-github-api',
  url: 'https://api.github.com',
  method: 'GET',
  headers: {
    'User-Agent': 'CyberFly-Node-Test',
  },
};

console.log('📤 Example Request 1 (GitHub API):');
console.log(JSON.stringify(request1, null, 2));
console.log();

// Example 2: POST request with body
const request2: ProcessLatencyRequest = {
  request_id: 'test-post-request',
  url: 'https://httpbin.org/post',
  method: 'POST',
  headers: {
    'Content-Type': 'application/json',
  },
  body: JSON.stringify({
    test: 'data',
    timestamp: Date.now(),
  }),
};

console.log('📤 Example Request 2 (POST with body):');
console.log(JSON.stringify(request2, null, 2));
console.log();

// Example expected response
const expectedResponse: ProcessLatencyResponse = {
  request_id: 'test-github-api',
  status: 200,
  statusText: 'OK',
  latency: 123.45,
  nodeRegion: 'us-east-1',
  nodeId: 'your-node-id-here',
};

console.log('✅ Expected Response Format:');
console.log(JSON.stringify(expectedResponse, null, 2));
console.log();

console.log('📝 To test this feature:');
console.log('  1. Start the Rust node: cargo run');
console.log('  2. Publish a request to the "process-latency-request" gossip topic');
console.log('  3. Subscribe to "process-latency-response" to receive results');
console.log('  4. Check node logs for:');
console.log('     - "⏱️  Received process-latency-request"');
console.log('     - "⏱️  Processing latency request..."');
console.log('     - "✅ Latency request ... completed: X ms"');
console.log('     - "📤 Published latency response..."');

