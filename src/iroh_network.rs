// Iroh-based P2P network implementation
// Replaces libp2p with Iroh's Endpoint + Router + Gossip + Blobs

use anyhow::Result;
use iroh::{Endpoint, NodeId, SecretKey, protocol::Router};
use iroh_blobs::{BlobsProtocol, store::fs::FsStore};
use iroh_gossip::{
    net::Gossip, 
    proto::TopicId,
    api::{Event as GossipEvent, GossipSender}, 
    ALPN as GOSSIP_ALPN
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, Mutex};
use serde::{Serialize, Deserialize};
use tokio_stream::StreamExt;
use rumqttc::QoS;

use crate::mqtt_bridge::{GossipToMqttMessage, MqttToGossipMessage, MessageOrigin};

/// Network event types
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    Message { peer: NodeId, data: Vec<u8> },
    PeerDiscovered { peer: NodeId },
    PeerExpired { peer: NodeId },
}

/// Message format for gossip protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GossipMessage {
    /// Bridge metadata for loop prevention and tracking
    #[serde(rename = "__origin")]
    origin: String,  // "mqtt" or "local"
    #[serde(rename = "__broker")]
    broker: String,  // Unique broker identifier (peer ID)
    #[serde(rename = "__timestamp")]
    timestamp: i64,  // Message timestamp
    /// Actual message data
    message_id: String,
    topic: Option<String>,  // MQTT topic (for MQTT-originated messages)
    #[serde(with = "base64_bytes")]
    payload: Vec<u8>,
}

mod base64_bytes {
    use serde::{Deserialize, Deserializer, Serializer};
    use base64::{Engine as _, engine::general_purpose};

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&general_purpose::STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        general_purpose::STANDARD
            .decode(s)
            .map_err(serde::de::Error::custom)
    }
}

/// Configuration for Iroh network
#[derive(Clone)]
pub struct IrohNetworkConfig {
    pub data_dir: PathBuf,
    pub secret_key: Option<SecretKey>,
}

/// Iroh-based P2P Network
pub struct IrohNetwork {
    endpoint: Endpoint,
    router: Router,
    gossip: Gossip,
    blobs: BlobsProtocol,
    store: FsStore,
    node_id: NodeId,
    event_tx: mpsc::UnboundedSender<NetworkEvent>,
    event_rx: Arc<RwLock<mpsc::UnboundedReceiver<NetworkEvent>>>,
    mqtt_to_libp2p_rx: Option<mpsc::UnboundedReceiver<MqttToGossipMessage>>,
    libp2p_to_mqtt_tx: Option<mpsc::UnboundedSender<GossipToMqttMessage>>,
    // Gossip topics
    data_topic: TopicId,
    discovery_topic: TopicId,
    sync_topic: TopicId,  // New topic for data sync
    // Senders for broadcasting (set after subscribing)
    data_sender: Option<Arc<Mutex<GossipSender>>>,
    discovery_sender: Option<Arc<Mutex<GossipSender>>>,
    sync_sender: Option<Arc<Mutex<GossipSender>>>,  // Sync topic sender
    // Simple peer tracking - stores peers seen in gossip messages
    discovered_peers: Arc<dashmap::DashMap<NodeId, chrono::DateTime<chrono::Utc>>>,
    // Bootstrap peers for initial gossip network join
    bootstrap_peers: Vec<NodeId>,
}

impl IrohNetwork {
    /// Parse bootstrap peers from config strings
    /// 
    /// Accepts formats:
    /// - Full address: "NodeId@ip:port" (extracts just the NodeId)
    /// - NodeId only: "8921781873f3b664e020c4fe1c5b9796e70adccbaa26d12a39de9b317d9e9269"
    fn parse_bootstrap_peers(peer_strings: &[String]) -> Vec<NodeId> {
        let mut node_ids = Vec::new();
        
        for peer_str in peer_strings {
            let peer_str = peer_str.trim();
            if peer_str.is_empty() {
                continue;
            }
            
            // Extract NodeId from "NodeId@ip:port" format or use as-is
            let node_id_str = if let Some(idx) = peer_str.find('@') {
                &peer_str[..idx]
            } else {
                peer_str
            };
            
            // Try to parse as NodeId
            match node_id_str.parse::<NodeId>() {
                Ok(node_id) => {
                    tracing::info!("Parsed bootstrap peer: {}", node_id);
                    node_ids.push(node_id);
                }
                Err(e) => {
                    tracing::warn!("Failed to parse bootstrap peer '{}': {}", peer_str, e);
                }
            }
        }
        
        if node_ids.is_empty() {
            tracing::warn!("No valid bootstrap peers configured - node will only discover peers through other means");
        } else {
            tracing::info!("Configured {} bootstrap peer(s)", node_ids.len());
        }
        
        node_ids
    }

