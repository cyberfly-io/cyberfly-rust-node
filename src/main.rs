mod config;
mod crdt;
mod crypto;
mod error;
mod filters;
mod gossip_discovery; // Improved gossip-based peer discovery
mod graphql;
mod ipfs;
mod iroh_network; // Iroh-based networking
mod kadena; // Kadena blockchain integration
mod metrics; // Performance metrics
mod mqtt_bridge;
mod network_resilience; // Circuit breaker, reputation, bandwidth throttling
mod node_region; // Node region detection
mod peer_registry; // Centralized peer lifecycle management
mod retry; // Enhanced retry and circuit breaker mechanisms
mod storage;
mod sync; // Data synchronization with CRDT
mod inference; // AI inference execution

// Use jemalloc on Linux for better multi-threaded allocation performance
#[cfg(all(target_os = "linux", feature = "jemalloc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use anyhow::Result;
use axum::{routing::get, Router};
use iroh::discovery::pkarr::dht::DhtDiscovery;
// Arc used in places during runtime; prefix to avoid unused import warning in some builds
#[allow(unused_imports)]
use std::sync::Arc;
use std::time::Instant;

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
    // Filter out noisy Iroh warnings that don't indicate real problems:
    // - AEAD errors: crypto mismatch with incompatible peers (normal in heterogeneous network)
    // - Protocol errors: when incompatible clients try to connect
    // - IPv6 unreachable: when peers advertise IPv6 but we're IPv4-only
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("info")
                    // Reduce Iroh protocol connection noise
                    .add_directive("iroh::protocol=error".parse().unwrap())
                    // Filter AEAD errors (incompatible peers, not actionable)
                    .add_directive("iroh::magicsock=error".parse().unwrap())
                    // Filter UDP sendmsg errors (IPv6 unreachable, network issues)
                    .add_directive("iroh_quinn_udp=error".parse().unwrap())
            }),
        )
        .init();

    #[cfg(all(target_os = "linux", feature = "jemalloc"))]
    tracing::info!("Using jemalloc allocator for improved performance");

    tracing::info!("Starting decentralized database node...");

    // Fetch and set node region on startup (same as JS implementation)
    node_region::fetch_and_set_node_region().await;

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
    // If Kadena config is available, derive from Kadena private key, otherwise use file-based key
    let secret_key = if let Some(ref kadena_config) = config.kadena_config {
        tracing::info!("Using Kadena private key to generate Iroh node identity");
        // Decode the Kadena secret key (hex string)
        let kadena_secret_bytes = hex::decode(&kadena_config.secret_key)
            .map_err(|e| anyhow::anyhow!("Failed to decode Kadena secret key: {}", e))?;
        
        if kadena_secret_bytes.len() != 32 {
            return Err(anyhow::anyhow!(
                "Invalid Kadena secret key length: expected 32 bytes, got {}",
                kadena_secret_bytes.len()
            ));
        }
        
        // Create Iroh SecretKey from the same 32 bytes
        iroh::SecretKey::try_from(&kadena_secret_bytes[..])?
    } else {
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

    // Use fixed port 31001 for predictable peer-to-peer connectivity
    let bind_addr = std::net::SocketAddrV4::new(
        std::net::Ipv4Addr::UNSPECIFIED, // 0.0.0.0 - listen on all interfaces
        31001,                           // Fixed port for bootstrap configuration
    );
    let dht_discovery = DhtDiscovery::builder();
    let mdns = iroh::discovery::mdns::MdnsDiscovery::builder();

    // Clone secret key for later use in inference worker (before it's consumed by endpoint builder)
    let secret_key_clone = secret_key.clone();

    let endpoint = iroh::Endpoint::builder()
        .secret_key(secret_key)
        .discovery(dht_discovery)
        .discovery(mdns)
        .relay_mode(iroh::RelayMode::Default) // Use n0's default relay servers for NAT traversal
        .bind_addr_v4(bind_addr) // Bind to fixed port 31001 for bootstrap peer connectivity
        .bind()
        .await?;

    // Log the actual bound addresses for bootstrap configuration
    let local_endpoints = endpoint.bound_sockets();
    for addr in local_endpoints {
        tracing::info!("🔌 Iroh QUIC endpoint listening on: {}", addr);
    }

    let node_id = endpoint.id();
    tracing::info!("═══════════════════════════════════════════════════════");
    tracing::info!("🆔 Iroh Node ID: {}", node_id);
    tracing::info!("🆔 Iroh Public Key: {}", hex::encode(node_id.as_bytes()));
    tracing::info!("═══════════════════════════════════════════════════════");
    tracing::info!("🔄 Relay mode enabled - using n0 default relay servers for NAT traversal");
    tracing::info!("🔍 Peer discovery: DHT + mDNS + relay fallback for peers behind NAT");

    // Start Iroh relay server if enabled
    let relay_url_with_public_ip = if config.relay_config.enabled {
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

        // Get public IP for relay URL
        let relay_public_ip = match kadena::get_public_ip().await {
            Ok(ip) => ip,
            Err(e) => {
                tracing::warn!("Failed to get public IP for relay: {}, using config.api_host", e);
                config.api_host.clone()
            }
        };

        let relay_url = format!("iroh-relay://{}:{}", relay_public_ip, config.relay_config.stun_port);

        tracing::info!("✅ Relay server started successfully");
        tracing::info!("📡 Other nodes can connect to this relay at: {}", relay_url);
        
        Some(relay_url)
    } else {
        tracing::info!("⚠️ Relay server disabled");
        None
    };

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

    // Initialize network resilience (circuit breaker, reputation, bandwidth)
    let resilience = std::sync::Arc::new(network_resilience::NetworkResilience::new(
        network_resilience::CircuitBreakerConfig::default(),
        network_resilience::ReputationConfig::default(),
        network_resilience::BandwidthConfig::default(),
    ));
    // Start background tasks (decay, etc.) for resilience
    resilience.clone().start_background_tasks();
    // Attach resilience manager to the network so dialing respects circuit/reputation
    network.attach_resilience(resilience.clone());
    tracing::info!("NetworkResilience attached to Iroh network");

    // Attach SyncManager to the network for inbound event handling
    network.attach_sync_manager(sync_manager.clone());
    tracing::info!("SyncManager attached to Iroh network for sync routing");

    // Initialize InferenceScheduler for AI inference job coordination
    let inference_capabilities = inference::InferenceCapabilities::from_system();
    let inference_scheduler = std::sync::Arc::new(inference::InferenceScheduler::new(
        node_id,
        inference_capabilities.clone(),
    ));
    tracing::info!(
        "🧠 InferenceScheduler initialized (tflite={}, cpu={}, ram={}MB)",
        inference_capabilities.supports_tflite,
        inference_capabilities.cpu_cores,
        inference_capabilities.ram_mb
    );

    // Attach scheduler to network for receiving gossip jobs
    network.attach_inference_scheduler(inference_scheduler.clone());
    tracing::info!("InferenceScheduler attached to network for job routing");

    // Create channel for worker to send results for gossip broadcast
    let (inference_result_tx, mut inference_result_rx) = tokio::sync::mpsc::unbounded_channel::<inference::InferenceResult>();

    // Get the inference jobs sender for broadcasting completed results
    let inference_results_sender = network.inference_results_sender();

    // Spawn inference result broadcaster (listens for worker results, broadcasts via gossip)
    let scheduler_for_broadcaster = inference_scheduler.clone();
    tokio::spawn(async move {
        while let Some(result) = inference_result_rx.recv().await {
            tracing::info!(
                "📤 Broadcasting inference result: job={} latency={}ms success={}",
                result.job_id, result.latency_ms, result.success
            );
            
            // Sign and broadcast result
            // Note: For now just log - full implementation needs SigningKey
            // let signed = SignedInferenceMessage::sign_and_encode(&result, &signing_key);
            // inference_results_sender.lock().await.broadcast(signed).await;
            
            // Update metrics
            if result.success {
                crate::metrics::INFERENCE_JOBS_COMPLETED.inc();
                crate::metrics::INFERENCE_LATENCY_MS.observe(result.latency_ms as f64);
            }
        }
    });

    // Initialize models directory for TFLite models
    let models_dir = data_dir.join("models");
    tokio::fs::create_dir_all(&models_dir).await?;
    tracing::info!("📁 Models directory: {:?}", models_dir);

    // Download default models if not present
    tracing::info!("🔍 Checking for default inference models...");
    let model_results = inference::ensure_models_downloaded(&models_dir).await;
    for (name, success, msg) in &model_results {
        if *success {
            tracing::info!("   ✓ {}: {}", name, msg);
        } else {
            tracing::warn!("   ✗ {}: {}", name, msg);
        }
    }
    let downloaded_count = model_results.iter().filter(|(_, s, _)| *s).count();
    tracing::info!("📦 {} inference models ready", downloaded_count);

    // Initialize and start inference worker
    // Convert iroh::SecretKey to ed25519_dalek::SigningKey
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&secret_key_clone.to_bytes());
    
    let inference_worker = std::sync::Arc::new(inference::InferenceWorker::new(
        models_dir.clone(),
        node_id, // EndpointId is Copy
        signing_key,
        storage.inner_store(),
    ));

    let worker_scheduler = inference_scheduler.clone();
    let worker_tx = inference_result_tx.clone();
    let worker = inference_worker.clone();
    
    // Spawn worker task
    tokio::spawn(async move {
        tracing::info!("🧠 Starting inference worker loop...");
        if let Err(e) = worker.run(worker_scheduler, worker_tx).await {
            tracing::error!("❌ Inference worker failed: {}", e);
        }
    });

    // Initialize Kadena node registry if configured
    if let Some(kadena_config) = config.kadena_config.clone() {
        tracing::info!("Initializing Kadena blockchain integration...");
        let registry = kadena::NodeRegistry::new(kadena_config.clone());
        
        // Generate libp2p peer ID from Kadena private key for backward compatibility
        let kadena_peer_id = match registry.generate_peer_id() {
            Ok(pid) => {
                tracing::info!("═══════════════════════════════════════════════════════");
                tracing::info!("🔑 libp2p PeerId (from Kadena key): {}", pid);
                tracing::info!("═══════════════════════════════════════════════════════");
                pid
            }
            Err(e) => {
                tracing::error!("Failed to generate peer ID from Kadena key: {}", e);
                tracing::info!("Falling back to Iroh peer ID for Kadena registration");
                peer_id.to_string()
            }
        };
        
        // Get public IP address
        let public_ip = match kadena::get_public_ip().await {
            Ok(ip) => {
                tracing::info!("Public IP detected: {}", ip);
                ip
            }
            Err(e) => {
                tracing::warn!("Failed to get public IP: {}, using 0.0.0.0", e);
                "0.0.0.0".to_string()
            }
        };
        
        // Get QUIC port from bound socket
        let local_endpoints = endpoint.bound_sockets();
        let quic_port = local_endpoints
            .first()
            .map(|addr| addr.port())
            .unwrap_or(0);
        
        // Derive public key from the private key
        let public_key = kadena_config.public_key()
            .map_err(|e| anyhow::anyhow!("Failed to derive public key: {}", e))?;
        
        // Format multiaddr as: publickey@ip:quicport
        let node_multiaddr = format!("{}@{}:{}", public_key, public_ip, quic_port);
        
        tracing::info!("═══════════════════════════════════════════════════════");
        tracing::info!("🔐 Kadena Public Key: {}", public_key);
        tracing::info!("🌐 Kadena Multiaddr: {}", node_multiaddr);
        tracing::info!("📍 Public IP: {} | Port: {}", public_ip, quic_port);
        tracing::info!("═══════════════════════════════════════════════════════");
        let registry = Arc::new(tokio::sync::Mutex::new(registry));

        // Ensure node is registered and active
        let multiaddr = node_multiaddr.clone();
        let registry_clone = registry.clone();
        let kadena_peer_id_clone = kadena_peer_id.clone();
        
        tokio::spawn(async move {
            let registry = registry_clone.lock().await;
            if let Err(e) = registry.ensure_registered(&kadena_peer_id_clone, &multiaddr).await {
                tracing::error!("Failed to register node with Kadena: {}", e);
            } else {
                tracing::info!("Node successfully registered with Kadena blockchain (PeerId: {})", kadena_peer_id_clone);
            }
        });

        // Spawn periodic status check and auto-claim task (every 1 minute)
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60)); // 1 minute
            interval.tick().await; // Skip first immediate tick
            
            loop {
                interval.tick().await;
                let registry = registry.lock().await;
                if let Err(e) = registry.check_and_claim_rewards(&kadena_peer_id).await {
                    tracing::warn!("Kadena status check/claim failed: {}", e);
                } else {
                    tracing::debug!("Kadena status check completed");
                }
            }
        });
    } else {
        tracing::info!("Kadena blockchain integration disabled (no KADENA_ACCOUNT configured)");
    }

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
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(200)); // Reduced from 100ms
            
            // Use a bounded VecDeque instead of unbounded HashSet to prevent memory growth
            let mut seen_message_ids: std::collections::VecDeque<String> = 
                std::collections::VecDeque::with_capacity(1000);
            const MAX_SEEN_IDS: usize = 1000;

            loop {
                interval.tick().await;

                // OPTIMIZED: Use new get_messages_since to filter at source
                let new_messages = mqtt_store_clone
                    .get_messages_since(last_processed_timestamp, None, None)
                    .await;

                // Update last processed timestamp
                if let Some(latest_msg) = new_messages.last() {
                    last_processed_timestamp = latest_msg.timestamp;
                }

                // Send new messages to broadcast channel, deduping by message_id
                for msg in new_messages.into_iter() {  // Use into_iter to consume and avoid clones
                    // If message_id exists and we've already sent it, skip
                    if !msg.message_id.is_empty() && seen_message_ids.contains(&msg.message_id) {
                        continue;
                    }

                    let event = graphql::MessageEvent {
                        topic: msg.topic,      // Move instead of clone
                        payload: msg.payload,  // Move instead of clone
                        timestamp: msg.timestamp,
                    };

                    if broadcast_clone.send(event).is_ok() {
                        if !msg.message_id.is_empty() {
                            // Bounded queue: remove oldest if at capacity
                            if seen_message_ids.len() >= MAX_SEEN_IDS {
                                seen_message_ids.pop_front();
                            }
                            seen_message_ids.push_back(msg.message_id);
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
    
    // Create PeerRegistry for centralized peer management
    let peer_registry = std::sync::Arc::new(peer_registry::PeerRegistry::new(
        peer_id,
        peer_registry::PeerRegistryConfig::default(),
    ));
    tracing::info!("PeerRegistry initialized for centralized peer management");
    
    // Create NetworkResilience for circuit breaker, reputation, and bandwidth throttling
    let network_resilience = std::sync::Arc::new(network_resilience::NetworkResilience::new(
        network_resilience::CircuitBreakerConfig::default(),
        network_resilience::ReputationConfig::default(),
        network_resilience::BandwidthConfig::default(),
    ));
    // Start background tasks (reputation decay)
    network_resilience::NetworkResilience::start_background_tasks(network_resilience.clone());
    tracing::info!("NetworkResilience initialized (circuit breaker, reputation, bandwidth throttling)");
    
    // Start TTL cleanup background task (runs every 60 seconds)
    storage::BlobStorage::start_ttl_cleanup_task(storage.clone(), Some(60));
    tracing::info!("TTL cleanup background task started (interval: 60s)");
    
    // Network will be moved into its own task - no Arc<Mutex<>> needed
    // GraphQL uses the Endpoint directly, not IrohNetwork

    // Create outbound sync channel so other components (GraphQL) can send SyncMessage to network
    let (sync_out_tx, sync_out_rx) = tokio::sync::mpsc::unbounded_channel::<crate::sync::SyncMessage>();

    let graphql_server = graphql::create_server(
        storage.clone(),
        ipfs,
        Some(sync_manager),
        Some(endpoint_for_graphql), // Pass endpoint instead of wrapped network
        Some(discovered_peers_map), // Pass discovered peers map
        None,   // IrohNetwork not needed - GraphQL uses Endpoint directly
        Some(peer_registry.clone()), // Pass PeerRegistry for mesh summary
        Some(network_resilience.clone()), // Pass NetworkResilience for circuit breaker, reputation, bandwidth
        relay_url_with_public_ip, // Pass relay URL with public IP
        mqtt_tx,
        mqtt_to_gossip_tx,
        mqtt_store,
        Some(message_broadcast_tx.clone()),
        Some(sync_out_tx.clone()),
    )
    .await?;
    tracing::info!("GraphQL server initialized with WebSocket subscription support");

    // Start network event loop on a dedicated Tokio runtime thread.
    // This prevents heavy peer churn / gossip processing from starving the HTTP server.
    // Attach sync outbound receiver so GraphQL can submit SyncMessage to be broadcast.
    network.set_sync_outbound_rx(sync_out_rx);

    std::thread::Builder::new()
        .name("iroh-network".to_string())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("iroh-net-worker")
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("Failed to build iroh network runtime: {e}");
                    return;
                }
            };

            rt.block_on(async move {
                let mut net = network;
                loop {
                    if let Err(e) = net.run().await {
                        tracing::error!("Network error: {}", e);
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    }
                }
            });
        })
        .map_err(|e| anyhow::anyhow!("Failed to spawn iroh network thread: {e}"))?;

    // Scheduler-lag watchdog for the API runtime.
    // If the runtime is being starved, this will log the observed lag.
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
        let mut last = Instant::now();
        loop {
            interval.tick().await;
            let now = Instant::now();
            let elapsed = now.saturating_duration_since(last);
            // Expected is ~1s; log only if we're badly delayed.
            if elapsed > std::time::Duration::from_secs(3) {
                tracing::warn!(
                    "API runtime lag detected: tick delay = {:?}",
                    elapsed
                );
            }
            last = now;
        }
    });

    // Start GraphQL API server
    let listener =
        tokio::net::TcpListener::bind(format!("{}:{}", config.api_host, config.api_port)).await?;
    
    // Get public IP for informational logging
    let api_public_ip = match kadena::get_public_ip().await {
        Ok(ip) => ip,
        Err(_) => config.api_host.clone(),
    };
    
    tracing::info!(
        "🚀 GraphQL API listening on {}:{} (bind: {}:{})",
        api_public_ip,
        config.api_port,
        config.api_host,
        config.api_port
    );
    tracing::info!("📊 GraphQL Playground: http://{}:{}/", api_public_ip, config.api_port);
    
    axum::serve(listener, graphql_server.into_make_service()).await?;

    Ok(())
}
