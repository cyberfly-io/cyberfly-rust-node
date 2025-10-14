import { CyberFlyClient, CryptoUtils } from '../src';

async function main() {
  console.log('=== Publishing Test Messages ===\n');

  // Generate key pair
  const keyPair = await CryptoUtils.generateKeyPair();
  console.log('Public Key:', CryptoUtils.bytesToHex(keyPair.publicKey));
  console.log('');

  // Create client
  const client = new CyberFlyClient({
    endpoint: 'http://localhost:8080/',
    keyPair,
    defaultDbName: 'test',
  });

  console.log('Publishing messages to different topics...\n');

  // Publish to sensors/temperature
  console.log('1. Publishing to sensors/temperature...');
  await client.storeString('msg:temp1', JSON.stringify({
    value: 22.5,
    unit: 'celsius',
    timestamp: Date.now()
  }));
  console.log('✓ Published temperature reading');
  await sleep(1000);

  // Publish to sensors/humidity
  console.log('\n2. Publishing to sensors/humidity...');
  await client.storeString('msg:humidity1', JSON.stringify({
    value: 65.2,
    unit: 'percent',
    timestamp: Date.now()
  }));
  console.log('✓ Published humidity reading');
  await sleep(1000);

  // Publish to devices/kitchen/temp
  console.log('\n3. Publishing to devices/kitchen/temp...');
  await client.storeString('msg:kitchen1', JSON.stringify({
    location: 'kitchen',
    temperature: 20.1,
    timestamp: Date.now()
  }));
  console.log('✓ Published kitchen temperature');
  await sleep(1000);

  // Publish to devices/bedroom/humidity
  console.log('\n4. Publishing to devices/bedroom/humidity...');
  await client.storeString('msg:bedroom1', JSON.stringify({
    location: 'bedroom',
    humidity: 58.3,
    timestamp: Date.now()
  }));
  console.log('✓ Published bedroom humidity');
  await sleep(1000);

  // Publish to other/random
  console.log('\n5. Publishing to other/random...');
  await client.storeString('msg:random1', JSON.stringify({
    message: 'This is a random message',
    timestamp: Date.now()
  }));
  console.log('✓ Published random message');

  console.log('\n✅ All messages published!');
  console.log('Check the subscription terminal to see received messages.\n');
}

function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

main().catch(console.error);
