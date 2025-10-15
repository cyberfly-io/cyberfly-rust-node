use anyhow::Result;
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS};
use tokio::sync::mpsc;
use std::collections::VecDeque;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use sha2::{Sha256, Digest};

/// Message origin to prevent loops
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageOrigin {
    Mqtt,      // Message originated from MQTT broker
    Libp2p,    // Message originated from libp2p network
}

/// Payload hash with timestamp for time-based deduplication
#[derive(Debug, Clone)]
struct PayloadHash {
    hash: String,
    timestamp: u64,  // Unix timestamp in seconds
}

/// Message from MQTT to be forwarded to libp2p
#[derive(Debug, Clone)]
pub struct MqttToLibp2pMessage {
    pub topic: String,
    pub payload: Vec<u8>,
    pub message_id: String,  // Unique ID to prevent loops
}

/// Message from libp2p to be forwarded to MQTT
#[derive(Debug, Clone)]
pub struct Libp2pToMqttMessage {
    pub topic: String,
    pub payload: Vec<u8>,
    pub qos: QoS,
    pub message_id: String,  // Unique ID to prevent loops
    pub origin: MessageOrigin,  // Track where message came from
}

/// Configuration for MQTT bridge
#[derive(Debug, Clone)]
pub struct MqttBridgeConfig {
    pub broker_host: String,
    pub broker_port: u16,
    pub client_id: String,
    pub keep_alive: Duration,
}

impl Default for MqttBridgeConfig {
    fn default() -> Self {
        Self {
            broker_host: "localhost".to_string(),
            broker_port: 1883,
            client_id: format!("cyberfly-node-{}", uuid::Uuid::new_v4()),
            keep_alive: Duration::from_secs(60),
        }
    }
}

/// MQTT Bridge for bidirectional communication between MQTT and libp2p
pub struct MqttBridge {
    client: AsyncClient,
    config: MqttBridgeConfig,
    mqtt_to_libp2p_tx: mpsc::UnboundedSender<MqttToLibp2pMessage>,
    mqtt_to_libp2p_rx: mpsc::UnboundedReceiver<MqttToLibp2pMessage>,
    libp2p_to_mqtt_rx: mpsc::UnboundedReceiver<Libp2pToMqttMessage>,
    seen_payloads: VecDeque<PayloadHash>,  // Time-based deduplication queue
    dedup_window_secs: u64,  // How long to remember payload hashes (in seconds)
    connected: bool,  // Track connection state
}

impl MqttBridge {
    /// Generate a hash of the payload for deduplication
    fn hash_payload(topic: &str, payload: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(topic.as_bytes());
        hasher.update(payload);
        format!("{:x}", hasher.finalize())
    }
    
    /// Generate a unique message ID with timestamp
    fn generate_message_id(topic: &str, payload: &[u8]) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros();
        
        let mut hasher = Sha256::new();
        hasher.update(topic.as_bytes());
        hasher.update(payload);
        hasher.update(timestamp.to_le_bytes());
        
