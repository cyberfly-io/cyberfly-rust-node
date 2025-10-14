import { CyberFlyClient, CryptoUtils } from '../src';

/**
 * Example: Managing key pairs securely
 */
async function keyManagementExample() {
  console.log('=== Key Management Example ===\n');

  // Generate new key pair
  console.log('1. Generating new key pair...');
  const keyPair = await CryptoUtils.generateKeyPair();
  
  // Convert to hex for storage
  const publicKeyHex = CryptoUtils.bytesToHex(keyPair.publicKey);
  const privateKeyHex = CryptoUtils.bytesToHex(keyPair.privateKey);
  
  console.log('Public Key (hex):', publicKeyHex);
  console.log('Private Key (hex):', privateKeyHex.substring(0, 32) + '...');
  console.log('');

  // In production, save these securely:
  // - Environment variables
  // - Secure key management system (KMS)
  // - Hardware security module (HSM)
  // - Encrypted local storage
  
  console.log('2. Simulating key storage and retrieval...');
  // Simulate saving to storage
  const savedKeys = {
    publicKey: publicKeyHex,
    privateKey: privateKeyHex,
  };
  
  // Simulate loading from storage
  const loadedPublicKey = CryptoUtils.hexToBytes(savedKeys.publicKey);
  const loadedPrivateKey = CryptoUtils.hexToBytes(savedKeys.privateKey);
  
  const restoredKeyPair = {
    publicKey: loadedPublicKey,
    privateKey: loadedPrivateKey,
  };
  
  console.log('✓ Keys saved and restored successfully');
  console.log('');

  // Verify the restored keys work
  console.log('3. Verifying restored keys...');
  const testMessage = 'Test message';
  const signature = await CryptoUtils.sign(testMessage, restoredKeyPair.privateKey);
  const isValid = await CryptoUtils.verify(testMessage, signature, restoredKeyPair.publicKey);
  
  console.log('✓ Signature verification:', isValid ? 'PASSED' : 'FAILED');
  console.log('');
}

/**
 * Example: Database naming conventions
 */
async function databaseNamingExample() {
  console.log('=== Database Naming Example ===\n');

  const keyPair = await CryptoUtils.generateKeyPair();
  
  // Create database name with public key
  const dbName = 'myapp';
  const fullDbName = CryptoUtils.createDbName(dbName, keyPair.publicKey);
  
  console.log('Short name:', dbName);
  console.log('Full name:', fullDbName);
  console.log('');

  // Extract public key from full name
  const extractedKey = CryptoUtils.extractPublicKeyFromDbName(fullDbName);
  
  if (extractedKey) {
    const matches = CryptoUtils.bytesToHex(extractedKey) === CryptoUtils.bytesToHex(keyPair.publicKey);
    console.log('✓ Public key extraction:', matches ? 'SUCCESS' : 'FAILED');
  }
  console.log('');
}

/**
 * Example: Multiple database support
 */
async function multipleDatabasesExample() {
  console.log('=== Multiple Databases Example ===\n');

  const keyPair = await CryptoUtils.generateKeyPair();
  
  const client = new CyberFlyClient({
    endpoint: 'http://localhost:8080/',
    keyPair,
  });

    // Use different databases
  console.log('1. Writing to different databases...');
  
  // Database 1: User data
  await client.storeString('name', 'Alice', 'users');
  console.log('✓ Saved to "users" database');
  
  // Database 2: Settings
  await client.storeString('theme', 'dark', 'settings');
  console.log('✓ Saved to "settings" database');
  
  // Database 3: Analytics
  await client.submitTimeSeries('pageviews', 150, undefined, 'analytics');
  console.log('✓ Saved to "analytics" database');
  console.log('');

  // Read from different databases
  console.log('2. Reading from different databases...');
  const userName = await client.queryString('name', 'users');
  const config = await client.queryJSON('config', undefined, undefined, 'settings');
  const views = await client.queryTimeSeries('pageviews', undefined, 'analytics');
  
  console.log('User name:', userName);
  console.log('Config:', config);
  console.log('Pageviews:', views);
  console.log('');
}

/**
 * Example: Signature verification workflow
 */
async function signatureVerificationExample() {
  console.log('=== Signature Verification Example ===\n');

  const keyPair = await CryptoUtils.generateKeyPair();
  
  console.log('1. Creating signed message...');
  
  // Data to sign
  const dbName = CryptoUtils.createDbName('mydb', keyPair.publicKey);
  const key = 'user:123';
  const value = 'Alice';
  
  // Create message (same format as server expects)
  const message = `${dbName}:${key}:${value}`;
  console.log('Message:', message);
  console.log('');
  
  // Sign the message
  console.log('2. Signing message with private key...');
  const signature = await CryptoUtils.sign(message, keyPair.privateKey);
  console.log('Signature:', signature.substring(0, 32) + '...');
  console.log('');
  
  // Verify the signature
  console.log('3. Verifying signature with public key...');
  const isValid = await CryptoUtils.verify(message, signature, keyPair.publicKey);
  console.log('✓ Verification result:', isValid ? 'VALID' : 'INVALID');
  console.log('');
  
  // Try to verify with wrong message
  console.log('4. Testing with tampered message...');
  const tamperedMessage = `${dbName}:${key}:Bob`; // Changed value
  const isTamperedValid = await CryptoUtils.verify(tamperedMessage, signature, keyPair.publicKey);
  console.log('✓ Tampered verification:', isTamperedValid ? 'VALID (ERROR!)' : 'INVALID (CORRECT)');
  console.log('');
}

/**
 * Example: Error handling
 */
async function errorHandlingExample() {
  console.log('=== Error Handling Example ===\n');

  // Try to create client without key pair
  console.log('1. Testing missing key pair...');
  const client = new CyberFlyClient({
    endpoint: 'http://localhost:8080/',
  });
  
  try {
    await client.storeString('key', 'value');
    console.log('❌ Should have thrown error');
  } catch (error) {
    console.log('✓ Caught expected error:', (error as Error).message);
  }
  console.log('');

  // Try to use without database name
  console.log('2. Testing missing database name...');
  const keyPair = await CryptoUtils.generateKeyPair();
  client.setKeyPair(keyPair);
  
  try {
    await client.storeString('key', 'value');
    console.log('❌ Should have thrown error');
  } catch (error) {
    console.log('✓ Caught expected error:', (error as Error).message);
  }
  console.log('');

  // Set database name and retry
  console.log('3. Retrying with database name...');
  client.setDefaultDbName('mydb');
  
  try {
    await client.submitString('key', 'value');
    console.log('✓ Success!');
  } catch (error) {
    console.log('Network error (expected if server not running):', (error as Error).message);
  }
  console.log('');
}

// Run all examples
async function main() {
  await keyManagementExample();
  await databaseNamingExample();
  await multipleDatabasesExample();
  await signatureVerificationExample();
  await errorHandlingExample();
  
  console.log('=== All advanced examples completed! ===');
}

main().catch(console.error);
