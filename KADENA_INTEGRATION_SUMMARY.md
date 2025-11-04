# Kadena Integration Summary

## Overview
This document summarizes the complete Kadena blockchain integration for the Rust node, including smart contract interactions, cryptographic key unification, and backward compatibility with the JavaScript implementation.

## Key Features Implemented

### 1. Smart Contract Integration
- **Contract**: `free.cyberfly_node` on Kadena mainnet01/testnet04
- **Operations**:
  - Node registration (`new-node`)
  - Node status query (`get-node`)
  - Node activation (`update-node`)
  - Rewards calculation (`calculate-days-and-reward`)
  - Rewards claiming (`claim-reward`)
- **Gas Station**: Gasless transactions via `free.cyberfly-account-gas-station.GAS_PAYER`

### 2. Cryptographic Key Unification
All cryptographic operations now use a **single Kadena Ed25519 private key**:

#### Iroh Node Identity
- The Kadena private key is used to initialize the Iroh node identity
- Ensures consistent peer identification across the network
- Location: `src/main.rs` lines 100-114

```rust
let secret_key = if let Some(ref kadena_config) = config.kadena_config {
    tracing::info!("Using Kadena private key to generate Iroh node identity");
    let kadena_secret_bytes = hex::decode(&kadena_config.secret_key)?;
    iroh::SecretKey::try_from(&kadena_secret_bytes[..])?
} else {
    // Fallback to file-based random key generation
}
```

#### libp2p Peer ID Generation
- libp2p peer ID is derived from the same Kadena private key
- Provides backward compatibility with JavaScript nodes
- Location: `src/kadena.rs` function `generate_peer_id_from_kadena_key()`

```rust
pub fn generate_peer_id_from_kadena_key(secret_key_hex: &str) -> Result<String> {
    let secret_bytes = hex::decode(secret_key_hex)?;
    let secret = libp2p_identity::ed25519::SecretKey::try_from_bytes(secret_bytes)?;
    let keypair = libp2p_identity::ed25519::Keypair::from(secret);
    let peer_id = libp2p_identity::PeerId::from_public_key(&keypair.public().into());
    Ok(peer_id.to_string())
}
```

#### Public Key Derivation
- Public key is now **automatically derived** from the private key
- No need to set `KADENA_PUBLIC_KEY` environment variable
- Location: `src/config.rs` method `KadenaConfig::public_key()`

```rust
impl KadenaConfig {
    pub fn public_key(&self) -> Result<String> {
        use ed25519_dalek::SigningKey;
        let secret_bytes = hex::decode(&self.secret_key)?;
        let signing_key = SigningKey::from_bytes(&secret_bytes.try_into().unwrap());
        let verifying_key = signing_key.verifying_key();
        Ok(hex::encode(verifying_key.as_bytes()))
    }
}
```

### 3. Public IP Detection
- Fetches external IP from `http://ip-api.com/json/` (matching JS implementation)
- Used for constructing the node multiaddr
- Location: `src/kadena.rs` function `get_public_ip()`

### 4. Multiaddr Format
- Format: `{public_key}@{public_ip}:{quic_port}`
- Example: `94faf73abc...@203.0.113.1:11204`
- **Not** using `/p2p/{peer_id}` format (as per user requirements)

### 5. Automatic Node Registration
- On startup, the node automatically:
  1. Derives peer ID from Kadena key
  2. Fetches public IP
  3. Constructs multiaddr
  4. Checks registration status on blockchain
  5. Registers or activates as needed
- Location: `src/main.rs` lines 280-340

### 6. Periodic Status Check and Auto-Claim
- Every 10 minutes (600 seconds), the node:
  1. Checks registration status
  2. Queries claimable rewards
  3. Automatically claims if rewards available
- Location: `src/main.rs` lines 341-370

## Configuration

### Required Environment Variables
```bash
KADENA_ACCOUNT=k:your-account-name
KADENA_SECRET_KEY=your-hex-encoded-ed25519-private-key  # 64 hex characters
```

### Optional Environment Variables
```bash
KADENA_NETWORK=mainnet01        # Default: mainnet01, options: mainnet01, testnet04
KADENA_CHAIN_ID=1              # Default: 1
KADENA_API_HOST=<custom-url>   # Auto-generated if not provided
```

