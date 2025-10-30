import { CyberFlyClient, CryptoUtils } from './src';
import * as fs from 'fs';

const PUBKEY_FILE = '/tmp/cyberfly-test-pubkey.txt';

async function checkLocalNode() {
  // Read the public key
  const pubKeyHex = fs.readFileSync(PUBKEY_FILE, 'utf8').trim();
  console.log('Public Key:', pubKeyHex);
  
  const dbName = `example-${pubKeyHex}`;
  console.log('Database Name:', dbName);
  console.log('');

  const queryKeyPair = await CryptoUtils.generateKeyPair();
  const localClient = new CyberFlyClient({
    endpoint: 'http://localhost:8080/graphql',
    keyPair: queryKeyPair,
    defaultDbName: dbName,
  });

  console.log('Querying LOCAL node...\n');
  
  const stringVal = await localClient.queryString('test:string');
  console.log('String:', stringVal);
  
  const hashVal = await localClient.queryHash('test:hash');
  console.log('Hash:', hashVal);
  
  const jsonVal = await localClient.queryJSON('test:json');
  console.log('JSON:', jsonVal);
}

checkLocalNode();
