//! Integration tests for resource management and state coordination
//!
//! Tests the interaction between ResourceManager, AppState, and Storage layers

use cyberfly_rust_node::{
    resource_manager::{ResourceManager, ResourceLimits},
    state_manager::{AppState, NodeState, PeerState, PeerStatus, DatabaseState},
};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_resource_manager_enforces_limits() {
    let limits = ResourceLimits {
        max_concurrent_operations: 5,
        max_peer_connections: 10,
        max_memory_bytes: 1_000_000,
        max_database_size_bytes: 10_000_000,
        max_cache_entries: 1000,
        max_value_size_bytes: 100_000,
    };
    
    let manager = ResourceManager::new(limits);
    
    // Acquire 5 operation guards (should succeed)
    let mut guards = vec![];
    for _ in 0..5 {
        let guard = manager.acquire_operation_slot().await.expect("Should acquire within limit");
        guards.push(guard);
    }
    
    // 6th acquisition should fail immediately
    let result = tokio::time::timeout(
        Duration::from_millis(100),
        manager.acquire_operation_slot()
    ).await;
    
    assert!(result.is_err(), "Should timeout waiting for 6th operation slot");
    
    // Release one guard
    drop(guards.pop());
    
    // Now 6th acquisition should succeed
    let _guard = manager.acquire_operation_slot().await.expect("Should acquire after release");
}

#[tokio::test]
async fn test_resource_stats_tracking() {
    let limits = ResourceLimits::default();
    let manager = ResourceManager::new(limits);
    
    let _guard1 = manager.acquire_operation_slot().await.unwrap();
    let _guard2 = manager.acquire_operation_slot().await.unwrap();
    
    let stats = manager.get_stats();
    assert_eq!(stats.operations_in_flight, 2);
    assert!(stats.utilization_percent() > 0.0);
    
    drop(_guard1);
    
    let stats = manager.get_stats();
    assert_eq!(stats.operations_in_flight, 1);
}

#[tokio::test]
async fn test_state_management_single_source_of_truth() {
    let state = Arc::new(AppState::new());
    
    // Initial state
    assert_eq!(state.get_node_state().await, NodeState::Initializing);
    
    // Transition to Running
    state.set_node_state(NodeState::Running).await;
    assert!(state.is_running().await);
    
    // Multiple readers should see consistent state
    let state_clone = state.clone();
    let handle = tokio::spawn(async move {
        state_clone.get_node_state().await
    });
    
    let node_state = handle.await.unwrap();
    assert_eq!(node_state, NodeState::Running);
}

