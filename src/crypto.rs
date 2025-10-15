use anyhow::{anyhow, Result};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

/// Verifies an Ed25519 signature for the given data
pub fn verify_signature(
    public_key_bytes: &[u8],
    message: &[u8],
    signature_bytes: &[u8],
) -> Result<()> {
    // Parse public key
    let public_key = VerifyingKey::from_bytes(
        public_key_bytes
            .try_into()
            .map_err(|_| anyhow!("Invalid public key length"))?,
    )
    .map_err(|e| anyhow!("Failed to parse public key: {}", e))?;

    // Parse signature
    let signature = Signature::from_bytes(
        signature_bytes
            .try_into()
            .map_err(|_| anyhow!("Invalid signature length"))?,
    );

    // Verify signature
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
