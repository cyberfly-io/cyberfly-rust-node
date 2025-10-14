import { CyberFlyClient, CryptoUtils } from '../src';

async function main() {
  console.log('=== CyberFly Client SDK Example ===\n');

  // 1. Generate key pair
  console.log('1. Generating Ed25519 key pair...');
  const keyPair = await CryptoUtils.generateKeyPair();
  console.log('Public Key:', CryptoUtils.bytesToHex(keyPair.publicKey));
  console.log('');

  // 2. Create client
  console.log('2. Creating CyberFly client...');
  const client = new CyberFlyClient({
    endpoint: 'http://localhost:8080/',
    keyPair,
    defaultDbName: 'example',
  });
  console.log('Client created!');
  console.log('');

  // 3. Submit string data
  console.log('3. Storing string data...');
  await client.storeString('user:alice', 'Alice Smith');
  console.log('✓ Stored: user:alice = "Alice Smith"');
  console.log('');

  // 4. Query string data
  console.log('4. Querying string data...');
  const userName = await client.queryString('user:alice');
  console.log('✓ Retrieved:', userName);
  console.log('');

  // 5. Submit hash data
  console.log('5. Storing hash data...');
  await client.storeHash('user:bob', 'name', 'Bob Johnson');
  await client.storeHash('user:bob', 'email', 'bob@example.com');
  await client.storeHash('user:bob', 'age', '35');
  console.log('✓ Stored hash with 3 fields');
  console.log('');

  // 6. Query hash data
  console.log('6. Querying hash data...');
  const userBob = await client.queryHash('user:bob');
  console.log('✓ Retrieved:', userBob);
  console.log('');

  // 7. Submit JSON data
  console.log('7. Storing JSON data...');
  await client.storeJSON('profile:charlie', {
    name: 'Charlie Brown',
    age: 28,
    interests: ['coding', 'music', 'travel'],
    address: {
      city: 'San Francisco',
      country: 'USA',
    },
  });
  console.log('✓ Stored JSON profile');
  console.log('');

  // 8. Query JSON data
  console.log('8. Querying JSON data...');
  const profile = await client.queryJSON('profile:charlie');
  console.log('✓ Retrieved:', JSON.stringify(profile, null, 2));
  console.log('');

  // 9. Query JSON with JSONPath
  console.log('9. Querying with JSONPath...');
  const city = await client.queryJSON('profile:charlie', '$.address.city');
  console.log('✓ City:', city);
  console.log('');

  // 10. Submit sorted set (leaderboard)
  console.log('10. Storing leaderboard scores...');
  await client.storeSortedSet('game:scores', 'alice', 1500);
  await client.storeSortedSet('game:scores', 'bob', 1200);
  await client.storeSortedSet('game:scores', 'charlie', 1800);
  await client.storeSortedSet('game:scores', 'diana', 1350);
  console.log('✓ Stored 4 scores');
  console.log('');

  // 11. Query sorted set with range
  console.log('11. Querying top scores (score > 1300)...');
  const topScores = await client.querySortedSet('game:scores', {
    minScore: 1300,
  });
  console.log('✓ Top scores:', topScores);
  console.log('');

  // 12. Submit time series data
  console.log('12. Storing time series data...');
  const now = Date.now();
  await client.storeTimeSeries('sensor:temp', 22.5, new Date(now - 3600000).toISOString());
  await client.storeTimeSeries('sensor:temp', 23.0, new Date(now - 1800000).toISOString());
  await client.storeTimeSeries('sensor:temp', 22.8, new Date(now).toISOString());
  console.log('✓ Stored 3 temperature readings');
  console.log('');

  // 13. Query time series
  console.log('13. Querying time series data...');
  const temps = await client.queryTimeSeries('sensor:temp');
  console.log('✓ Temperature readings:', temps);
  console.log('');

  // 14. Submit geospatial data
  console.log('14. Storing location data...');
  await client.storeGeo('locations', 'Eiffel Tower', 2.2945, 48.8584);
  await client.storeGeo('locations', 'Louvre Museum', 2.3376, 48.8606);
  await client.storeGeo('locations', 'Notre-Dame', 2.3522, 48.8530);
  console.log('✓ Stored 3 locations in Paris');
  console.log('');

  // 15. Query nearby locations
  console.log('15. Finding locations near Eiffel Tower (within 5km)...');
  const nearby = await client.queryGeo('locations', {
    longitude: 2.2945,
    latitude: 48.8584,
    radius: 5,
    unit: 'km',
  });
  console.log('✓ Nearby locations:', nearby);
  console.log('');

  // 16. Submit list data
  console.log('16. Storing list data...');
  await client.storeList('todos', 'Buy groceries');
  await client.storeList('todos', 'Walk the dog');
  await client.storeList('todos', 'Finish project');
  console.log('✓ Stored 3 todo items');
  console.log('');

  // 17. Query list
  console.log('17. Querying list data...');
  const todos = await client.queryList('todos');
  console.log('✓ Todo list:', todos);
  console.log('');

  // 18. Manual signature verification
  console.log('18. Testing manual signature...');
  const message = 'Hello, CyberFly!';
  const signature = await CryptoUtils.sign(message, keyPair.privateKey);
  const isValid = await CryptoUtils.verify(message, signature, keyPair.publicKey);
  console.log('✓ Message:', message);
  console.log('✓ Signature:', signature.substring(0, 32) + '...');
  console.log('✓ Valid:', isValid);
  console.log('');

  console.log('=== All examples completed successfully! ===');
}

// Run the example
main().catch(console.error);
