mod config;
mod crypto;
mod storage;
mod crdt;
mod sync;  // Data synchronization with CRDT
mod iroh_network;  // Iroh-based networking
mod graphql;
mod error;
mod ipfs;
mod mqtt_bridge;

use anyhow::Result;
use tracing_subscriber;
use std::sync::Arc;
use axum::{routing::get, Router};

/// Start the Iroh Relay Server
async fn start_relay_server(endpoint: iroh::Endpoint, bind_addr: String) -> Result<()> {
    tracing::info!("🔧 Initializing relay server on {}", bind_addr);
    
    // Parse the bind address
    let addr: std::net::SocketAddr = bind_addr.parse()
        .map_err(|e| anyhow::anyhow!("Invalid relay bind address: {}", e))?;
    
    // Create a simple HTTP server for relay handshake
    let app = Router::new()
        .route("/relay", get(relay_handler))
        .route("/health", get(health_handler));
    
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


#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging with filters
    // Filter out noisy Iroh protocol warnings about unsupported protocols
    // (these occur when incompatible clients try to connect)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| {
                    tracing_subscriber::EnvFilter::new("info")
                        // Reduce Iroh protocol connection noise
                        .add_directive("iroh::protocol=error".parse().unwrap())
                })
        )
        .init();

    tracing::info!("Starting decentralized database node...");

    // Load configuration
    let config = config::Config::load()?;
    
    // Initialize components
    let storage = storage::RedisStorage::new(&config.redis_url).await?;
    tracing::info!("Redis storage initialized");
    
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
            let key = iroh::SecretKey::generate(&mut rand::thread_rng());
            tokio::fs::write(&key_path, key.to_bytes()).await?;
            tracing::info!("Generated new Iroh secret key");
            key
        }
    };
    
    // Create Iroh endpoint with relay server capabilities
    // Use fixed port 11204 for predictable peer-to-peer connectivity
    let bind_addr = std::net::SocketAddrV4::new(
        std::net::Ipv4Addr::UNSPECIFIED,  // 0.0.0.0 - listen on all interfaces
        11204  // Fixed port for bootstrap configuration
    );
    
    let endpoint = iroh::Endpoint::builder()
        .discovery_n0()  // Enable n0 DNS discovery for peer finding
       //discovery_dht()  // Enable DHT discovery for peer finding
        .discovery_local_network()  // Enable local network discovery (mDNS)
        .secret_key(secret_key)
        .relay_mode(iroh::RelayMode::Custom(iroh::RelayMap::empty()))  // Enable relay mode
        .bind_addr_v4(bind_addr)  // Bind to fixed port 11204 (fallback to random if unavailable)
        .bind()
        .await?;
    
    // Log the actual bound addresses for bootstrap configuration
    let local_endpoints = endpoint.bound_sockets();
    for addr in local_endpoints {
        tracing::info!("🔌 Iroh QUIC endpoint listening on: {}", addr);
    }
    
    let node_id = endpoint.node_id();
    tracing::info!("Iroh endpoint created with Node ID: {}", node_id);
    tracing::info!("🔄 Relay mode enabled - this node can act as a relay for other peers");
    tracing::info!("🔍 Peer discovery: Use n0 DNS discovery or share node addresses manually");
    
    // Start Iroh relay server if enabled
    if config.relay_config.enabled {
        tracing::info!("🚀 Starting Iroh Relay Server...");
        tracing::info!("   HTTP Bind Address: {}", config.relay_config.http_bind_addr);
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
        tracing::info!("📡 Other nodes can connect to this relay at: iroh-relay://{}:{}", 
            config.api_host, config.relay_config.stun_port);
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
    
    // Initialize IpfsStorage using shared Iroh components
    let ipfs = ipfs::IpfsStorage::from_components(router.clone(), blobs.clone(), store.clone());
    tracing::info!("IPFS storage initialized with shared Iroh node");
    
    // Initialize SyncManager with blob store for persistent operations
    let sync_manager = sync::SyncManager::with_store(storage.clone(), node_id.into(), store.clone());
    tracing::info!("SyncManager initialized with persistent blob storage");
    
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
        
    let (mut mqtt_bridge, gossip_to_mqtt_tx, mqtt_to_gossip_tx, mqtt_eventloop) = mqtt_bridge::MqttBridge::new(bridge_config)?;
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
            let mut seen_message_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

            loop {
                interval.tick().await;

                // Get all messages since last processed timestamp
                let all_messages = mqtt_store_clone.get_messages(None, None).await;
                let new_messages: Vec<_> = all_messages.into_iter()
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
                        timestamp: msg.timestamp as i64,
                    };

                    if let Ok(_) = broadcast_clone.send(event) {
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
    (Some(gossip_to_mqtt_tx), Some(mqtt_store), Some(mqtt_to_gossip_tx))
    } else {
        tracing::info!("MQTT bridge disabled");
        (None, None, None)
    };
    
    // For GraphQL queries, pass the endpoint directly instead of the network
    // This avoids the mutex deadlock issue with run()
    let endpoint_for_graphql = endpoint.clone();
    
    // Get cloneable reference to discovered peers map before moving network
    let discovered_peers_map = network.discovered_peers_map();
    
    let graphql_server = graphql::create_server(
        storage.clone(), 
        ipfs,
        Some(sync_manager),
        Some(endpoint_for_graphql),  // Pass endpoint instead of wrapped network
        Some(discovered_peers_map),  // Pass discovered peers map
        mqtt_tx, 
        mqtt_to_gossip_tx,
        mqtt_store,
        Some(message_broadcast_tx.clone())
    ).await?;
    tracing::info!("GraphQL server initialized with WebSocket subscription support");

    // Start network event loop
    tokio::spawn(async move {
        if let Err(e) = network.run().await {
            tracing::error!("Network error: {}", e);
        }
    });

    // Start GraphQL API server
    tracing::info!("GraphQL API listening on http://{}:{}", config.api_host, config.api_port);
    let listener = tokio::net::TcpListener::bind(format!("{}:{}", config.api_host, config.api_port)).await?;
    axum::serve(listener, graphql_server.into_make_service()).await?;

    Ok(())
}
