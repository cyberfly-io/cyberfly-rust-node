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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttConfig {
    pub enabled: bool,
    pub broker_host: String,
    pub broker_port: u16,
    pub client_id: String,
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
            },
        })
    }
}
