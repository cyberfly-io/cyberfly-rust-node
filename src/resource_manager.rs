//! Resource Management Module
//!
//! Provides explicit resource limits and monitoring to prevent unbounded growth
//! and ensure system stability under load.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use anyhow::{Result, anyhow};
use tokio::sync::Semaphore;

/// System-wide resource limits configuration
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum memory usage in bytes (approximate)
    pub max_memory_bytes: usize,
    /// Maximum number of concurrent operations
    pub max_concurrent_operations: usize,
    /// Maximum number of peer connections
    pub max_peer_connections: usize,
    /// Maximum database size in bytes
    pub max_database_size_bytes: usize,
    /// Maximum number of cached entries
    pub max_cache_entries: usize,
    /// Maximum size of a single value in bytes
    pub max_value_size_bytes: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: 4 * 1024 * 1024 * 1024, // 4GB
            max_concurrent_operations: 1000,
            max_peer_connections: 100,
            max_database_size_bytes: 10 * 1024 * 1024 * 1024, // 10GB
            max_cache_entries: 100_000,
            max_value_size_bytes: 10 * 1024 * 1024, // 10MB
        }
    }
}

/// Resource usage metrics
#[derive(Debug, Default)]
pub struct ResourceMetrics {
    operations_in_flight: AtomicUsize,
    active_connections: AtomicUsize,
    cache_entries: AtomicUsize,
    estimated_memory_bytes: AtomicUsize,
}

/// Resource manager with automatic enforcement of limits
pub struct ResourceManager {
    limits: ResourceLimits,
    metrics: Arc<ResourceMetrics>,
    operation_semaphore: Arc<Semaphore>,
}

impl ResourceManager {
    pub fn new(limits: ResourceLimits) -> Self {
        let operation_semaphore = Arc::new(Semaphore::new(limits.max_concurrent_operations));
        
        Self {
            limits,
            metrics: Arc::new(ResourceMetrics::default()),
            operation_semaphore,
        }
    }
    
    /// Acquire a slot for an operation (blocks if limit reached)
    pub async fn acquire_operation_slot(&self) -> Result<OperationGuard<'_>> {
        let permit = self.operation_semaphore.acquire().await
            .map_err(|e| anyhow!("Failed to acquire operation slot: {}", e))?;
        
        self.metrics.operations_in_flight.fetch_add(1, Ordering::Relaxed);
        
        Ok(OperationGuard {
            _permit: permit,
            metrics: self.metrics.clone(),
        })
    }
    
    /// Try to acquire operation slot without blocking
    pub fn try_acquire_operation_slot(&self) -> Result<OperationGuard<'_>> {
        let permit = self.operation_semaphore.try_acquire()
            .map_err(|_| anyhow!("Operation limit reached: {} concurrent operations", 
                self.limits.max_concurrent_operations))?;
        
        self.metrics.operations_in_flight.fetch_add(1, Ordering::Relaxed);
        
        Ok(OperationGuard {
            _permit: permit,
            metrics: self.metrics.clone(),
        })
    }
    
    /// Check if value size is within limits
    pub fn check_value_size(&self, size: usize) -> Result<()> {
        if size > self.limits.max_value_size_bytes {
            return Err(anyhow!(
                "Value size {} bytes exceeds limit of {} bytes",
                size,
                self.limits.max_value_size_bytes
            ));
        }
        Ok(())
    }
    
    /// Register a new connection
    pub fn register_connection(&self) -> Result<ConnectionGuard> {
        let current = self.metrics.active_connections.fetch_add(1, Ordering::Relaxed);
        
        if current >= self.limits.max_peer_connections {
            self.metrics.active_connections.fetch_sub(1, Ordering::Relaxed);
            return Err(anyhow!(
                "Connection limit reached: {} active connections",
                self.limits.max_peer_connections
            ));
        }
        
        Ok(ConnectionGuard {
            metrics: self.metrics.clone(),
        })
    }
    
    /// Update cache metrics
    pub fn update_cache_size(&self, entries: usize) {
        self.metrics.cache_entries.store(entries, Ordering::Relaxed);
    }
    
    /// Update estimated memory usage
    pub fn update_memory_estimate(&self, bytes: usize) {
        self.metrics.estimated_memory_bytes.store(bytes, Ordering::Relaxed);
    }
    
    /// Get current resource usage statistics
    pub fn get_stats(&self) -> ResourceStats {
        ResourceStats {
            operations_in_flight: self.metrics.operations_in_flight.load(Ordering::Relaxed),
            active_connections: self.metrics.active_connections.load(Ordering::Relaxed),
            cache_entries: self.metrics.cache_entries.load(Ordering::Relaxed),
            estimated_memory_bytes: self.metrics.estimated_memory_bytes.load(Ordering::Relaxed),
            limits: self.limits.clone(),
        }
    }
    
    /// Check if system is under resource pressure
    pub fn is_under_pressure(&self) -> bool {
        let stats = self.get_stats();
        
        // Consider under pressure if any resource is >80% utilized
        let ops_pressure = stats.operations_in_flight as f64 / stats.limits.max_concurrent_operations as f64 > 0.8;
        let conn_pressure = stats.active_connections as f64 / stats.limits.max_peer_connections as f64 > 0.8;
        let mem_pressure = stats.estimated_memory_bytes as f64 / stats.limits.max_memory_bytes as f64 > 0.8;
        
        ops_pressure || conn_pressure || mem_pressure
    }
}