        format!("{:x}", hasher.finalize())
    }
    
    /// Check if we've seen this payload recently (within dedup window)
    fn is_duplicate(&mut self, topic: &str, payload: &[u8]) -> bool {
        let payload_hash = Self::hash_payload(topic, payload);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        // Remove old entries outside the deduplication window
        while let Some(front) = self.seen_payloads.front() {
            if now - front.timestamp > self.dedup_window_secs {
                self.seen_payloads.pop_front();
            } else {
                break;
            }
        }
        
        // Check if this payload hash exists in recent history
        let is_dup = self.seen_payloads.iter().any(|ph| ph.hash == payload_hash);
        
        if !is_dup {
            // Add to seen payloads
            self.seen_payloads.push_back(PayloadHash {
                hash: payload_hash,
                timestamp: now,
            });
            
            tracing::trace!("Tracking {} payload hashes in deduplication window", self.seen_payloads.len());
        }
        
        is_dup
    }
    
    /// Create a new MQTT bridge
    pub fn new(config: MqttBridgeConfig) -> Result<(Self, mpsc::UnboundedSender<Libp2pToMqttMessage>, EventLoop)> {
        let mut mqttoptions = MqttOptions::new(
            &config.client_id,
            &config.broker_host,
            config.broker_port,
        );
        mqttoptions.set_keep_alive(config.keep_alive);
        
        let (client, eventloop) = AsyncClient::new(mqttoptions, 10);
        
        let (mqtt_to_libp2p_tx, mqtt_to_libp2p_rx) = mpsc::unbounded_channel();
        let (libp2p_to_mqtt_tx, libp2p_to_mqtt_rx) = mpsc::unbounded_channel();
        
        let bridge = Self {
            client,
            config,
            mqtt_to_libp2p_tx,
            mqtt_to_libp2p_rx,
            libp2p_to_mqtt_rx,
            seen_payloads: VecDeque::new(),
            dedup_window_secs: 300,  // 5 minutes deduplication window
            connected: false,
        };
        
        Ok((bridge, libp2p_to_mqtt_tx, eventloop))
    }
    
    /// Subscribe to MQTT topics
    async fn subscribe_to_topics(&self) -> Result<()> {
        tracing::info!("Subscribing to all MQTT topics: #");
        self.client.subscribe("#", QoS::AtLeastOnce).await?;
        Ok(())
    }
    
    /// Check if a topic matches a wildcard pattern
    /// Supports both + (single-level) and # (multi-level) wildcards
    fn matches_wildcard(pattern: &str, topic: &str) -> bool {
        let pattern_parts: Vec<&str> = pattern.split('/').collect();
        let topic_parts: Vec<&str> = topic.split('/').collect();
        
        let mut pattern_idx = 0;
        let mut topic_idx = 0;
        
        while pattern_idx < pattern_parts.len() && topic_idx < topic_parts.len() {
            let pattern_part = pattern_parts[pattern_idx];
            let topic_part = topic_parts[topic_idx];
            
            if pattern_part == "#" {
                // Multi-level wildcard matches everything remaining
                return true;
            } else if pattern_part == "+" {
                // Single-level wildcard matches one level
                pattern_idx += 1;
                topic_idx += 1;
            } else if pattern_part == topic_part {
                // Exact match
                pattern_idx += 1;
                topic_idx += 1;
            } else {
                // No match
                return false;
            }
        }
        
        // Check if we've consumed both pattern and topic
        // Handle trailing # in pattern
        if pattern_idx < pattern_parts.len() && pattern_parts[pattern_idx] == "#" {
            return true;
        }
        
        pattern_idx == pattern_parts.len() && topic_idx == topic_parts.len()
    }
    
    /// Run the MQTT bridge event loop
    pub async fn run(mut self, mut eventloop: EventLoop) -> Result<()> {
        tracing::info!("MQTT bridge started");
        
        loop {
            tokio::select! {
                // Handle MQTT events (incoming messages from MQTT broker)
                event = eventloop.poll() => {
                    match event {
                        Ok(Event::Incoming(Packet::Publish(publish))) => {
                            let topic = publish.topic.clone();
                            let payload = publish.payload.to_vec();
                            
                            // Check if we've seen this payload recently (loop prevention)
                            if self.is_duplicate(&topic, &payload) {
                                tracing::debug!("Ignoring duplicate MQTT message payload within dedup window: {}", topic);
                                continue;
                            }
                            
                            // Generate unique message ID
                            let message_id = Self::generate_message_id(&topic, &payload);
                            
                            tracing::debug!("Received MQTT message on topic: {}", topic);
                            
                            // Forward to libp2p - keep original MQTT topic for propagation
                            let message = MqttToLibp2pMessage {
                                topic: topic.clone(),  // Use original MQTT topic, not mapped libp2p topic
                                payload,
                                message_id,
                            };
                            
                            if let Err(e) = self.mqtt_to_libp2p_tx.send(message) {
                                tracing::error!("Failed to forward MQTT message to libp2p: {}", e);
                            }
                        }
                        Ok(Event::Incoming(Packet::ConnAck(_))) => {
                            tracing::info!("Connected to MQTT broker");
                            self.connected = true;
                            
                            // Subscribe to topics only after connection is established
                            if let Err(e) = self.subscribe_to_topics().await {
                                tracing::error!("Failed to subscribe to MQTT topics after connection: {}", e);
                            }
                        }
                        Ok(Event::Incoming(Packet::Disconnect)) => {
                            tracing::info!("Disconnected from MQTT broker");
                            self.connected = false;
                        }
                        Ok(Event::Incoming(Packet::SubAck(suback))) => {
                            tracing::info!("Subscribed to MQTT topics: {:?}", suback);
                        }
                        Ok(_) => {}
                        Err(e) => {
                            tracing::error!("MQTT connection error: {}", e);
                            // Reset connection state on error
                            self.connected = false;
                            tokio::time::sleep(Duration::from_secs(5)).await;
                        }
                    }
                }
                
                // Handle messages from libp2p to be published to MQTT
                Some(message) = self.libp2p_to_mqtt_rx.recv() => {
                    tracing::info!("📥 Received message from libp2p - topic: {}, origin: {:?}, payload_size: {}", 
                        message.topic, message.origin, message.payload.len());
                    
                    // Only forward messages that originated from libp2p (not MQTT)
                    // This prevents: MQTT → libp2p → back to MQTT loop
                    if message.origin == MessageOrigin::Mqtt {
                        tracing::debug!("Skipping MQTT publish - message originated from MQTT (loop prevention)");
                        continue;
                    }
                    
                    // Check if we've seen this payload recently (deduplication)
                    if self.is_duplicate(&message.topic, &message.payload) {
                        tracing::debug!("Ignoring duplicate libp2p message payload within dedup window");
                        continue;
                    }
                    
                    tracing::info!("📤 Publishing to MQTT broker - topic: {}, payload_size: {}", 
                        message.topic, message.payload.len());
                    
                    if let Err(e) = self.client.publish(
                        message.topic.clone(),
                        message.qos,
                        false,
                        message.payload,
                    ).await {
                        tracing::error!("Failed to publish to MQTT: {}", e);
                    } else {
                        tracing::info!("✅ Published to MQTT broker successfully - topic: {}", message.topic);
                    }
                }
            }
        }
    }
    
    /// Get the receiver for messages from MQTT to libp2p
    pub fn get_mqtt_to_libp2p_receiver(&mut self) -> mpsc::UnboundedReceiver<MqttToLibp2pMessage> {
        let (tx, rx) = mpsc::unbounded_channel();
        let old_rx = std::mem::replace(&mut self.mqtt_to_libp2p_rx, rx);
        self.mqtt_to_libp2p_tx = tx;
        old_rx
    }
}

