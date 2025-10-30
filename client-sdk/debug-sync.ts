import { CyberFlyClient, CryptoUtils } from './src';
import * as fs from 'fs';

const PUBKEY_FILE = '/tmp/cyberfly-debug-pubkey.txt';

async function debugSync() {
  console.log('========================================');
  console.log('  SYNC DEBUG - STEP BY STEP');
  console.log('========================================\n');

  // Generate keypair
  const keyPair = await CryptoUtils.generateKeyPair();
  const pubKeyHex = CryptoUtils.bytesToHex(keyPair.publicKey);
  
  console.log('✓ Generated keypair');
  console.log('  Public Key:', pubKeyHex);
  fs.writeFileSync(PUBKEY_FILE, pubKeyHex);
  
  const dbName = `debug-${pubKeyHex}`;
  console.log('  Database Name:', dbName);
  console.log('');

  // Create client for LOCAL node
  const localClient = new CyberFlyClient({
    endpoint: 'http://localhost:8080/graphql',
    keyPair,
    defaultDbName: 'debug',
  });

  // STEP 1: Store data on local
  console.log('STEP 1: Storing data on LOCAL node...');
  const testValue = `Test at ${Date.now()}`;
  await localClient.storeString('sync-test', testValue);
  console.log(`✓ Stored: sync-test = "${testValue}"`);
  console.log('');

  // STEP 2: Immediately query back from local
  console.log('STEP 2: Query from LOCAL node (should work immediately)...');
  const localResult = await localClient.queryString('sync-test');
  if (localResult === testValue) {
    console.log(`✅ LOCAL STORAGE WORKS: "${localResult}"`);
  } else {
    console.log(`❌ LOCAL STORAGE FAILED: got "${localResult}", expected "${testValue}"`);
    process.exit(1);
  }
  console.log('');

  // STEP 3: Wait for sync
  console.log('STEP 3: Waiting 8 seconds for sync to propagate...');
  console.log('⏱️  During this time, check your node terminal for:');
  console.log('   - "GraphQL: sending outbound SyncMessage::Operation"');
  console.log('   - "📤 Broadcasting operation"');
  console.log('   - Thelease logs will show if sync message was sent');
  console.log('');
  await new Promise(resolve => setTimeout(resolve, 8000));

  // STEP 4: Query from bootstrap
  console.log('STEP 4: Query from BOOTSTRAP node...');
  const bootstrapClient = new CyberFlyClient({
    endpoint: 'http://208.73.202.62:8080/graphql',
    keyPair,
    defaultDbName: dbName,
  });

  const bootstrapResult = await bootstrapClient.queryString('sync-test');
  console.log('');
  
  if (bootstrapResult === testValue) {
    console.log('========================================');
    console.log('✅✅✅ SYNC WORKS! ✅✅✅');
    console.log('========================================');
    console.log(`Data synced successfully: "${bootstrapResult}"`);
  } else if (bootstrapResult === null || bootstrapResult === 'null') {
    console.log('========================================');
    console.log('❌ SYNC FAILED - Data not found on bootstrap');
    console.log('========================================');
    console.log('');
    console.log('Possible causes:');
    console.log('1. Nodes not connected - check "discoveredPeers" in logs');
    console.log('2. Sync messages not being sent - check for 📤 emoji in local logs');
    console.log('3. Bootstrap not receiving - check for 📥 emoji in bootstrap logs');
    console.log('4. Signature verification failing on bootstrap');
    console.log('5. Database name mismatch');
    console.log('');
    console.log(`Expected database: ${dbName}`);
    console.log(`Expected value: "${testValue}"`);
    console.log(`Got: "${bootstrapResult}"`);
  } else {
    console.log('========================================');
    console.log('⚠️  UNEXPECTED RESULT');
    console.log('========================================');
    console.log(`Expected: "${testValue}"`);
    console.log(`Got: "${bootstrapResult}"`);
  }
}

debugSync().catch(console.error);
