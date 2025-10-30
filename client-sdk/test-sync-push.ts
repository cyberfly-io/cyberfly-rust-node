import { CyberFlyClient, CryptoUtils } from './src';

async function main() {
  console.log('=== Testing Sync Push ===\n');

  const keyPair = await CryptoUtils.generateKeyPair();
  const publicKeyHex = CryptoUtils.bytesToHex(keyPair.publicKey);
  
  const client = new CyberFlyClient({
    endpoint: 'http://localhost:8080/graphql',
    keyPair,
    defaultDbName: 'synctest',
  });

  console.log('Storing test data...');
  await client.storeString('test:data', 'Hello from local node');
  console.log('✓ Data stored locally\n');

  console.log('Waiting 5 seconds for sync...');
  await new Promise(resolve => setTimeout(resolve, 5000));

  console.log('\nQuerying bootstrap node for the data...');
  const bootstrapClient = new CyberFlyClient({
    endpoint: 'http://208.73.202.62:8080/graphql',
    keyPair,
    defaultDbName: 'synctest',
  });

  try {
    const value = await bootstrapClient.queryString('test:data');
    if (value) {
      console.log('✓ SUCCESS: Data synced to bootstrap!', value);
    } else {
      console.log('✗ FAIL: Data not found on bootstrap');
    }
  } catch (e: any) {
    console.log('✗ FAIL: Error querying bootstrap:', e?.message || e);
  }
}

main().catch(console.error);
