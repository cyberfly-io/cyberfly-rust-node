import { CyberFlyClient, CryptoUtils } from './src';

async function main() {
  console.log('=== Testing Broadcast ===\n');

  const keyPair = await CryptoUtils.generateKeyPair();
  const publicKeyHex = CryptoUtils.bytesToHex(keyPair.publicKey);
  
  console.log('Public Key:', publicKeyHex);
  console.log('');

  const client = new CyberFlyClient({
    endpoint: 'http://localhost:8080/graphql',
    keyPair,
    defaultDbName: 'broadcasttest',
  });

  console.log('Storing test data...');
  const testKey = `test:${Date.now()}`;
  const testValue = `Broadcast test at ${new Date().toISOString()}`;
  
  await client.storeString(testKey, testValue);
  console.log(`✓ Stored: ${testKey} = "${testValue}"`);
  console.log('');
  console.log('Check the node logs (target/debug/cyberfly-rust-node terminal)');
  console.log('You should see:');
  console.log('  - "GraphQL: sending outbound SyncMessage::Operation: <op_id>"');
  console.log('  - "Broadcast sync message (<N> bytes)"');
}

main().catch(console.error);
