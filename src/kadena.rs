use anyhow::{anyhow, Result};
use rust_pact::utils::KeyPair;
use rust_pact::LocalOptions;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

/// Response from ip-api.com
#[derive(Debug, Deserialize)]
struct IpApiResponse {
    query: String,
}

/// Get public IP address using ip-api.com service
pub async fn get_public_ip() -> Result<String> {
    let response = reqwest::get("http://ip-api.com/json/")
        .await
        .map_err(|e| anyhow!("Failed to fetch public IP: {}", e))?;
    
    let ip_data: IpApiResponse = response
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse IP response: {}", e))?;
    
    Ok(ip_data.query)
}

/// Generate libp2p peer ID from Kadena private key for backward compatibility with JavaScript nodes
pub fn generate_peer_id_from_kadena_key(secret_key_hex: &str) -> Result<String> {
    // Decode the hex-encoded secret key
    let secret_bytes = hex::decode(secret_key_hex)
        .map_err(|e| anyhow!("Failed to decode secret key: {}", e))?;
    
    if secret_bytes.len() != 32 {
        return Err(anyhow!("Invalid secret key length: expected 32 bytes, got {}", secret_bytes.len()));
    }

    // Create ed25519 secret key and derive keypair
    let secret = libp2p_identity::ed25519::SecretKey::try_from_bytes(secret_bytes)
        .map_err(|e| anyhow!("Failed to create secret key: {}", e))?;
    
    let keypair = libp2p_identity::ed25519::Keypair::from(secret);
    
    // Generate PeerId from the keypair's public key
    let peer_id = libp2p_identity::PeerId::from_public_key(&keypair.public().into());
    
    Ok(peer_id.to_string())
}

use crate::config::KadenaConfig;

/// Node status returned from smart contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    #[serde(rename = "peerId", skip_serializing_if = "Option::is_none", default)]
    pub peer_id: Option<String>,
    pub status: String, // "active" or "inactive"
    pub multiaddr: String,
    pub account: String,
    pub guard: Value,
    #[serde(rename = "registerDate", skip_serializing_if = "Option::is_none", default)]
    pub register_date: Option<String>,
    #[serde(rename = "lastActiveDate", skip_serializing_if = "Option::is_none", default)]
    pub last_active_date: Option<String>,
}

/// Reward calculation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardInfo {
    // Some contract responses return fractional days (e.g. 0.1),
    // so accept days as f64 to be tolerant during deserialization.
    pub days: f64,
    pub reward: f64,
}

/// Kadena smart contract interaction handler
pub struct NodeRegistry {
    config: KadenaConfig,
}

impl NodeRegistry {
    /// Create a new NodeRegistry instance
    pub fn new(config: KadenaConfig) -> Self {
        Self { config }
    }

    /// Generate libp2p peer ID from Kadena private key for backward compatibility
    pub fn generate_peer_id(&self) -> Result<String> {
        generate_peer_id_from_kadena_key(&self.config.secret_key)
    }

    /// Get keyset guard for new-node (includes both account pubkey and signing pubkey for initial registration)
    fn get_guard_for_new_node(&self) -> Result<Value> {
        let account_pubkey = self.config.account_pubkey();
        let signing_pubkey = self.config.public_key()?;
        
        Ok(json!({
            "pred": "keys-any",
            "keys": [account_pubkey, signing_pubkey]
        }))
    }

    /// Create capability for gas payer
    fn gas_payer_capability(&self) -> Value {
        json!({
            "name": "free.cyberfly-account-gas-station.GAS_PAYER",
            "args": ["cyberfly-account-gas", {"int": 1}, 1.0]
        })
    }

    /// Create capability for new node registration
    fn new_node_capability(&self) -> Value {
        json!({
            "name": "free.cyberfly_node.NEW_NODE",
            "args": []
        })
    }

    /// Create capability for node guard (for updates and claims)
    fn node_guard_capability(&self, peer_id: &str) -> Value {
        json!({
            "name": "free.cyberfly_node.NODE_GUARD",
            "args": [peer_id]
        })
    }