/// RAII guard that releases operation slot on drop
pub struct OperationGuard<'a> {
    _permit: tokio::sync::SemaphorePermit<'a>,
    metrics: Arc<ResourceMetrics>,
}

impl<'a> Drop for OperationGuard<'a> {
    fn drop(&mut self) {
        self.metrics.operations_in_flight.fetch_sub(1, Ordering::Relaxed);
    }
}

/// RAII guard that releases connection slot on drop
pub struct ConnectionGuard {
    metrics: Arc<ResourceMetrics>,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.metrics.active_connections.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Resource usage statistics
#[derive(Debug, Clone)]
pub struct ResourceStats {
    pub operations_in_flight: usize,
    pub active_connections: usize,
    pub cache_entries: usize,
    pub estimated_memory_bytes: usize,
    pub limits: ResourceLimits,
}

impl ResourceStats {
    pub fn utilization_percent(&self) -> f64 {
        let ops_util = self.operations_in_flight as f64 / self.limits.max_concurrent_operations as f64;
        let conn_util = self.active_connections as f64 / self.limits.max_peer_connections as f64;
        let mem_util = self.estimated_memory_bytes as f64 / self.limits.max_memory_bytes as f64;
        
        // Return highest utilization
        ops_util.max(conn_util).max(mem_util) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_operation_limit() {
        let limits = ResourceLimits {
            max_concurrent_operations: 2,
            ..Default::default()
        };
        let manager = ResourceManager::new(limits);
        
        // Acquire 2 slots
        let _guard1 = manager.acquire_operation_slot().await.unwrap();
        let _guard2 = manager.acquire_operation_slot().await.unwrap();
        
        // Third should fail (try_acquire)
        assert!(manager.try_acquire_operation_slot().is_err());
        
        // Drop one guard
        drop(_guard1);
        
        // Now should succeed
        assert!(manager.try_acquire_operation_slot().is_ok());
    }
    
    #[test]
    fn test_value_size_check() {
        let limits = ResourceLimits {
            max_value_size_bytes: 1000,
            ..Default::default()
        };
        let manager = ResourceManager::new(limits);
        
        assert!(manager.check_value_size(500).is_ok());
        assert!(manager.check_value_size(1001).is_err());
    }
    
    #[test]
    fn test_connection_limit() {
        let limits = ResourceLimits {
            max_peer_connections: 2,
            ..Default::default()
        };
        let manager = ResourceManager::new(limits);
        
        let _conn1 = manager.register_connection().unwrap();
        let _conn2 = manager.register_connection().unwrap();
        assert!(manager.register_connection().is_err());
        
        drop(_conn1);
        assert!(manager.register_connection().is_ok());
    }
}
