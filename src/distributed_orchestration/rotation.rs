//! Rotation Manager
//!
//! Manages coordinator rotation based on configured triggers.

use super::types::{NodeId, RotationEvent, RotationReason, RotationTrigger};
use anyhow::Result;
use chrono::Utc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Manages coordinator rotation
pub struct RotationManager {
    /// Rotation trigger type
    trigger: RotationTrigger,
    
    /// Jobs processed since last rotation
    jobs_since_rotation: AtomicU64,
    
    /// Last rotation timestamp
    last_rotation: Arc<RwLock<std::time::Instant>>,
    
    /// Rotation history
    history: Arc<RwLock<Vec<RotationEvent>>>,
}

/// Trait for rotation management
#[async_trait::async_trait]
pub trait RotationManagerTrait: Send + Sync {
    /// Check if rotation should occur
    async fn should_rotate(&self, current_coordinator: &NodeId, current_affinity: f64) -> Result<bool>;
    
    /// Record a rotation event
    fn record_rotation(&self, from: NodeId, to: NodeId, reason: RotationReason, affinity_before: f64, affinity_after: f64);
    
    /// Get rotation history
    async fn get_rotation_history(&self) -> Vec<RotationEvent>;
    
    /// Record that a job was processed
    fn record_job_processed(&self);
}

impl RotationManager {
    /// Create a new RotationManager
    pub fn new(trigger: RotationTrigger) -> Self {
        Self {
            trigger,
            jobs_since_rotation: AtomicU64::new(0),
            last_rotation: Arc::new(RwLock::new(std::time::Instant::now())),
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Get jobs since last rotation
    pub fn jobs_since_rotation(&self) -> u64 {
        self.jobs_since_rotation.load(Ordering::Relaxed)
    }
    
    /// Reset job counter (called after rotation)
    pub fn reset_job_counter(&self) {
        self.jobs_since_rotation.store(0, Ordering::Relaxed);
    }
}

#[async_trait::async_trait]
impl RotationManagerTrait for RotationManager {
    async fn should_rotate(&self, _current_coordinator: &NodeId, current_affinity: f64) -> Result<bool> {
        match &self.trigger {
            RotationTrigger::Periodic { interval_jobs } => {
                let jobs = self.jobs_since_rotation.load(Ordering::Relaxed);
                Ok(jobs >= *interval_jobs)
            }
            RotationTrigger::EventDriven => {
                // Only rotate on explicit events (failure, manual)
                // This is typically triggered externally
                Ok(false)
            }
            RotationTrigger::Adaptive => {
                // Rotate if affinity drops significantly
                // or if another node has much higher affinity
                let affinity_threshold = 0.3;
                Ok(current_affinity < affinity_threshold)
            }
        }
    }
    
    fn record_rotation(&self, from: NodeId, to: NodeId, reason: RotationReason, affinity_before: f64, affinity_after: f64) {
        let event = RotationEvent {
            timestamp: Utc::now(),
            from_node: from,
            to_node: to,
            reason,
            affinity_before,
            affinity_after,
        };
        
        // Use try_write to avoid blocking
        if let Ok(mut history) = self.history.try_write() {
            history.push(event);
            
            // Keep only last 1000 events
            if history.len() > 1000 {
                history.remove(0);
            }
        }
        
        // Reset job counter
        self.reset_job_counter();
    }
    
    async fn get_rotation_history(&self) -> Vec<RotationEvent> {
        let history = self.history.read().await;
        history.clone()
    }
    
    fn record_job_processed(&self) {
        self.jobs_since_rotation.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_periodic_rotation() {
        let manager = RotationManager::new(RotationTrigger::Periodic { interval_jobs: 10 });
        
        // Should not rotate initially
        assert!(!manager.should_rotate(&"node-1".to_string(), 0.8).await.unwrap());
        
        // Process 10 jobs
        for _ in 0..10 {
            manager.record_job_processed();
        }
        
        // Should now rotate
        assert!(manager.should_rotate(&"node-1".to_string(), 0.8).await.unwrap());
    }
    
    #[tokio::test]
    async fn test_adaptive_rotation() {
        let manager = RotationManager::new(RotationTrigger::Adaptive);
        
        // High affinity - no rotation
        assert!(!manager.should_rotate(&"node-1".to_string(), 0.8).await.unwrap());
        
        // Low affinity - should rotate
        assert!(manager.should_rotate(&"node-1".to_string(), 0.2).await.unwrap());
    }
    
    #[tokio::test]
    async fn test_rotation_history() {
        let manager = RotationManager::new(RotationTrigger::Adaptive);
        
        manager.record_rotation(
            "node-1".to_string(),
            "node-2".to_string(),
            RotationReason::AffinityChange,
            0.3,
            0.8,
        );
        
        let history = manager.get_rotation_history().await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].from_node, "node-1");
        assert_eq!(history[0].to_node, "node-2");
    }
}

