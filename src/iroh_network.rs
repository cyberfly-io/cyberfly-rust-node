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

// Shared hardcoded bootstrap (used when no bootstrap peers configured).
pub const HARDCODED_BOOTSTRAP: &str = "04b754ba2a3da0970d72d08b8740fb2ad96e63cf8f8bef6b7f1ab84e5b09a7f8@67.211.219.34:31001";

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
    sync_topic: TopicId,  // Topic for data sync
    discovery_topic: TopicId,  // Unified discovery topic (postcard + ed25519)
    latency_topic: TopicId,  // Topic for fetch-latency-request
    // Senders for broadcasting (set after subscribing)
    data_sender: Option<Arc<Mutex<GossipSender>>>,
    sync_sender: Option<Arc<Mutex<GossipSender>>>,  // Sync topic sender
    latency_sender: Option<Arc<Mutex<GossipSender>>>,  // Latency topic sender
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
    // Optional network resilience manager (circuit breaker, reputation, bandwidth)
    resilience: Option<Arc<crate::network_resilience::NetworkResilience>>,
    // Per-peer connect backoff state: (failure_count, next_allowed_attempt)
    peer_backoff: Arc<dashmap::DashMap<EndpointId, (u32, chrono::DateTime<chrono::Utc>)>>,
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
        let sync_topic = TopicId::from_bytes(*b"decentralized-db-sync-v1-iroh!!!");
        // Unified discovery topic using postcard + ed25519 signatures
        let discovery_topic = TopicId::from_bytes(*b"cyberfly-discovery-v2-postcard!!");
        // Latency request topic for fetch-latency-request API
        let latency_topic = TopicId::from_bytes(*b"cyberfly-fetch-latency-request!!");
        
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
            sync_topic,
            discovery_topic,
            latency_topic,
            data_sender: None,
            sync_sender: None,
            latency_sender: None,
            improved_discovery_peers: Arc::new(dashmap::DashMap::new()),
            discovered_peers: Arc::new(dashmap::DashMap::new()),
            peer_announcement_cache: Arc::new(dashmap::DashMap::new()),
            bootstrap_peers,
            bootstrap_peer_strings,
            resilience: None,
            peer_backoff: Arc::new(dashmap::DashMap::new()),
        }
    }

    /// Attach a `NetworkResilience` manager so the network can consult
    /// circuit breakers / reputation before attempting connections.
    pub fn attach_resilience(&mut self, resilience: Arc<crate::network_resilience::NetworkResilience>) {
        self.resilience = Some(resilience);
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
        // Metric: record connection attempt
        crate::metrics::PEER_CONNECTIONS_TOTAL.inc();
        
        // Use gossip ALPN for peer connections
        let alpn = iroh_gossip::ALPN;
        
        // Add the peer to the endpoint's address book
        // The endpoint will attempt to establish a connection
        let conn = match self.endpoint.connect(peer_id, alpn).await {
            Ok(c) => c,
            Err(e) => {
                crate::metrics::PEER_CONNECTION_FAILURES.inc();
                return Err(anyhow::anyhow!("Failed to connect to peer {}: {}", peer_id, e));
            }
        };
        
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
            
            // Add bootstrap peers to discovered_peers immediately (they should never expire)
            let now = chrono::Utc::now();
            for (node_id, socket_addr) in &connected_bootstrap_peers {
                self.discovered_peers.insert(*node_id, (now, Some(*socket_addr)));
            }
            tracing::info!("✓ Added {} bootstrap peer(s) to discovered_peers", connected_bootstrap_peers.len());
            
            // Start connection monitor for bootstrap peers
            let endpoint = self.endpoint.clone();
            let discovered_peers = Arc::clone(&self.discovered_peers);
            tokio::spawn(async move {
                Self::monitor_bootstrap_connections(endpoint, connected_bootstrap_peers, discovered_peers).await;
            });
        } else {
            tracing::warn!("⚠️  Failed to connect to any bootstrap peers - will rely on DHT/mDNS discovery");
        }
        
        Ok(())
    }
    
    /// Monitor bootstrap peer connections and reconnect if disconnected
    /// Also keeps bootstrap peers in discovered_peers to prevent them from being used for peer sharing
    async fn monitor_bootstrap_connections(
        endpoint: Endpoint,
        bootstrap_peers: Vec<(EndpointId, std::net::SocketAddr)>,
        discovered_peers: Arc<dashmap::DashMap<EndpointId, (chrono::DateTime<chrono::Utc>, Option<std::net::SocketAddr>)>>,
    ) {
        const CHECK_INTERVAL_SECS: u64 = 30; // Check every 30 seconds
        const RECONNECT_DELAY_SECS: u64 = 3; // Wait 3 seconds before reconnecting
        
        tracing::info!("🔍 Started bootstrap connection monitor (checks every {}s) - reconnects only when isolated", 
            CHECK_INTERVAL_SECS);
        
        let mut check_interval = tokio::time::interval(std::time::Duration::from_secs(CHECK_INTERVAL_SECS));
        
        loop {
            check_interval.tick().await;
            
            // Check ACTUAL connection states, not just discovered_peers map
            // discovered_peers can have stale entries between NeighborDown and cleanup
            let active_peer_count = discovered_peers.len();
            
            // Double-check with actual connection types for bootstrap peers
            let connected_bootstrap_count = bootstrap_peers.iter().filter(|(node_id, _)| {
                if let Some(mut watcher) = endpoint.conn_type(*node_id) {
                    use iroh::endpoint::ConnectionType;
                    !matches!(watcher.get(), ConnectionType::None)
                } else {
                    false
                }
            }).count();
            
            // If we have ANY active peers or any connected bootstrap nodes, attempt
            // to reconnect only the disconnected bootstrap peers instead of treating
            // the node as isolated. This reduces reconnection blast and keeps
            // bootstrap links resilient per-peer.
            if active_peer_count > 0 || connected_bootstrap_count > 0 {
                tracing::trace!("✓ Node has {} active peers, {} connected bootstrap nodes", 
                    active_peer_count, connected_bootstrap_count);

                // For each bootstrap peer, if it's disconnected, spawn a reconnect task
                for (node_id, socket_addr) in &bootstrap_peers {
                    let endpoint_clone = endpoint.clone();
                    let node_id_clone = *node_id;
                    let socket_addr_clone = *socket_addr;
                    let discovered_peers_clone = Arc::clone(&discovered_peers);

                    // Check connection type; if disconnected, attempt reconnect
                    let is_connected = if let Some(mut watcher) = endpoint.conn_type(*node_id) {
                        use iroh::endpoint::ConnectionType;
                        !matches!(watcher.get(), ConnectionType::None)
                    } else {
                        false
                    };

                    if !is_connected {
                        tracing::warn!(%node_id_clone, "Bootstrap peer disconnected - attempting reconnect");
                        tokio::spawn(async move {
                            // Small delay to avoid immediate retry storms
                            tokio::time::sleep(std::time::Duration::from_secs(RECONNECT_DELAY_SECS)).await;
                            match Self::connect_bootstrap_peer_with_retry(
                                endpoint_clone,
                                node_id_clone,
                                socket_addr_clone,
                            ).await {
                                Ok(_) => {
                                    tracing::info!("✅ Reconnected to bootstrap peer {}", node_id_clone.fmt_short());
                                    discovered_peers_clone.insert(
                                        node_id_clone,
                                        (chrono::Utc::now(), Some(socket_addr_clone)),
                                    );
                                }
                                Err(e) => {
                                    tracing::error!("❌ Failed to reconnect to bootstrap peer {}: {}", node_id_clone.fmt_short(), e);
                                }
                            }
                        });
                    }
                }

                // Continue monitoring loop
                continue;
            }

            // Node is fully isolated - reconnect to all bootstrap peers
            tracing::warn!("⚠️  Node is ISOLATED (0 active peers, 0 bootstrap connections) - reconnecting to bootstrap nodes");

            for (node_id, socket_addr) in &bootstrap_peers {
                tokio::time::sleep(std::time::Duration::from_secs(RECONNECT_DELAY_SECS)).await;

                let endpoint_clone = endpoint.clone();
                let node_id_clone = *node_id;
                let socket_addr_clone = *socket_addr;
                let discovered_peers_clone = Arc::clone(&discovered_peers);

                tokio::spawn(async move {
                    match Self::connect_bootstrap_peer_with_retry(
                        endpoint_clone,
                        node_id_clone,
                        socket_addr_clone,
                    ).await {
                        Ok(_) => {
                            tracing::info!(
                                "✅ Reconnected to bootstrap peer {}",
                                node_id_clone.fmt_short()
                            );
                            discovered_peers_clone.insert(
                                node_id_clone, 
                                (chrono::Utc::now(), Some(socket_addr_clone))
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                "❌ Failed to reconnect to bootstrap peer {}: {}",
                                node_id_clone.fmt_short(),
                                e
                            );
                        }
                    }
                });
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
            
            // Metric: record connection attempt
            crate::metrics::PEER_CONNECTIONS_TOTAL.inc();
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
                        crate::metrics::PEER_CONNECTION_FAILURES.inc();
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
        
        // Subscribe to sync topic with bootstrap peers
        let sync_topic = self.gossip.subscribe(self.sync_topic, bootstrap_peers.clone()).await?;
        let (sync_sender, sync_receiver) = sync_topic.split();
        self.sync_sender = Some(Arc::new(Mutex::new(sync_sender)));
        tracing::info!("Subscribed to sync topic");
        
        // Subscribe to latency topic for fetch-latency-request API
        let latency_topic = self.gossip.subscribe(self.latency_topic, bootstrap_peers.clone()).await?;
        let (latency_sender, latency_receiver) = latency_topic.split();
        self.latency_sender = Some(Arc::new(Mutex::new(latency_sender)));
        tracing::info!("Subscribed to fetch-latency-request topic");

        // Initialize unified gossip discovery (postcard + ed25519 signatures)
        // This is the only discovery mechanism - simple and secure
        let (mut improved_sender, mut improved_receiver) = GossipDiscoveryBuilder::new()
            .with_expiration_timeout(std::time::Duration::from_secs(300))
            .with_broadcast_interval(std::time::Duration::from_secs(5))
            .build(
                self.gossip.clone(),
                self.discovery_topic,
                bootstrap_peers.clone(),
                &self.endpoint,
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to initialize discovery: {}", e))?;
        tracing::info!("✓ Initialized gossip discovery (postcard + ed25519)");

        // Store reference to discovery peer map
        let improved_peers_ref = Arc::clone(&improved_receiver.neighbor_map);
        self.improved_discovery_peers = improved_peers_ref;

        // Start cleanup task for `discovered_peers` to avoid unbounded growth.
        // Mirrors behavior of `DiscoveryReceiver::start_cleanup_task` and
        // removes entries older than `DISCOVERED_PEER_EXPIRY_SECS`.
        {
            let discovered_peers = Arc::clone(&self.discovered_peers);
            const DISCOVERED_PEER_EXPIRY_SECS: i64 = 300; // 5 minutes
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
                loop {
                    interval.tick().await;
                    let now = chrono::Utc::now();
                    let expired: Vec<EndpointId> = discovered_peers
                        .iter()
                        .filter_map(|entry| {
                            if now.signed_duration_since(entry.value().0).num_seconds() > DISCOVERED_PEER_EXPIRY_SECS {
                                Some(*entry.key())
                            } else {
                                None
                            }
                        })
                        .collect();

                    let mut removed = 0;
                    for peer_id in expired {
                        if discovered_peers.remove(&peer_id).is_some() {
                            removed += 1;
                            crate::metrics::PEER_EXPIRATIONS.inc();
                            tracing::info!(%peer_id, "Cleanup: removed expired discovered peer");
                        }
                    }

                    if removed > 0 {
                        tracing::debug!(count = removed, "Removed expired discovered peers");
                    }
                    crate::metrics::NETWORK_PEERS.set(discovered_peers.len() as i64);
                }
            });
        }

        // Wait for peers to join before starting broadcasts (following iroh-gossip best practices)
        if !bootstrap_peers.is_empty() {
            tracing::info!("Waiting for peers to join gossip network...");
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            tracing::info!("✓ Gossip network initialization complete");
        }

        // Get node_id first before using in beacon task
        let node_id = self.node_id;
        let secret_key = self.endpoint.secret_key().clone();

        // Start discovery sender task
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
        tracing::info!("🚀 Started discovery sender (postcard + ed25519, 5s interval)");

        // Start discovery receiver task
        let improved_discovered_peers = Arc::clone(&self.discovered_peers);
        let improved_neighbor_map = Arc::clone(&improved_receiver.neighbor_map);
        let endpoint_for_improved = self.endpoint.clone();
        
        // Task 1: Run the receiver (processes incoming gossip messages)
        tokio::spawn(async move {
            if let Err(e) = improved_receiver.run().await {
                tracing::error!("Discovery receiver error: {}", e);
            }
        });
        
        // Task 2: Sync discovered peers from neighbor_map to discovered_peers
        // This bridges the gap between the gossip discovery module and the main peer tracking
        let resilience_ref = self.resilience.clone();
        let peer_backoff = Arc::clone(&self.peer_backoff);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            const MAX_CONNECTIONS_PER_CYCLE: usize = 3; // Limit connection attempts per cycle
            
            loop {
                interval.tick().await;
                
                let mut connections_this_cycle = 0;
                
                // Iterate over all peers in the discovery neighbor map
                for entry in improved_neighbor_map.iter() {
                    // Limit connections per cycle to prevent overwhelming the system
                    if connections_this_cycle >= MAX_CONNECTIONS_PER_CYCLE {
                        break;
                    }
                    
                    let peer_id = *entry.key();
                    let peer_info = entry.value();
                    
                    // Skip if we already have this peer
                    if improved_discovered_peers.contains_key(&peer_id) {
                        continue;
                    }
                    
                    connections_this_cycle += 1;
                    
                            // Check resilience gate (circuit breaker / reputation) if attached
                            if let Some(res) = &resilience_ref {
                                if !res.should_communicate(peer_id) {
                                    tracing::debug!(%peer_id, "Skipping connect due to resilience gate");
                                    continue;
                                }
                            }

                    // Check per-peer backoff
                    let now = chrono::Utc::now();
                    if let Some(back) = peer_backoff.get(&peer_id) {
                        let (_fails, next_allowed) = *back.value();
                        if now < next_allowed {
                            tracing::debug!(%peer_id, next_allowed=%next_allowed, "Skipping connect due to backoff");
                            continue;
                        }
                    }

                            // Try to connect to the discovered peer
                            tracing::info!(
                                "🔗 Discovery found new peer: {} ({}), attempting connection...",
                                peer_info.name,
                                peer_id
                            );

                            // Metric: record connection attempt from discovery
                            crate::metrics::PEER_CONNECTIONS_TOTAL.inc();
                            match endpoint_for_improved.connect(peer_id, iroh_gossip::ALPN).await {
                    Ok(_conn) => {
                        // Success: clear backoff state
                        peer_backoff.remove(&peer_id);
                        improved_discovered_peers.insert(peer_id, (chrono::Utc::now(), None));
                        tracing::info!(
                            "✓ Connected to peer: {} (region: {})",
                            peer_info.name,
                            peer_info.region
                        );
                    }
                    Err(e) => {
                        crate::metrics::PEER_CONNECTION_FAILURES.inc();
                        tracing::debug!("Failed to connect to peer {}: {}", peer_id, e);

                        // Update backoff state: exponential backoff
                        let mut failures = 1u32;
                        if let Some(mut entry) = peer_backoff.get_mut(&peer_id) {
                            failures = entry.value_mut().0.saturating_add(1);
                        }

                        let base_secs = 2u64;
                        let max_secs = 300u64; // 5 minutes
                        let backoff_secs = (base_secs.checked_shl((failures - 1) as u32).unwrap_or(max_secs)).min(max_secs);
                        let next_allowed = now + chrono::Duration::seconds(backoff_secs as i64);
                        peer_backoff.insert(peer_id, (failures, next_allowed));
                        tracing::info!(%peer_id, failures, %next_allowed, "Set backoff after failed connect");
                    }
                            }
                }
            }
        });
        tracing::info!("🚀 Started discovery receiver (ed25519 signature verification)");

        // NOTE: Peer expiration is DISABLED - peers are never removed automatically
        // Bootstrap connection monitor handles reconnection to maintain network connectivity
        tracing::info!("✓ Peer expiration disabled - peers persist until node restart");

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
                                        // Add timeout to prevent sync handler from blocking forever
                                        let handle_result = tokio::time::timeout(
                                            std::time::Duration::from_secs(30),
                                            manager.handle_sync_message(sync_msg, peer)
                                        ).await;
                                        
                                        match handle_result {
                                            Ok(Ok(Some(response))) => {
                                                if let Some(sender) = &sync_sender {
                                                    if let Ok(payload) = serde_json::to_vec(&response) {
                                                        if let Err(e) = sender.lock().await.broadcast(payload.into()).await {
                                                            tracing::error!("Failed to broadcast sync response to {}: {}", peer, e);
                                                        }
                                                    }
                                                }
                                            }
                                            Ok(Ok(None)) => {}
                                            Ok(Err(e)) => {
                                                tracing::error!("Failed to handle sync message from {}: {}", peer, e);
                                            }
                                            Err(_) => {
                                                tracing::warn!("Sync message handler timed out for peer {}", peer);
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
        let latency_sender_clone = self.latency_sender.clone().unwrap();
        let libp2p_to_mqtt_tx = &self.libp2p_to_mqtt_tx;
        let event_tx = &self.event_tx;
        let discovered_peers = &self.discovered_peers;

        // Convert receivers to streams
        let mut data_stream = data_receiver;
        let mut sync_stream = sync_receiver;
        let mut latency_stream = latency_receiver;
        
        // Heartbeat interval to prove the event loop is alive
        let mut heartbeat_interval = tokio::time::interval(std::time::Duration::from_secs(60));
        let mut event_count: u64 = 0;
        
        // Yield counter - yield to runtime periodically to prevent starvation
        let mut ops_since_yield: u32 = 0;
        const YIELD_EVERY_N_OPS: u32 = 50;

        loop {
            // Yield periodically to allow other tasks to run (prevents runtime starvation)
            if ops_since_yield >= YIELD_EVERY_N_OPS {
                tokio::task::yield_now().await;
                ops_since_yield = 0;
            }
            
            tokio::select! {
                // Heartbeat to prove liveness (helps debug hangs)
                _ = heartbeat_interval.tick() => {
                    tracing::info!("💓 Network event loop alive - processed {} events, {} active peers", 
                        event_count, discovered_peers.len());
                }
                
                // Handle gossip events from data topic
                event_result = data_stream.next() => {
                    event_count += 1;
                    ops_since_yield += 1;
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
                
                // Handle gossip events from sync topic
                event_result = sync_stream.next() => {
                    ops_since_yield += 1;
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
                
                // Handle gossip events from fetch-latency-request topic
                event_result = latency_stream.next() => {
                    ops_since_yield += 1;
                    match event_result {
                        Some(Ok(event)) => {
                            if let Err(e) = Self::handle_latency_event(
                                event,
                                node_id,
                                discovered_peers,
                                &latency_sender_clone,
                            ).await {
                                tracing::error!("Error handling latency gossip event: {}", e);
                            }
                        }
                        Some(Err(e)) => {
                            tracing::error!("Error reading latency stream: {}", e);
                        }
                        None => {
                            tracing::warn!("Latency stream ended");
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
                
                // Remove peer from discovered_peers to ensure accurate isolation detection
                discovered_peers.remove(&peer_node_id);
            }
            GossipEvent::Lagged => {
                tracing::warn!("Sync gossip lagged - missed messages");
            }
        }

        Ok(())
    }

    /// Handle fetch-latency-request gossip events
    async fn handle_latency_event(
        event: GossipEvent,
        node_id: EndpointId,
        discovered_peers: &Arc<dashmap::DashMap<EndpointId, (chrono::DateTime<chrono::Utc>, Option<std::net::SocketAddr>)>>,
        latency_sender: &Arc<Mutex<GossipSender>>,
    ) -> Result<()> {
        match event {
            GossipEvent::Received(msg) => {
                let from = msg.delivered_from;
                tracing::info!("⏱️  Received fetch-latency-request from {} ({} bytes)", from, msg.content.len());

                // Ignore our own messages
                if from == node_id {
                    return Ok(());
                }

                // Track this peer (update timestamp, keep existing address if any)
                discovered_peers.entry(from).and_modify(|(ts, _addr)| *ts = chrono::Utc::now())
                    .or_insert((chrono::Utc::now(), None));

                // Handle the latency request in a separate task
                let data = msg.content.to_vec();
                let sender = latency_sender.clone();
                let node_id_str = node_id.to_string();
                tokio::spawn(async move {
                    if let Err(e) = Self::handle_fetch_latency_request(data, sender, node_id_str).await {
                        tracing::error!("Failed to handle fetch latency request: {}", e);
                    }
                });
            }
            GossipEvent::NeighborUp(peer_node_id) => {
                tracing::debug!("Latency topic neighbor up: {}", peer_node_id);
                discovered_peers.entry(peer_node_id).and_modify(|(ts, _addr)| *ts = chrono::Utc::now())
                    .or_insert((chrono::Utc::now(), None));
            }
            GossipEvent::NeighborDown(peer_node_id) => {
                tracing::debug!("Latency topic neighbor down: {}", peer_node_id);
                // Don't remove from discovered_peers here - other topics may still have the peer
            }
            GossipEvent::Lagged => {
                tracing::warn!("Latency gossip lagged - missed messages");
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
                                    
                                    // Metric: record connection attempt (peer discovery announcement)
                                    crate::metrics::PEER_CONNECTIONS_TOTAL.inc();
                                    match endpoint.connect(peer_addr, iroh_gossip::ALPN).await {
                                        Ok(_conn) => {
                                            discovered_peers.insert(peer_id, (chrono::Utc::now(), Some(socket_addr)));
                                            new_connections += 1;
                                            tracing::info!("✓ Successfully connected to peer {} at {}", peer_id, socket_addr);
                                        }
                                        Err(e) => {
                                            crate::metrics::PEER_CONNECTION_FAILURES.inc();
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
                                    
                                    // Metric: record connection attempt (DHT/relay)
                                    crate::metrics::PEER_CONNECTIONS_TOTAL.inc();
                                    match endpoint.connect(peer_id, iroh_gossip::ALPN).await {
                                        Ok(_conn) => {
                                            discovered_peers.insert(peer_id, (chrono::Utc::now(), None));
                                            new_connections += 1;
                                            tracing::info!("✓ Successfully connected to peer {} via DHT/relay", peer_id);
                                        }
                                        Err(e) => {
                                            crate::metrics::PEER_CONNECTION_FAILURES.inc();
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
                // Remove peer to ensure accurate isolation detection
                discovered_peers.remove(&peer_node_id);
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
                
                // Remove peer from discovered_peers to ensure accurate isolation detection
                discovered_peers.remove(&peer_node_id);
                
                // Don't send PeerExpired event - the peer might reconnect shortly
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
        // Discovery is handled exclusively by the postcard+ed25519 discovery module.
        // Keeping this method for API compatibility.
        tracing::debug!("announce_presence is a no-op; discovery is automatic");
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
            "f53f94261cd3c60832c347fda7b92c6c8b7249baab8196a5bfc3915418c43e72"
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
        latency_sender: Arc<Mutex<GossipSender>>,
        node_id: String,
    ) -> Result<()> {
        // Publish response to api-latency topic (on the latency gossip topic)
        let response_msg = GossipMessage {
            origin: "local".to_string(),
            broker: node_id.clone(),
            timestamp: chrono::Utc::now().timestamp(),
            message_id: uuid::Uuid::new_v4().to_string(),
            topic: Some("api-latency".to_string()),
            payload: serde_json::to_vec(&response)?,
        };

        let payload = serde_json::to_vec(&response_msg)?;
        latency_sender.lock().await.broadcast(payload.into()).await?;
        
        tracing::info!("📤 Published api-latency response to latency topic");
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
