use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use anyhow::{anyhow, Result};

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
            .map_err(|_| anyhow!("Invalid public key length"))?
    )
    .map_err(|e| anyhow!("Failed to parse public key: {}", e))?;

    // Parse signature
    let signature = Signature::from_bytes(
        signature_bytes
            .try_into()
            .map_err(|_| anyhow!("Invalid signature length"))?
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

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{SigningKey, Signer};
    use rand::rngs::OsRng;

    #[test]
    fn test_signature_verification() {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();
        
        let message = b"Test message";
        let signature = signing_key.sign(message);

        // Valid signature should verify
        assert!(verify_signature(
            verifying_key.as_bytes(),
            message,
            &signature.to_bytes()
        ).is_ok());

        // Invalid signature should fail
        let wrong_message = b"Wrong message";
        assert!(verify_signature(
            verifying_key.as_bytes(),
            wrong_message,
            &signature.to_bytes()
        ).is_err());
    }

    #[test]
    fn test_db_name_generation() {
        let name = "myapp";
        let public_key = "a1b2c3d4e5f6";
        let db_name = generate_db_name(name, public_key);
        assert_eq!(db_name, "myapp-a1b2c3d4e5f6");
    }

    #[test]
    fn test_db_name_verification() {
        let db_name = "myapp-a1b2c3d4e5f6";
        let public_key = "a1b2c3d4e5f6";
        assert!(verify_db_name(db_name, public_key).is_ok());

        let wrong_key = "wrongkey";
        assert!(verify_db_name(db_name, wrong_key).is_err());
    }

    #[test]
    fn test_extract_name_from_db() {
        let db_name = "myapp-a1b2c3d4e5f6";
        let name = extract_name_from_db(db_name);
        assert_eq!(name, Some("myapp".to_string()));
    }
}
