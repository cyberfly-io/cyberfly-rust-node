use cyberfly_rust_node::crypto;
use ed25519_dalek::{Signer, SigningKey};
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::test]
async fn test_verify_signature_valid() {
    let mut csprng = rand::thread_rng();
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    let public_key_bytes = verifying_key.as_bytes();
    
    let message = b"test message";
    let signature = signing_key.sign(message);
    let signature_bytes = signature.to_bytes();
    
    let result = crypto::verify_signature(public_key_bytes, message, &signature_bytes);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_verify_signature_invalid() {
    let mut csprng = rand::thread_rng();
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    let public_key_bytes = verifying_key.as_bytes();
    
    let message = b"test message";
    let wrong_message = b"wrong message";
    let signature = signing_key.sign(message);
    let signature_bytes = signature.to_bytes();
    
    // Should fail with wrong message
    let result = crypto::verify_signature(public_key_bytes, wrong_message, &signature_bytes);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_verify_signature_invalid_key_length() {
    let message = b"test message";
    let signature_bytes = [0u8; 64]; // Valid signature length
    let invalid_key = [0u8; 16]; // Invalid key length (should be 32)
    
    let result = crypto::verify_signature(&invalid_key, message, &signature_bytes);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Invalid public key length"));
}

#[tokio::test]
async fn test_verify_signature_invalid_signature_length() {
    let mut csprng = rand::thread_rng();
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    let public_key_bytes = verifying_key.as_bytes();
    
    let message = b"test message";
    let invalid_signature = [0u8; 32]; // Invalid signature length (should be 64)
    
    let result = crypto::verify_signature(public_key_bytes, message, &invalid_signature);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Invalid signature length"));
}

#[tokio::test]
async fn test_verify_signature_message_too_large() {
    let mut csprng = rand::thread_rng();
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    let public_key_bytes = verifying_key.as_bytes();
    
    // Create a message larger than MAX_MESSAGE_LENGTH (1MB)
    let large_message = vec![0u8; 1024 * 1024 + 1];
    let signature_bytes = [0u8; 64];
    
    let result = crypto::verify_signature(public_key_bytes, &large_message, &signature_bytes);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Message exceeds maximum allowed size"));
}

#[tokio::test]
async fn test_validate_timestamp_current() {
    let current_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    
    let result = crypto::validate_timestamp(current_timestamp, None);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_validate_timestamp_future() {
    let future_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64 + 7200000; // 2 hours in the future (beyond 1 hour tolerance)
    
    let result = crypto::validate_timestamp(future_timestamp, None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Timestamp is too far in the future"));
}

#[tokio::test]
async fn test_validate_timestamp_too_old() {
    let old_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64 - 7200000; // 2 hours ago
    
    let result = crypto::validate_timestamp(old_timestamp, Some(3600)); // 1 hour tolerance in seconds
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Timestamp is too old"));
}

#[tokio::test]
async fn test_validate_timestamp_with_tolerance() {
    let old_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64 - 1800000; // 30 minutes ago
    
    let result = crypto::validate_timestamp(old_timestamp, Some(3600000)); // 1 hour tolerance
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_secure_hex_decode_valid() {
    let hex_string = "deadbeef";
    let result = crypto::secure_hex_decode(hex_string);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), vec![0xde, 0xad, 0xbe, 0xef]);
}

#[tokio::test]
async fn test_secure_hex_decode_invalid() {
    let invalid_hex = "invalid_hex";
    let result = crypto::secure_hex_decode(invalid_hex);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Malformed hexadecimal encoding"));
}

#[tokio::test]
async fn test_secure_hex_decode_empty() {
    let empty_hex = "";
    let result = crypto::secure_hex_decode(empty_hex);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Vec::<u8>::new());
}

#[tokio::test]
async fn test_verify_db_name_secure_valid() {
    let mut csprng = rand::thread_rng();
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    let public_key_hex = hex::encode(verifying_key.as_bytes());
    let db_name = format!("testdb-{}", public_key_hex);
    
    let result = crypto::verify_db_name_secure(&db_name, &public_key_hex);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_verify_db_name_secure_invalid() {
    let mut csprng = rand::thread_rng();
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    let public_key_hex = hex::encode(verifying_key.as_bytes());
    let wrong_db_name = "wrong-database-name";
    
    let result = crypto::verify_db_name_secure(wrong_db_name, &public_key_hex);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_constant_time_eq() {
    let str1 = "test_string";
    let str2 = "test_string";
    let str3 = "different_string";
    
    assert!(crypto::constant_time_eq(str1, str2));
    assert!(!crypto::constant_time_eq(str1, str3));
}

#[tokio::test]
async fn test_constant_time_eq_different_lengths() {
    let str1 = "short";
    let str2 = "much_longer_string";
    
    assert!(!crypto::constant_time_eq(str1, str2));
}

#[tokio::test]
async fn test_generate_db_name() {
    let mut csprng = rand::thread_rng();
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    let public_key_hex = hex::encode(verifying_key.as_bytes());
    
    let db_name = crypto::generate_db_name("testdb", &public_key_hex);
    assert!(db_name.starts_with("testdb-"));
    assert!(db_name.contains(&public_key_hex));
}

#[tokio::test]
async fn test_verify_db_name() {
    let mut csprng = rand::thread_rng();
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    let public_key_hex = hex::encode(verifying_key.as_bytes());
    
    let db_name = crypto::generate_db_name("testdb", &public_key_hex);
    let result = crypto::verify_db_name(&db_name, &public_key_hex);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_extract_name_from_db() {
    let db_name = "testdb-abcdef123456";
    let result = crypto::extract_name_from_db(db_name);
    assert_eq!(result, Some("testdb".to_string()));
    
    let simple_name = "simple";
    let result = crypto::extract_name_from_db(simple_name);
    assert_eq!(result, None); // No dash found, so returns None
}