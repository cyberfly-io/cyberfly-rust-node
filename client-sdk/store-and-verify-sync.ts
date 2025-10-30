import { CyberFlyClient, CryptoUtils } from './src';
import * as fs from 'fs';

const PUBKEY_FILE = '/tmp/cyberfly-test-pubkey.txt';

async function storeDataOnLocal() {
  console.log('===========================================');
  console.log('  STEP 1: STORE DATA ON LOCAL NODE');
  console.log('===========================================\n');

  // Generate key pair
  const keyPair = await CryptoUtils.generateKeyPair();
  const pubKeyHex = CryptoUtils.bytesToHex(keyPair.publicKey);
  
  console.log('Public Key:', pubKeyHex);
  
  // Save public key to file for verification step
  fs.writeFileSync(PUBKEY_FILE, pubKeyHex);
  console.log(`Saved public key to ${PUBKEY_FILE}\n`);

  // Create client for LOCAL node
  const localClient = new CyberFlyClient({
    endpoint: 'http://localhost:8080/graphql',
    keyPair,
    defaultDbName: 'example',
  });

  // Store some test data
  console.log('Storing test data on LOCAL node...');
  
  await localClient.storeString('test:string', 'Hello from local!');
  console.log('✓ Stored string: test:string');
  
  await localClient.storeHash('test:hash', 'field1', 'value1');
  await localClient.storeHash('test:hash', 'field2', 'value2');
  console.log('✓ Stored hash: test:hash');
  
  await localClient.storeJSON('test:json', { message: 'Sync test', timestamp: Date.now() });
  console.log('✓ Stored JSON: test:json');
  
  console.log('\n✅ Data stored on LOCAL node\n');
}

async function verifyDataOnBootstrap() {
  console.log('===========================================');
  console.log('  STEP 2: VERIFY DATA ON BOOTSTRAP NODE');
  console.log('===========================================\n');
  
  console.log('⏱️  Waiting 5 seconds for sync...\n');
  await new Promise(resolve => setTimeout(resolve, 5000));

  // Read the public key from file
  if (!fs.existsSync(PUBKEY_FILE)) {
    console.error('❌ Public key file not found! Run step 1 first.');
    process.exit(1);
  }
  
  const pubKeyHex = fs.readFileSync(PUBKEY_FILE, 'utf8').trim();
  console.log('Using Public Key:', pubKeyHex);
  
  // Create database name with the same public key
  const dbName = `example-${pubKeyHex}`;
  console.log('Database Name:', dbName);
  console.log('');

  // Create client for BOOTSTRAP node (can use any keypair for querying)
  const queryKeyPair = await CryptoUtils.generateKeyPair();
  const bootstrapClient = new CyberFlyClient({
    endpoint: 'http://208.73.202.62:8080/graphql',
    keyPair: queryKeyPair,
    defaultDbName: dbName,
  });

  // Verify data on bootstrap
  console.log('Querying BOOTSTRAP node for synced data...\n');
  
  try {
    const stringVal = await bootstrapClient.queryString('test:string');
    console.log('1. String value:', stringVal);
    if (stringVal === 'Hello from local!') {
      console.log('   ✅ String data synced correctly!\n');
    } else {
      console.log('   ❌ String data NOT synced (got:', stringVal, ')\n');
    }
    
    const hashVal = await bootstrapClient.queryHash('test:hash');
    console.log('2. Hash value:', hashVal);
    if (hashVal.field1 === 'value1' && hashVal.field2 === 'value2') {
      console.log('   ✅ Hash data synced correctly!\n');
    } else {
      console.log('   ❌ Hash data NOT synced\n');
    }
    
    const jsonVal = await bootstrapClient.queryJSON('test:json');
    console.log('3. JSON value:', jsonVal);
    if (jsonVal && jsonVal.message === 'Sync test') {
      console.log('   ✅ JSON data synced correctly!\n');
    } else {
      console.log('   ❌ JSON data NOT synced\n');
    }
    
    console.log('===========================================');
    console.log('✅ SYNC VERIFICATION COMPLETE!');
    console.log('===========================================');
    
  } catch (error) {
    console.error('\n❌ Error querying bootstrap node:');
    console.error(error);
    process.exit(1);
  }
}

// Run both steps
async function main() {
  await storeDataOnLocal();
  await verifyDataOnBootstrap();
}

main();
