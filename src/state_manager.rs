//! State Management Module
//!
//! Provides centralized state management with single source of truth pattern.
//! Uses atomic operations and RwLock for thread-safe state access.

use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Node operational state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeState {
    /// Node is initializing
    Initializing,
    /// Node is running normally
    Running,
    /// Node is syncing with peers
    Syncing { peer_count: usize },
    /// Node is under resource pressure
    UnderPressure { reason: String },
    /// Node is shutting down gracefully
    ShuttingDown,
    /// Node has stopped
    Stopped,
}

/// Peer connection state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerState {
    pub peer_id: String,
    pub connected_at: i64,
    pub last_sync_at: Option<i64>,
    pub operations_synced: usize,
    pub status: PeerStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PeerStatus {
    Connecting,
    Connected,
    Syncing,
    Idle,
    Disconnected,
}

/// Database state information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseState {
    pub name: String,
    pub entry_count: usize,
    pub last_modified_at: i64,
    pub size_bytes: usize,
}

/// Centralized application state (single source of truth)
pub struct AppState {
    node_state: Arc<RwLock<NodeState>>,
    peers: Arc<RwLock<HashMap<String, PeerState>>>,
    databases: Arc<RwLock<HashMap<String, DatabaseState>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            node_state: Arc::new(RwLock::new(NodeState::Initializing)),
            peers: Arc::new(RwLock::new(HashMap::new())),
            databases: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    // Node state management
    
    pub async fn get_node_state(&self) -> NodeState {
        self.node_state.read().await.clone()
    }
    
    pub async fn set_node_state(&self, state: NodeState) {
        let mut current = self.node_state.write().await;
        tracing::info!("Node state transition: {:?} -> {:?}", *current, state);
        *current = state;
    }
    
    pub async fn is_running(&self) -> bool {
        matches!(*self.node_state.read().await, NodeState::Running | NodeState::Syncing { .. })
    }
    
    // Peer state management
    
    pub async fn add_peer(&self, peer_id: String, state: PeerState) {
        self.peers.write().await.insert(peer_id.clone(), state);
        tracing::info!("Peer added: {}", peer_id);
    }
    
    pub async fn remove_peer(&self, peer_id: &str) {
        self.peers.write().await.remove(peer_id);
        tracing::info!("Peer removed: {}", peer_id);
    }
    
    pub async fn update_peer_status(&self, peer_id: &str, status: PeerStatus) {
        if let Some(peer) = self.peers.write().await.get_mut(peer_id) {
            peer.status = status;
        }
    }
    
    pub async fn get_peer(&self, peer_id: &str) -> Option<PeerState> {
        self.peers.read().await.get(peer_id).cloned()
    }
    
    pub async fn get_all_peers(&self) -> Vec<PeerState> {
        self.peers.read().await.values().cloned().collect()
    }
    
    pub async fn get_connected_peer_count(&self) -> usize {
        self.peers.read().await
            .values()
            .filter(|p| p.status == PeerStatus::Connected || p.status == PeerStatus::Syncing)
            .count()
    }
    
    // Database state management
    
    pub async fn update_database(&self, name: String, state: DatabaseState) {
        self.databases.write().await.insert(name, state);
    }
    
    pub async fn get_database(&self, name: &str) -> Option<DatabaseState> {
        self.databases.read().await.get(name).cloned()
    }
    
    pub async fn get_all_databases(&self) -> Vec<DatabaseState> {
        self.databases.read().await.values().cloned().collect()
    }
    
    pub async fn increment_database_entries(&self, name: &str, count: usize) {
        if let Some(db) = self.databases.write().await.get_mut(name) {
            db.entry_count += count;
            db.last_modified_at = chrono::Utc::now().timestamp();
        }
    }
    
    // Snapshot for observability
    
    pub async fn snapshot(&self) -> StateSnapshot {
        StateSnapshot {
            node_state: self.get_node_state().await,
            peer_count: self.peers.read().await.len(),
            connected_peers: self.get_connected_peer_count().await,
            database_count: self.databases.read().await.len(),
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Immutable snapshot of application state for reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub node_state: NodeState,
    pub peer_count: usize,
    pub connected_peers: usize,
    pub database_count: usize,
    pub timestamp: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_node_state_transitions() {
        let state = AppState::new();
        
        assert_eq!(state.get_node_state().await, NodeState::Initializing);
        
        state.set_node_state(NodeState::Running).await;
        assert_eq!(state.get_node_state().await, NodeState::Running);
        
        assert!(state.is_running().await);
    }
    
    #[tokio::test]
    async fn test_peer_management() {
        let state = AppState::new();
        
        let peer = PeerState {
            peer_id: "peer1".to_string(),
            connected_at: 12345,
            last_sync_at: None,
            operations_synced: 0,
            status: PeerStatus::Connected,
        };
        
        state.add_peer("peer1".to_string(), peer.clone()).await;
        assert_eq!(state.get_connected_peer_count().await, 1);
        
        let retrieved = state.get_peer("peer1").await.unwrap();
        assert_eq!(retrieved.peer_id, "peer1");
        
        state.remove_peer("peer1").await;
        assert_eq!(state.get_connected_peer_count().await, 0);
    }
    
    #[tokio::test]
    async fn test_database_state() {
        let state = AppState::new();
        
        let db_state = DatabaseState {
            name: "test_db".to_string(),
            entry_count: 10,
            last_modified_at: 12345,
            size_bytes: 1024,
        };
        
        state.update_database("test_db".to_string(), db_state).await;
        
        let retrieved = state.get_database("test_db").await.unwrap();
        assert_eq!(retrieved.entry_count, 10);
        
        state.increment_database_entries("test_db", 5).await;
        let updated = state.get_database("test_db").await.unwrap();
        assert_eq!(updated.entry_count, 15);
    }
}
