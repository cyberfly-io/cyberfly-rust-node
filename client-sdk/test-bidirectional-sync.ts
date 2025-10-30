import { CyberFlyClient, CryptoUtils } from './src';

async function main() {
  console.log('===========================================');
  console.log('  BIDIRECTIONAL SYNC TEST');
  console.log('===========================================\n');

  const keyPair = await CryptoUtils.generateKeyPair();
  const publicKeyHex = CryptoUtils.bytesToHex(keyPair.publicKey);
  
  console.log('Using keypair:', publicKeyHex.substring(0, 16) + '...\n');

  const testId = Date.now();
  const dbName = `synctest`;

  // Client for local node
  const localClient = new CyberFlyClient({
    endpoint: 'http://localhost:8080/graphql',
    keyPair,
    defaultDbName: dbName,
  });

  // Client for bootstrap node
  const bsClient = new CyberFlyClient({
    endpoint: 'http://208.73.202.62:8080/graphql',
    keyPair,
    defaultDbName: dbName,
  });

  console.log('===========================================');
  console.log('TEST 1: Local → Bootstrap');
  console.log('===========================================\n');

  const testKey1 = `local_to_bs:${testId}`;
  const testValue1 = `Sent from local at ${new Date().toISOString()}`;

  console.log(`Storing on LOCAL: ${testKey1} = "${testValue1}"`);
  await localClient.storeString(testKey1, testValue1);
  console.log('✓ Stored on local node\n');

  console.log('📋 Check local node logs for:');
  console.log('   - "GraphQL: sending outbound SyncMessage::Operation"');
  console.log('   - "📤 Broadcasting operation"');
  console.log('');

  console.log('⏱️  Waiting 5 seconds for sync...');
  await new Promise(resolve => setTimeout(resolve, 5000));

  console.log('\nQuerying BOOTSTRAP for the data...');
  try {
    const value = await bsClient.queryString(testKey1);
    if (value === testValue1) {
      console.log('✅ SUCCESS: Data synced Local → Bootstrap!');
      console.log(`   Retrieved: "${value}"\n`);
    } else if (value) {
      console.log('⚠️  PARTIAL: Found different value:', value);
    } else {
      console.log('❌ FAIL: Data NOT found on bootstrap');
      console.log('   This means bootstrap is not receiving/storing operations\n');
    }
  } catch (e: any) {
    console.log('❌ ERROR querying bootstrap:', e?.message || e);
  }

  console.log('\n===========================================');
  console.log('TEST 2: Bootstrap → Local');
  console.log('===========================================\n');

  const testKey2 = `bs_to_local:${testId}`;
  const testValue2 = `Sent from bootstrap at ${new Date().toISOString()}`;

  console.log(`Storing on BOOTSTRAP: ${testKey2} = "${testValue2}"`);
  try {
    await bsClient.storeString(testKey2, testValue2);
    console.log('✓ Stored on bootstrap node\n');
  } catch (e: any) {
    console.log('❌ Failed to store on bootstrap:', e?.message || e);
    console.log('   Bootstrap node might not be running or accessible\n');
    return;
  }

  console.log('⏱️  Waiting 5 seconds for sync...');
  await new Promise(resolve => setTimeout(resolve, 5000));

  console.log('\nQuerying LOCAL for the data...');
  try {
    const value = await localClient.queryString(testKey2);
    if (value === testValue2) {
      console.log('✅ SUCCESS: Data synced Bootstrap → Local!');
      console.log(`   Retrieved: "${value}"\n`);
    } else if (value) {
      console.log('⚠️  PARTIAL: Found different value:', value);
    } else {
      console.log('❌ FAIL: Data NOT found on local node\n');
    }
  } catch (e: any) {
    console.log('❌ ERROR querying local:', e?.message || e);
  }

  console.log('\n===========================================');
  console.log('SUMMARY');
  console.log('===========================================\n');
  console.log('If LOCAL → BOOTSTRAP fails:');
  console.log('  - Bootstrap node is not storing received operations');
  console.log('  - Check bootstrap logs for signature verification errors');
  console.log('  - Bootstrap might be running older code\n');
  
  console.log('If BOOTSTRAP → LOCAL fails:');
  console.log('  - Check if local node received sync messages');
  console.log('  - Look for "📥 Received operation" in local logs\n');
  
  console.log('Both directions should work for proper P2P sync!');
}

main().catch(console.error);
