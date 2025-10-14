import { CyberFlyClient, CryptoUtils } from '../src';

async function main() {
  console.log('=== CyberFly WebSocket Subscription Example ===\n');

  // 1. Generate key pair
  console.log('1. Generating Ed25519 key pair...');
  const keyPair = await CryptoUtils.generateKeyPair();
  console.log('Public Key:', CryptoUtils.bytesToHex(keyPair.publicKey));
  console.log('');

  // 2. Create client
  console.log('2. Creating CyberFly client...');
  const client = new CyberFlyClient({
    endpoint: 'http://localhost:8080/',
    // wsEndpoint is optional - defaults to ws://localhost:8080/ws
    wsEndpoint: 'ws://localhost:8080/ws',
    keyPair,
    defaultDbName: 'example',
  });
  console.log('Client created!');
  console.log('');

  // 3. Subscribe to specific topic
  console.log('3. Subscribing to topic "sensors/temperature"...');
  const tempUnsubscribe = client.subscribeToTopic(
    'sensors/temperature',
    (message) => {
      console.log('📥 Temperature Update:');
      console.log('  Topic:', message.topic);
      console.log('  Payload:', message.payload);
      console.log('  Timestamp:', new Date(parseInt(message.timestamp)).toISOString());
      console.log('');
    },
    (error) => {
      console.error('❌ Subscription error (sensors/temperature):', error.message);
      console.error('   Full error:', error);
    }
  );
  console.log('✓ Subscribed to sensors/temperature');
  console.log('');

  // 4. Subscribe with wildcard (single level)
  console.log('4. Subscribing to "sensors/+" (all sensor types)...');
  const sensorUnsubscribe = client.subscribeToTopic(
    'sensors/+',
    (message) => {
      console.log('📥 Sensor Update:');
      console.log('  Topic:', message.topic);
      console.log('  Payload:', message.payload);
      console.log('  Timestamp:', new Date(parseInt(message.timestamp)).toISOString());
      console.log('');
    }
  );
  console.log('✓ Subscribed to sensors/+');
  console.log('');

  // 5. Subscribe with multi-level wildcard
  console.log('5. Subscribing to "devices/#" (all device topics)...');
  const devicesUnsubscribe = client.subscribeToTopic(
    'devices/#',
    (message) => {
      console.log('📥 Device Update:');
      console.log('  Topic:', message.topic);
      console.log('  Payload:', message.payload);
      console.log('  Timestamp:', new Date(parseInt(message.timestamp)).toISOString());
      console.log('');
    }
  );
  console.log('✓ Subscribed to devices/#');
  console.log('');

  // 6. Subscribe to all messages
  console.log('6. Subscribing to all messages...');
  const allUnsubscribe = client.subscribeToMessages(
    (message) => {
      console.log('📥 Message (All):');
      console.log('  Topic:', message.topic);
      console.log('  Payload:', message.payload);
      console.log('  Timestamp:', new Date(parseInt(message.timestamp)).toISOString());
      console.log('');
    }
  );
  console.log('✓ Subscribed to all messages');
  console.log('');

  // 7. Wait for WebSocket to be ready
  console.log('7. Waiting for WebSocket connection to establish...');
  await new Promise(resolve => setTimeout(resolve, 2000));
  console.log('✓ WebSocket ready');
  console.log('');

  // 8. Wait and listen for messages
  console.log('8. Listening for messages (press Ctrl+C to exit)...');
  console.log('   You can publish messages using MQTT or the GraphQL API');
  console.log('');

  // Keep the process running indefinitely
  const keepAlive = setInterval(() => {
    // This keeps the event loop active
  }, 1000);

  // Handle graceful shutdown
  process.on('SIGINT', async () => {
    console.log('\n\n9. Shutting down...');
    
    clearInterval(keepAlive);
    
    // Unsubscribe from specific topics
    tempUnsubscribe();
    sensorUnsubscribe();
    devicesUnsubscribe();
    allUnsubscribe();
    
    // Or use unsubscribeAll() for all subscriptions at once
    // client.unsubscribeAll();
    
    // Close WebSocket connection
    await client.disconnect();
    
    console.log('✓ Disconnected');
    process.exit(0);
  });
}

// Run the example
main().catch(console.error);