### Removed Environment Variables
- ~~`KADENA_PUBLIC_KEY`~~ - Now automatically derived from private key

## File Changes

### New Files
- `src/kadena.rs` - Complete Kadena integration module (~487 lines)

### Modified Files
1. **Cargo.toml**
   - Added `rust_pact = "0.1.3"`
   - Added `libp2p-identity = { version = "0.2", features = ["ed25519", "peerid"] }`
   - Added `reqwest = { version = "0.12", features = ["json"] }`

2. **src/config.rs**
   - Removed `public_key` field from `KadenaConfig` struct
   - Added `public_key()` method to derive public key from private key
   - Updated `Config::load()` to not require `KADENA_PUBLIC_KEY` env var

3. **src/main.rs**
   - Modified Iroh secret key initialization to use Kadena key (lines 100-114)
   - Added peer ID generation from Kadena key (line 289)
   - Added public IP detection (lines 301-317)
   - Added multiaddr construction with derived public key (lines 320-328)
   - Added node registration task (lines 333-343)
   - Added periodic status check and reward claim task (lines 345-367)

4. **src/lib.rs**
   - Added `pub mod kadena;`

## Dependencies

### Kadena & Crypto
- `rust_pact` - Pact smart contract interaction
- `ed25519-dalek` - Ed25519 signature operations
- `libp2p-identity` - Peer ID generation

### Networking
- `reqwest` - HTTP client for IP detection API
- `iroh` - P2P networking

### Utilities
- `hex` - Hex encoding/decoding
- `serde_json` - JSON serialization
- `chrono` - Timestamp handling
- `tokio` - Async runtime

## Security Considerations

### Single Private Key Usage
- **Benefit**: Simplified key management, consistent identity
- **Risk**: If the key is compromised, both Kadena account and P2P network identity are at risk
- **Recommendation**: 
  - Store the private key securely (environment variable, secrets manager)
  - Never commit the key to version control
  - Use testnet04 for testing before mainnet deployment

### Key Format
- Kadena private key: 64 hex characters (32 bytes)
- Must be a valid Ed25519 private key
- Public key is automatically derived, so no need to store it separately

## Testing Checklist

### Before Mainnet Deployment
1. ✅ Compile successfully with `cargo build --release`
2. ⏳ Test on testnet04:
   - Set `KADENA_NETWORK=testnet04`
   - Verify node registration
   - Check status updates
   - Test reward calculation
   - Verify reward claiming
3. ⏳ Verify peer ID matches between Rust and JS implementations
4. ⏳ Test P2P connectivity using the Kadena-derived peer ID
5. ⏳ Verify multiaddr format is correct
6. ⏳ Monitor logs for registration and reward claim events

### Test Commands
```bash
# Build release version
cargo build --release

# Run with testnet
KADENA_NETWORK=testnet04 \
KADENA_ACCOUNT=k:your-account \
KADENA_SECRET_KEY=your-hex-key \
./target/release/cyberfly-rust-node

# Check logs
tail -f logs/node.log
```

## Backward Compatibility

### JavaScript Node Compatibility
- ✅ Same Ed25519 key used for both libp2p and Kadena
- ✅ Same peer ID generation algorithm
- ✅ Same multiaddr format (publickey@ip:port)
- ✅ Same public IP detection service (ip-api.com)
- ✅ Same smart contract interface
- ✅ Same gas station for gasless transactions

## Future Improvements
1. Add retry logic for failed registration attempts
2. Implement exponential backoff for status checks
3. Add metrics for registration and reward claim operations
4. Support multiple Kadena accounts per node
5. Add health check endpoint for registration status
6. Implement graceful shutdown with final status update

## References
- [rust_pact GitHub](https://github.com/cyberfly-io/rust-pact)
- [cyberfly-node JavaScript Implementation](https://github.com/cyberfly-io/cyberfly-node)
- [Kadena Chainweb API](https://api.chainweb.com/openapi/pact.html)
- [Ed25519 Signature Scheme](https://ed25519.cr.yp.to/)
