//! Threshold Adapter
//!
//! Adapts node thresholds based on decision outcomes.
//! Part of the Response Threshold Model implementation.

use super::types::NodeId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Record of a threshold adaptation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdAdaptation {
    pub timestamp: DateTime<Utc>,
    pub old_threshold: f64,
    pub new_threshold: f64,
    pub success: bool,
    pub decision_type: String,
}

/// Manages threshold adaptation
pub struct ThresholdAdapter {
    /// Adaptation rate (how much to change per outcome)
    adaptation_rate: f64,
    
    /// Minimum allowed threshold
    min_threshold: f64,
    
    /// Maximum allowed threshold
    max_threshold: f64,
    
    /// History of adaptations
    adaptation_history: Arc<RwLock<HashMap<NodeId, Vec<ThresholdAdaptation>>>>,
}

/// Trait for threshold adaptation
#[async_trait::async_trait]
pub trait ThresholdAdapterTrait: Send + Sync {
    /// Adapt threshold based on outcome
    async fn adapt_threshold(
        &self,
        node_id: &NodeId,
        current_threshold: f64,
        success: bool,
        decision_type: &str,
    ) -> f64;
    
    /// Get adaptation history for a node
    async fn get_adaptation_history(&self, node_id: &NodeId) -> Vec<ThresholdAdaptation>;
    
    /// Calculate success rate for a node
    async fn calculate_success_rate(&self, node_id: &NodeId) -> f64;
}

impl ThresholdAdapter {
    /// Create a new ThresholdAdapter
    pub fn new() -> Self {
        Self {
            adaptation_rate: 0.05,
            min_threshold: 0.3,
            max_threshold: 0.9,
            adaptation_history: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Create with custom parameters
    pub fn with_params(adaptation_rate: f64, min_threshold: f64, max_threshold: f64) -> Self {
        Self {
            adaptation_rate,
            min_threshold,
            max_threshold,
            adaptation_history: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for ThresholdAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ThresholdAdapterTrait for ThresholdAdapter {
    async fn adapt_threshold(
        &self,
        node_id: &NodeId,
        current_threshold: f64,
        success: bool,
        decision_type: &str,
    ) -> f64 {
        let new_threshold = if success {
            // Success: lower threshold (more likely to accept)
            (current_threshold - self.adaptation_rate).max(self.min_threshold)
        } else {
            // Failure: raise threshold (more selective)
            (current_threshold + self.adaptation_rate).min(self.max_threshold)
        };
        
        // Record adaptation
        let adaptation = ThresholdAdaptation {
            timestamp: Utc::now(),
            old_threshold: current_threshold,
            new_threshold,
            success,
            decision_type: decision_type.to_string(),
        };
        
        {
            let mut history = self.adaptation_history.write().await;
            history
                .entry(node_id.clone())
                .or_default()
                .push(adaptation);
            
            // Keep only last 1000 adaptations per node
            if let Some(node_history) = history.get_mut(node_id) {
                if node_history.len() > 1000 {
                    node_history.remove(0);
                }
            }
        }
        
        new_threshold
    }
    
    async fn get_adaptation_history(&self, node_id: &NodeId) -> Vec<ThresholdAdaptation> {
        let history = self.adaptation_history.read().await;
        history.get(node_id).cloned().unwrap_or_default()
    }
    
    async fn calculate_success_rate(&self, node_id: &NodeId) -> f64 {
        let history = self.adaptation_history.read().await;
        
        if let Some(adaptations) = history.get(node_id) {
            if adaptations.is_empty() {
                return 0.5; // Default
            }
            
            let successes = adaptations.iter().filter(|a| a.success).count();
            successes as f64 / adaptations.len() as f64
        } else {
            0.5 // Default for unknown nodes
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_threshold_adaptation_success() {
        let adapter = ThresholdAdapter::new();
        
        let initial = 0.6;
        let new = adapter.adapt_threshold(
            &"node-1".to_string(),
            initial,
            true,
            "test",
        ).await;
        
        assert!(new < initial);
        assert!(new >= 0.3);
    }
    
    #[tokio::test]
    async fn test_threshold_adaptation_failure() {
        let adapter = ThresholdAdapter::new();
        
        let initial = 0.6;
        let new = adapter.adapt_threshold(
            &"node-1".to_string(),
            initial,
            false,
            "test",
        ).await;
        
        assert!(new > initial);
        assert!(new <= 0.9);
    }
    
    #[tokio::test]
    async fn test_threshold_bounds() {
        let adapter = ThresholdAdapter::new();
        
        // Test minimum bound
        let min_test = adapter.adapt_threshold(
            &"node-1".to_string(),
            0.31, // Just above minimum
            true,
            "test",
        ).await;
        assert!(min_test >= 0.3);
        
        // Test maximum bound
        let max_test = adapter.adapt_threshold(
            &"node-2".to_string(),
            0.89, // Just below maximum
            false,
            "test",
        ).await;
        assert!(max_test <= 0.9);
    }
    
    #[tokio::test]
    async fn test_adaptation_history() {
        let adapter = ThresholdAdapter::new();
        
        adapter.adapt_threshold(&"node-1".to_string(), 0.6, true, "type-a").await;
        adapter.adapt_threshold(&"node-1".to_string(), 0.55, false, "type-b").await;
        
        let history = adapter.get_adaptation_history(&"node-1".to_string()).await;
        
        assert_eq!(history.len(), 2);
        assert!(history[0].success);
        assert!(!history[1].success);
    }
    
    #[tokio::test]
    async fn test_success_rate() {
        let adapter = ThresholdAdapter::new();
        
        // 3 successes, 1 failure = 75% success rate
        adapter.adapt_threshold(&"node-1".to_string(), 0.6, true, "test").await;
        adapter.adapt_threshold(&"node-1".to_string(), 0.55, true, "test").await;
        adapter.adapt_threshold(&"node-1".to_string(), 0.5, true, "test").await;
        adapter.adapt_threshold(&"node-1".to_string(), 0.45, false, "test").await;
        
        let rate = adapter.calculate_success_rate(&"node-1".to_string()).await;
        
        assert!((rate - 0.75).abs() < 0.001);
    }
}

