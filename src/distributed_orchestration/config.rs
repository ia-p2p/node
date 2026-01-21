//! Configuration for Distributed Orchestration
//!
//! Defines all configurable parameters for the distributed orchestration system.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::types::{
    AffinityWeights, ConsensusCriteria, OrchestrationMode, ReconciliationStrategy,
};

/// Main configuration for Distributed Orchestrator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedOrchestratorConfig {
    /// Operating mode
    pub mode: OrchestrationMode,
    
    /// Weights for affinity calculation
    pub affinity_weights: AffinityWeights,
    
    /// Consensus criteria
    pub consensus_criteria: ConsensusCriteria,
    
    /// Decision routing configuration
    pub decision_routing: DecisionRoutingConfig,
    
    /// Context group configuration
    pub context_groups: ContextGroupConfig,
    
    /// Partition handling configuration
    pub partition_handling: PartitionHandlingConfig,
    
    /// Maximum consensus rounds before fallback
    pub max_consensus_rounds: usize,
    
    /// Timeout for consensus in milliseconds
    pub consensus_timeout_ms: u64,
    
    /// Whether distributed mode is enabled
    pub enabled: bool,
    
    /// Simulation mode - no side effects, useful for testing
    /// When enabled:
    /// - P2P messages are not sent
    /// - Decisions are logged but not executed
    /// - External services are mocked
    #[serde(default)]
    pub simulation_mode: bool,
}

impl Default for DistributedOrchestratorConfig {
    fn default() -> Self {
        Self {
            mode: OrchestrationMode::default(),
            affinity_weights: AffinityWeights::default(),
            consensus_criteria: ConsensusCriteria::default(),
            decision_routing: DecisionRoutingConfig::default(),
            context_groups: ContextGroupConfig::default(),
            partition_handling: PartitionHandlingConfig::default(),
            max_consensus_rounds: 3,
            consensus_timeout_ms: 5000,
            enabled: true,
            simulation_mode: false,
        }
    }
}

impl DistributedOrchestratorConfig {
    /// Create a configuration for simulation/testing mode
    pub fn simulation() -> Self {
        Self {
            simulation_mode: true,
            ..Default::default()
        }
    }
    
    /// Check if in simulation mode
    pub fn is_simulation(&self) -> bool {
        self.simulation_mode
    }
}

/// Configuration for decision routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRoutingConfig {
    /// Threshold for considering a job "high value" (triggers LLM strategy)
    pub high_value_threshold: f64,
    
    /// Threshold for considering a decision "complex" (triggers hybrid/LLM)
    pub complexity_threshold: f64,
    
    /// List of decision types considered critical (always use LLM)
    pub critical_decisions: Vec<String>,
    
    /// Cost per decision for large LLM
    pub llm_large_cost: f64,
    
    /// Cost per decision for small LLM
    pub llm_small_cost: f64,
    
    /// Cost per heuristic decision (typically 0)
    pub heuristic_cost: f64,
    
    /// Expected latency for large LLM in ms
    pub llm_large_latency_ms: u64,
    
    /// Expected latency for small LLM in ms
    pub llm_small_latency_ms: u64,
    
    /// Expected latency for heuristic in ms
    pub heuristic_latency_ms: u64,
    
    /// Default strategy per decision type
    pub default_strategies: HashMap<String, String>,
}

impl Default for DecisionRoutingConfig {
    fn default() -> Self {
        let mut default_strategies = HashMap::new();
        default_strategies.insert("job_routing".to_string(), "heuristic".to_string());
        default_strategies.insert("coordinator_election".to_string(), "consensus".to_string());
        default_strategies.insert("group_formation".to_string(), "heuristic".to_string());
        
        Self {
            high_value_threshold: 10.0, // $10 USD
            complexity_threshold: 0.7,
            critical_decisions: vec![
                "coordinator_election".to_string(),
                "partition_reconciliation".to_string(),
            ],
            llm_large_cost: 0.01,
            llm_small_cost: 0.001,
            heuristic_cost: 0.0,
            llm_large_latency_ms: 2000,
            llm_small_latency_ms: 500,
            heuristic_latency_ms: 10,
            default_strategies,
        }
    }
}

