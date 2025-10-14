# CyberFly Client SDK Examples

This directory contains example code demonstrating how to use the CyberFly Client SDK.

## Examples

### 1. Basic Usage (`basic-usage.ts`)

Demonstrates fundamental operations:
- Generating Ed25519 key pairs
- Creating a client instance
- Submitting data (String, Hash, JSON, Sorted Set, Time Series, Geo, List)
- Querying data
- Using filters
- Manual signature verification

```bash
npm install
npx ts-node examples/basic-usage.ts
```

### 2. Advanced Usage (`advanced-usage.ts`)

Demonstrates advanced features:
- Key management and persistence
- Database naming conventions
- Multiple database support
- Signature verification workflow
- Error handling

```bash
npx ts-node examples/advanced-usage.ts
```

### 3. Subscription Usage (`subscription-usage.ts`)

Demonstrates real-time WebSocket subscriptions:
- Subscribing to specific topics
- Using MQTT wildcards (+ for single level, # for multi-level)
- Subscribing to all messages
- Handling subscription errors
- Graceful cleanup and disconnection

```bash
npx ts-node examples/subscription-usage.ts
```

## Prerequisites

1. Install dependencies:
```bash
npm install
```

2. Make sure the CyberFly node is running:
```bash
# In the main project directory
cargo run
```

The node should be accessible at `http://localhost:8080/graphql`

## Running Examples

### TypeScript (with ts-node)

```bash
# Install ts-node if not already installed
npm install -g ts-node

# Run examples
npx ts-node examples/basic-usage.ts
npx ts-node examples/advanced-usage.ts
```

### JavaScript (after building)

```bash
# Build the SDK
npm run build

# Convert examples to JS
npx tsc examples/basic-usage.ts --outDir examples/dist
npx tsc examples/advanced-usage.ts --outDir examples/dist

# Run the compiled JS
node examples/dist/basic-usage.js
node examples/dist/advanced-usage.js
```

## Example Output

### Basic Usage

```
=== CyberFly Client SDK Example ===

1. Generating Ed25519 key pair...
Public Key: a1b2c3d4e5f6...

2. Creating CyberFly client...
Client created!

3. Submitting string data...
✓ Submitted: user:alice = "Alice Smith"

4. Querying string data...
✓ Retrieved: Alice Smith

5. Submitting hash data...
✓ Submitted hash with 3 fields

6. Querying hash data...
✓ Retrieved: { name: 'Bob Johnson', email: 'bob@example.com', age: '35' }

...
```

## Common Issues

### Connection Refused

If you get a connection error:
- Make sure the CyberFly node is running
- Check that the endpoint URL is correct (default: `http://localhost:8080/graphql`)
- Verify Redis is running

### Signature Verification Failed

If signature verification fails:
- Ensure you're using the same key pair for signing and verification
- Check that the message format matches the server's expectation (`db_name:key:value`)
- Verify the public key is correctly associated with the database name

### GraphQL Errors

If you get GraphQL errors:
- Check the GraphQL schema matches your queries
- Ensure all required fields are provided
- Verify data types match the schema

## Learn More

- [SDK README](../README.md) - Full SDK documentation
- [API Reference](../README.md#api-reference) - Complete API documentation
- [Main Project README](../../README.md) - CyberFly node documentation
