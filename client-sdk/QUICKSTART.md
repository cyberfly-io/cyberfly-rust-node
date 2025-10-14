# CyberFly Client SDK - Quick Start Guide

## Installation

```bash
cd client-sdk
npm install
```

## Build

```bash
npm run build
```

This will compile TypeScript to JavaScript in the `dist/` directory.

## Project Structure

```
client-sdk/
├── src/
│   ├── index.ts         # Main exports
│   ├── client.ts        # CyberFlyClient class
│   └── crypto.ts        # Ed25519 utilities
├── examples/
│   ├── basic-usage.ts   # Basic examples
│   └── advanced-usage.ts # Advanced examples
├── dist/                # Compiled JavaScript (after build)
├── package.json
├── tsconfig.json
└── README.md
```

## Quick Example

```typescript
import { CyberFlyClient, CryptoUtils } from '@cyberfly/client-sdk';

// 1. Generate keys
const keyPair = await CryptoUtils.generateKeyPair();

// 2. Create client
const client = new CyberFlyClient({
  endpoint: 'http://localhost:8080/graphql',
  keyPair,
  defaultDbName: 'mydb',
});

// 3. Store data (automatically signed)
await client.storeString('user:123', 'Alice');

// 4. Query data
const value = await client.queryString('user:123');
console.log(value); // "Alice"
```

## Running Examples

```bash
# Install ts-node for running TypeScript directly
npm install -g ts-node

# Run basic examples
npx ts-node examples/basic-usage.ts

# Run advanced examples
npx ts-node examples/advanced-usage.ts
```

## Using in Your Project

### Option 1: Local Installation (Development)

```bash
# In your project directory
npm install ../cyberfly-rust-node/client-sdk
```

```typescript
import { CyberFlyClient, CryptoUtils } from '@cyberfly/client-sdk';
```

### Option 2: NPM Link (Development)

```bash
# In the client-sdk directory
npm link

# In your project directory
npm link @cyberfly/client-sdk
```

### Option 3: Publish to NPM (Production)

```bash
# In the client-sdk directory
npm run build
npm publish
```

Then in your project:
```bash
npm install @cyberfly/client-sdk
```

## Environment Setup

### Prerequisites

1. **Node.js**: Version 16 or higher
2. **CyberFly Node**: Running on `localhost:8080`
3. **Redis**: Running and accessible

### Starting the CyberFly Node

```bash
# In the main project directory
cargo run
```

The GraphQL endpoint will be available at:
```
http://localhost:8080/graphql
```

## Development

### Watch Mode

```bash
npm run dev
```

This will watch for file changes and recompile automatically.

### Type Checking

```bash
npx tsc --noEmit
```

## Testing

### Manual Testing

Run the example scripts to test functionality:

```bash
npx ts-node examples/basic-usage.ts
npx ts-node examples/advanced-usage.ts
```

### Integration Testing

1. Start the CyberFly node:
```bash
cargo run
```

2. Run examples to verify:
```bash
npx ts-node examples/basic-usage.ts
```

Expected output should show successful data submission and queries.

## Common Use Cases

### 1. User Authentication

```typescript
const keyPair = await CryptoUtils.generateKeyPair();
const client = new CyberFlyClient({
  endpoint: 'http://localhost:8080/graphql',
  keyPair,
  defaultDbName: 'auth',
});

// Store user profile
await client.storeJSON('user:' + userId, {
  username: 'alice',
  email: 'alice@example.com',
  createdAt: new Date().toISOString(),
});
```

### 2. IoT Data Collection

```typescript
// Store sensor readings
await client.storeTimeSeries('sensor:temp', 22.5);
await client.storeTimeSeries('sensor:humidity', 65.2);

// Query recent readings
const temps = await client.queryTimeSeries('sensor:temp', {
  startTime: new Date(Date.now() - 3600000).toISOString(),
});
```

### 3. Geospatial Tracking

```typescript
// Track locations
await client.submitGeo('locations', 'Alice', -74.0060, 40.7128);

// Find nearby
const nearby = await client.queryGeo('locations', {
  longitude: -74.0060,
  latitude: 40.7128,
  radius: 10,
  unit: 'km',
});
```

### 4. Leaderboard System

```typescript
// Submit scores
await client.submitSortedSet('leaderboard', 'alice', 1500);
await client.submitSortedSet('leaderboard', 'bob', 1200);

// Get top players
const top = await client.querySortedSet('leaderboard', {
  minScore: 1000,
});
```

## Browser Usage

To use in the browser, you'll need to bundle the SDK with a tool like webpack, rollup, or vite.

### Example with Vite

```typescript
// vite.config.ts
import { defineConfig } from 'vite';

export default defineConfig({
  optimizeDeps: {
    include: ['@cyberfly/client-sdk'],
  },
});
```

```html
<!-- index.html -->
<script type="module">
  import { CyberFlyClient, CryptoUtils } from '@cyberfly/client-sdk';
  
  const keyPair = await CryptoUtils.generateKeyPair();
  const client = new CyberFlyClient({
    endpoint: 'http://localhost:8080/graphql',
    keyPair,
    defaultDbName: 'web-app',
  });
  
  await client.storeString('greeting', 'Hello from browser!');
</script>
```

## Security Best Practices

### 1. Key Storage

**Never** store private keys in:
- Version control (Git)
- Client-side code
- Logs or console output

**Do** store keys in:
- Environment variables
- Secure key management systems (KMS)
- Hardware security modules (HSM)
- Encrypted storage

```typescript
// Good: Load from environment
const privateKeyHex = process.env.CYBERFLY_PRIVATE_KEY;
const privateKey = CryptoUtils.hexToBytes(privateKeyHex);
```

### 2. HTTPS in Production

Always use HTTPS for the GraphQL endpoint in production:

```typescript
const client = new CyberFlyClient({
  endpoint: 'https://api.example.com/graphql', // HTTPS!
  keyPair,
});
```

### 3. Key Rotation

Implement key rotation for enhanced security:

```typescript
// Generate new key pair
const newKeyPair = await CryptoUtils.generateKeyPair();

// Update client
client.setKeyPair(newKeyPair);

// Migrate data to new database with new key
```

## Troubleshooting

### Issue: "Cannot find module '@noble/ed25519'"

Solution:
```bash
npm install
```

### Issue: "Key pair not set"

Solution:
```typescript
const keyPair = await CryptoUtils.generateKeyPair();
client.setKeyPair(keyPair);
```

### Issue: "Database name not provided"

Solution:
```typescript
client.setDefaultDbName('mydb');
// or
await client.storeString('key', 'value', 'mydb');
```

### Issue: Connection refused

Solutions:
- Start the CyberFly node: `cargo run`
- Check endpoint URL
- Verify Redis is running

## Next Steps

1. ✅ Read the [README](./README.md) for full API documentation
2. ✅ Try the [examples](./examples/)
3. ✅ Build your application
4. ✅ Deploy to production

## Support

For issues or questions:
- Open an issue on GitHub
- Check the examples directory
- Read the main project documentation

## License

MIT
