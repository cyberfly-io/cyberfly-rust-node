import { CyberFlyClient, CryptoUtils } from './src';

async function testSyncLikeBasicUsage() {
  console.log('=== Testing Sync (Same as basic-usage.ts) ===\n');

  // 1. Generate key pair (same as basic-usage.ts)
  const keyPair = await CryptoUtils.generateKeyPair();
  const pubKeyHex = CryptoUtils.bytesToHex(keyPair.publicKey);
  console.log('Public Key:', pubKeyHex);
  console.log('');

  // 2. Create client for LOCAL node (same as basic-usage.ts)
  const localClient = new CyberFlyClient({
    endpoint: 'http://localhost:8080/graphql',
    keyPair,
    defaultDbName: 'example',
  });

  // 3. Store data on LOCAL
  console.log('Storing on LOCAL node...');
  await localClient.storeString('user:sync-test', 'Sync test value');
  console.log('✓ Stored on local\n');

  // 4. Verify on LOCAL immediately
  const localResult = await localClient.queryString('user:sync-test');
  console.log('Query from LOCAL:', localResult);
  console.log('');

  // 5. Wait for sync
  console.log('Waiting 5 seconds for sync...');
  await new Promise(resolve => setTimeout(resolve, 5000));

  // 6. Query from BOOTSTRAP using the SAME keypair and database name format
  console.log('Query from BOOTSTRAP node...');
  const bootstrapClient = new CyberFlyClient({
    endpoint: 'http://208.73.202.62:8080/graphql',
    keyPair,  // Same keypair!
    defaultDbName: 'example',  // Same default name!
  });

  const bootstrapResult = await bootstrapClient.queryString('user:sync-test');
  console.log('Result from BOOTSTRAP:', bootstrapResult);
  console.log('');

  if (bootstrapResult === 'Sync test value') {
    console.log('✅✅✅ SYNC WORKS! ✅✅✅');
    console.log('Data successfully synced from local to bootstrap');
  } else {
    console.log('❌ SYNC FAILED');
    console.log('Expected: "Sync test value"');
    console.log('Got:', bootstrapResult);
    console.log('');
    console.log('Database name should be: example-' + pubKeyHex);
  }
}

testSyncLikeBasicUsage();