    /// Create Iroh network from existing components (recommended)
    /// 
    /// This constructor allows sharing a single Iroh node across multiple
    /// components (network, storage, etc.) for better resource efficiency.
    /// 
    /// # Arguments
    /// * `endpoint` - Shared Iroh endpoint
    /// * `router` - Shared Iroh router
    /// * `gossip` - Shared Gossip protocol handler
    /// * `blobs` - Shared BlobsProtocol handler
    /// * `store` - Shared FsStore for persistent storage
    /// * `bootstrap_peers` - Optional list of bootstrap peer strings (NodeId or NodeId@ip:port)
    pub fn from_components(
        endpoint: Endpoint,
        router: Router,
        gossip: Gossip,
        blobs: BlobsProtocol,
        store: FsStore,
        bootstrap_peer_strings: Vec<String>,
    ) -> Self {
        tracing::info!("Initializing Iroh network from shared components");
        
        let node_id = endpoint.node_id();
        
        // Create event channel
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        // Create gossip topics (all must be exactly 32 bytes)
        let data_topic = TopicId::from_bytes(*b"decentralized-db-data-v1-iroh!!!");
        let discovery_topic = TopicId::from_bytes(*b"decentralized-db-discovery-iroh!");
        let sync_topic = TopicId::from_bytes(*b"decentralized-db-sync-v1-iroh!!!");
        
        // Parse bootstrap peers
        let bootstrap_peers = Self::parse_bootstrap_peers(&bootstrap_peer_strings);

        Self {
            endpoint,
            router,
            gossip,
            blobs,
            store,
            node_id,
            event_tx,
            event_rx: Arc::new(RwLock::new(event_rx)),
            mqtt_to_libp2p_rx: None,
            libp2p_to_mqtt_tx: None,
            data_topic,
            discovery_topic,
            sync_topic,
            data_sender: None,
            discovery_sender: None,
            sync_sender: None,
            discovered_peers: Arc::new(dashmap::DashMap::new()),
            bootstrap_peers,
        }
    }

    /// Get the local node ID
    pub fn peer_id(&self) -> NodeId {
        self.node_id
    }

    /// Get node ID as string (for backward compatibility with libp2p PeerId)
    pub fn peer_id_string(&self) -> String {
        self.node_id.to_string()
    }

    /// Get reference to Iroh endpoint
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Get reference to gossip protocol
    pub fn gossip(&self) -> &Gossip {
        &self.gossip
    }

    /// Get reference to blobs protocol
    pub fn blobs(&self) -> &BlobsProtocol {
        &self.blobs
    }

    /// Connect MQTT bridge to network
    pub fn connect_mqtt_bridge(
        &mut self,
        mqtt_to_gossip_rx: mpsc::UnboundedReceiver<MqttToGossipMessage>,
        gossip_to_mqtt_tx: mpsc::UnboundedSender<GossipToMqttMessage>,
    ) {
        self.mqtt_to_libp2p_rx = Some(mqtt_to_gossip_rx);
        self.libp2p_to_mqtt_tx = Some(gossip_to_mqtt_tx);
        tracing::info!("MQTT bridge connected to Iroh network");
    }

