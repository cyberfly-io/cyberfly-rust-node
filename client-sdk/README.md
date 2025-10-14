# CyberFly Client SDK

JavaScript/TypeScript SDK for interacting with the CyberFly decentralized database. Includes Ed25519 signing for secure data submission.

## Installation

```bash
npm install @cyberfly/client-sdk
```

## Quick Start

```typescript
import { CyberFlyClient, CryptoUtils } from '@cyberfly/client-sdk';

// Generate a key pair
const keyPair = await CryptoUtils.generateKeyPair();

// Create client
const client = new CyberFlyClient({
  endpoint: 'http://localhost:8080/graphql',
  keyPair,
  defaultDbName: 'mydb',
});

// Store data (automatically signed)
await client.storeString('user:123', 'Alice');

// Query data
const value = await client.queryString('user:123');
console.log(value); // "Alice"
```

## Features

- ✅ **Ed25519 Signing**: Automatic signing of all data submissions
- ✅ **Type-Safe**: Full TypeScript support with type definitions
- ✅ **All Store Types**: Support for String, Hash, List, Set, SortedSet, JSON, Stream, TimeSeries, Geo
- ✅ **GraphQL**: Uses GraphQL for efficient querying
- ✅ **Real-Time Subscriptions**: WebSocket support for live message updates
- ✅ **MQTT Integration**: Subscribe to topics with wildcard support (+, #)
- ✅ **Filtering**: Advanced filtering for queries
- ✅ **Easy to Use**: Simple, intuitive API

## Usage

### Key Management

```typescript
import { CryptoUtils } from '@cyberfly/client-sdk';

// Generate new key pair
const keyPair = await CryptoUtils.generateKeyPair();

// Save keys (example - use secure storage in production)
const publicKeyHex = CryptoUtils.bytesToHex(keyPair.publicKey);
const privateKeyHex = CryptoUtils.bytesToHex(keyPair.privateKey);

// Load keys later
const publicKey = CryptoUtils.hexToBytes(publicKeyHex);
const privateKey = CryptoUtils.hexToBytes(privateKeyHex);
const restoredKeyPair = { publicKey, privateKey };
```

### Client Initialization

```typescript
import { CyberFlyClient } from '@cyberfly/client-sdk';

const client = new CyberFlyClient({
  endpoint: 'http://localhost:8080/graphql',
  keyPair: myKeyPair,
  defaultDbName: 'mydb', // optional
});

// Or set later
client.setKeyPair(myKeyPair);
client.setDefaultDbName('mydb');
```

### String Operations

```typescript
// Store string
await client.storeString('user:123', 'Alice');

// Query string
const value = await client.queryString('user:123');
```

### Hash Operations

```typescript
// Store hash fields
await client.storeHash('user:123', 'name', 'Alice');
await client.storeHash('user:123', 'age', '30');

// Query all fields
const allFields = await client.queryHash('user:123');
// { name: 'Alice', age: '30' }

// Query specific field
const nameField = await client.queryHash('user:123', 'name');
// { name: 'Alice' }

// Query with pattern filter
const filtered = await client.queryHash('user:123', undefined, {
  pattern: 'nam*',
});
```

### List Operations

```typescript
// Submit list items
await client.submitList('tasks', 'Buy groceries');
await client.submitList('tasks', 'Walk dog');

// Query list
const tasks = await client.queryList('tasks');
// ['Buy groceries', 'Walk dog']

// Query with pattern filter
const filtered = await client.queryList('tasks', {
  pattern: '*dog*',
});
```

### Set Operations

```typescript
// Submit set members
await client.submitSet('tags', 'javascript');
await client.submitSet('tags', 'typescript');

// Query set
const tags = await client.querySet('tags');
// ['javascript', 'typescript']
```

### Sorted Set Operations

```typescript
// Submit with scores
await client.submitSortedSet('leaderboard', 'Alice', 100);
await client.submitSortedSet('leaderboard', 'Bob', 85);
await client.submitSortedSet('leaderboard', 'Charlie', 95);

// Query all
const all = await client.querySortedSet('leaderboard');
// [
//   { value: 'Bob', score: 85 },
//   { value: 'Charlie', score: 95 },
//   { value: 'Alice', score: 100 }
// ]

// Query with score range
const topScores = await client.querySortedSet('leaderboard', {
  minScore: 90,
});
// [
//   { value: 'Charlie', score: 95 },
//   { value: 'Alice', score: 100 }
// ]
```

### JSON Operations

```typescript
// Store JSON object
await client.storeJSON('user:123', {
  name: 'Alice',
  age: 30,
  address: {
    city: 'New York',
    country: 'USA',
  },
});

// Query entire object
const user = await client.queryJSON('user:123');

// Query with JSONPath
const city = await client.queryJSON('user:123', '$.address.city');
// "New York"

// Store with JSONPath (update nested field)
await client.storeJSON('user:123', 'Los Angeles', '$.address.city');
```

### Stream Operations

```typescript
// Store stream entry
await client.storeStream('events', {
  type: 'login',
  user: 'alice',
  timestamp: Date.now(),
});

await client.submitStream('events', {
  type: 'purchase',
  user: 'alice',
  amount: 99.99,
});

// Query stream
const events = await client.queryStream('events');

// Query with pattern filter
const loginEvents = await client.queryStream('events', {
  pattern: '*login*',
});
```

### Time Series Operations

```typescript
// Submit time series data
await client.submitTimeSeries('temperature', 22.5, '2024-01-01T12:00:00Z');
await client.submitTimeSeries('temperature', 23.0, '2024-01-01T13:00:00Z');
await client.submitTimeSeries('temperature', 22.8, '2024-01-01T14:00:00Z');

// Query all
const temps = await client.queryTimeSeries('temperature');

// Query with time range
const afternoon = await client.queryTimeSeries('temperature', {
  startTime: '2024-01-01T13:00:00Z',
  endTime: '2024-01-01T15:00:00Z',
});
```

### Geospatial Operations

```typescript
// Submit locations
await client.submitGeo('locations', 'Eiffel Tower', 2.2945, 48.8584);
await client.submitGeo('locations', 'Statue of Liberty', -74.0445, 40.6892);

// Query all locations
const all = await client.queryGeo('locations');

// Query with radius (find locations near a point)
const nearby = await client.queryGeo('locations', {
  longitude: 2.3522,
  latitude: 48.8566, // Paris coordinates
  radius: 10,
  unit: 'km',
});
```

### Advanced Filtering

All query methods support filtering:

```typescript
// Pattern matching
const filtered = await client.queryList('items', {
  pattern: 'user:*',
});

// Score range (for sorted sets)
const range = await client.querySortedSet('scores', {
  minScore: 50,
  maxScore: 100,
});

// Time range (for streams and time series)
const timeRange = await client.queryTimeSeries('metrics', {
  startTime: '2024-01-01T00:00:00Z',
  endTime: '2024-01-02T00:00:00Z',
});

// Geospatial radius
const geoFiltered = await client.queryGeo('places', {
  longitude: -74.0060,
  latitude: 40.7128, // New York
  radius: 50,
  unit: 'km',
});
```

### Raw Data Submission

For advanced use cases, you can submit raw signed data:

```typescript
import { DataInput } from '@cyberfly/client-sdk';

const data: DataInput = {
  dbName: 'mydb-abcdef123...',
  key: 'custom:key',
  value: 'custom value',
  storeType: 'String',
};

await client.submitData(data);
```

## Database Naming

The SDK automatically creates database names in the format:
```
<name>-<public_key_hex>
```

This ensures that each user's data is isolated and verifiable.

```typescript
// Get full database name
const fullName = client.getFullDbName('mydb');
// "mydb-a1b2c3d4e5f6..."

// Extract public key from database name
const publicKey = CryptoUtils.extractPublicKeyFromDbName(fullName);
```

## Signature Verification

All data submissions are automatically signed:

```typescript
// The SDK creates a signature for the message: db_name:key:value
// Message: "mydb-abc123:user:123:Alice"
// Signature is created using Ed25519

// On the server side, the signature is verified before storing data
```

Manual signing/verification (if needed):

```typescript
import { CryptoUtils } from '@cyberfly/client-sdk';

const message = 'Hello, World!';
const signature = await CryptoUtils.sign(message, privateKey);

const isValid = await CryptoUtils.verify(message, signature, publicKey);
console.log(isValid); // true
```

## Error Handling

```typescript
try {
  await client.storeString('key', 'value');
} catch (error) {
  console.error('Failed to store data:', error);
}

try {
  const value = await client.queryString('key');
} catch (error) {
  console.error('Failed to query data:', error);
}
```

## TypeScript Support

The SDK is written in TypeScript and includes full type definitions:

```typescript
import { 
  CyberFlyClient, 
  CyberFlyConfig,
  KeyPair,
  DataInput,
  SignedData,
  StoreType,
  FilterOptions 
} from '@cyberfly/client-sdk';

// Full autocomplete and type checking
const config: CyberFlyConfig = {
  endpoint: 'http://localhost:8080/graphql',
  keyPair: myKeyPair,
};

const client = new CyberFlyClient(config);
```

## Examples

### User Profile Management

```typescript
// Create user profile
await client.storeJSON('profile:alice', {
  name: 'Alice',
  email: 'alice@example.com',
  age: 30,
  interests: ['coding', 'music'],
});

// Update specific field
await client.storeJSON('profile:alice', ['coding', 'music', 'travel'], '$.interests');

// Query profile
const profile = await client.queryJSON('profile:alice');
```

### Leaderboard System

```typescript
// Add scores
await client.submitSortedSet('game:leaderboard', 'alice', 1500);
await client.submitSortedSet('game:leaderboard', 'bob', 1200);
await client.submitSortedSet('game:leaderboard', 'charlie', 1800);

// Get top 10
const top10 = await client.querySortedSet('game:leaderboard', {
  minScore: 0,
  maxScore: 10000,
});
```

### Location Tracking

```typescript
// Track user locations
await client.submitGeo('user:locations', 'alice', -74.0060, 40.7128);
await client.submitGeo('user:locations', 'bob', 2.3522, 48.8566);

// Find nearby users
const nearbyUsers = await client.queryGeo('user:locations', {
  longitude: -74.0060,
  latitude: 40.7128,
  radius: 100,
  unit: 'km',
});
```

### IoT Sensor Data

```typescript
// Store sensor readings
await client.submitTimeSeries('sensor:temperature', 22.5);
await client.submitTimeSeries('sensor:humidity', 65.2);

// Query recent readings
const recent = await client.queryTimeSeries('sensor:temperature', {
  startTime: new Date(Date.now() - 3600000).toISOString(), // Last hour
});
```

## Real-Time Subscriptions

The SDK supports WebSocket subscriptions for receiving real-time message updates. You can subscribe to specific topics or all messages.

### Subscribe to Specific Topic

```typescript
// Subscribe to a specific topic
const unsubscribe = client.subscribeToTopic(
  'sensors/temperature',
  (message) => {
    console.log('Topic:', message.topic);
    console.log('Payload:', message.payload);
    console.log('Timestamp:', message.timestamp);
  },
  (error) => {
    console.error('Subscription error:', error);
  }
);

// Later: unsubscribe
unsubscribe();
```

### Subscribe with MQTT Wildcards

```typescript
// Single-level wildcard (+)
const unsubscribe1 = client.subscribeToTopic('sensors/+', (message) => {
  // Matches: sensors/temperature, sensors/humidity, etc.
  console.log('Sensor data:', message);
});

// Multi-level wildcard (#)
const unsubscribe2 = client.subscribeToTopic('devices/#', (message) => {
  // Matches: devices/kitchen/temp, devices/bedroom/humidity, etc.
  console.log('Device data:', message);
});
```

### Subscribe to All Messages

```typescript
// Subscribe to all messages (no topic filter)
const unsubscribe = client.subscribeToMessages((message) => {
  console.log('Received:', message);
});
```

### Cleanup

```typescript
// Unsubscribe from all active subscriptions
client.unsubscribeAll();

// Disconnect and cleanup WebSocket connection
await client.disconnect();
```

### WebSocket Configuration

```typescript
const client = new CyberFlyClient({
  endpoint: 'http://localhost:8080/',
  wsEndpoint: 'ws://localhost:8080/ws', // Optional, auto-detected from endpoint
  keyPair,
  defaultDbName: 'mydb',
});
```

### Subscription Message Format

```typescript
interface MessageUpdate {
  topic: string;      // MQTT topic
  payload: string;    // Message payload (UTF-8 string)
  timestamp: string;  // Unix timestamp in milliseconds
}
```

### Complete Subscription Example

```typescript
import { CyberFlyClient, CryptoUtils } from '@cyberfly/client-sdk';

const keyPair = await CryptoUtils.generateKeyPair();

const client = new CyberFlyClient({
  endpoint: 'http://localhost:8080/',
  keyPair,
});

// Subscribe to temperature sensors
const tempSub = client.subscribeToTopic(
  'sensors/temperature',
  (message) => {
    const data = JSON.parse(message.payload);
    console.log(`Temperature: ${data.value}°C at ${message.timestamp}`);
  },
  (error) => {
    console.error('Error:', error.message);
  }
);

// Subscribe to all sensor data
const allSensorsSub = client.subscribeToTopic('sensors/#', (message) => {
  console.log('Sensor update:', message.topic, message.payload);
});

// Cleanup on exit
process.on('SIGINT', async () => {
  tempSub();
  allSensorsSub();
  await client.disconnect();
  process.exit(0);
});
```

## API Reference

### CyberFlyClient

#### Constructor
- `new CyberFlyClient(config: CyberFlyConfig)`

#### Configuration Methods
- `setKeyPair(keyPair: KeyPair): void`
- `setDefaultDbName(dbName: string): void`
- `getFullDbName(dbName?: string): string`

#### Store Methods
- `storeString(key: string, value: string, dbName?: string): Promise<boolean>`
- `storeHash(key: string, field: string, value: string, dbName?: string): Promise<boolean>`
- `storeList(key: string, value: string, dbName?: string): Promise<boolean>`
- `storeSet(key: string, value: string, dbName?: string): Promise<boolean>`
- `storeSortedSet(key: string, value: string, score: number, dbName?: string): Promise<boolean>`
- `storeJSON(key: string, value: object, jsonPath?: string, dbName?: string): Promise<boolean>`
- `storeStream(key: string, fields: Record<string, any>, dbName?: string): Promise<boolean>`
- `storeTimeSeries(key: string, value: number, timestamp?: string, dbName?: string): Promise<boolean>`
- `storeGeo(key: string, member: string, longitude: number, latitude: number, dbName?: string): Promise<boolean>`
- `storeData(data: DataInput): Promise<boolean>`

**Deprecated (use store methods instead):**
- `submitString`, `submitHash`, `submitList`, `submitSet`, `submitSortedSet`, `submitJSON`, `submitStream`, `submitTimeSeries`, `submitGeo`, `submitData`

#### Query Methods
- `queryString(key: string, dbName?: string): Promise<string | null>`
- `queryHash(key: string, field?: string, filter?: FilterOptions, dbName?: string): Promise<Record<string, string>>`
- `queryList(key: string, filter?: FilterOptions, dbName?: string): Promise<string[]>`
- `querySet(key: string, filter?: FilterOptions, dbName?: string): Promise<string[]>`
- `querySortedSet(key: string, filter?: FilterOptions, dbName?: string): Promise<Array<{ value: string; score: number }>>`
- `queryJSON(key: string, jsonPath?: string, filter?: FilterOptions, dbName?: string): Promise<any>`
- `queryStream(key: string, filter?: FilterOptions, dbName?: string): Promise<any[]>`
- `queryTimeSeries(key: string, filter?: FilterOptions, dbName?: string): Promise<Array<{ timestamp: string; value: number }>>`
- `queryGeo(key: string, filter?: FilterOptions, dbName?: string): Promise<Array<{ member: string; longitude: number; latitude: number; distance?: number }>>`

#### Subscription Methods
- `subscribeToTopic(topic: string, callback: SubscriptionCallback, onError?: SubscriptionErrorCallback): () => void`
- `subscribeToMessages(callback: SubscriptionCallback, onError?: SubscriptionErrorCallback): () => void`
- `unsubscribeAll(): void`
- `disconnect(): Promise<void>`

### CryptoUtils

#### Static Methods
- `generateKeyPair(): Promise<KeyPair>`
- `sign(message: string | Uint8Array, privateKey: Uint8Array): Promise<string>`
- `verify(message: string | Uint8Array, signature: string, publicKey: Uint8Array): Promise<boolean>`
- `bytesToHex(bytes: Uint8Array): string`
- `hexToBytes(hex: string): Uint8Array`
- `createDbName(name: string, publicKey: Uint8Array): string`
- `extractPublicKeyFromDbName(dbName: string): Uint8Array | null`

## License

MIT

## Contributing

Contributions welcome! Please open an issue or PR.

## Support

For issues or questions, please open an issue on GitHub.
