//! Affinity Calculator
//!
//! Calculates coordination affinity score to determine which node should coordinate.
//! Uses weighted factors: capacity, latency, uptime, and specialization.

use super::types::{AffinityHistory, AffinityWeights, NodeId};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Calculates affinity scores for nodes
pub struct AffinityCalculator {
    /// Weights for each factor
    weights: AffinityWeights,
    
    /// Cached affinity scores
    cache: Arc<RwLock<HashMap<NodeId, f64>>>,
    
    /// History of affinity scores per node
    history: Arc<RwLock<HashMap<NodeId, AffinityHistory>>>,
}

/// Trait for affinity calculation
#[async_trait::async_trait]
pub trait AffinityCalculatorTrait: Send + Sync {
    /// Calculate affinity for a single node
    async fn calculate_affinity(&self, node_id: &NodeId, metrics: &NodeMetrics) -> Result<f64>;
    
    /// Calculate affinity for all nodes
    async fn calculate_all_affinities(
        &self,
        nodes: &[(NodeId, NodeMetrics)],
    ) -> Result<HashMap<NodeId, f64>>;
    
    /// Get the node with highest affinity
    async fn get_highest_affinity_node(
        &self,
        nodes: &[(NodeId, NodeMetrics)],
    ) -> Result<Option<NodeId>>;
}

/// Metrics used for affinity calculation
#[derive(Debug, Clone)]
pub struct NodeMetrics {
    /// Available capacity (0.0-1.0, higher is better)
    pub capacity: f64,
    /// Network latency in ms (lower is better)
    pub latency_ms: u64,
    /// Uptime in seconds
    pub uptime_secs: u64,
    /// Specialization match (0.0-1.0)
    pub specialization_match: f64,
}

impl Default for NodeMetrics {
    fn default() -> Self {
        Self {
            capacity: 0.5,
            latency_ms: 100,
            uptime_secs: 3600,
            specialization_match: 0.5,
        }
    }
}

