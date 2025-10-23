use anyhow::{anyhow, Result};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use std::time::{SystemTime, UNIX_EPOCH};

// Security constants
pub const ED25519_PUBLIC_KEY_LENGTH: usize = 32;
pub const ED25519_SIGNATURE_LENGTH: usize = 64;
pub const MAX_MESSAGE_LENGTH: usize = 1024 * 1024; // 1MB max message size
pub const MIN_TIMESTAMP_TOLERANCE: u64 = 300; // 5 minutes in seconds
pub const MAX_TIMESTAMP_TOLERANCE: u64 = 3600; // 1 hour in seconds

// Security error messages
pub const INVALID_PUBLIC_KEY_LENGTH: &str = "Invalid public key length - must be 32 bytes";
pub const INVALID_SIGNATURE_LENGTH: &str = "Invalid signature length - must be 64 bytes";
pub const MESSAGE_TOO_LARGE: &str = "Message exceeds maximum allowed size";
pub const TIMESTAMP_TOO_OLD: &str = "Timestamp is too old";
pub const TIMESTAMP_TOO_FUTURE: &str = "Timestamp is too far in the future";
pub const MALFORMED_HEX_ENCODING: &str = "Malformed hexadecimal encoding";

/// Verifies an Ed25519 signature for the given data with enhanced security checks
pub fn verify_signature(
    public_key_bytes: &[u8],
    message: &[u8],
    signature_bytes: &[u8],
) -> Result<()> {
    // Input validation
    if public_key_bytes.len() != ED25519_PUBLIC_KEY_LENGTH {
        return Err(anyhow!(INVALID_PUBLIC_KEY_LENGTH));
    }
    
    if signature_bytes.len() != ED25519_SIGNATURE_LENGTH {
        return Err(anyhow!(INVALID_SIGNATURE_LENGTH));
    }
    
    if message.len() > MAX_MESSAGE_LENGTH {
        return Err(anyhow!(MESSAGE_TOO_LARGE));
    }

    // Parse public key with enhanced error handling
    let public_key = VerifyingKey::from_bytes(
        public_key_bytes
            .try_into()
            .map_err(|_| anyhow!(INVALID_PUBLIC_KEY_LENGTH))?,
    )
    .map_err(|e| anyhow!("Failed to parse public key: {}", e))?;

    // Parse signature with enhanced error handling
    let signature = Signature::from_bytes(
        signature_bytes
            .try_into()
            .map_err(|_| anyhow!(INVALID_SIGNATURE_LENGTH))?,
    );

    // Verify signature using constant-time comparison
    public_key
        .verify(message, &signature)
        .map_err(|e| anyhow!("Signature verification failed: {}", e))?;

    Ok(())
}

/// Generate database name from name and public key
/// Format: <name>-<public_key_hex>
pub fn generate_db_name(name: &str, public_key_hex: &str) -> String {
    format!("{}-{}", name, public_key_hex)
}

/// Verify that the database name matches the public key
pub fn verify_db_name(db_name: &str, public_key_hex: &str) -> Result<()> {
    if !db_name.ends_with(&format!("-{}", public_key_hex)) {
        return Err(anyhow!("Database name does not match public key"));
    }
    Ok(())
}

/// Extract name part from database name (removes public key suffix)
pub fn extract_name_from_db(db_name: &str) -> Option<String> {
    db_name.rfind('-').map(|pos| db_name[..pos].to_string())
}

/// Validate timestamp against current time with configurable tolerance
pub fn validate_timestamp(timestamp: i64, tolerance_seconds: Option<u64>) -> Result<()> {
    let tolerance = tolerance_seconds.unwrap_or(MAX_TIMESTAMP_TOLERANCE);
    
    // Get current timestamp
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| anyhow!("Failed to get current time: {}", e))?
        .as_secs() as i64;
    
    let timestamp_seconds = timestamp / 1000; // Convert from milliseconds
    let time_diff = (current_time - timestamp_seconds).abs() as u64;
    
    if time_diff > tolerance {
        if timestamp_seconds < current_time {
            return Err(anyhow!(TIMESTAMP_TOO_OLD));
        } else {
            return Err(anyhow!(TIMESTAMP_TOO_FUTURE));
        }
    }
    
    Ok(())
}

/// Securely decode hex string with validation
pub fn secure_hex_decode(hex_str: &str) -> Result<Vec<u8>> {
    // Validate hex string format
    if hex_str.len() % 2 != 0 {
        return Err(anyhow!(MALFORMED_HEX_ENCODING));
    }
    
    // Check for valid hex characters
    if !hex_str.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(anyhow!(MALFORMED_HEX_ENCODING));
    }
    
    hex::decode(hex_str).map_err(|_| anyhow!(MALFORMED_HEX_ENCODING))
}

/// Enhanced database name verification with additional security checks
pub fn verify_db_name_secure(db_name: &str, public_key_hex: &str) -> Result<()> {
    // Basic format validation
    if db_name.is_empty() || public_key_hex.is_empty() {
        return Err(anyhow!("Database name and public key cannot be empty"));
    }
    
    // Validate public key hex format
    if public_key_hex.len() != ED25519_PUBLIC_KEY_LENGTH * 2 {
        return Err(anyhow!("Public key must be {} hex characters", ED25519_PUBLIC_KEY_LENGTH * 2));
    }
    
    // Validate hex encoding
    secure_hex_decode(public_key_hex)?;
    
    // Check database name format
    if !db_name.ends_with(&format!("-{}", public_key_hex)) {
        return Err(anyhow!("Database name does not match public key"));
    }
    
    // Extract and validate the name part
    let name_part = extract_name_from_db(db_name)
        .ok_or_else(|| anyhow!("Invalid database name format"))?;
    
    // Validate name part (no special characters that could cause issues)
    if name_part.is_empty() || !name_part.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return Err(anyhow!("Database name contains invalid characters"));
    }
    
    Ok(())
}

/// Constant-time string comparison to prevent timing attacks
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    
    let mut result = 0u8;
    for (byte_a, byte_b) in a.bytes().zip(b.bytes()) {
        result |= byte_a ^ byte_b;
    }
    
    result == 0
}
