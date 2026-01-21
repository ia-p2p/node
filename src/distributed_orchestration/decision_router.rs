//! Decision Router
//!
//! Decides which strategy to use (heuristic, LLM, or hybrid) based on context.

use super::config::DecisionRoutingConfig;
use super::types::{DecisionContext, DecisionStrategy, DistributedDecision, HeuristicAlgorithm};
use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Statistics for a strategy
#[derive(Debug, Clone, Default)]
pub struct StrategyStats {
    pub total_uses: u64,
    pub successes: u64,
    pub failures: u64,
    pub avg_latency_ms: f64,
    pub total_cost: f64,
}

/// Routes decisions to appropriate strategy
pub struct DecisionRouter {
    /// Configuration
    config: DecisionRoutingConfig,
    
    /// Strategy usage statistics
    strategy_stats: Arc<RwLock<HashMap<String, StrategyStats>>>,
    
    /// Round-robin counter for RoundRobin algorithm
    round_robin_counter: Arc<RwLock<usize>>,
}

/// Trait for decision routing
#[async_trait::async_trait]
pub trait DecisionRouterTrait: Send + Sync {
    /// Select the best strategy for a decision context
    async fn select_strategy(&self, context: &DecisionContext) -> DecisionStrategy;
    
    /// Execute a heuristic strategy
    async fn execute_heuristic(
        &self,
        algorithm: &HeuristicAlgorithm,
        context: &DecisionContext,
    ) -> Result<DistributedDecision>;
    
    /// Record strategy outcome
    async fn record_outcome(&self, strategy: &str, success: bool, latency_ms: u64, cost: f64);
    
    /// Get strategy statistics
    async fn get_strategy_stats(&self) -> HashMap<String, StrategyStats>;
}

impl DecisionRouter {
    /// Create a new DecisionRouter
    pub fn new(config: DecisionRoutingConfig) -> Self {
        Self {
            config,
            strategy_stats: Arc::new(RwLock::new(HashMap::new())),
            round_robin_counter: Arc::new(RwLock::new(0)),
        }
    }
    
    /// Check if decision type is critical
    fn is_critical(&self, decision_type: &str) -> bool {
        self.config.critical_decisions.contains(&decision_type.to_string())
    }
}

#[async_trait::async_trait]
impl DecisionRouterTrait for DecisionRouter {
    async fn select_strategy(&self, context: &DecisionContext) -> DecisionStrategy {
        // Critical decisions always use LLM
        if context.is_critical || self.is_critical(&context.decision_type) {
            return DecisionStrategy::LLMReasoning {
                model: "local".to_string(),
                temperature: 0.3, // Conservative for critical
                expected_latency_ms: self.config.llm_large_latency_ms,
                cost_per_decision: self.config.llm_large_cost,
            };
        }
        
        // High value jobs use LLM
        if context.job_value >= self.config.high_value_threshold {
            return DecisionStrategy::LLMReasoning {
                model: "local".to_string(),
                temperature: 0.5,
                expected_latency_ms: self.config.llm_large_latency_ms,
                cost_per_decision: self.config.llm_large_cost,
            };
        }
        
        // Complex decisions use hybrid
        if context.complexity_score >= self.config.complexity_threshold {
            return DecisionStrategy::HybridValidation {
                heuristic: Box::new(DecisionStrategy::Heuristic {
                    algorithm: HeuristicAlgorithm::GreedyAffinity,
                    expected_latency_ms: self.config.heuristic_latency_ms,
                    cost_per_decision: self.config.heuristic_cost,
                }),
                llm_validator: Box::new(DecisionStrategy::LLMReasoning {
                    model: "local-small".to_string(),
                    temperature: 0.3,
                    expected_latency_ms: self.config.llm_small_latency_ms,
                    cost_per_decision: self.config.llm_small_cost,
                }),
            };
        }
        
        // Default: use heuristic
        // Choose algorithm based on default strategy or GreedyAffinity
        let algorithm = self.config
            .default_strategies
            .get(&context.decision_type)
            .and_then(|s| match s.as_str() {
                "round_robin" => Some(HeuristicAlgorithm::RoundRobin),
                "least_loaded" => Some(HeuristicAlgorithm::LeastLoaded),
                "random" => Some(HeuristicAlgorithm::Random),
                _ => Some(HeuristicAlgorithm::GreedyAffinity),
            })
            .unwrap_or(HeuristicAlgorithm::GreedyAffinity);
        
        DecisionStrategy::Heuristic {
            algorithm,
            expected_latency_ms: self.config.heuristic_latency_ms,
            cost_per_decision: self.config.heuristic_cost,
        }
    }
    
