// Iroh-based P2P network implementation
// Replaces libp2p with Iroh's Endpoint + Router + Gossip + Blobs

use anyhow::Result;
use iroh::{Endpoint, EndpointId, EndpointAddr, TransportAddr, SecretKey, protocol::Router, Watcher};
use iroh_blobs::{BlobsProtocol, store::fs::FsStore};
use iroh_gossip::{
    net::Gossip, 
    proto::TopicId,
    api::{Event as GossipEvent, GossipSender},
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, Mutex};
use serde::{Serialize, Deserialize};
use tokio_stream::StreamExt;
use rumqttc::QoS;

use crate::mqtt_bridge::{GossipToMqttMessage, MqttToGossipMessage, MessageOrigin};
use crate::gossip_discovery::{
    GossipDiscoveryBuilder, DiscoverySender, DiscoveryReceiver, DiscoveryNode, 
    NodeCapabilities, PeerInfo,
};

/// Network event types
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    Message { peer: EndpointId, data: Vec<u8> },
    PeerDiscovered { peer: EndpointId },
    PeerExpired { peer: EndpointId },
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

/// Peer discovery announcement message
/// Broadcasts list of connected peers to enable full mesh topology
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PeerDiscoveryAnnouncement {
    /// Node ID of the sender
    node_id: String,
    /// List of connected peer addresses in format "peerId@ip:port"
    connected_peers: Vec<String>,
    /// Unix timestamp when announcement was created
    timestamp: i64,
    /// Region of the announcing node
    region: String,
    /// Ed25519 signature of the announcement (node_id + timestamp + peers)
    signature: String,
}

impl PeerDiscoveryAnnouncement {
    /// Create and sign a new peer announcement
    fn new(
        node_id: EndpointId,
        connected_peers: Vec<String>,
        region: String,
        secret_key: &iroh::SecretKey,
    ) -> Self {
        let timestamp = chrono::Utc::now().timestamp();
        
        // Create message to sign: node_id + timestamp + sorted peers
        let mut peers_sorted = connected_peers.clone();
        peers_sorted.sort();
        let message = format!("{}:{}:{}", node_id, timestamp, peers_sorted.join(","));
        
        // Sign with node's secret key
        let signature_obj = secret_key.sign(message.as_bytes());
        let signature = hex::encode(signature_obj.to_bytes());
        
        Self {
            node_id: node_id.to_string(),
            connected_peers,
            timestamp,
            region,
            signature,
        }
    }
    
    /// Verify the signature of an announcement
    fn verify(&self, node_id: EndpointId, public_key: &iroh::PublicKey) -> bool {
        // Reconstruct the message
        let mut peers_sorted = self.connected_peers.clone();
        peers_sorted.sort();
        let message = format!("{}:{}:{}", self.node_id, self.timestamp, peers_sorted.join(","));
        
        // Decode signature
        let signature_bytes = match hex::decode(&self.signature) {
            Ok(bytes) => bytes,
            Err(_) => return false,
        };
        
        // Convert to fixed-size array for Signature type
        let signature_array: [u8; 64] = match signature_bytes.try_into() {
            Ok(arr) => arr,
            Err(_) => return false,
        };
        
        let signature = iroh::Signature::from_bytes(&signature_array);
        
        // Verify signature
        public_key.verify(message.as_bytes(), &signature).is_ok()
    }
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
    node_id: EndpointId,
    event_tx: mpsc::UnboundedSender<NetworkEvent>,
    event_rx: Arc<RwLock<mpsc::UnboundedReceiver<NetworkEvent>>>,
    mqtt_to_libp2p_rx: Option<mpsc::UnboundedReceiver<MqttToGossipMessage>>,
    libp2p_to_mqtt_tx: Option<mpsc::UnboundedSender<GossipToMqttMessage>>,
    /// Optional receiver for outbound sync messages from other components (e.g. GraphQL)
    sync_outbound_rx: Option<mpsc::UnboundedReceiver<crate::sync::SyncMessage>>,
    /// Optional SyncManager to process inbound sync messages and drive bootstrap sync
    sync_manager: Option<crate::sync::SyncManager>,
    // Gossip topics
    data_topic: TopicId,
    discovery_topic: TopicId,
    sync_topic: TopicId,  // New topic for data sync
    peer_discovery_topic: TopicId,  // New topic for peer list announcements
    improved_discovery_topic: TopicId,  // Improved gossip discovery topic (postcard + ed25519)
    // Senders for broadcasting (set after subscribing)
    data_sender: Option<Arc<Mutex<GossipSender>>>,
    discovery_sender: Option<Arc<Mutex<GossipSender>>>,
    sync_sender: Option<Arc<Mutex<GossipSender>>>,  // Sync topic sender
    peer_discovery_sender: Option<Arc<Mutex<GossipSender>>>,  // Peer discovery sender
    // Improved gossip discovery (postcard serialization + ed25519 signatures)
    improved_discovery_peers: Arc<dashmap::DashMap<EndpointId, PeerInfo>>,
    // Peer tracking with addresses - stores peers seen in gossip messages
    // Maps EndpointId -> (last_seen_timestamp, optional_address)
    discovered_peers: Arc<dashmap::DashMap<EndpointId, (chrono::DateTime<chrono::Utc>, Option<std::net::SocketAddr>)>>,
    // Peer announcement cache - prevents reconnection loops
    peer_announcement_cache: Arc<dashmap::DashMap<String, i64>>,  // node_id -> last_timestamp
    // Bootstrap peers for initial gossip network join
    bootstrap_peers: Vec<EndpointId>,
    // Original bootstrap peer strings (with addresses)
    bootstrap_peer_strings: Vec<String>,
}

