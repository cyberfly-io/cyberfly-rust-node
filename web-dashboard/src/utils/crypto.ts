import nacl from 'tweetnacl';
import { Buffer } from 'buffer';

export interface KeyPair {
  publicKey: string;
  secretKey: string;
}

const KEYPAIR_STORAGE_KEY = 'cyberfly_keypair';

/**
 * Generate a new Ed25519 keypair
 */
export function generateKeyPair(): KeyPair {
  const keyPair = nacl.sign.keyPair();
  
  return {
    publicKey: Buffer.from(keyPair.publicKey).toString('hex'),
    secretKey: Buffer.from(keyPair.secretKey).toString('hex'),
  };
}

/**
 * Save keypair to localStorage
 */
export function saveKeyPair(keyPair: KeyPair): void {
  localStorage.setItem(KEYPAIR_STORAGE_KEY, JSON.stringify(keyPair));
}

/**
 * Load keypair from localStorage
 */
export function loadKeyPair(): KeyPair | null {
  const stored = localStorage.getItem(KEYPAIR_STORAGE_KEY);
  if (!stored) return null;
  
  try {
    return JSON.parse(stored);
  } catch {
    return null;
  }
}

/**
 * Delete keypair from localStorage
 */
export function deleteKeyPair(): void {
  localStorage.removeItem(KEYPAIR_STORAGE_KEY);
}

/**
 * Sign data with Ed25519 secret key
 */
export function signData(data: any, secretKeyHex: string): string {
  const message = typeof data === 'string' ? data : JSON.stringify(data);
  const messageBytes = Buffer.from(message, 'utf-8');
  const secretKey = Buffer.from(secretKeyHex, 'hex');
  
  const signature = nacl.sign.detached(messageBytes, secretKey);
  return Buffer.from(signature).toString('hex');
}

/**
 * Verify signature
 */
export function verifySignature(
  data: any,
  signatureHex: string,
  publicKeyHex: string
): boolean {
  try {
    const message = typeof data === 'string' ? data : JSON.stringify(data);
    const messageBytes = Buffer.from(message, 'utf-8');
    const signature = Buffer.from(signatureHex, 'hex');
    const publicKey = Buffer.from(publicKeyHex, 'hex');
    
    return nacl.sign.detached.verify(messageBytes, signature, publicKey);
  } catch {
    return false;
  }
}

/**
 * Check if keypair exists in localStorage
 */
export function hasKeyPair(): boolean {
  return localStorage.getItem(KEYPAIR_STORAGE_KEY) !== null;
}
