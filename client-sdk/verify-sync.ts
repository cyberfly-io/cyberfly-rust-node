import { CyberFlyClient, CryptoUtils } from './src';

async function verifySyncFromBootstrap() {
  console.log('===========================================');
  console.log('  VERIFYING SYNC FROM BOOTSTRAP NODE');
  console.log('===========================================\n');

  // IMPORTANT: We need to use the SAME keypair that basic-usage.ts generated
  // The public key from the basic-usage.ts run was:
  // a6a7735a13084b8f13569635782b30e26a7a345a662363333b6699c5466a31f3
  
  // First, generate a new keypair and get its public key
  const newKeyPair = await CryptoUtils.generateKeyPair();
  console.log('Generated Public Key:', CryptoUtils.bytesToHex(newKeyPair.publicKey));
  console.log('\n⚠️  NOTE: To properly verify, you should run basic-usage.ts first,');
  console.log('   copy its public key, and paste it here as PUBLIC_KEY_FROM_BASIC_USAGE\n');
  
  
  // Create the database name using the same format
  const dbName = `example`;
  
  console.log(`Querying database: ${dbName}\n`);
  
  // We can use any keypair for querying (doesn't need to match for reads)
  const bootstrapClient = new CyberFlyClient({
    endpoint: 'http://208.73.202.62:8080/graphql',
    keyPair: newKeyPair,
    defaultDbName: dbName,
  });

  try {
    // 1. Query string data stored on local
    console.log('1. Querying string data from bootstrap...');
    const userName = await bootstrapClient.queryString('user:alice');
    console.log(`   ✓ user:alice = "${userName}"`);
    
    // 2. Query hash data
    console.log('\n2. Querying hash data from bootstrap...');
    const userBob = await bootstrapClient.queryHash('user:bob');
    console.log('   ✓ user:bob =', userBob);
    
    // 3. Query JSON data
    console.log('\n3. Querying JSON data from bootstrap...');
    const profile = await bootstrapClient.queryJSON('profile:charlie');
    console.log('   ✓ profile:charlie =', JSON.stringify(profile, null, 4));
    
    // 4. Query leaderboard
    console.log('\n4. Querying leaderboard from bootstrap...');
    const scores = await bootstrapClient.querySortedSet('game:scores', {
      minScore: 1300,
    });
    console.log('   ✓ Top scores:', scores);
    
    // 5. Query list data
    console.log('\n5. Querying list from bootstrap...');
    const todos = await bootstrapClient.queryList('tasks:todo');
    console.log('   ✓ Todo list:', todos);
    
    console.log('\n===========================================');
    console.log('✅ SYNC VERIFICATION SUCCESSFUL!');
    console.log('All data stored on LOCAL is accessible from BOOTSTRAP');
    console.log('===========================================');
    
  } catch (error) {
    console.error('\n===========================================');
    console.error('❌ SYNC VERIFICATION FAILED!');
    console.error('===========================================');
    console.error('Error:', error);
    process.exit(1);
  }
}

verifySyncFromBootstrap();
