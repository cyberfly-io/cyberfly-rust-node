mod config;
mod crdt;
mod crypto;
mod error;
mod filters;
mod graphql;
mod ipfs;
mod iroh_network; // Iroh-based networking
mod metrics; // Performance metrics
mod mqtt_bridge;
mod retry; // Enhanced retry and circuit breaker mechanisms
mod storage;
mod sync; // Data synchronization with CRDT

use anyhow::Result;
use axum::{routing::get, Router};
// Arc used in places during runtime; prefix to avoid unused import warning in some builds
#[allow(unused_imports)]
use std::sync::Arc;

/// Start the Iroh Relay Server
async fn start_relay_server(_endpoint: iroh::Endpoint, bind_addr: String) -> Result<()> {
    tracing::info!("🔧 Initializing relay server on {}", bind_addr);

    // Parse the bind address
    let addr: std::net::SocketAddr = bind_addr
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid relay bind address: {}", e))?;

    // Create a simple HTTP server for relay handshake
    let app = Router::new()
        .route("/relay", get(relay_handler))
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("🌐 Relay HTTP server listening on {}", addr);

    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow::anyhow!("Relay server error: {}", e))
}

/// Relay handler endpoint
async fn relay_handler() -> &'static str {
    "Iroh Relay Server - Active"
}

/// Health check endpoint
async fn health_handler() -> &'static str {
    "OK"
}

