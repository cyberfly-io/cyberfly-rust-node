import * as ed25519 from '@noble/ed25519';

/**
 * Ed25519 key pair for signing data
 */
export interface KeyPair {
  publicKey: Uint8Array;
  privateKey: Uint8Array;
}

/**
 * Crypto utilities for Ed25519 signing and verification
 */
export class CryptoUtils {
  /**
   * Generate a new Ed25519 key pair
   */
  static async generateKeyPair(): Promise<KeyPair> {
    const privateKey = ed25519.utils.randomPrivateKey();
    const publicKey = await ed25519.getPublicKeyAsync(privateKey);
    
    return {
      publicKey,
      privateKey,
    };
  }

  /**
   * Sign a message with Ed25519 private key
   * @param message - Message to sign (string or Uint8Array)
   * @param privateKey - Ed25519 private key (32 bytes)
   * @returns Signature as hex string
   */
  static async sign(message: string | Uint8Array, privateKey: Uint8Array): Promise<string> {
    const messageBytes = typeof message === 'string' 
      ? new TextEncoder().encode(message) 
      : message;
    
    const signature = await ed25519.signAsync(messageBytes, privateKey);
    return this.bytesToHex(signature);
  }

  /**
   * Verify an Ed25519 signature
   * @param message - Original message
   * @param signature - Signature as hex string
   * @param publicKey - Ed25519 public key
   * @returns true if signature is valid
   */
  static async verify(
    message: string | Uint8Array,
    signature: string,
    publicKey: Uint8Array
  ): Promise<boolean> {
    const messageBytes = typeof message === 'string'
      ? new TextEncoder().encode(message)
      : message;
    
    const signatureBytes = this.hexToBytes(signature);
    
    try {
      return await ed25519.verifyAsync(signatureBytes, messageBytes, publicKey);
    } catch {
      return false;
    }
  }

  /**
   * Convert bytes to hex string
   */
  static bytesToHex(bytes: Uint8Array): string {
    return Array.from(bytes)
      .map(b => b.toString(16).padStart(2, '0'))
      .join('');
  }

  /**
   * Convert hex string to bytes
   */
  static hexToBytes(hex: string): Uint8Array {
    const bytes = new Uint8Array(hex.length / 2);
    for (let i = 0; i < bytes.length; i++) {
      bytes[i] = parseInt(hex.substr(i * 2, 2), 16);
    }
    return bytes;
  }

  /**
   * Create database name with public key
   * Format: <name>-<public_key_hex>
   */
  static createDbName(name: string, publicKey: Uint8Array): string {
    return `${name}-${this.bytesToHex(publicKey)}`;
  }

  /**
   * Extract public key from database name
   */
  static extractPublicKeyFromDbName(dbName: string): Uint8Array | null {
    const parts = dbName.split('-');
    if (parts.length < 2) return null;
    
    const publicKeyHex = parts[parts.length - 1];
    try {
      return this.hexToBytes(publicKeyHex);
    } catch {
      return null;
    }
  }
}
