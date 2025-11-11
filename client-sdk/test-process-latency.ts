/**
 * Example: Test Fetch Latency Request Feature
 * 
 * This demonstrates how to:
 * 1. Publish a fetch-latency-request via the data submission (with signature)
 * 2. Subscribe to api-latency to receive results
 * 
 * Note: Since the client SDK uses GraphQL for data submission,
 * you would need to publish the request directly via MQTT or
 * by adding a GraphQL mutation for gossip messages.
 * 
 * For now, this shows the expected message format.
 */

interface FetchLatencyRequestData {
  request_id: string;
  url: string;
  method?: string;
  headers?: Record<string, string>;
  body?: string;
}

interface SignedFetchLatencyRequest {
  data: FetchLatencyRequestData;
  sig: string;
  pubkey: string;
}

interface FetchLatencyResponse {
  request_id: string;
  status: number;
  statusText: string;
  latency: number;
  nodeRegion: string;
  nodeId: string;
  error?: string;
}

console.log('🧪 Fetch Latency Request Test Examples\n');

// Example 1: Simple GET request (needs to be signed)
const requestData1: FetchLatencyRequestData = {
  request_id: 'test-github-api',
  url: 'https://api.github.com',
  method: 'GET',
  headers: {
    'User-Agent': 'CyberFly-Node-Test',
  },
};

// Note: In production, you need to sign this with your private key
const signedRequest1: SignedFetchLatencyRequest = {
  data: requestData1,
  sig: 'YOUR_SIGNATURE_HERE', // Sign JSON.stringify(requestData1) with your private key
  pubkey: 'efcfe1ac4de7bcb991d8b08a7d8ebed2377a6ed1070636dc66d9cdd225458aaa', // Whitelisted key
};

console.log('📤 Example Request 1 (GitHub API - Signed):');
console.log(JSON.stringify(signedRequest1, null, 2));
console.log();

// Example 2: POST request with body
const requestData2: FetchLatencyRequestData = {
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

const signedRequest2: SignedFetchLatencyRequest = {
  data: requestData2,
  sig: 'YOUR_SIGNATURE_HERE',
  pubkey: 'efcfe1ac4de7bcb991d8b08a7d8ebed2377a6ed1070636dc66d9cdd225458aaa',
};

console.log('📤 Example Request 2 (POST with body - Signed):');
console.log(JSON.stringify(signedRequest2, null, 2));
console.log();

// Example expected response
const expectedResponse: FetchLatencyResponse = {
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
console.log('  2. Publish a SIGNED request to the "fetch-latency-request" gossip topic');
console.log('  3. Subscribe to "api-latency" to receive results');
console.log('  4. Check node logs for:');
console.log('     - "⏱️  Received fetch-latency-request"');
console.log('     - "✅ Signature verified for request ..."');
console.log('     - "⏱️  Processing latency request..."');
console.log('     - "✅ Latency request ... completed: X ms"');
console.log('     - "📤 Published api-latency response"');
console.log();
console.log('⚠️  Security Notes:');
console.log('  - Requests MUST be signed with Ed25519');
console.log('  - Only whitelisted public keys are accepted');
console.log('  - Signature verification prevents unauthorized requests');
console.log('  - Current whitelist: efcfe1ac4de7bcb991d8b08a7d8ebed2377a6ed1070636dc66d9cdd225458aaa');