/// Prometheus metrics endpoint
async fn metrics_handler() -> String {
    metrics::export_metrics()
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging with filters
    // Filter out noisy Iroh protocol warnings about unsupported protocols
    // (these occur when incompatible clients try to connect)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("info")
                    // Reduce Iroh protocol connection noise
                    .add_directive("iroh::protocol=error".parse().unwrap())
            }),
        )
        .init();

    tracing::info!("Starting decentralized database node...");

    // Initialize metrics
    metrics::init_metrics();
    tracing::info!("Metrics system initialized");

    // Initialize ResourceManager and AppState (single source of truth)
    let resource_manager = std::sync::Arc::new(cyberfly_rust_node::resource_manager::ResourceManager::new(
        cyberfly_rust_node::resource_manager::ResourceLimits::default(),
    ));
    tracing::info!("ResourceManager initialized with defaults");

    let app_state = std::sync::Arc::new(cyberfly_rust_node::state_manager::AppState::new());
    tracing::info!("AppState (single source of truth) initialized");

    // Load configuration
    let config = config::Config::load()?;

    // Initialize single shared Iroh node</parameter>
    // Initialize single shared Iroh node
    tracing::info!("Initializing shared Iroh node...");
    let data_dir = std::path::PathBuf::from("./data/iroh");
    tokio::fs::create_dir_all(&data_dir).await?;

    // Load or generate secret key for persistent identity
    let secret_key = {
        let key_path = data_dir.join("secret_key");
        if key_path.exists() {
            let key_bytes = tokio::fs::read(&key_path).await?;
            iroh::SecretKey::try_from(&key_bytes[0..32])?
        } else {
            // thread_rng is deprecated in some dependency versions; silence the local deprecation warning
            #[allow(deprecated)]
            let key = iroh::SecretKey::generate(&mut rand::thread_rng());
            tokio::fs::write(&key_path, key.to_bytes()).await?;
            tracing::info!("Generated new Iroh secret key");
            key
        }
    };

    // Create Iroh endpoint with relay server capabilities
    // Use fixed port 11204 for predictable peer-to-peer connectivity
    let bind_addr = std::net::SocketAddrV4::new(
        std::net::Ipv4Addr::UNSPECIFIED, // 0.0.0.0 - listen on all interfaces
        11204,                           // Fixed port for bootstrap configuration
    );

    let endpoint = iroh::Endpoint::builder()
        // TODO: Migrate to new discovery API in iroh 0.94 (unified discovery())
        .secret_key(secret_key)
        .relay_mode(iroh::RelayMode::Custom(iroh::RelayMap::empty())) // Enable relay mode
        .bind_addr_v4(bind_addr) // Bind to fixed port 11204 (fallback to random if unavailable)
        .bind()
        .await?;

    // Log the actual bound addresses for bootstrap configuration
    let local_endpoints = endpoint.bound_sockets();
    for addr in local_endpoints {
        tracing::info!("🔌 Iroh QUIC endpoint listening on: {}", addr);
    }

    let node_id = endpoint.id();
    tracing::info!("Iroh endpoint created with Node ID: {}", node_id);
    tracing::info!("🔄 Relay mode enabled - this node can act as a relay for other peers");
    tracing::info!("🔍 Peer discovery: Use n0 DNS discovery or share node addresses manually");

    // Start Iroh relay server if enabled
    if config.relay_config.enabled {
        tracing::info!("🚀 Starting Iroh Relay Server...");
        tracing::info!(
            "   HTTP Bind Address: {}",
            config.relay_config.http_bind_addr
        );
        tracing::info!("   STUN Port: {}", config.relay_config.stun_port);

        let relay_endpoint = endpoint.clone();
        let relay_bind_addr = config.relay_config.http_bind_addr.clone();

        tokio::spawn(async move {
            // Start the relay server
            if let Err(e) = start_relay_server(relay_endpoint, relay_bind_addr).await {
                tracing::error!("❌ Relay server error: {}", e);
            }
        });

        tracing::info!("✅ Relay server started successfully");
        tracing::info!(
            "📡 Other nodes can connect to this relay at: iroh-relay://{}:{}",
            config.api_host,
            config.relay_config.stun_port
        );
    } else {
        tracing::info!("⚠️  Relay server disabled");
    }

    // Create blob storage
    let store = iroh_blobs::store::fs::FsStore::load(&data_dir).await?;
    tracing::info!("Blob store loaded from {:?}", data_dir);

    // Create blobs protocol handler
    let blobs = iroh_blobs::BlobsProtocol::new(&store, None);

    // Create gossip protocol
    let gossip = iroh_gossip::net::Gossip::builder().spawn(endpoint.clone());
    tracing::info!("Gossip protocol initialized");

    // Build protocol router with both blobs AND gossip protocols
    let router = iroh::protocol::Router::builder(endpoint.clone())
        .accept(iroh_blobs::ALPN, blobs.clone())
        .accept(iroh_gossip::ALPN, gossip.clone())
        .spawn();

    tracing::info!("Iroh router spawned with shared components");

    // Initialize BlobStorage (Redis-like API on top of blob storage)
    let sled_db_path = data_dir.join("sled_db");
    let storage = storage::BlobStorage::new(store.clone(), Some(sled_db_path)).await?;
    tracing::info!("BlobStorage initialized (Redis-like API on blob store)");

    // Initialize IpfsStorage using shared Iroh components
    let ipfs = ipfs::IpfsStorage::from_components(router.clone(), blobs.clone(), store.clone());
    tracing::info!("IPFS storage initialized with shared Iroh node");

    // Initialize SyncManager with blob store for persistent operations
    let sync_manager =
        sync::SyncManager::with_store(storage.clone(), node_id, store.clone());
    tracing::info!("SyncManager initialized with persistent blob storage");

    // Attempt to load previous sync index hashes from disk (if present)
    let index_hash_path = data_dir.join("sync_index_hashes.json");
    if index_hash_path.exists() {
        match tokio::fs::read_to_string(&index_hash_path).await {
            Ok(s) => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&s) {
                    // Load storage index if present
                    if let Some(storage_hash) = json.get("storage_index_hash").and_then(|v| v.as_str()) {
                        if let Ok(hash) = storage_hash.parse() {
                            match storage.load_index_from_hash(hash).await {
                                Ok(_) => tracing::info!("Loaded storage index from {}", storage_hash),
                                Err(e) => tracing::warn!("Failed to load storage index {}: {}", storage_hash, e),
                            }
                        }
                    }
                    if let Some(ops_hash) = json.get("ops_index_hash").and_then(|v| v.as_str()) {
                        if let Ok(hash) = ops_hash.parse() {
                            match sync_manager.load_from_storage(hash).await {
                                Ok(count) => tracing::info!("Loaded {} operations from ops index {}", count, ops_hash),
                                Err(e) => tracing::warn!("Failed to load ops index {}: {}", ops_hash, e),
                            }
                        }
                    }

                    if let Some(applied_hash) = json.get("applied_index_hash").and_then(|v| v.as_str()) {
                        if let Ok(hash) = applied_hash.parse() {
                            match sync_manager.load_applied_index(hash).await {
                                Ok(_) => tracing::info!("Loaded applied index from {}", applied_hash),
                                Err(e) => tracing::warn!("Failed to load applied index {}: {}", applied_hash, e),
                            }
                        }
                    }
                }
            }
            Err(e) => tracing::warn!("Failed to read sync index hash file: {}", e),
        }
    }

    // Background task: persist sync indexes periodically to blob storage
    let sync_mgr_clone = sync_manager.clone();
    let index_hash_path_clone = index_hash_path.clone();
    // Clone the BlobStorage for use inside the background task so we don't move the
    // original `storage` which is still needed later when constructing other components.
    let _storage_for_saver = storage.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            match sync_mgr_clone.save_indexes_to_storage().await {
                Ok((ops_hash, applied_hash)) => {
                    tracing::info!("Saved sync indexes (ops: {}, applied: {})", ops_hash, applied_hash);

                    let mut json_obj = serde_json::Map::new();
                    json_obj.insert("ops_index_hash".to_string(), serde_json::Value::String(ops_hash.to_string()));
                    json_obj.insert("applied_index_hash".to_string(), serde_json::Value::String(applied_hash.to_string()));

                    // Atomic write: write to tmp file then rename
                    let tmp_path = index_hash_path_clone.with_extension("tmp");
                    if let Err(e) = tokio::fs::write(&tmp_path, serde_json::to_string(&serde_json::Value::Object(json_obj)).unwrap()).await {
                        tracing::warn!("Failed to write temp sync index hash file: {}", e);
                    } else if let Err(e) = tokio::fs::rename(&tmp_path, &index_hash_path_clone).await {
                        tracing::warn!("Failed to rename temp sync index hash file: {}", e);
                    }
                }
                Err(e) => tracing::warn!("Failed to save sync indexes: {}", e),
            }
        }
    });

    // Initialize IrohNetwork using shared Iroh components (single instance)
    let mut network = iroh_network::IrohNetwork::from_components(
        endpoint.clone(),
        router,
        gossip,
        blobs,
        store,
        config.iroh_config.bootstrap_peers.clone(),
    );
    let peer_id = network.peer_id();
    tracing::info!("Iroh network initialized with shared Node ID: {}", peer_id);

    // Attach SyncManager to the network for inbound event handling
    network.attach_sync_manager(sync_manager.clone());
    tracing::info!("SyncManager attached to Iroh network for sync routing");

    // Initialize MQTT bridge if enabled
    // Create broadcast channel for real-time message subscriptions first
    let (message_broadcast_tx, _message_broadcast_rx) = tokio::sync::broadcast::channel(1000);

    let (mqtt_tx, mqtt_store, mqtt_to_gossip_tx) = if config.mqtt_config.enabled {
        tracing::info!("Initializing MQTT bridge...");

        // Use Iroh node ID as MQTT client ID for consistent identification
        let mqtt_client_id = format!("cyberfly-{}", peer_id);

        let bridge_config = mqtt_bridge::MqttBridgeConfig {
            broker_host: config.mqtt_config.broker_host.clone(),
            broker_port: config.mqtt_config.broker_port,
            client_id: mqtt_client_id.clone(),
            keep_alive: std::time::Duration::from_secs(60),
        };

        tracing::info!("MQTT client ID: {}", mqtt_client_id);

        let (mut mqtt_bridge, gossip_to_mqtt_tx, mqtt_to_gossip_tx, mqtt_eventloop) =
            mqtt_bridge::MqttBridge::new(bridge_config)?;
        let mqtt_to_gossip_rx = mqtt_bridge.get_mqtt_to_gossip_receiver();

        // Connect MQTT bridge to Iroh network
        network.connect_mqtt_bridge(mqtt_to_gossip_rx, gossip_to_mqtt_tx.clone());

        // Create message store for GraphQL queries and wire broadcast for subscriptions
        let mqtt_store = mqtt_bridge::MqttMessageStore::new(1000);

        // Set the message store on the MQTT bridge so it can store incoming messages
        mqtt_bridge.set_message_store(mqtt_store.clone());

        let mqtt_store_clone = mqtt_store.clone();
        let broadcast_clone = message_broadcast_tx.clone();

        // Forward MQTT messages to broadcast channel for subscriptions
        tokio::spawn(async move {
            let mut last_processed_timestamp = chrono::Utc::now().timestamp();
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
            // Track message_ids we've already broadcast to avoid sending duplicates
            let mut seen_message_ids: std::collections::HashSet<String> =
                std::collections::HashSet::new();

            loop {
                interval.tick().await;

                // Get all messages since last processed timestamp
                let all_messages = mqtt_store_clone.get_messages(None, None).await;
                let new_messages: Vec<_> = all_messages
                    .into_iter()
                    .filter(|msg| msg.timestamp > last_processed_timestamp)
                    .collect();

                // Update last processed timestamp
                if let Some(latest_msg) = new_messages.last() {
                    last_processed_timestamp = latest_msg.timestamp;
                }

                // Send new messages to broadcast channel, deduping by message_id
                for msg in new_messages {
                    // If message_id exists and we've already sent it, skip
                    if !msg.message_id.is_empty() && seen_message_ids.contains(&msg.message_id) {
                        continue;
                    }

                    let event = graphql::MessageEvent {
                        topic: msg.topic.clone(),
                        payload: msg.payload.clone(),
                        timestamp: msg.timestamp,
                    };

                    if broadcast_clone.send(event).is_ok() {
                        if !msg.message_id.is_empty() {
                            seen_message_ids.insert(msg.message_id.clone());
                        }
                    }
                }
            }
        });

        // Start MQTT bridge event loop
        tokio::spawn(async move {
            if let Err(e) = mqtt_bridge.run(mqtt_eventloop).await {
                tracing::error!("MQTT bridge error: {}", e);
            }
        });

        tracing::info!("MQTT bridge initialized and connected to Iroh network");
        // Pass both directions: gossip_to_mqtt_tx (gossip->mqtt) and mqtt_to_gossip_tx (mqtt->gossip)
        (
            Some(gossip_to_mqtt_tx),
            Some(mqtt_store),
            Some(mqtt_to_gossip_tx),
        )
    } else {
        tracing::info!("MQTT bridge disabled");
        (None, None, None)
    };

    // For GraphQL queries, pass the endpoint directly instead of the network
    // This avoids the mutex deadlock issue with run()
    let endpoint_for_graphql = endpoint.clone();

    // Get cloneable reference to discovered peers map before moving network
    let discovered_peers_map = network.discovered_peers_map();
    
    // Wrap network in Arc so we can share it with GraphQL while still moving it to tokio::spawn
    let network = Arc::new(tokio::sync::Mutex::new(network));
    let network_for_graphql = network.clone();

    // Create outbound sync channel so other components (GraphQL) can send SyncMessage to network
    let (sync_out_tx, sync_out_rx) = tokio::sync::mpsc::unbounded_channel::<crate::sync::SyncMessage>();

    let graphql_server = graphql::create_server(
        storage.clone(),
        ipfs,
        Some(sync_manager),
        Some(endpoint_for_graphql), // Pass endpoint instead of wrapped network
        Some(discovered_peers_map), // Pass discovered peers map
        Some(network_for_graphql),   // Pass Arc<Mutex<IrohNetwork>> for dial_peer
        config.relay_config.relay_url.clone(), // Pass relay URL
        mqtt_tx,
        mqtt_to_gossip_tx,
        mqtt_store,
        Some(message_broadcast_tx.clone()),
        Some(sync_out_tx.clone()),
    )
    .await?;
    tracing::info!("GraphQL server initialized with WebSocket subscription support");

    // Start network event loop
    // Attach sync outbound receiver so GraphQL can submit SyncMessage to be broadcast
    let mut network_locked = network.lock().await;
    network_locked.set_sync_outbound_rx(sync_out_rx);
    drop(network_locked);

    tokio::spawn(async move {
        loop {
            let mut net = network.lock().await;
            if let Err(e) = net.run().await {
                tracing::error!("Network error: {}", e);
            }
            drop(net);
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    });

    // Start GraphQL API server
    tracing::info!(
        "GraphQL API listening on http://{}:{}",
        config.api_host,
        config.api_port
    );
    let listener =
        tokio::net::TcpListener::bind(format!("{}:{}", config.api_host, config.api_port)).await?;
    axum::serve(listener, graphql_server.into_make_service()).await?;

    Ok(())
}