/// Configuration for context groups
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextGroupConfig {
    /// Minimum jobs before forming a group
    pub min_jobs_for_formation: u64,
    
    /// Minimum nodes per group
    pub min_nodes_per_group: usize,
    
    /// Maximum groups a node can participate in
    pub max_groups_per_node: usize,
    
    /// Idle time before dissolving group (seconds)
    pub dissolution_idle_threshold_secs: u64,
    
    /// Whether leader rotation is enabled
    pub leader_rotation_enabled: bool,
    
    /// Jobs before rotating leader (if periodic)
    pub leader_rotation_interval_jobs: u64,
}

impl Default for ContextGroupConfig {
    fn default() -> Self {
        Self {
            min_jobs_for_formation: 5,
            min_nodes_per_group: 2,
            max_groups_per_node: 3,
            dissolution_idle_threshold_secs: 300, // 5 minutes
            leader_rotation_enabled: true,
            leader_rotation_interval_jobs: 100,
        }
    }
}

/// Configuration for partition handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionHandlingConfig {
    /// How often to check for partitions (seconds)
    pub detection_interval_secs: u64,
    
    /// Heartbeat timeout before considering node unreachable (seconds)
    pub heartbeat_timeout_secs: u64,
    
    /// Strategy for reconciliation
    pub reconciliation_strategy: ReconciliationStrategy,
    
    /// Whether to auto-reconcile when partition heals
    pub auto_reconcile: bool,
    
    /// Minimum nodes for quorum during partition
    pub min_partition_quorum: usize,
}

impl Default for PartitionHandlingConfig {
    fn default() -> Self {
        Self {
            detection_interval_secs: 5,
            heartbeat_timeout_secs: 15,
            reconciliation_strategy: ReconciliationStrategy::default(),
            auto_reconcile: true,
            min_partition_quorum: 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_config() {
        let config = DistributedOrchestratorConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_consensus_rounds, 3);
        assert_eq!(config.consensus_timeout_ms, 5000);
    }
    
    #[test]
    fn test_decision_routing_defaults() {
        let config = DecisionRoutingConfig::default();
        assert_eq!(config.high_value_threshold, 10.0);
        assert_eq!(config.heuristic_latency_ms, 10);
        assert!(config.critical_decisions.contains(&"coordinator_election".to_string()));
    }
    
    #[test]
    fn test_context_group_defaults() {
        let config = ContextGroupConfig::default();
        assert_eq!(config.min_nodes_per_group, 2);
        assert!(config.leader_rotation_enabled);
    }
    
    #[test]
    fn test_partition_handling_defaults() {
        let config = PartitionHandlingConfig::default();
        assert!(config.auto_reconcile);
        assert_eq!(config.heartbeat_timeout_secs, 15);
    }
    
    #[test]
    fn test_config_serialization() {
        let config = DistributedOrchestratorConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: DistributedOrchestratorConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.max_consensus_rounds, config.max_consensus_rounds);
    }
    
    #[test]
    fn test_simulation_mode_default() {
        let config = DistributedOrchestratorConfig::default();
        assert!(!config.simulation_mode);
        assert!(!config.is_simulation());
    }
    
    #[test]
    fn test_simulation_mode_constructor() {
        let config = DistributedOrchestratorConfig::simulation();
        assert!(config.simulation_mode);
        assert!(config.is_simulation());
        // Other defaults should be preserved
        assert!(config.enabled);
        assert_eq!(config.max_consensus_rounds, 3);
    }
    
    #[test]
    fn test_simulation_mode_serialization() {
        let config = DistributedOrchestratorConfig::simulation();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"simulation_mode\":true"));
        
        let parsed: DistributedOrchestratorConfig = serde_json::from_str(&json).unwrap();
        assert!(parsed.simulation_mode);
    }
}