impl AffinityCalculator {
    /// Create a new AffinityCalculator with given weights
    pub fn new(weights: AffinityWeights) -> Self {
        Self {
            weights,
            cache: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Get cached affinity for a node
    pub fn get_cached_affinity(&self, node_id: &NodeId) -> Option<f64> {
        // Use try_read to avoid blocking
        self.cache
            .try_read()
            .ok()
            .and_then(|cache| cache.get(node_id).copied())
    }
    
    /// Normalize latency to 0.0-1.0 (lower latency = higher score)
    fn normalize_latency(&self, latency_ms: u64) -> f64 {
        // Assume max acceptable latency is 1000ms
        let max_latency = 1000.0;
        let normalized = 1.0 - (latency_ms as f64 / max_latency).min(1.0);
        normalized.max(0.0)
    }
    
    /// Normalize uptime to 0.0-1.0
    fn normalize_uptime(&self, uptime_secs: u64) -> f64 {
        // Assume 24 hours is "full" uptime credit
        let max_uptime = 24.0 * 3600.0;
        (uptime_secs as f64 / max_uptime).min(1.0)
    }
}

#[async_trait::async_trait]
impl AffinityCalculatorTrait for AffinityCalculator {
    async fn calculate_affinity(&self, node_id: &NodeId, metrics: &NodeMetrics) -> Result<f64> {
        // Calculate weighted score
        let capacity_score = metrics.capacity * self.weights.capacity_weight;
        let latency_score = self.normalize_latency(metrics.latency_ms) * self.weights.latency_weight;
        let uptime_score = self.normalize_uptime(metrics.uptime_secs) * self.weights.uptime_weight;
        let specialization_score = metrics.specialization_match * self.weights.specialization_weight;
        
        let total_score = capacity_score + latency_score + uptime_score + specialization_score;
        
        // Clamp to 0.0-1.0
        let normalized_score = total_score.clamp(0.0, 1.0);
        
        // Cache the result
        {
            let mut cache = self.cache.write().await;
            cache.insert(node_id.clone(), normalized_score);
        }
        
        // Update history
        {
            let mut history = self.history.write().await;
            let entry = history
                .entry(node_id.clone())
                .or_insert_with(|| AffinityHistory {
                    node_id: node_id.clone(),
                    recent_scores: std::collections::VecDeque::new(),
                    avg_score: 0.0,
                    times_coordinated: 0,
                });
            
            entry.recent_scores.push_back((std::time::Instant::now(), normalized_score));
            
            // Keep only last 100 scores
            while entry.recent_scores.len() > 100 {
                entry.recent_scores.pop_front();
            }
            
            // Recalculate average
            if !entry.recent_scores.is_empty() {
                let sum: f64 = entry.recent_scores.iter().map(|(_, s)| s).sum();
                entry.avg_score = sum / entry.recent_scores.len() as f64;
            }
        }
        
        Ok(normalized_score)
    }
    
    async fn calculate_all_affinities(
        &self,
        nodes: &[(NodeId, NodeMetrics)],
    ) -> Result<HashMap<NodeId, f64>> {
        let mut results = HashMap::new();
        
        for (node_id, metrics) in nodes {
            let affinity = self.calculate_affinity(node_id, metrics).await?;
            results.insert(node_id.clone(), affinity);
        }
        
        Ok(results)
    }
    
    async fn get_highest_affinity_node(
        &self,
        nodes: &[(NodeId, NodeMetrics)],
    ) -> Result<Option<NodeId>> {
        let affinities = self.calculate_all_affinities(nodes).await?;
        
        let best = affinities
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(id, _)| id.clone());
        
        Ok(best)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_affinity_calculation() {
        let calc = AffinityCalculator::new(AffinityWeights::default());
        
        let metrics = NodeMetrics {
            capacity: 0.8,
            latency_ms: 50,
            uptime_secs: 12 * 3600, // 12 hours
            specialization_match: 0.9,
        };
        
        let affinity = calc.calculate_affinity(&"node-1".to_string(), &metrics).await.unwrap();
        
        // Should be a reasonable score
        assert!(affinity > 0.5);
        assert!(affinity <= 1.0);
    }
    
    #[tokio::test]
    async fn test_affinity_normalization() {
        let calc = AffinityCalculator::new(AffinityWeights::default());
        
        // High latency should give lower score
        let high_latency = NodeMetrics {
            latency_ms: 900,
            ..Default::default()
        };
        
        let low_latency = NodeMetrics {
            latency_ms: 10,
            ..Default::default()
        };
        
        let high_affinity = calc.calculate_affinity(&"node-high".to_string(), &high_latency).await.unwrap();
        let low_affinity = calc.calculate_affinity(&"node-low".to_string(), &low_latency).await.unwrap();
        
        assert!(low_affinity > high_affinity);
    }
    
    #[tokio::test]
    async fn test_highest_affinity_selection() {
        let calc = AffinityCalculator::new(AffinityWeights::default());
        
        let nodes = vec![
            ("node-1".to_string(), NodeMetrics { capacity: 0.3, ..Default::default() }),
            ("node-2".to_string(), NodeMetrics { capacity: 0.9, ..Default::default() }),
            ("node-3".to_string(), NodeMetrics { capacity: 0.5, ..Default::default() }),
        ];
        
        let best = calc.get_highest_affinity_node(&nodes).await.unwrap();
        
        assert_eq!(best, Some("node-2".to_string()));
    }
    
    #[tokio::test]
    async fn test_affinity_caching() {
        let calc = AffinityCalculator::new(AffinityWeights::default());
        
        let metrics = NodeMetrics::default();
        let _ = calc.calculate_affinity(&"node-1".to_string(), &metrics).await.unwrap();
        
        let cached = calc.get_cached_affinity(&"node-1".to_string());
        assert!(cached.is_some());
    }
}