    /// Get node information from smart contract
    pub async fn get_node_info(&self, peer_id: &str) -> Result<Option<NodeStatus>> {
        let code = format!(r#"(free.cyberfly_node.get-node "{}")"#, peer_id);

        let cmd = json!({
            "pactCode": code,
            "envData": {},
            "meta": {
                "chainId": self.config.chain_id,
                "sender": "",
                "gasLimit": 1000,
                "gasPrice": 0.0000001,
                "ttl": 600,
                "creationTime": chrono::Utc::now().timestamp()
            },
            "networkId": self.config.network_id,
            "nonce": chrono::Utc::now().to_rfc3339(),
        });


        info!("Fetching node info for peer_id: {}", peer_id);

        // Use local API call with options to skip signature verification
        let api_host = self.config.api_host.clone();
        let options = LocalOptions {
            preflight: Some(false),
            signature_verification: Some(false),
        };
        
        let response = tokio::task::spawn_blocking(move || {
            rust_pact::fetch::local_with_opts(&cmd, &api_host, Some(options))
        }).await?;

        info!("Full response from get-node: {:?}", response);

        if let Some(result) = response.get("result") {
            info!("Result: {:?}", result);
            
            // Check status field
            if let Some(status) = result.get("status") {
                let status_str = status.as_str().unwrap_or("");
                
                if status_str == "success" {
                    // Node exists - parse the data
                    if let Some(data) = result.get("data") {
                        match serde_json::from_value::<NodeStatus>(data.clone()) {
                            Ok(node_status) => {
                                info!("Node found: {} - status: {}", peer_id, node_status.status);
                                return Ok(Some(node_status));
                            }
                            Err(e) => {
                                error!("Failed to parse node status: {}. Data: {:?}", e, data);
                                return Err(anyhow!("Failed to parse node status: {}", e));
                            }
                        }
                    }
                } else if status_str == "failure" {
                    // Check if failure is due to node not existing
                    if let Some(error) = result.get("error") {
                        let error_msg = error.get("message").and_then(|m| m.as_str()).unwrap_or("unknown");
                        if error_msg.contains("No value found") || error_msg.contains("not found") {
                            info!("Node not found in contract: {}", peer_id);
                            return Ok(None);
                        }
                        warn!("Error fetching node info: {}", error_msg);
                        return Err(anyhow!("Contract error: {}", error_msg));
                    }
                }
            }
        }
        
        warn!("Unexpected response format from get-node");
        Ok(None)
    }

    /// Register a new node in the smart contract
    pub async fn create_node(&self, peer_id: &str, multiaddr: &str) -> Result<String> {
        info!("Registering new node: {} with multiaddr: {}", peer_id, multiaddr);

        let code = format!(
            r#"(free.cyberfly_node.new-node "{}" "active" "{}" "{}" (read-keyset "ks"))"#,
            peer_id, multiaddr, self.config.account
        );

        let signing_pubkey = self.config.public_key()?;
        let guard = self.get_guard_for_new_node()?;

        let keypair = KeyPair {
            public_key: signing_pubkey.clone(),
            secret_key: self.config.secret_key.clone(),
            clist: Some(vec![
                self.gas_payer_capability(),
                self.new_node_capability(),
            ]),
        };

        let cmd = json!({
            "pactCode": code,
            "envData": {
                "ks": guard
            },
            "meta": {
                "chainId": self.config.chain_id,
                "sender": "cyberfly-account-gas",
                "gasLimit": 2000,
                "gasPrice": 0.0000001,
                "ttl": 600,
                "creationTime": chrono::Utc::now().timestamp()
            },
            "networkId": self.config.network_id,
            "nonce": chrono::Utc::now().to_rfc3339(),
            "keyPairs": [json!({
                "publicKey": keypair.public_key,
                "secretKey": keypair.secret_key,
                "clist": keypair.clist
            })]
        });

        // First, test with local to verify transaction will succeed
        let api_host = self.config.api_host.clone();
        let cmd_clone = cmd.clone();
        let response = tokio::task::spawn_blocking(move || {
            rust_pact::fetch::local(&cmd_clone, &api_host)
        }).await?;

        if let Some(result) = response.get("result") {
            if result.get("status") != Some(&json!("success")) {
                let error_msg = result.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error");
                error!("Transaction validation failed: {}", error_msg);
                return Err(anyhow!("Transaction validation failed: {}", error_msg));
            }
            info!("Transaction validated successfully");
        }

        // Submit the transaction
        let api_host = self.config.api_host.clone();
        let response = tokio::task::spawn_blocking(move || {
            rust_pact::fetch::send(&cmd, &api_host, false)
        }).await?;

        if let Some(request_keys) = response.get("requestKeys").and_then(|k| k.as_array()) {
            if let Some(request_key) = request_keys.first().and_then(|k| k.as_str()) {
                info!("Node registration submitted. Request key: {}", request_key);
                
                // Poll for result
                let poll_result = self.poll_transaction(request_key).await?;
                info!("Node registered successfully: {}", peer_id);
                return Ok(poll_result);
            }
        }
        Err(anyhow!("No request key in response"))
    }

    /// Update node status (activate node)
    pub async fn activate_node(&self, peer_id: &str, multiaddr: &str) -> Result<String> {
        info!("Activating node: {} with multiaddr: {}", peer_id, multiaddr);

        let code = format!(
            r#"(free.cyberfly_node.update-node "{}" "{}" "active")"#,
            peer_id, multiaddr
        );

        let signing_pubkey = self.config.public_key()?;

        let keypair = KeyPair {
            public_key: signing_pubkey.clone(),
            secret_key: self.config.secret_key.clone(),
            clist: Some(vec![
                self.gas_payer_capability(),
                self.node_guard_capability(peer_id),
            ]),
        };

        let cmd = json!({
            "pactCode": code,
            "envData": {},
            "meta": {
                "chainId": self.config.chain_id,
                "sender": "cyberfly-account-gas",
                "gasLimit": 2000,
                "gasPrice": 0.0000001,
                "ttl": 600,
                "creationTime": chrono::Utc::now().timestamp()
            },
            "networkId": self.config.network_id,
            "nonce": chrono::Utc::now().to_rfc3339(),
            "keyPairs": [json!({
                "publicKey": keypair.public_key,
                "secretKey": keypair.secret_key,
                "clist": keypair.clist
            })]
        });

        let api_host = self.config.api_host.clone();
        let response = tokio::task::spawn_blocking(move || {
            rust_pact::fetch::send(&cmd, &api_host, false)
        }).await?;

        if let Some(request_keys) = response.get("requestKeys").and_then(|k| k.as_array()) {
            if let Some(request_key) = request_keys.first().and_then(|k| k.as_str()) {
                info!("Node activation submitted. Request key: {}", request_key);
                let poll_result = self.poll_transaction(request_key).await?;
                info!("Node activated successfully: {}", peer_id);
                return Ok(poll_result);
            }
        }
        Err(anyhow!("No request key in response"))
    }

    /// Calculate claimable rewards
    pub async fn calculate_rewards(&self, peer_id: &str) -> Result<Option<RewardInfo>> {
        let code = format!(
            r#"(free.cyberfly_node.calculate-days-and-reward "{}")"#,
            peer_id
        );

        let cmd = json!({
            "pactCode": code,
            "envData": {},
            "meta": {
                "chainId": self.config.chain_id,
                "sender": "",
                "gasLimit": 1000,
                "gasPrice": 0.0000001,
                "ttl": 600,
                "creationTime": chrono::Utc::now().timestamp()
            },
            "networkId": self.config.network_id,
            "nonce": chrono::Utc::now().to_rfc3339(),
            "keyPairs": []
        });

        let api_host = self.config.api_host.clone();
        let options = LocalOptions {
            preflight: Some(false),
            signature_verification: Some(false),
        };
        
        let response = tokio::task::spawn_blocking(move || {
            rust_pact::fetch::local_with_opts(&cmd, &api_host, Some(options))
        }).await?;

        if let Some(result) = response.get("result") {
            if result.get("status") == Some(&json!("success")) {
                if let Some(data) = result.get("data") {
                    match serde_json::from_value::<RewardInfo>(data.clone()) {
                        Ok(reward_info) => {
                            debug!("Rewards calculated: days={}, reward={}", reward_info.days, reward_info.reward);
                            return Ok(Some(reward_info));
                        }
                        Err(e) => {
                            warn!("Failed to parse reward info: {}", e);
                        }
                    }
                }
            }
        }
        Ok(None)
    }

    /// Claim rewards
    pub async fn claim_reward(&self, peer_id: &str) -> Result<String> {
        info!("Claiming rewards for node: {}", peer_id);

        let code = format!(
            r#"(free.cyberfly_node.claim-reward "{}" "{}")"#,
            self.config.account, peer_id
        );

        let signing_pubkey = self.config.public_key()?;

        let keypair = KeyPair {
            public_key: signing_pubkey.clone(),
            secret_key: self.config.secret_key.clone(),
            clist: Some(vec![
                self.gas_payer_capability(),
                self.node_guard_capability(peer_id),
            ]),
        };

        let cmd = json!({
            "pactCode": code,
            "envData": {},
            "meta": {
                "chainId": self.config.chain_id,
                "sender": "cyberfly-account-gas",
                "gasLimit": 2000,
                "gasPrice": 0.0000001,
                "ttl": 600,
                "creationTime": chrono::Utc::now().timestamp()
            },
            "networkId": self.config.network_id,
            "nonce": chrono::Utc::now().to_rfc3339(),
            "keyPairs": [json!({
                "publicKey": keypair.public_key,
                "secretKey": keypair.secret_key,
                "clist": keypair.clist
            })]
        });

        let api_host = self.config.api_host.clone();
        let response = tokio::task::spawn_blocking(move || {
            rust_pact::fetch::send(&cmd, &api_host, false)
        }).await?;

        if let Some(request_keys) = response.get("requestKeys").and_then(|k| k.as_array()) {
            if let Some(request_key) = request_keys.first().and_then(|k| k.as_str()) {
                info!("Reward claim submitted. Request key: {}", request_key);
                let poll_result = self.poll_transaction(request_key).await?;
                info!("Rewards claimed successfully for: {}", peer_id);
                return Ok(poll_result);
            }
        }
        Err(anyhow!("No request key in response"))
    }

    /// Poll transaction until completion
    async fn poll_transaction(&self, request_key: &str) -> Result<String> {
        let poll_cmd = json!({
            "requestKeys": [request_key]
        });

        // Poll up to 30 times (1 minute with 2-second intervals)
        for attempt in 1..=30 {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            
            let api_host = self.config.api_host.clone();
            let poll_cmd_clone = poll_cmd.clone();
            let response = tokio::task::spawn_blocking(move || {
                rust_pact::fetch::poll(&poll_cmd_clone, &api_host)
            }).await?;

            if let Some(results) = response.as_object() {
                if let Some(result) = results.get(request_key) {
                    if result.get("result").and_then(|r| r.get("status")) == Some(&json!("success")) {
                        debug!("Transaction confirmed after {} attempts", attempt);
                        return Ok(request_key.to_string());
                    } else if let Some(error) = result.get("result").and_then(|r| r.get("error")) {
                        let error_msg = error.get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("unknown error");
                        error!("Transaction failed: {}", error_msg);
                        return Err(anyhow!("Transaction failed: {}", error_msg));
                    }
                }
            }
        }

        Err(anyhow!("Transaction polling timeout"))
    }

    /// Ensure node is registered and active (main entry point)
    pub async fn ensure_registered(&self, peer_id: &str, multiaddr: &str) -> Result<()> {
        match self.get_node_info(peer_id).await? {
            Some(node_status) => {
                info!("Node already registered: {} - status: {}", peer_id, node_status.status);
                
                // If inactive, activate it
                if node_status.status == "inactive" {
                    info!("Node is inactive, activating...");
                    self.activate_node(peer_id, multiaddr).await?;
                }
            }
            None => {
                info!("Node not found, registering...");
                self.create_node(peer_id, multiaddr).await?;
            }
        }
        Ok(())
    }

    /// Check status and auto-claim rewards if available (for periodic task)
    pub async fn check_and_claim_rewards(&self, peer_id: &str) -> Result<()> {
        debug!("Checking node status and rewards for: {}", peer_id);

        // Check if rewards are claimable
        if let Some(reward_info) = self.calculate_rewards(peer_id).await? {
            if reward_info.reward > 0.0 {
                info!("Rewards available: {} days, {} tokens - claiming now", reward_info.days, reward_info.reward);
                self.claim_reward(peer_id).await?;
            } else {
                debug!("No claimable rewards yet (days: {}, reward: {})", reward_info.days, reward_info.reward);
            }
        }

        Ok(())
    }
}

/// Shared registry wrapped in Arc<Mutex>
pub type SharedNodeRegistry = Arc<Mutex<NodeRegistry>>;
