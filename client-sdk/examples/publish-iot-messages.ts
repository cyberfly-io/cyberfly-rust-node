import { GraphQLClient, gql } from 'graphql-request';

async function main() {
  console.log('=== Publishing IoT Messages via GraphQL ===\n');

  const client = new GraphQLClient('http://localhost:8080/');

  // Publish to sensors/temperature
  console.log('1. Publishing to sensors/temperature...');
  await publishMessage(client, 'sensors/temperature', JSON.stringify({
    value: 22.5,
    unit: 'celsius',
    timestamp: Date.now()
  }));
  console.log('✓ Published');
  await sleep(1000);

  // Publish to sensors/humidity
  console.log('\n2. Publishing to sensors/humidity...');
  await publishMessage(client, 'sensors/humidity', JSON.stringify({
    value: 65.2,
    unit: 'percent',
    timestamp: Date.now()
  }));
  console.log('✓ Published');
  await sleep(1000);

  // Publish to devices/kitchen/temp
  console.log('\n3. Publishing to devices/kitchen/temp...');
  await publishMessage(client, 'devices/kitchen/temp', JSON.stringify({
    location: 'kitchen',
    temperature: 20.1,
    timestamp: Date.now()
  }));
  console.log('✓ Published');
  await sleep(1000);

  // Publish to devices/bedroom/humidity
  console.log('\n4. Publishing to devices/bedroom/humidity...');
  await publishMessage(client, 'devices/bedroom/humidity', JSON.stringify({
    location: 'bedroom',
    humidity: 58.3,
    timestamp: Date.now()
  }));
  console.log('✓ Published');
  await sleep(1000);

  // Publish to other/random
  console.log('\n5. Publishing to other/random...');
  await publishMessage(client, 'other/random', JSON.stringify({
    message: 'This is a random message',
    timestamp: Date.now()
  }));
  console.log('✓ Published');

  console.log('\n✅ All messages published!');
  console.log('Check the subscription terminal to see received messages.\n');
}

async function publishMessage(client: GraphQLClient, topic: string, payload: string) {
  const mutation = gql`
    mutation PublishIotMessage($topic: String!, $payload: String!, $qos: Int) {
      publishIotMessage(topic: $topic, payload: $payload, qos: $qos) {
        success
        topic
        message
      }
    }
  `;

  const result = await client.request(mutation, {
    topic,
    payload,
    qos: 1
  });

  return result;
}

function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

main().catch(console.error);
