use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub api_host: String,
    pub api_port: u16,
    pub iroh_config: IrohConfig,
    pub mqtt_config: MqttConfig,
    pub relay_config: RelayConfig,
    pub kadena_config: Option<KadenaConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrohConfig {
    pub bootstrap_peers: Vec<String>, // NodeId@ip:port format
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayConfig {
    pub enabled: bool,
    pub http_bind_addr: String,
    pub stun_port: u16,
    pub relay_url: Option<String>, // Full relay URL for clients to use
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttConfig {
    pub enabled: bool,
    pub broker_host: String,
    pub broker_port: u16,
    pub client_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KadenaConfig {
    pub account: String,
    pub secret_key: String,
    pub network_id: String, // "mainnet01" or "testnet04"
    pub chain_id: String,   // "1"
    pub api_host: String,   // API endpoint URL
}

impl KadenaConfig {
    /// Derive the Ed25519 public key from the private key (hex format)
    pub fn public_key(&self) -> Result<String> {
        use ed25519_dalek::SigningKey;
        
        // Decode the hex secret key
        let secret_bytes = hex::decode(&self.secret_key)
            .map_err(|e| anyhow::anyhow!("Failed to decode secret key: {}", e))?;
        
        if secret_bytes.len() != 32 {
            return Err(anyhow::anyhow!("Invalid secret key length: expected 32 bytes, got {}", secret_bytes.len()));
        }
        
        // Create SigningKey from bytes
        let signing_key = SigningKey::from_bytes(&secret_bytes.try_into().unwrap());
        
        // Get the public key (verifying key)
        let verifying_key = signing_key.verifying_key();
        
        // Return hex-encoded public key
        Ok(hex::encode(verifying_key.as_bytes()))
    }
    
    /// Extract the public key from the account (removes "k:" prefix if present)
    pub fn account_pubkey(&self) -> String {
        if self.account.starts_with("k:") {
            self.account[2..].to_string()
        } else {
            self.account.clone()
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let api_host = env::var("API_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());

        let api_port = env::var("API_PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse()
            .unwrap_or(8080);

        // Iroh bootstrap peers (NodeId@ip:port format)
        let bootstrap_peers = env::var("BOOTSTRAP_PEERS")
            .unwrap_or_else(|_| String::new())
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.trim().to_string())
            .collect();

        // MQTT Configuration
        let mqtt_enabled = env::var("MQTT_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true);

        let mqtt_broker_host =
            env::var("MQTT_BROKER_HOST").unwrap_or_else(|_| "localhost".to_string());

        let mqtt_broker_port = env::var("MQTT_BROKER_PORT")
            .unwrap_or_else(|_| "1883".to_string())
            .parse()
            .unwrap_or(1883);

        let mqtt_client_id =
            env::var("MQTT_CLIENT_ID").unwrap_or_else(|_| "cyberfly-node".to_string()); // Default, will be overridden by peer ID

        // Relay Configuration
        let relay_enabled = env::var("RELAY_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true);

        let relay_http_bind =
            env::var("RELAY_HTTP_BIND").unwrap_or_else(|_| "0.0.0.0:3340".to_string());

        let relay_stun_port = env::var("RELAY_STUN_PORT")
            .unwrap_or_else(|_| "3478".to_string())
            .parse()
            .unwrap_or(3478);

        // Build relay URL if relay is enabled
        let relay_url = if relay_enabled {
            let host = env::var("RELAY_PUBLIC_HOST").unwrap_or_else(|_| api_host.clone());
            Some(format!("iroh-relay://{}:{}", host, relay_stun_port))
        } else {
            None
        };

        // Kadena Configuration (optional)
        let kadena_config = if let Ok(account) = env::var("KADENA_ACCOUNT") {
            let network_id =
                env::var("KADENA_NETWORK").unwrap_or_else(|_| "mainnet01".to_string());
            let chain_id = env::var("KADENA_CHAIN_ID").unwrap_or_else(|_| "1".to_string());
            let api_host = env::var("KADENA_API_HOST").unwrap_or_else(|_| {
                format!(
                    "https://api.chainweb.com/chainweb/0.0/{}/chain/{}/pact",
                    network_id, chain_id
                )
            });

            Some(KadenaConfig {
                account,
                secret_key: env::var("KADENA_SECRET_KEY")
                    .expect("KADENA_SECRET_KEY must be set when KADENA_ACCOUNT is provided"),
                network_id,
                chain_id,
                api_host,
            })
        } else {
            None
        };

        Ok(Self {
            api_host,
            api_port,
            iroh_config: IrohConfig { bootstrap_peers },
            mqtt_config: MqttConfig {
                enabled: mqtt_enabled,
                broker_host: mqtt_broker_host,
                broker_port: mqtt_broker_port,
                client_id: mqtt_client_id,
            },
            relay_config: RelayConfig {
                enabled: relay_enabled,
                http_bind_addr: relay_http_bind,
                stun_port: relay_stun_port,
                relay_url,
            },
            kadena_config,
        })
    }
}