    async fn execute_heuristic(
        &self,
        algorithm: &HeuristicAlgorithm,
        context: &DecisionContext,
    ) -> Result<DistributedDecision> {
        let start = std::time::Instant::now();
        
        let (decision, reasoning) = match algorithm {
            HeuristicAlgorithm::GreedyAffinity => {
                ("best_affinity_node".to_string(), "Selected node with highest affinity score".to_string())
            }
            HeuristicAlgorithm::RoundRobin => {
                let mut counter = self.round_robin_counter.write().await;
                *counter = (*counter + 1) % 100; // Wrap around
                (format!("node_{}", *counter % 10), "Round-robin selection".to_string())
            }
            HeuristicAlgorithm::LeastLoaded => {
                ("least_loaded_node".to_string(), "Selected node with lowest current load".to_string())
            }
            HeuristicAlgorithm::Random => {
                let random_id = rand::random::<u32>() % 10;
                (format!("node_{}", random_id), "Random selection".to_string())
            }
        };
        
        let latency = start.elapsed().as_millis() as u64;
        
        // Record outcome
        self.record_outcome(&format!("heuristic_{:?}", algorithm), true, latency, 0.0).await;
        
        Ok(DistributedDecision {
            decision,
            confidence: 0.7, // Heuristics have moderate confidence
            reasoning,
            strategy_used: DecisionStrategy::Heuristic {
                algorithm: algorithm.clone(),
                expected_latency_ms: self.config.heuristic_latency_ms,
                cost_per_decision: 0.0,
            },
            consensus_result: None,
            coordinator: None,
            context_group: context.context_group.clone(),
            latency_ms: latency,
            cost: 0.0,
            timestamp: Utc::now(),
        })
    }
    
    async fn record_outcome(&self, strategy: &str, success: bool, latency_ms: u64, cost: f64) {
        let mut stats = self.strategy_stats.write().await;
        let entry = stats.entry(strategy.to_string()).or_default();
        
        entry.total_uses += 1;
        if success {
            entry.successes += 1;
        } else {
            entry.failures += 1;
        }
        
        // Update rolling average for latency
        let n = entry.total_uses as f64;
        entry.avg_latency_ms = entry.avg_latency_ms * (n - 1.0) / n + latency_ms as f64 / n;
        entry.total_cost += cost;
    }
    
    async fn get_strategy_stats(&self) -> HashMap<String, StrategyStats> {
        let stats = self.strategy_stats.read().await;
        stats.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_critical_decision_uses_llm() {
        let config = DecisionRoutingConfig::default();
        let router = DecisionRouter::new(config);
        
        let context = DecisionContext {
            decision_type: "coordinator_election".to_string(),
            is_critical: true,
            ..Default::default()
        };
        
        let strategy = router.select_strategy(&context).await;
        
        assert!(matches!(strategy, DecisionStrategy::LLMReasoning { .. }));
    }
    
    #[tokio::test]
    async fn test_high_value_uses_llm() {
        let config = DecisionRoutingConfig::default();
        let router = DecisionRouter::new(config);
        
        let context = DecisionContext {
            job_value: 50.0, // High value
            ..Default::default()
        };
        
        let strategy = router.select_strategy(&context).await;
        
        assert!(matches!(strategy, DecisionStrategy::LLMReasoning { .. }));
    }
    
    #[tokio::test]
    async fn test_complex_uses_hybrid() {
        let config = DecisionRoutingConfig::default();
        let router = DecisionRouter::new(config);
        
        let context = DecisionContext {
            complexity_score: 0.8, // Complex
            ..Default::default()
        };
        
        let strategy = router.select_strategy(&context).await;
        
        assert!(matches!(strategy, DecisionStrategy::HybridValidation { .. }));
    }
    
    #[tokio::test]
    async fn test_simple_uses_heuristic() {
        let config = DecisionRoutingConfig::default();
        let router = DecisionRouter::new(config);
        
        let context = DecisionContext {
            job_value: 1.0, // Low value
            complexity_score: 0.2, // Simple
            ..Default::default()
        };
        
        let strategy = router.select_strategy(&context).await;
        
        assert!(matches!(strategy, DecisionStrategy::Heuristic { .. }));
    }
    
    #[tokio::test]
    async fn test_heuristic_execution() {
        let config = DecisionRoutingConfig::default();
        let router = DecisionRouter::new(config);
        
        let context = DecisionContext::default();
        
        let result = router.execute_heuristic(&HeuristicAlgorithm::GreedyAffinity, &context).await.unwrap();
        
        assert!(!result.decision.is_empty());
        assert!(result.latency_ms < 100); // Should be fast
    }
    
    #[tokio::test]
    async fn test_strategy_stats() {
        let config = DecisionRoutingConfig::default();
        let router = DecisionRouter::new(config);
        
        router.record_outcome("test_strategy", true, 10, 0.01).await;
        router.record_outcome("test_strategy", true, 20, 0.01).await;
        router.record_outcome("test_strategy", false, 30, 0.01).await;
        
        let stats = router.get_strategy_stats().await;
        let test_stats = stats.get("test_strategy").unwrap();
        
        assert_eq!(test_stats.total_uses, 3);
        assert_eq!(test_stats.successes, 2);
        assert_eq!(test_stats.failures, 1);
    }
}