/// Helper struct to manage MQTT message history (for GraphQL queries)
#[derive(Clone)]
pub struct MqttMessageStore {
    messages: std::sync::Arc<tokio::sync::RwLock<Vec<MqttMessage>>>,
    max_messages: usize,
}

#[derive(Debug, Clone)]
pub struct MqttMessage {
    pub topic: String,
    pub payload: Vec<u8>,
    pub timestamp: i64,
}

impl MqttMessageStore {
    pub fn new(max_messages: usize) -> Self {
        Self {
            messages: std::sync::Arc::new(tokio::sync::RwLock::new(Vec::new())),
            max_messages,
        }
    }
    
    pub async fn add_message(&self, topic: String, payload: Vec<u8>) {
        let mut messages = self.messages.write().await;
        
        let message = MqttMessage {
            topic,
            payload,
            timestamp: chrono::Utc::now().timestamp(),
        };
        
        messages.push(message);
        
        // Keep only the most recent messages
        if messages.len() > self.max_messages {
            let excess = messages.len() - self.max_messages;
            messages.drain(0..excess);
        }
    }
    
    pub async fn get_messages(&self, topic_filter: Option<String>, limit: Option<usize>) -> Vec<MqttMessage> {
        let messages = self.messages.read().await;
        
        let filtered: Vec<MqttMessage> = if let Some(filter) = topic_filter {
            messages.iter()
                .filter(|m| m.topic.contains(&filter))
                .cloned()
                .collect()
        } else {
            messages.clone()
        };
        
        let limit = limit.unwrap_or(100);
        filtered.into_iter().rev().take(limit).collect()
    }
    
    pub async fn clear(&self) {
        let mut messages = self.messages.write().await;
        messages.clear();
    }
}