    /// Main event loop
    pub async fn run(&mut self) -> Result<()> {
        // Initialize gossip discovery for automatic peer discovery
        tracing::info!("Starting Iroh network...");
        
        // Clone bootstrap peers for use in subscriptions
        let bootstrap_peers = self.bootstrap_peers.clone();
        
        if bootstrap_peers.is_empty() {
            tracing::warn!("⚠️  No bootstrap peers configured - peers won't auto-discover on local network");
            tracing::warn!("   Set BOOTSTRAP_PEERS env var to enable peer discovery");
        } else {
            tracing::info!("✓ Using {} bootstrap peer(s) for gossip network join", bootstrap_peers.len());
        }
        
        // Subscribe to data topic with bootstrap peers
        let data_topic = self.gossip.subscribe(self.data_topic, bootstrap_peers.clone()).await?;
        let (data_sender, data_receiver) = data_topic.split();
        self.data_sender = Some(Arc::new(Mutex::new(data_sender)));
        tracing::info!("Subscribed to data topic");

        // Subscribe to discovery topic with bootstrap peers
        let discovery_topic = self.gossip.subscribe(self.discovery_topic, bootstrap_peers.clone()).await?;
        let (discovery_sender, discovery_receiver) = discovery_topic.split();
        self.discovery_sender = Some(Arc::new(Mutex::new(discovery_sender)));
        tracing::info!("Subscribed to discovery topic");
        
        // Subscribe to sync topic with bootstrap peers
        let sync_topic = self.gossip.subscribe(self.sync_topic, bootstrap_peers.clone()).await?;
        let (sync_sender, sync_receiver) = sync_topic.split();
        self.sync_sender = Some(Arc::new(Mutex::new(sync_sender)));
        tracing::info!("Subscribed to sync topic");

        // Get node_id first before using in beacon task
        let node_id = self.node_id;

        // Start a peer discovery broadcast task
        // This helps peers find each other by periodically sending discovery beacons
        let discovery_beacon_sender = Arc::clone(&self.discovery_sender.as_ref().unwrap());
        let beacon_node_id = node_id;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            loop {
                interval.tick().await;
                
                // Broadcast a simple beacon message
                let beacon = serde_json::json!({
                    "type": "discovery_beacon",
                    "node_id": beacon_node_id.to_string(),
                    "timestamp": chrono::Utc::now().timestamp(),
                });
                
                if let Ok(beacon_bytes) = serde_json::to_vec(&beacon) {
                    let mut sender = discovery_beacon_sender.lock().await;
                    if let Err(e) = sender.broadcast(beacon_bytes.into()).await {
                        tracing::debug!("Discovery beacon broadcast error: {}", e);
                    } else {
                        tracing::trace!("Sent discovery beacon");
                    }
                }
            }
        });
        tracing::info!("🔍 Started peer discovery beacon (broadcasts every 10s)");

        // Get clones for the event loop
        let data_sender_clone = self.data_sender.clone().unwrap();
        let libp2p_to_mqtt_tx = self.libp2p_to_mqtt_tx.clone();
        let event_tx = self.event_tx.clone();
        let discovered_peers = self.discovered_peers.clone();

        // Convert receivers to streams (no need to Box them - they're already streamable)
        let mut data_stream = data_receiver;
        let mut discovery_stream = discovery_receiver;
        let mut sync_stream = sync_receiver;

        loop {
            tokio::select! {
                // Handle gossip events from data topic
                event_result = data_stream.next() => {
                    match event_result {
                        Some(Ok(event)) => {
                            if let Err(e) = Self::handle_gossip_event(
                                event, 
                                "data", 
                                node_id, 
                                &event_tx,
                                &libp2p_to_mqtt_tx,
                                &discovered_peers,
                            ).await {
                                tracing::error!("Error handling data gossip event: {}", e);
                            }
                        }
                        Some(Err(e)) => {
                            tracing::error!("Error reading data stream: {}", e);
                        }
                        None => {
                            tracing::warn!("Data stream ended");
                            break;
                        }
                    }
                }

                // Handle gossip events from discovery topic
                event_result = discovery_stream.next() => {
                    match event_result {
                        Some(Ok(event)) => {
                            if let Err(e) = Self::handle_gossip_event(
                                event, 
                                "discovery", 
                                node_id,
                                &event_tx,
                                &libp2p_to_mqtt_tx,
                                &discovered_peers,
                            ).await {
                                tracing::error!("Error handling discovery gossip event: {}", e);
                            }
                        }
                        Some(Err(e)) => {
                            tracing::error!("Error reading discovery stream: {}", e);
                        }
                        None => {
                            tracing::warn!("Discovery stream ended");
                        }
                    }
                }
                
                // Handle gossip events from sync topic
                event_result = sync_stream.next() => {
                    match event_result {
                        Some(Ok(event)) => {
                            if let Err(e) = Self::handle_sync_event(event, node_id, &event_tx, &discovered_peers).await {
                                tracing::error!("Error handling sync gossip event: {}", e);
                            }
                        }
                        Some(Err(e)) => {
                            tracing::error!("Error reading sync stream: {}", e);
                        }
                        None => {
                            tracing::warn!("Sync stream ended");
                        }
                    }
                }

                // Handle messages from MQTT to be published to gossip
                Some(mqtt_msg) = async {
                    if let Some(ref mut rx) = self.mqtt_to_libp2p_rx {
                        rx.recv().await
                    } else {
                        None
                    }
                } => {
                    if let Err(e) = Self::forward_mqtt_to_gossip(mqtt_msg, data_sender_clone.clone(), node_id).await {
                        tracing::error!("Error forwarding MQTT message: {}", e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle sync protocol events
    async fn handle_sync_event(
        event: GossipEvent, 
        node_id: NodeId,
        event_tx: &mpsc::UnboundedSender<NetworkEvent>,
        discovered_peers: &Arc<dashmap::DashMap<NodeId, chrono::DateTime<chrono::Utc>>>,
    ) -> Result<()> {
        match event {
            GossipEvent::Received(msg) => {
                let from = msg.delivered_from;
                tracing::debug!("Received sync message from {}", from);

                // Ignore our own messages
                if from == node_id {
                    return Ok(());
                }
                
                // Track this peer (update timestamp)
                discovered_peers.insert(from, chrono::Utc::now());

                // Emit network event with sync data
                // The sync manager will handle the actual parsing and processing
                let _ = event_tx.send(NetworkEvent::Message {
                    peer: from,
                    data: msg.content.to_vec(),
                });
            }
            GossipEvent::NeighborUp(peer_node_id) => {
                tracing::info!("Sync neighbor up: {}", peer_node_id);
                
                // Track this peer immediately when they connect
                discovered_peers.insert(peer_node_id, chrono::Utc::now());
                
                let _ = event_tx.send(NetworkEvent::PeerDiscovered { peer: peer_node_id });
            }
            GossipEvent::NeighborDown(peer_node_id) => {
                tracing::info!("Sync neighbor down: {}", peer_node_id);
                
                // Remove peer when they disconnect
                discovered_peers.remove(&peer_node_id);
                
                let _ = event_tx.send(NetworkEvent::PeerExpired { peer: peer_node_id });
            }
            GossipEvent::Lagged => {
                tracing::warn!("Sync gossip lagged - missed messages");
            }
        }

        Ok(())
    }

    /// Handle gossip protocol events
    async fn handle_gossip_event(
        event: GossipEvent, 
        topic_type: &str,
        node_id: NodeId,
        event_tx: &mpsc::UnboundedSender<NetworkEvent>,
        libp2p_to_mqtt_tx: &Option<mpsc::UnboundedSender<GossipToMqttMessage>>,
        discovered_peers: &Arc<dashmap::DashMap<NodeId, chrono::DateTime<chrono::Utc>>>,
    ) -> Result<()> {
        match event {
            GossipEvent::Received(msg) => {
                let from = msg.delivered_from;
                tracing::info!("📡 Received gossip message from {} on {} topic ({} bytes)", from, topic_type, msg.content.len());

                // Ignore our own messages
                if from == node_id {
                    return Ok(());
                }
                
                // Track this peer (update timestamp)
                discovered_peers.insert(from, chrono::Utc::now());

                // Only parse as GossipMessage on the data topic
                // Discovery and sync topics have different message formats
                if topic_type == "data" {
                    // If DEBUG_GOSSIP_RAW is set, dump the raw payload (base64)
                    if std::env::var("DEBUG_GOSSIP_RAW").is_ok() {
                        use base64::Engine;
                        let encoded = base64::engine::general_purpose::STANDARD.encode(&msg.content);
                        tracing::info!("RAW_GOSSIP_PAYLOAD(base64) from {}: {}", from, encoded);
                    }
                    // Try to parse as GossipMessage
                    match serde_json::from_slice::<GossipMessage>(&msg.content) {
                        Ok(gossip_msg) => {
                            tracing::info!("📨 Received gossip message - origin: {}, topic: {:?}, from: {}", 
                                gossip_msg.origin, gossip_msg.topic, from);
                            
                            // Check if this is a bridge message with metadata
                            let actual_data = gossip_msg.payload;
                            let origin = gossip_msg.origin;
                            let broker = gossip_msg.broker;
                            
                            // Skip if message originated from THIS node's MQTT broker
                            // (prevents local MQTT clients from receiving duplicates)
                            if origin == "mqtt" && broker == node_id.to_string() {
                                tracing::debug!("Skipped message from own MQTT broker - loop prevention");
                                return Ok(());
                            }
                            
                            // Forward MQTT messages to MQTT broker on other machines
                            // Messages originating from MQTT should be published to MQTT on remote peers
                            if origin == "mqtt" {
                                if let Some(ref tx) = libp2p_to_mqtt_tx {
                                    // Use the original MQTT topic from the message
                                    let mqtt_topic = gossip_msg.topic.clone()
                                        .unwrap_or_else(|| format!("iroh/{}", topic_type));
                                    
                                    tracing::info!("🔀 Forwarding gossip MQTT message to MQTT broker - topic: {}", mqtt_topic);
                                    
                                    let mqtt_msg = GossipToMqttMessage {
                                        topic: mqtt_topic.clone(),
                                        payload: actual_data.clone(),
                                        message_id: gossip_msg.message_id.clone(),
                                        origin: MessageOrigin::Gossip,  // Mark as Gossip so it gets published on remote peers
                                        qos: QoS::AtMostOnce,
                                    };
                                    
                                    if let Err(e) = tx.send(mqtt_msg) {
                                        tracing::error!("❌ Failed to send to MQTT bridge: {}", e);
                                    } else {
                                        tracing::info!("✅ Sent to MQTT bridge - topic: {}", mqtt_topic);
                                    }
                                } else {
                                    tracing::warn!("⚠️  No MQTT bridge connected");
                                }
                            } else {
                                // For non-MQTT messages, forward to MQTT with iroh/ prefix
                                if let Some(ref tx) = libp2p_to_mqtt_tx {
                                    let mqtt_msg = GossipToMqttMessage {
                                        topic: format!("iroh/{}", topic_type),
                                        payload: actual_data.clone(),
                                        message_id: gossip_msg.message_id.clone(),
                                        origin: MessageOrigin::Gossip,
                                        qos: QoS::AtMostOnce,
                                    };
                                    let _ = tx.send(mqtt_msg);
                                }
                            }

                            // Emit network event
                            let _ = event_tx.send(NetworkEvent::Message {
                                peer: from,
                                data: actual_data.clone(),
                            });

                            // No direct GraphQL broadcast here: rely on MQTT bridge + message_store + main broadcaster
                        }
                        Err(e) => {
                            // If DEBUG_GOSSIP_RAW is set, include the base64 payload in the warning
                            if std::env::var("DEBUG_GOSSIP_RAW").is_ok() {
                                use base64::Engine;
                                let encoded = base64::engine::general_purpose::STANDARD.encode(&msg.content);
                                tracing::warn!("Failed to parse gossip message as GossipMessage: {} ({} bytes from {}) - raw(base64): {}", e, msg.content.len(), from, encoded);
                            } else {
                                tracing::warn!("Failed to parse gossip message as GossipMessage: {} ({} bytes from {})", e, msg.content.len(), from);
                            }
                        }
                    }
                } else {
                    // For discovery and sync topics, just log the message without parsing
                    tracing::debug!("Received {} message from {} ({} bytes)", topic_type, from, msg.content.len());
                }
            }
            GossipEvent::NeighborUp(node_id) => {
                tracing::info!("Neighbor up: {}", node_id);
                let _ = event_tx.send(NetworkEvent::PeerDiscovered { peer: node_id });
            }
            GossipEvent::NeighborDown(node_id) => {
                tracing::info!("Neighbor down: {}", node_id);
                let _ = event_tx.send(NetworkEvent::PeerExpired { peer: node_id });
            }
            GossipEvent::Lagged => {
                tracing::warn!("Gossip lagged - missed messages");
            }
        }

        Ok(())
    }

    /// Forward MQTT message to gossip network
    async fn forward_mqtt_to_gossip(
        mqtt_msg: MqttToGossipMessage,
        data_sender: Arc<Mutex<GossipSender>>,
        node_id: NodeId,
    ) -> Result<()> {
        tracing::info!("🔄 Forwarding MQTT message to gossip - topic: {}, payload_size: {}", 
            mqtt_msg.topic, mqtt_msg.payload.len());

        let gossip_msg = GossipMessage {
            origin: "mqtt".to_string(),
            broker: node_id.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            message_id: mqtt_msg.message_id.clone(),
            topic: Some(mqtt_msg.topic.clone()),  // Include MQTT topic
            payload: mqtt_msg.payload,
        };

        let payload = serde_json::to_vec(&gossip_msg)?;

        // Publish to data topic using sender
        data_sender.lock().await.broadcast(payload.into()).await?;
        
        tracing::info!("✅ MQTT message broadcasted to gossip network - message_id: {}", gossip_msg.message_id);

        Ok(())
    }

    /// Broadcast data to all peers
    pub async fn broadcast(&self, data: Vec<u8>) -> Result<()> {
        let Some(ref sender) = self.data_sender else {
            anyhow::bail!("Network not started - call run() first");
        };

        let gossip_msg = GossipMessage {
            origin: "local".to_string(),
            broker: self.node_id.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            message_id: uuid::Uuid::new_v4().to_string(),
            topic: None,  // No MQTT topic for local broadcasts
            payload: data,
        };

        let payload = serde_json::to_vec(&gossip_msg)?;
        sender.lock().await.broadcast(payload.into()).await?;
        Ok(())
    }

    /// Announce presence to network
    pub async fn announce_presence(&self) -> Result<()> {
        let Some(ref sender) = self.discovery_sender else {
            anyhow::bail!("Network not started - call run() first");
        };

        let node_addr = self.endpoint.node_addr();
        
        let announcement = serde_json::json!({
            "node_id": self.node_id.to_string(),
            "relay_url": node_addr.relay_url().map(|u| u.to_string()),
            "direct_addresses": node_addr.direct_addresses().map(|addr| addr.to_string()).collect::<Vec<_>>(),
        });

        let payload = serde_json::to_vec(&announcement)?;
        sender.lock().await.broadcast(payload.into()).await?;

        tracing::debug!("Announced presence to network");
        Ok(())
    }
    
    /// Broadcast sync message to network
    pub async fn broadcast_sync(&self, sync_msg: crate::sync::SyncMessage) -> Result<()> {
        let Some(ref sender) = self.sync_sender else {
            anyhow::bail!("Network not started - call run() first");
        };
        
        let payload = serde_json::to_vec(&sync_msg)?;
        sender.lock().await.broadcast(payload.into()).await?;
        
        tracing::debug!("Broadcast sync message");
        Ok(())
    }
    
    /// Get sync sender for external use
    pub fn sync_sender(&self) -> Option<Arc<Mutex<GossipSender>>> {
        self.sync_sender.clone()
    }

    /// Get event receiver (for compatibility with old API)
    pub async fn event_receiver(&self) -> mpsc::UnboundedReceiver<NetworkEvent> {
        // This is a bit hacky but needed for API compatibility
        // In production, consider refactoring to use a broadcast channel
        let (tx, rx) = mpsc::unbounded_channel();
        // TODO: Forward events from self.event_rx to this new channel
        rx
    }

    /// Shutdown the network gracefully
    pub async fn shutdown(self) -> Result<()> {
        tracing::info!("Shutting down Iroh network...");
        self.router.shutdown().await?;
        self.endpoint.close().await;
        tracing::info!("Iroh network shut down");
        Ok(())
    }

    // ============ Network Statistics ============

    /// Get network statistics
    pub async fn get_stats(&self) -> NetworkStats {
        // Get connected peers from discovery
        let connected_peers = self.discovered_peers.len();
        
        // For Iroh with gossip discovery, connected and discovered are the same
        let discovered_peers = connected_peers;
        
        // Calculate uptime (simplified - in production, track start time)
        let uptime_seconds = 0u64; // TODO: Track actual uptime
        
        // Get relay URL if configured
        let relay_url = None; // TODO: Get from endpoint configuration
        
        NetworkStats {
            connected_peers,
            discovered_peers,
            uptime_seconds,
            relay_url,
        }
    }

    /// Get count of connected peers
    async fn get_connected_peer_count(&self) -> usize {
        self.discovered_peers.len()
    }

    /// Get list of connected peers with last seen timestamp
    pub async fn get_connected_peers(&self) -> Vec<(NodeId, chrono::DateTime<chrono::Utc>)> {
        self.discovered_peers
            .iter()
            .map(|entry| (*entry.key(), *entry.value()))
            .collect()
    }

    /// Get list of discovered peers (same as connected for Iroh)
    pub async fn get_discovered_peers(&self) -> Vec<(NodeId, chrono::DateTime<chrono::Utc>)> {
        // In Iroh with gossip discovery, discovered and connected are essentially the same
        self.get_connected_peers().await
    }
    
    /// Get a cloneable reference to the discovered peers map
    pub fn discovered_peers_map(&self) -> Arc<dashmap::DashMap<NodeId, chrono::DateTime<chrono::Utc>>> {
        self.discovered_peers.clone()
    }
}

/// Network statistics structure
#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub connected_peers: usize,
    pub discovered_peers: usize,
    pub uptime_seconds: u64,
    pub relay_url: Option<String>,
}

// Re-export types for compatibility
pub use iroh::NodeId as PeerId;