impl IrohNetwork {
    /// Parse bootstrap peers from config strings
    /// 
    /// Accepts formats:
    /// - Full address: "NodeId@ip:port" (extracts just the EndpointId)
    /// - EndpointId only: "8921781873f3b664e020c4fe1c5b9796e70adccbaa26d12a39de9b317d9e9269"
    fn parse_bootstrap_peers(peer_strings: &[String], local_node_id: EndpointId) -> Vec<EndpointId> {
        let mut node_ids = Vec::new();
        
        // Hardcoded bootstrap node
        const HARDCODED_BOOTSTRAP: &str = "04b754ba2a3da0970d72d08b8740fb2ad96e63cf8f8bef6b7f1ab84e5b09a7f8@67.211.219.34:31001";
        
        // Combine hardcoded peer with configured peers
        let mut all_peers: Vec<String> = vec![HARDCODED_BOOTSTRAP.to_string()];
        all_peers.extend(peer_strings.iter().cloned());
        
        for peer_str in &all_peers {
            let peer_str = peer_str.trim();
            if peer_str.is_empty() {
                continue;
            }
            
            // Extract EndpointId from "EndpointId@ip:port" format or use as-is
            let node_id_str = if let Some(idx) = peer_str.find('@') {
                &peer_str[..idx]
            } else {
                peer_str
            };
            
            // Try to parse as EndpointId
            match node_id_str.parse::<EndpointId>() {
                Ok(node_id) => {
                    // Skip if this is our own node ID (don't dial ourselves)
                    if node_id == local_node_id {
                        tracing::info!("Skipping bootstrap peer {} (matches local node ID)", node_id);
                        continue;
                    }
                    
                    tracing::info!("Parsed bootstrap peer: {} from '{}'", node_id, peer_str);
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

    /// Attach an outbound sync receiver so other components (e.g. GraphQL) can send
    /// SyncMessage values to be broadcast by the network. This avoids exposing the
    /// internal field directly.
    pub fn set_sync_outbound_rx(&mut self, rx: mpsc::UnboundedReceiver<crate::sync::SyncMessage>) {
        self.sync_outbound_rx = Some(rx);
    }

    /// Attach a SyncManager so the network can process inbound sync traffic
    pub fn attach_sync_manager(&mut self, sync_manager: crate::sync::SyncManager) {
        self.sync_manager = Some(sync_manager);
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
        
        let node_id = endpoint.id();
        
        // Create event channel
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        // Create gossip topics (all must be exactly 32 bytes)
        let data_topic = TopicId::from_bytes(*b"decentralized-db-data-v1-iroh!!!");
        let discovery_topic = TopicId::from_bytes(*b"decentralized-db-discovery-iroh!");
        let sync_topic = TopicId::from_bytes(*b"decentralized-db-sync-v1-iroh!!!");
        let peer_discovery_topic = TopicId::from_bytes(*b"decentralized-peer-list-v1-iroh!");
        let improved_discovery_topic = TopicId::from_bytes(*b"cyberfly-discovery-v2-postcard!!");
        
        // Parse bootstrap peers (includes hardcoded peer, filtered by local node_id)
        let bootstrap_peers = Self::parse_bootstrap_peers(&bootstrap_peer_strings, node_id);

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
            sync_outbound_rx: None,
            sync_manager: None,
            data_topic,
            discovery_topic,
            sync_topic,
            peer_discovery_topic,
            improved_discovery_topic,
            data_sender: None,
            discovery_sender: None,
            sync_sender: None,
            peer_discovery_sender: None,
            improved_discovery_peers: Arc::new(dashmap::DashMap::new()),
            discovered_peers: Arc::new(dashmap::DashMap::new()),
            peer_announcement_cache: Arc::new(dashmap::DashMap::new()),
            bootstrap_peers,
            bootstrap_peer_strings,
        }
    }

    /// Get the local node ID
    pub fn peer_id(&self) -> EndpointId {
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

    /// Dial a peer by their public key (EndpointId)
    pub async fn dial_peer(&self, peer_id: EndpointId) -> anyhow::Result<()> {
        tracing::info!("Attempting to dial peer: {}", peer_id);
        
        // Use gossip ALPN for peer connections
        let alpn = iroh_gossip::ALPN;
        
        // Add the peer to the endpoint's address book
        // The endpoint will attempt to establish a connection
        let conn = self.endpoint.connect(peer_id, alpn)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect to peer {}: {}", peer_id, e))?;
        
        tracing::info!("Successfully connected to peer: {}", peer_id);
        
        // Track the discovered peer (no address since we used EndpointId-only connection)
        self.discovered_peers.insert(peer_id, (chrono::Utc::now(), None));
        
        // Note: The connection is managed by iroh internally.
        // For gossip protocol, the connection stays alive as long as there are active streams.
        // Dropping the Connection handle here is fine - iroh keeps the underlying QUIC connection
        // alive based on the protocol's needs (gossip keeps it alive for message exchange).
        drop(conn);
        
        Ok(())
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

    /// Add bootstrap peer addresses to endpoint's address book with retry logic
    async fn add_bootstrap_addresses(&self, peer_strings: &[String]) -> Result<()> {
        use std::net::SocketAddr;
        
        // Hardcoded bootstrap node
        const HARDCODED_BOOTSTRAP: &str = "04b754ba2a3da0970d72d08b8740fb2ad96e63cf8f8bef6b7f1ab84e5b09a7f8@67.211.219.34:31001";
        
        // Combine hardcoded peer with configured peers
        let mut all_peers: Vec<String> = vec![HARDCODED_BOOTSTRAP.to_string()];
        all_peers.extend(peer_strings.iter().cloned());
        
        // Track which peers we actually spawn connection tasks for (to match results correctly)
        let mut spawned_peers: Vec<(EndpointId, SocketAddr)> = Vec::new();
        let mut connection_tasks = Vec::new();
        
        for peer_str in &all_peers {
            let peer_str = peer_str.trim();
            if peer_str.is_empty() {
                continue;
            }
            
            // Parse "EndpointId@ip:port" format
            if let Some(at_idx) = peer_str.find('@') {
                let node_id_str = &peer_str[..at_idx];
                let socket_addr_str = &peer_str[at_idx + 1..];
                
                match node_id_str.parse::<EndpointId>() {
                    Ok(node_id) => {
                        // Skip our own node ID
                        if node_id == self.node_id {
                            continue;
                        }
                        
                        match socket_addr_str.parse::<SocketAddr>() {
                            Ok(socket_addr) => {
                                // Track this peer so we can match results correctly
                                spawned_peers.push((node_id, socket_addr));
                                
                                // Clone endpoint for use in async task
                                let endpoint = self.endpoint.clone();
                                
                                // Spawn retry task for this peer
                                let task = tokio::spawn(async move {
                                    Self::connect_bootstrap_peer_with_retry(
                                        endpoint,
                                        node_id,
                                        socket_addr,
                                    ).await
                                });
                                
                                connection_tasks.push(task);
                            }
                            Err(e) => {
                                tracing::warn!("Failed to parse socket address '{}': {}", socket_addr_str, e);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse node ID '{}': {}", node_id_str, e);
                    }
                }
            }
        }
        
        // Wait for all connection attempts to complete (or fail after retries)
        let results = futures::future::join_all(connection_tasks).await;
        
        // Collect successfully connected peers for monitoring
        // Now spawned_peers and results are correctly aligned
        let mut connected_bootstrap_peers = Vec::new();
        for ((node_id, socket_addr), result) in spawned_peers.iter().zip(results.iter()) {
            if let Ok(Ok(())) = result {
                connected_bootstrap_peers.push((*node_id, *socket_addr));
                
                // CRITICAL: Add bootstrap peer to discovered_peers so it gets broadcasted!
                // This allows other nodes to discover and connect to the bootstrap peer
                self.discovered_peers.insert(*node_id, (chrono::Utc::now(), Some(*socket_addr)));
                tracing::debug!("Added bootstrap peer {} ({}) to discovered peers", node_id.fmt_short(), socket_addr);
            }
        }
        
        // Count successful connections
        let successful_count = results.iter().filter(|r| {
            matches!(r, Ok(Ok(())))
        }).count();
        
        if successful_count > 0 {
            tracing::info!("✓ Successfully connected to {}/{} bootstrap peer(s)", successful_count, results.len());
            
            // Start connection monitor for bootstrap peers
            let endpoint = self.endpoint.clone();
            tokio::spawn(async move {
                Self::monitor_bootstrap_connections(endpoint, connected_bootstrap_peers).await;
            });
        } else {
            tracing::warn!("⚠️  Failed to connect to any bootstrap peers - will rely on DHT/mDNS discovery");
        }
        
        Ok(())
    }
    
    /// Monitor bootstrap peer connections and reconnect if disconnected
    async fn monitor_bootstrap_connections(
        endpoint: Endpoint,
        bootstrap_peers: Vec<(EndpointId, std::net::SocketAddr)>,
    ) {
        const CHECK_INTERVAL_SECS: u64 = 30; // Check every 30 seconds
        const RECONNECT_DELAY_SECS: u64 = 5; // Wait 5 seconds before reconnecting
        
        tracing::info!("🔍 Started bootstrap connection monitor (checks every {}s)", CHECK_INTERVAL_SECS);
        
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(CHECK_INTERVAL_SECS));
        
        loop {
            interval.tick().await;
            
            for (node_id, socket_addr) in &bootstrap_peers {
                // Check if we still have an active connection to this peer
                // conn_type() returns Option<Watcher<ConnectionType>>
                let conn_watcher = endpoint.conn_type(*node_id);
                
                let is_connected = if let Some(mut watcher) = conn_watcher {
                    // Check connection type - ConnectionType::None means no connection
                    use iroh::endpoint::ConnectionType;
                    !matches!(watcher.get(), ConnectionType::None)
                } else {
                    // No watcher means no connection info available
                    false
                };
                
                if !is_connected {
                    tracing::warn!(
                        "⚠️  Bootstrap peer {} at {} disconnected - attempting reconnection",
                        node_id.fmt_short(),
                        socket_addr
                    );
                    
                    // Wait a bit before reconnecting to avoid rapid reconnection attempts
                    tokio::time::sleep(std::time::Duration::from_secs(RECONNECT_DELAY_SECS)).await;
                    
                    // Attempt to reconnect with retry logic
                    let endpoint_clone = endpoint.clone();
                    let node_id_clone = *node_id;
                    let socket_addr_clone = *socket_addr;
                    
                    tokio::spawn(async move {
                        match Self::connect_bootstrap_peer_with_retry(
                            endpoint_clone,
                            node_id_clone,
                            socket_addr_clone,
                        ).await {
                            Ok(_) => {
                                tracing::info!(
                                    "✅ Successfully reconnected to bootstrap peer {} at {}",
                                    node_id_clone.fmt_short(),
                                    socket_addr_clone
                                );
                            }
                            Err(e) => {
                                tracing::error!(
                                    "❌ Failed to reconnect to bootstrap peer {} at {}: {}",
                                    node_id_clone.fmt_short(),
                                    socket_addr_clone,
                                    e
                                );
                            }
                        }
                    });
                } else {
                    tracing::trace!(
                        "✓ Bootstrap peer {} at {} still connected",
                        node_id.fmt_short(),
                        socket_addr
                    );
                }
            }
        }
    }
    
    /// Connect to a bootstrap peer with exponential backoff retry logic
    /// Tries direct IP connection first, then falls back to relay if direct fails
    async fn connect_bootstrap_peer_with_retry(
        endpoint: Endpoint,
        node_id: EndpointId,
        socket_addr: std::net::SocketAddr,
    ) -> Result<()> {
        const MAX_RETRIES: u32 = 5;
        const INITIAL_DELAY_MS: u64 = 1000; // 1 second
        const MAX_DELAY_MS: u64 = 30000; // 30 seconds
        
        let mut attempt = 0;
        let mut delay_ms = INITIAL_DELAY_MS;
        let mut use_relay = false;
        
        // Get our relay URL to share with peers (if connected to relay)
        let our_relay_url = endpoint.addr().relay_urls().next().cloned();
        
        loop {
            attempt += 1;
            
            // Build connection addresses - start with direct IP
            let mut addrs: Vec<TransportAddr> = vec![TransportAddr::Ip(socket_addr)];
            
            // After first failed attempt, add relay fallback if available
            if use_relay {
                if let Some(ref relay_url) = our_relay_url {
                    addrs.push(TransportAddr::Relay(relay_url.clone()));
                    tracing::info!(
                        "🔄 Adding relay fallback {} for peer {} (attempt {}/{})",
                        relay_url,
                        node_id.fmt_short(),
                        attempt,
                        MAX_RETRIES
                    );
                }
            }
            
            // Create EndpointAddr with node ID and addresses (direct + optional relay)
            let endpoint_addr = EndpointAddr::from_parts(node_id, addrs);
            
            tracing::info!(
                "🔄 Attempting to connect to bootstrap peer {} at {} (attempt {}/{}){}",
                node_id.fmt_short(),
                socket_addr,
                attempt,
                MAX_RETRIES,
                if use_relay { " [with relay fallback]" } else { "" }
            );
            
            match endpoint.connect(endpoint_addr, iroh_gossip::ALPN).await {
                Ok(_conn) => {
                    tracing::info!(
                        "✅ Successfully connected to bootstrap peer {} at {} (attempt {}){}",
                        node_id.fmt_short(),
                        socket_addr,
                        attempt,
                        if use_relay { " via relay" } else { " direct" }
                    );
                    return Ok(());
                }
                Err(e) => {
                    if attempt >= MAX_RETRIES {
                        tracing::error!(
                            "❌ Failed to connect to bootstrap peer {} at {} after {} attempts: {}",
                            node_id.fmt_short(),
                            socket_addr,
                            MAX_RETRIES,
                            e
                        );
                        return Err(anyhow::anyhow!(
                            "Failed to connect to bootstrap peer after {} attempts: {}",
                            MAX_RETRIES,
                            e
                        ));
                    }
                    
                    tracing::warn!(
                        "⚠️  Connection attempt {}/{} failed for peer {} at {}: {}",
                        attempt,
                        MAX_RETRIES,
                        node_id.fmt_short(),
                        socket_addr,
                        e
                    );
                    
                    // Enable relay fallback after first direct attempt fails
                    if !use_relay && our_relay_url.is_some() {
                        use_relay = true;
                        tracing::info!("🌐 Enabling relay fallback for next connection attempt");
                    }
                    
                    tracing::info!("⏳ Retrying in {}ms...", delay_ms);
                    
                    // Wait before retrying with exponential backoff
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    
                    // Exponential backoff: double the delay, up to MAX_DELAY_MS
                    delay_ms = (delay_ms * 2).min(MAX_DELAY_MS);
                }
            }
        }
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
            
            // Add bootstrap addresses to endpoint before subscribing
            if let Err(e) = self.add_bootstrap_addresses(&self.bootstrap_peer_strings.clone()).await {
                tracing::warn!("Failed to add bootstrap addresses: {}", e);
            }
        }
        
        // Subscribe to data topic with bootstrap peers
        let data_topic = self.gossip.subscribe(self.data_topic, bootstrap_peers.clone()).await?;
        let (data_sender, data_receiver) = data_topic.split();
        self.data_sender = Some(Arc::new(Mutex::new(data_sender)));
        tracing::info!("Subscribed to data topic");

        // Subscribe to discovery topic with bootstrap peers
        let discovery_topic = self.gossip.subscribe(self.discovery_topic, bootstrap_peers.clone()).await?;
        let (discovery_sender, mut discovery_receiver) = discovery_topic.split();
        self.discovery_sender = Some(Arc::new(Mutex::new(discovery_sender)));
        tracing::info!("Subscribed to discovery topic");
        
        // Subscribe to sync topic with bootstrap peers
        let sync_topic = self.gossip.subscribe(self.sync_topic, bootstrap_peers.clone()).await?;
        let (sync_sender, sync_receiver) = sync_topic.split();
        self.sync_sender = Some(Arc::new(Mutex::new(sync_sender)));
        tracing::info!("Subscribed to sync topic");

        // Subscribe to peer discovery topic with bootstrap peers
        let peer_discovery_topic = self.gossip.subscribe(self.peer_discovery_topic, bootstrap_peers.clone()).await?;
        let (peer_discovery_sender, peer_discovery_receiver) = peer_discovery_topic.split();
        self.peer_discovery_sender = Some(Arc::new(Mutex::new(peer_discovery_sender)));
        tracing::info!("Subscribed to peer discovery topic");

        // Initialize improved gossip discovery (postcard + ed25519 signatures)
        // This runs alongside the legacy JSON-based discovery for backward compatibility
        let (mut improved_sender, mut improved_receiver) = GossipDiscoveryBuilder::new()
            .with_expiration_timeout(std::time::Duration::from_secs(30))
            .with_broadcast_interval(std::time::Duration::from_secs(5))
            .build(
                self.gossip.clone(),
                self.improved_discovery_topic,
                bootstrap_peers.clone(),
                &self.endpoint,
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to initialize improved discovery: {}", e))?;
        tracing::info!("✓ Initialized improved gossip discovery (postcard + ed25519)");

        // Store reference to improved discovery peer map
        let improved_peers_ref = Arc::clone(&improved_receiver.neighbor_map);
        self.improved_discovery_peers = improved_peers_ref;

        // Wait for peers to join before starting broadcasts (following iroh-gossip best practices)
        if !bootstrap_peers.is_empty() {
            tracing::info!("Waiting for peers to join gossip network...");
            tokio::select! {
                result = discovery_receiver.joined() => {
                    match result {
                        Ok(_) => tracing::info!("✓ Successfully joined gossip network with peers"),
                        Err(e) => tracing::warn!("Failed to join gossip network: {}", e),
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {
                    tracing::warn!("Timeout waiting for peers to join - continuing anyway");
                }
            }
        }

        // Get node_id first before using in beacon task
        let node_id = self.node_id;
        let secret_key = self.endpoint.secret_key().clone();

        // Start improved discovery sender task
        let discovery_node = DiscoveryNode {
            name: format!("cyberfly-{}", &node_id.to_string()[..8]),
            node_id,
            count: 0,
            region: crate::node_region::get_node_region(),
            capabilities: NodeCapabilities {
                mqtt: self.mqtt_to_libp2p_rx.is_some(),
                streams: true,
                timeseries: true,
                geo: true,
                blobs: true,
            },
        };
        tokio::spawn(async move {
            if let Err(e) = improved_sender.run(discovery_node).await {
                tracing::error!("Improved discovery sender error: {}", e);
            }
        });
        tracing::info!("🚀 Started improved discovery sender (postcard serialization, 5s interval)");

        // Start improved discovery receiver task
        let improved_event_tx = self.event_tx.clone();
        let improved_discovered_peers = Arc::clone(&self.discovered_peers);
        tokio::spawn(async move {
            if let Err(e) = improved_receiver.run().await {
                tracing::error!("Improved discovery receiver error: {}", e);
            }
        });
        tracing::info!("🚀 Started improved discovery receiver (ed25519 signature verification)");

        // Start peer list announcement task - broadcasts connected peers every 10 seconds
        // This enables full mesh topology by sharing peer lists across the network
        let peer_list_sender = Arc::clone(self.peer_discovery_sender.as_ref().unwrap());
        let peer_list_node_id = node_id;
        let peer_list_discovered_peers = Arc::clone(&self.discovered_peers);
        let peer_list_secret_key = secret_key.clone();
        let peer_list_endpoint = self.endpoint.clone();
        tokio::spawn(async move {
            // Wait before starting announcements to allow gossip network to stabilize
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            loop {
                interval.tick().await;
                
                // Get current list of connected peers with addresses
                let connected_peers: Vec<String> = peer_list_discovered_peers
                    .iter()
                    .filter_map(|entry| {
                        let peer_id = *entry.key();
                        let (_timestamp, stored_addr) = entry.value().clone();
                        
                        // Include peer with address if available
                        // For peers without addresses (connected TO us), include them without address
                        // Other nodes can discover them via DHT or relay
                        if let Some(addr) = stored_addr {
                            Some(format!("{}@{}", peer_id, addr))
                        } else {
                            // Include peer ID without address - let DHT/relay handle discovery
                            Some(peer_id.to_string())
                        }
                    })
                    .collect();
                
                // Create signed peer announcement
                let announcement = PeerDiscoveryAnnouncement::new(
                    peer_list_node_id,
                    connected_peers.clone(),
                    crate::node_region::get_node_region(),
                    &peer_list_secret_key,
                );
                
                if let Ok(announcement_bytes) = serde_json::to_vec(&announcement) {
                    let sender = peer_list_sender.lock().await;
                    if let Err(e) = sender.broadcast(announcement_bytes.into()).await {
                        tracing::debug!("Peer list announcement broadcast error: {}", e);
                    } else {
                        tracing::debug!(
                            "📡 Broadcasted signed peer list: {} connected peers from region {}",
                            connected_peers.len(),
                            announcement.region
                        );
                    }
                }
            }
        });
        tracing::info!("🌐 Started peer discovery protocol with cryptographic signatures (broadcasts every 10s)");

        // Start peer expiration cleanup task - removes inactive peers every 10 seconds
        let cleanup_discovered_peers = Arc::clone(&self.discovered_peers);
        let expiration_timeout = std::time::Duration::from_secs(30); // 30 second timeout
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            loop {
                interval.tick().await;
                
                let now = chrono::Utc::now();
                let mut expired_count = 0;
                
                // Collect expired peers
                let expired_peers: Vec<EndpointId> = cleanup_discovered_peers
                    .iter()
                    .filter_map(|entry| {
                        let (last_seen, _addr) = entry.value();
                        let duration_since = now.signed_duration_since(*last_seen);
                        
                        if duration_since.num_seconds() > expiration_timeout.as_secs() as i64 {
                            Some(*entry.key())
                        } else {
                            None
                        }
                    })
                    .collect();
                
                // Remove expired peers
                for peer_id in expired_peers {
                    cleanup_discovered_peers.remove(&peer_id);
                    expired_count += 1;
                    tracing::info!("🕒 Removed expired peer: {}", peer_id);
                }
                
                if expired_count > 0 {
                    tracing::info!("🧹 Cleaned up {} expired peer(s)", expired_count);
                }
            }
        });
        tracing::info!("🧹 Started peer expiration cleanup (checks every 10s, timeout: 30s)");

        // Start a peer discovery broadcast task (only after successful gossip join)
        // This helps peers find each other by periodically sending discovery beacons
        let discovery_beacon_sender = Arc::clone(self.discovery_sender.as_ref().unwrap());
        let beacon_node_id = node_id;
        tokio::spawn(async move {
            // Wait a bit before starting beacons to allow gossip network to stabilize
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(15)); // Reduced frequency
            loop {
                interval.tick().await;
                
                // Broadcast a simple beacon message
                let beacon = serde_json::json!({
                    "type": "discovery_beacon",
                    "node_id": beacon_node_id.to_string(),
                    "timestamp": chrono::Utc::now().timestamp(),
                });
                
                if let Ok(beacon_bytes) = serde_json::to_vec(&beacon) {
                    let sender = discovery_beacon_sender.lock().await;
                    if let Err(e) = sender.broadcast(beacon_bytes.into()).await {
                        tracing::debug!("Discovery beacon broadcast error: {}", e);
                    } else {
                        tracing::trace!("Sent discovery beacon");
                    }
                }
            }
        });
        tracing::info!("🔍 Started peer discovery beacon (broadcasts every 10s)");

        // Process inbound network events and route to SyncManager
        {
            let event_rx = Arc::clone(&self.event_rx);
            let sync_manager = self.sync_manager.clone();
            let sync_sender = self.sync_sender.clone();
            let local_node_id = node_id;
            tokio::spawn(async move {
                loop {
                    // Receive next network event
                    let evt_opt = {
                        let mut rx = event_rx.write().await;
                        rx.recv().await
                    };
                    let Some(evt) = evt_opt else {
                        // Channel closed; exit task
                        break;
                    };
        
                    match evt {
                        NetworkEvent::Message { peer, data } => {
                            // Ignore our own messages
                            if peer == local_node_id {
                                continue;
                            }
                            // Parse payload as SyncMessage
                            match serde_json::from_slice::<crate::sync::SyncMessage>(&data) {
                                Ok(sync_msg) => {
                                    if let Some(manager) = sync_manager.as_ref() {
                                        match manager.handle_sync_message(sync_msg, peer).await {
                                            Ok(Some(response)) => {
                                                if let Some(sender) = &sync_sender {
                                                    if let Ok(payload) = serde_json::to_vec(&response) {
                                                        if let Err(e) = sender.lock().await.broadcast(payload.into()).await {
                                                            tracing::error!("Failed to broadcast sync response to {}: {}", peer, e);
                                                        }
                                                    }
                                                }
                                            }
                                            Ok(None) => {}
                                            Err(e) => {
                                                tracing::error!("Failed to handle sync message from {}: {}", peer, e);
                                            }
                                        }
                                    } else {
                                        tracing::warn!("SyncManager not attached; dropping inbound sync message");
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to parse inbound sync payload as SyncMessage: {}", e);
                                }
                            }
                        }
                        NetworkEvent::PeerDiscovered { peer } => {
                            // Initiate sync with newly discovered peer
                            if let Some(manager) = sync_manager.as_ref() {
                                let op_count = manager.sync_store().operation_count().await;
                                if op_count == 0 {
                                    tracing::info!("Initiated peer discovery with {}; requesting full sync (empty local store)", peer);
                                    match manager.request_full_sync(peer).await {
                                        Ok(request) => {
                                            if let Some(sender) = &sync_sender {
                                                if let Ok(payload) = serde_json::to_vec(&request) {
                                                    if let Err(e) = sender.lock().await.broadcast(payload.into()).await {
                                                        tracing::error!("Failed to broadcast full sync request to {}: {}", peer, e);
                                                    } else {
                                                        tracing::info!("Initiated bootstrap full sync with {}", peer);
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => tracing::error!("Failed to create full sync request: {}", e),
                                    }
                                } else {
                                    let since_ts = manager.sync_store().last_applied_timestamp().await.unwrap_or(0);
                                    tracing::info!("Peer discovered {}; requesting incremental sync since {} ({} local ops)", peer, since_ts, op_count);
                                    match manager.request_incremental_sync(peer, since_ts).await {
                                        Ok(request) => {
                                            if let Some(sender) = &sync_sender {
                                                if let Ok(payload) = serde_json::to_vec(&request) {
                                                    if let Err(e) = sender.lock().await.broadcast(payload.into()).await {
                                                        tracing::error!("Failed to broadcast incremental sync request to {}: {}", peer, e);
                                                    } else {
                                                        tracing::info!("Initiated incremental sync with {} since {}", peer, since_ts);
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => tracing::error!("Failed to create incremental sync request: {}", e),
                                    }
                                }
                            }
                        }
                        NetworkEvent::PeerExpired { peer } => {
                            tracing::info!("Peer expired: {}", peer);
                        }
                    }
                }
            });
        }
        // Get references for the event loop (no clones needed - already Arc-wrapped)
        let data_sender_clone = self.data_sender.clone().unwrap();
        let libp2p_to_mqtt_tx = &self.libp2p_to_mqtt_tx;
        let event_tx = &self.event_tx;
        let discovered_peers = &self.discovered_peers;
        let peer_announcement_cache = &self.peer_announcement_cache;
        let endpoint_for_dial = &self.endpoint;

        // Convert receivers to streams (no need to Box them - they're already streamable)
        let mut data_stream = data_receiver;
        let mut discovery_stream = discovery_receiver;
        let mut sync_stream = sync_receiver;
        let mut peer_discovery_stream = peer_discovery_receiver;

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
                                event_tx,
                                libp2p_to_mqtt_tx,
                                discovered_peers,
                                &data_sender_clone,
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
                                event_tx,
                                libp2p_to_mqtt_tx,
                                discovered_peers,
                                &data_sender_clone,
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
                            if let Err(e) = Self::handle_sync_event(event, node_id, event_tx, discovered_peers).await {
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

                // Handle peer discovery announcements
                event_result = peer_discovery_stream.next() => {
                    match event_result {
                        Some(Ok(event)) => {
                            if let Err(e) = Self::handle_peer_discovery_event(
                                event,
                                node_id,
                                discovered_peers,
                                peer_announcement_cache,
                                endpoint_for_dial,
                            ).await {
                                tracing::error!("Error handling peer discovery event: {}", e);
                            }
                        }
                        Some(Err(e)) => {
                            tracing::error!("Error reading peer discovery stream: {}", e);
                        }
                        None => {
                            tracing::warn!("Peer discovery stream ended");
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
                // Handle outbound sync messages submitted by other components (GraphQL, etc.)
                Some(sync_msg) = async {
                    if let Some(ref mut rx) = self.sync_outbound_rx {
                        rx.recv().await
                    } else {
                        None
                    }
                } => {
                    // Broadcast the sync message to peers
                    tracing::debug!("IrohNetwork: received outbound sync message to broadcast");
                    // Log operation id if this is an operation message for easier tracing
                    match &sync_msg {
                        crate::sync::SyncMessage::Operation { operation } => {
                            tracing::debug!("IrohNetwork: broadcasting operation {} to sync topic", operation.op_id);
                        }
                        _ => {
                            tracing::debug!("IrohNetwork: broadcasting non-operation sync message");
                        }
                    }

                    if let Err(e) = self.broadcast_sync(sync_msg).await {
                        tracing::error!("IrohNetwork: Failed to broadcast outbound sync message: {}", e);
                    } else {
                        tracing::debug!("IrohNetwork: outbound sync message broadcast completed");
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle sync protocol events
    async fn handle_sync_event(
        event: GossipEvent, 
        node_id: EndpointId,
        event_tx: &mpsc::UnboundedSender<NetworkEvent>,
        discovered_peers: &Arc<dashmap::DashMap<EndpointId, (chrono::DateTime<chrono::Utc>, Option<std::net::SocketAddr>)>>,
    ) -> Result<()> {
                match event {
                    GossipEvent::Received(msg) => {
                        let from = msg.delivered_from;
                        // Use info level so remote nodes will show receipt in default log level
                        tracing::info!("📨 Received sync message from {} ({} bytes)", from, msg.content.len());

                // Ignore our own messages
                if from == node_id {
                    return Ok(());
                }
                
                // Track this peer (update timestamp, keep existing address if any)
                discovered_peers.entry(from).and_modify(|(ts, _addr)| *ts = chrono::Utc::now())
                    .or_insert((chrono::Utc::now(), None));

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
                discovered_peers.entry(peer_node_id).and_modify(|(ts, _addr)| *ts = chrono::Utc::now())
                    .or_insert((chrono::Utc::now(), None));
                
                let _ = event_tx.send(NetworkEvent::PeerDiscovered { peer: peer_node_id });
            }
                GossipEvent::NeighborDown(peer_node_id) => {
                tracing::info!("Sync neighbor down: {}", peer_node_id);
                
                // Don't send PeerExpired event - let the expiration cleanup task handle it
                // This prevents confusing "Peer expired" logs on temporary disconnects
            }
            GossipEvent::Lagged => {
                tracing::warn!("Sync gossip lagged - missed messages");
            }
        }

        Ok(())
    }

    /// Handle peer discovery announcement events
    /// Processes peer lists from other nodes and automatically connects to unknown peers
    async fn handle_peer_discovery_event(
        event: GossipEvent,
        node_id: EndpointId,
        discovered_peers: &Arc<dashmap::DashMap<EndpointId, (chrono::DateTime<chrono::Utc>, Option<std::net::SocketAddr>)>>,
        peer_announcement_cache: &Arc<dashmap::DashMap<String, i64>>,
        endpoint: &Endpoint,
    ) -> Result<()> {
        match event {
            GossipEvent::Received(msg) => {
                let from = msg.delivered_from;
                
                // Ignore our own messages
                if from == node_id {
                    return Ok(());
                }
                
                // Parse peer discovery announcement
                match serde_json::from_slice::<PeerDiscoveryAnnouncement>(&msg.content) {
                    Ok(announcement) => {
                        // Verify the announcing node ID matches the sender
                        let announced_node_id = match announcement.node_id.parse::<EndpointId>() {
                            Ok(id) => id,
                            Err(e) => {
                                tracing::warn!("Invalid node ID in announcement: {}", e);
                                return Ok(());
                            }
                        };
                        
                        if announced_node_id != from {
                            tracing::warn!(
                                "Node ID mismatch: announced {} but message from {}",
                                announced_node_id,
                                from
                            );
                            return Ok(());
                        }
                        
                        // Get the public key for signature verification
                        let public_key = iroh::PublicKey::from(from);
                        
                        // Verify the signature
                        if !announcement.verify(from, &public_key) {
                            tracing::warn!(
                                "Invalid signature from {} - ignoring announcement",
                                from
                            );
                            return Ok(());
                        }
                        
                        // Check if we've seen this announcement recently (deduplication)
                        let cache_key = format!("{}:{}", announcement.node_id, announcement.timestamp);
                        if let Some(last_ts) = peer_announcement_cache.get(&announcement.node_id) {
                            if *last_ts >= announcement.timestamp {
                                // Already processed this or a newer announcement
                                return Ok(());
                            }
                        }
                        
                        // Update cache
                        peer_announcement_cache.insert(announcement.node_id.clone(), announcement.timestamp);
                        
                        // Track the announcing peer (update timestamp, keep existing address if any)
                        discovered_peers.entry(from).and_modify(|(ts, _addr)| *ts = chrono::Utc::now())
                            .or_insert((chrono::Utc::now(), None));
                        
                        tracing::info!(
                            "📋 Received verified peer list from {} (region: {}): {} peers",
                            announcement.node_id,
                            announcement.region,
                            announcement.connected_peers.len()
                        );
                        
                        // Attempt to connect to unknown peers
                        let mut new_connections = 0;
                        
                        // Get our relay URL for fallback connections
                        let our_relay_url = endpoint.addr().relay_urls().next().cloned();
                        
                        for peer_addr_str in &announcement.connected_peers {
                            // Parse peer address in format "peerId@ip:port" or just "peerId"
                            if let Some(at_idx) = peer_addr_str.find('@') {
                                // Format: peerId@ip:port
                                let peer_id_str = &peer_addr_str[..at_idx];
                                let socket_addr_str = &peer_addr_str[at_idx + 1..];
                                
                                if let (Ok(peer_id), Ok(socket_addr)) = (
                                    peer_id_str.parse::<EndpointId>(),
                                    socket_addr_str.parse::<std::net::SocketAddr>()
                                ) {
                                    // Skip if it's our own ID
                                    if peer_id == node_id {
                                        continue;
                                    }
                                    
                                    // Skip if already connected
                                    if discovered_peers.contains_key(&peer_id) {
                                        continue;
                                    }
                                    
                                    // Attempt to connect to this peer with explicit address + relay fallback
                                    tracing::info!("🔗 Attempting to connect to peer {} at {} (discovered via {})", 
                                        peer_id, socket_addr, announcement.node_id);
                                    
                                    // Build addresses: direct IP + relay fallback
                                    let mut addrs: Vec<TransportAddr> = vec![TransportAddr::Ip(socket_addr)];
                                    if let Some(ref relay_url) = our_relay_url {
                                        addrs.push(TransportAddr::Relay(relay_url.clone()));
                                    }
                                    let peer_addr = EndpointAddr::from_parts(peer_id, addrs);
                                    
                                    match endpoint.connect(peer_addr, iroh_gossip::ALPN).await {
                                        Ok(_conn) => {
                                            discovered_peers.insert(peer_id, (chrono::Utc::now(), Some(socket_addr)));
                                            new_connections += 1;
                                            tracing::info!("✓ Successfully connected to peer {} at {}", peer_id, socket_addr);
                                        }
                                        Err(e) => {
                                            tracing::debug!("Failed to connect to peer {} at {}: {}", peer_id, socket_addr, e);
                                        }
                                    }
                                }
                            } else {
                                // Format: just peerId (no address) - try DHT/relay discovery
                                if let Ok(peer_id) = peer_addr_str.parse::<EndpointId>() {
                                    // Skip if it's our own ID
                                    if peer_id == node_id {
                                        continue;
                                    }
                                    
                                    // Skip if already connected
                                    if discovered_peers.contains_key(&peer_id) {
                                        continue;
                                    }
                                    
                                    // Attempt to connect via DHT/relay (no explicit address)
                                    tracing::info!("🔗 Attempting to connect to peer {} via DHT/relay (discovered via {})", 
                                        peer_id, announcement.node_id);
                                    
                                    match endpoint.connect(peer_id, iroh_gossip::ALPN).await {
                                        Ok(_conn) => {
                                            discovered_peers.insert(peer_id, (chrono::Utc::now(), None));
                                            new_connections += 1;
                                            tracing::info!("✓ Successfully connected to peer {} via DHT/relay", peer_id);
                                        }
                                        Err(e) => {
                                            tracing::debug!("Failed to connect to peer {} via DHT: {}", peer_id, e);
                                        }
                                    }
                                }
                            }
                        }
                        
                        if new_connections > 0 {
                            tracing::info!("✓ Established {} new peer connection(s) via discovery", new_connections);
                        }
                    }
                    Err(e) => {
                        tracing::debug!("Failed to parse peer discovery announcement: {}", e);
                    }
                }
            }
            GossipEvent::NeighborUp(peer_node_id) => {
                tracing::debug!("Peer discovery neighbor up: {}", peer_node_id);
                discovered_peers.entry(peer_node_id).and_modify(|(ts, _addr)| *ts = chrono::Utc::now())
                    .or_insert((chrono::Utc::now(), None));
            }
            GossipEvent::NeighborDown(peer_node_id) => {
                tracing::debug!("Peer discovery neighbor down: {}", peer_node_id);
                // Don't remove immediately - let the expiration cleanup task handle it
            }
            GossipEvent::Lagged => {
                tracing::warn!("Peer discovery gossip lagged - missed messages");
            }
        }
        
        Ok(())
    }

    /// Handle gossip protocol events
    async fn handle_gossip_event(
        event: GossipEvent, 
        topic_type: &str,
        node_id: EndpointId,
        event_tx: &mpsc::UnboundedSender<NetworkEvent>,
        libp2p_to_mqtt_tx: &Option<mpsc::UnboundedSender<GossipToMqttMessage>>,
        discovered_peers: &Arc<dashmap::DashMap<EndpointId, (chrono::DateTime<chrono::Utc>, Option<std::net::SocketAddr>)>>,
        data_sender: &Arc<Mutex<GossipSender>>,
    ) -> Result<()> {
        match event {
            GossipEvent::Received(msg) => {
                let from = msg.delivered_from;
                tracing::info!("📡 Received gossip message from {} on {} topic ({} bytes)", from, topic_type, msg.content.len());

                // Ignore our own messages
                if from == node_id {
                    return Ok(());
                }
                
                // Track this peer (update timestamp, keep existing address if any)
                discovered_peers.entry(from).and_modify(|(ts, _addr)| *ts = chrono::Utc::now())
                    .or_insert((chrono::Utc::now(), None));

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
                            
                            // Check if this is a fetch latency request
                            if let Some(ref topic) = gossip_msg.topic {
                                if topic == "fetch-latency-request" {
                                    tracing::info!("⏱️  Received fetch-latency-request");
                                    // Handle fetch latency request in a separate task
                                    let data = gossip_msg.payload.clone();
                                    let sender = data_sender.clone();
                                    let node_id_str = node_id.to_string();
                                    tokio::spawn(async move {
                                        if let Err(e) = Self::handle_fetch_latency_request(data, sender, node_id_str).await {
                                            tracing::error!("Failed to handle fetch latency request: {}", e);
                                        }
                                    });
                                    return Ok(());
                                }
                            }
                            
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
            GossipEvent::NeighborUp(peer_node_id) => {
                tracing::info!("Neighbor up: {}", peer_node_id);
                
                // Track this peer immediately when they connect
                discovered_peers.entry(peer_node_id).and_modify(|(ts, _addr)| *ts = chrono::Utc::now())
                    .or_insert((chrono::Utc::now(), None));
                
                let _ = event_tx.send(NetworkEvent::PeerDiscovered { peer: peer_node_id });
            }
            GossipEvent::NeighborDown(peer_node_id) => {
                tracing::info!("Neighbor down: {}", peer_node_id);
                
                // Don't send PeerExpired event - let the expiration cleanup task handle it
                // This prevents confusing "Peer expired" logs on temporary disconnects
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
        node_id: EndpointId,
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

        let endpoint_addr = self.endpoint.addr();
        let relay_url = endpoint_addr
            .relay_urls()
            .next()
            .map(|u| u.to_string());
        let ip_addresses: Vec<String> = endpoint_addr
            .ip_addrs()
            .map(|addr| addr.to_string())
            .collect();
        
        let announcement = serde_json::json!({
            "node_id": self.node_id.to_string(),
            "relay_url": relay_url,
            "ip_addresses": ip_addresses,
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
        let len = payload.len();
        
        // Log operation details if it's an operation message
        match &sync_msg {
            crate::sync::SyncMessage::Operation { operation } => {
                tracing::info!(
                    "📤 Broadcasting operation {} (db: {}, key: {}, type: {}, {} bytes)",
                    operation.op_id, operation.db_name, operation.key, operation.store_type, len
                );
            }
            crate::sync::SyncMessage::SyncRequest { requester, since_timestamp } => {
                tracing::info!(
                    "📤 Broadcasting sync request from {} (since: {:?}, {} bytes)",
                    requester, since_timestamp, len
                );
            }
            crate::sync::SyncMessage::SyncResponse { requester, operations, .. } => {
                tracing::info!(
                    "📤 Broadcasting sync response to {} ({} operations, {} bytes)",
                    requester, operations.len(), len
                );
            }
        }
        
        sender.lock().await.broadcast(payload.into()).await?;

        tracing::debug!("✓ Broadcast complete");
        Ok(())
    }
    
    /// Get sync sender for external use
    pub fn sync_sender(&self) -> Option<Arc<Mutex<GossipSender>>> {
        self.sync_sender.clone()
    }

    /// Handle fetch latency request with signature verification
    async fn handle_fetch_latency_request(
        data: Vec<u8>,
        data_sender: Arc<Mutex<GossipSender>>,
        node_id: String,
    ) -> Result<()> {
        use std::time::Instant;
        
        // Whitelist of allowed public keys (same as TypeScript version)
        const WHITELISTED_KEYS: &[&str] = &[
            "efcfe1ac4de7bcb991d8b08a7d8ebed2377a6ed1070636dc66d9cdd225458aaa"
        ];
        
        // Parse the outer request structure with signature
        #[derive(serde::Deserialize)]
        struct SignedRequest {
            data: LatencyRequestData,
            sig: String,
            pubkey: String,
        }
        
        // Parse the request
        #[derive(serde::Deserialize, serde::Serialize)]
        struct LatencyRequestData {
            request_id: String,
            url: String,
            method: Option<String>,
            #[serde(default)]
            headers: std::collections::HashMap<String, String>,
            body: Option<String>,
        }

        #[derive(serde::Serialize)]
        struct LatencyResponse {
            request_id: String,
            status: u16,
            #[serde(rename = "statusText")]
            status_text: String,
            latency: f64,
            #[serde(rename = "nodeRegion")]
            node_region: Option<String>,
            #[serde(rename = "nodeId")]
            node_id: String,
            error: Option<String>,
        }

        let signed_request: SignedRequest = match serde_json::from_slice(&data) {
            Ok(req) => req,
            Err(e) => {
                tracing::error!("Failed to parse latency request: {}", e);
                return Err(anyhow::anyhow!("Invalid request format: {}", e));
            }
        };

        let request_id = signed_request.data.request_id.clone();

        // Verify public key is whitelisted
        if !WHITELISTED_KEYS.contains(&signed_request.pubkey.as_str()) {
            tracing::warn!("⚠️  Public key not whitelisted: {}", signed_request.pubkey);
            
            let response = LatencyResponse {
                request_id,
                status: 403,
                status_text: "Forbidden".to_string(),
                latency: 0.0,
                node_region: Some(super::node_region::get_node_region()),
                node_id: node_id.clone(),
                error: Some("Public key not whitelisted".to_string()),
            };
            
            Self::publish_latency_response(response, data_sender, node_id).await?;
            return Ok(());
        }

        // Verify signature
        let public_key_bytes = match hex::decode(&signed_request.pubkey) {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::error!("Failed to decode public key: {}", e);
                let response = LatencyResponse {
                    request_id,
                    status: 403,
                    status_text: "Forbidden".to_string(),
                    latency: 0.0,
                    node_region: Some(super::node_region::get_node_region()),
                    node_id: node_id.clone(),
                    error: Some("Invalid public key format".to_string()),
                };
                Self::publish_latency_response(response, data_sender, node_id).await?;
                return Ok(());
            }
        };

        let signature_bytes = match hex::decode(&signed_request.sig) {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::error!("Failed to decode signature: {}", e);
                let response = LatencyResponse {
                    request_id,
                    status: 403,
                    status_text: "Forbidden".to_string(),
                    latency: 0.0,
                    node_region: Some(super::node_region::get_node_region()),
                    node_id: node_id.clone(),
                    error: Some("Invalid signature format".to_string()),
                };
                Self::publish_latency_response(response, data_sender, node_id).await?;
                return Ok(());
            }
        };

        // Serialize the data for verification (must match how it was signed)
        let message = serde_json::to_vec(&signed_request.data)?;

        if let Err(e) = crate::crypto::verify_signature(&public_key_bytes, &message, &signature_bytes) {
            tracing::error!("❌ Signature verification failed: {}", e);
            let response = LatencyResponse {
                request_id,
                status: 403,
                status_text: "Forbidden".to_string(),
                latency: 0.0,
                node_region: Some(super::node_region::get_node_region()),
                node_id: node_id.clone(),
                error: Some("Invalid signature".to_string()),
            };
            Self::publish_latency_response(response, data_sender, node_id).await?;
            return Ok(());
        }

        tracing::info!("✅ Signature verified for request {}", request_id);
        tracing::info!("⏱️  Processing latency request {} for URL: {}", request_id, signed_request.data.url);

        // Build HTTP client request
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        let method = signed_request.data.method.as_deref().unwrap_or("GET").to_uppercase();
        let method = match method.as_str() {
            "GET" => reqwest::Method::GET,
            "POST" => reqwest::Method::POST,
            "PUT" => reqwest::Method::PUT,
            "PATCH" => reqwest::Method::PATCH,
            "DELETE" => reqwest::Method::DELETE,
            _ => reqwest::Method::GET,
        };

        let mut req_builder = client.request(method, &signed_request.data.url);
        
        // Add headers
        for (key, value) in signed_request.data.headers {
            req_builder = req_builder.header(&key, &value);
        }

        // Add body if present
        if let Some(body) = signed_request.data.body {
            req_builder = req_builder.body(body);
        }

        // Measure latency
        let start_time = Instant::now();
        let result = req_builder.send().await;
        let latency_ms = start_time.elapsed().as_secs_f64() * 1000.0;

        let response = match result {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let status_text = resp.status().canonical_reason().unwrap_or("Unknown").to_string();
                
                // Consume response body to complete the request
                let _ = resp.text().await;
                
                tracing::info!("✅ Latency request {} completed: {} ms (status: {})", 
                    request_id, latency_ms, status);

                LatencyResponse {
                    request_id: request_id.clone(),
                    status,
                    status_text,
                    latency: latency_ms,
                    node_region: Some(super::node_region::get_node_region()),
                    node_id: node_id.clone(),
                    error: None,
                }
            }
            Err(e) => {
                tracing::error!("❌ Latency request {} failed: {}", request_id, e);
                
                LatencyResponse {
                    request_id: request_id.clone(),
                    status: 0,
                    status_text: "Error".to_string(),
                    latency: latency_ms,
                    node_region: Some(super::node_region::get_node_region()),
                    node_id: node_id.clone(),
                    error: Some(e.to_string()),
                }
            }
        };

        Self::publish_latency_response(response, data_sender, node_id).await?;
        Ok(())
    }

    /// Publish latency response to api-latency topic
    async fn publish_latency_response(
        response: impl serde::Serialize,
        data_sender: Arc<Mutex<GossipSender>>,
        node_id: String,
    ) -> Result<()> {
        // Publish response to api-latency topic
        let response_msg = GossipMessage {
            origin: "local".to_string(),
            broker: node_id.clone(),
            timestamp: chrono::Utc::now().timestamp(),
            message_id: uuid::Uuid::new_v4().to_string(),
            topic: Some("api-latency".to_string()),
            payload: serde_json::to_vec(&response)?,
        };

        let payload = serde_json::to_vec(&response_msg)?;
        data_sender.lock().await.broadcast(payload.into()).await?;
        
        tracing::info!("📤 Published api-latency response");
        Ok(())
    }

    /// Get event receiver (for compatibility with old API)
    pub async fn event_receiver(&self) -> mpsc::UnboundedReceiver<NetworkEvent> {
        // This is a bit hacky but needed for API compatibility
        // In production, consider refactoring to use a broadcast channel
    let (_tx, rx) = mpsc::unbounded_channel();
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
    pub async fn get_connected_peers(&self) -> Vec<(EndpointId, chrono::DateTime<chrono::Utc>)> {
        self.discovered_peers
            .iter()
            .map(|entry| {
                let (timestamp, _addr) = entry.value();
                (*entry.key(), *timestamp)
            })
            .collect()
    }

    /// Get list of discovered peers (same as connected for Iroh)
    pub async fn get_discovered_peers(&self) -> Vec<(EndpointId, chrono::DateTime<chrono::Utc>)> {
        self.get_connected_peers().await
    }
    
    /// Get a cloneable reference to the discovered peers map  
    pub fn discovered_peers_map(&self) -> Arc<dashmap::DashMap<EndpointId, (chrono::DateTime<chrono::Utc>, Option<std::net::SocketAddr>)>> {
        self.discovered_peers.clone()
    }

    /// Get peers from improved discovery system with detailed info
    pub fn get_improved_discovery_peers(&self) -> Vec<(EndpointId, PeerInfo)> {
        self.improved_discovery_peers
            .iter()
            .map(|entry| (*entry.key(), entry.value().clone()))
            .collect()
    }

    /// Get count of peers from improved discovery
    pub fn improved_discovery_peer_count(&self) -> usize {
        self.improved_discovery_peers.len()
    }

    /// Get peers by region from improved discovery
    pub fn get_peers_by_region(&self, region: &str) -> Vec<EndpointId> {
        self.improved_discovery_peers
            .iter()
            .filter(|entry| entry.value().region == region)
            .map(|entry| *entry.key())
            .collect()
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
// Re-export for compatibility when needed. Keep commented to avoid unused import warnings.
// pub use iroh::NodeId as PeerId;