#[tokio::test]
async fn test_peer_state_coordination() {
    let state = Arc::new(AppState::new());
    
    // Add multiple peers concurrently
    let mut handles = vec![];
    for i in 0..10 {
        let state_clone = state.clone();
        let handle = tokio::spawn(async move {
            let peer = PeerState {
                peer_id: format!("peer{}", i),
                connected_at: chrono::Utc::now().timestamp(),
                last_sync_at: None,
                operations_synced: 0,
                status: PeerStatus::Connected,
            };
            state_clone.add_peer(format!("peer{}", i), peer).await;
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.await.unwrap();
    }
    
    assert_eq!(state.get_connected_peer_count().await, 10);
    
    // Update peer states
    for i in 0..5 {
        state.update_peer_status(&format!("peer{}", i), PeerStatus::Syncing).await;
    }
    
    let peers = state.get_all_peers().await;
    let syncing_count = peers.iter().filter(|p| p.status == PeerStatus::Syncing).count();
    assert_eq!(syncing_count, 5);
}

#[tokio::test]
async fn test_database_state_tracking() {
    let state = Arc::new(AppState::new());
    
    let db_state = DatabaseState {
        name: "test_db".to_string(),
        entry_count: 0,
        last_modified_at: chrono::Utc::now().timestamp(),
        size_bytes: 0,
    };
    
    state.update_database("test_db".to_string(), db_state).await;
    
    // Simulate concurrent updates
    let mut handles = vec![];
    for _ in 0..100 {
        let state_clone = state.clone();
        let handle = tokio::spawn(async move {
            state_clone.increment_database_entries("test_db", 1).await;
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.await.unwrap();
    }
    
    let db = state.get_database("test_db").await.unwrap();
    assert_eq!(db.entry_count, 100);
}

#[tokio::test]
async fn test_state_snapshot_consistency() {
    let state = Arc::new(AppState::new());
    
    state.set_node_state(NodeState::Running).await;
    
    // Add some peers
    for i in 0..3 {
        let peer = PeerState {
            peer_id: format!("peer{}", i),
            connected_at: chrono::Utc::now().timestamp(),
            last_sync_at: None,
            operations_synced: 0,
            status: PeerStatus::Connected,
        };
        state.add_peer(format!("peer{}", i), peer).await;
    }
    
    // Add a database
    let db_state = DatabaseState {
        name: "test_db".to_string(),
        entry_count: 50,
        last_modified_at: chrono::Utc::now().timestamp(),
        size_bytes: 1024,
    };
    state.update_database("test_db".to_string(), db_state).await;
    
    // Get snapshot
    let snapshot = state.snapshot().await;
    
    assert_eq!(snapshot.node_state, NodeState::Running);
    assert_eq!(snapshot.peer_count, 3);
    assert_eq!(snapshot.connected_peers, 3);
    assert_eq!(snapshot.database_count, 1);
}

#[tokio::test]
async fn test_resource_and_state_coordination() {
    let limits = ResourceLimits {
        max_concurrent_operations: 10,
        max_peer_connections: 5,
        max_memory_bytes: 10_000_000,
        max_database_size_bytes: 100_000_000,
        max_cache_entries: 1000,
        max_value_size_bytes: 1_000_000,
    };
    
    let resource_manager = Arc::new(ResourceManager::new(limits));
    let state = Arc::new(AppState::new());
    
    state.set_node_state(NodeState::Running).await;
    
    // Simulate operations with resource management
    let mut handles = vec![];
    for i in 0..20 {
        let rm = resource_manager.clone();
        let st = state.clone();
        
        let handle = tokio::spawn(async move {
            let _guard = rm.acquire_operation_slot().await.ok();
            
            // Simulate peer addition
            let peer = PeerState {
                peer_id: format!("peer{}", i),
                connected_at: chrono::Utc::now().timestamp(),
                last_sync_at: None,
                operations_synced: 0,
                status: PeerStatus::Connected,
            };
            st.add_peer(format!("peer{}", i), peer).await;
            
            sleep(Duration::from_millis(10)).await;
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Verify state consistency
    assert_eq!(state.get_all_peers().await.len(), 20);
    
    // Verify resource stats
    let stats = resource_manager.get_stats();
    assert_eq!(stats.operations_in_flight, 0, "All operations should be completed");
}

#[tokio::test]
async fn test_under_pressure_state() {
    let limits = ResourceLimits {
        max_concurrent_operations: 2,
        max_peer_connections: 5,
        max_memory_bytes: 1_000_000,
        max_database_size_bytes: 10_000_000,
        max_cache_entries: 100,
        max_value_size_bytes: 100_000,
    };
    
    let resource_manager = Arc::new(ResourceManager::new(limits));
    let state = Arc::new(AppState::new());
    
    state.set_node_state(NodeState::Running).await;
    
    // Fill up operation slots
    let _guard1 = resource_manager.acquire_operation_slot().await.unwrap();
    let _guard2 = resource_manager.acquire_operation_slot().await.unwrap();
    
    // Try to acquire another (should block)
    let rm_clone = resource_manager.clone();
    let state_clone = state.clone();
    
    tokio::spawn(async move {
        let result = tokio::time::timeout(
            Duration::from_millis(100),
            rm_clone.acquire_operation_slot()
        ).await;
        
        if result.is_err() {
            state_clone.set_node_state(NodeState::UnderPressure {
                reason: "Max concurrent operations reached".to_string()
            }).await;
        }
    });
    
    sleep(Duration::from_millis(200)).await;
    
    // Check state changed to under pressure
    match state.get_node_state().await {
        NodeState::UnderPressure { reason } => {
            assert!(reason.contains("concurrent operations"));
        }
        _ => panic!("Expected UnderPressure state"),
    }
}
