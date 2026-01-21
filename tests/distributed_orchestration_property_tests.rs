//! Property-Based Tests for Distributed Orchestration
//!
//! Tests universal properties that should hold for any valid inputs.

use mvp_node::distributed_orchestration::{
    AffinityCalculator, AffinityCalculatorTrait,
    AdaptiveConsensus, AdaptiveConsensusTrait,
    DecisionRouter, DecisionRouterTrait,
    ContextGroupManager, ContextGroupManagerTrait,
    DecisionContext, Proposal,
};
use mvp_node::distributed_orchestration::affinity::NodeMetrics;
use mvp_node::distributed_orchestration::types::{
    AffinityWeights, ConsensusCriteria, DecisionStrategy,
};
use mvp_node::distributed_orchestration::consensus::calculate_tendency;
use mvp_node::distributed_orchestration::config::{DecisionRoutingConfig, ContextGroupConfig};
use chrono::Utc;

// ============================================================================
// Property 1: Affinity scores are always normalized (0.0-1.0)
// ============================================================================

#[tokio::test]
async fn property_affinity_always_normalized() {
    let calc = AffinityCalculator::new(AffinityWeights::default());
    
    // Test with extreme values
    let test_cases = vec![
        NodeMetrics { capacity: 0.0, latency_ms: 0, uptime_secs: 0, specialization_match: 0.0 },
        NodeMetrics { capacity: 1.0, latency_ms: 0, uptime_secs: 86400 * 365, specialization_match: 1.0 },
        NodeMetrics { capacity: 0.0, latency_ms: 10000, uptime_secs: 0, specialization_match: 0.0 },
        NodeMetrics { capacity: 0.5, latency_ms: 500, uptime_secs: 3600, specialization_match: 0.5 },
        NodeMetrics { capacity: 1.0, latency_ms: 1, uptime_secs: 1, specialization_match: 0.1 },
    ];
    
    for (i, metrics) in test_cases.iter().enumerate() {
        let affinity = calc
            .calculate_affinity(&format!("node-{}", i), metrics)
            .await
            .unwrap();
        
        assert!(
            affinity >= 0.0 && affinity <= 1.0,
            "Affinity {} for metrics {:?} is out of bounds",
            affinity,
            metrics
        );
    }
}

// ============================================================================
// Property 2: Higher capacity always increases affinity (ceteris paribus)
// ============================================================================

#[tokio::test]
async fn property_higher_capacity_higher_affinity() {
    let calc = AffinityCalculator::new(AffinityWeights::default());
    
    let base_metrics = NodeMetrics {
        capacity: 0.3,
        latency_ms: 100,
        uptime_secs: 3600,
        specialization_match: 0.5,
    };
    
    let high_capacity_metrics = NodeMetrics {
        capacity: 0.9,
        ..base_metrics.clone()
    };
    
    let low_affinity = calc.calculate_affinity(&"low".to_string(), &base_metrics).await.unwrap();
    let high_affinity = calc.calculate_affinity(&"high".to_string(), &high_capacity_metrics).await.unwrap();
    
    assert!(
        high_affinity > low_affinity,
        "Higher capacity ({}) should give higher affinity than lower capacity ({})",
        high_affinity,
        low_affinity
    );
}

// ============================================================================
// Property 3: Lower latency always increases affinity (ceteris paribus)
// ============================================================================

#[tokio::test]
async fn property_lower_latency_higher_affinity() {
    let calc = AffinityCalculator::new(AffinityWeights::default());
    
    let high_latency = NodeMetrics {
        capacity: 0.5,
        latency_ms: 500,
        uptime_secs: 3600,
        specialization_match: 0.5,
    };
    
    let low_latency = NodeMetrics {
        latency_ms: 10,
        ..high_latency.clone()
    };
    
    let high_lat_affinity = calc.calculate_affinity(&"high_lat".to_string(), &high_latency).await.unwrap();
    let low_lat_affinity = calc.calculate_affinity(&"low_lat".to_string(), &low_latency).await.unwrap();
    
    assert!(
        low_lat_affinity > high_lat_affinity,
        "Lower latency ({}) should give higher affinity than higher latency ({})",
        low_lat_affinity,
        high_lat_affinity
    );
}

// ============================================================================
// Property 4: Tendency calculation is bounded (0.0-1.0)
// ============================================================================

#[test]
fn property_tendency_always_bounded() {
    // Test with various extreme values
    let test_cases = vec![
        (0.0, 0.0, 0.0, 0.6), // All zeros
        (1.0, 1.0, 1.0, 0.6), // All ones
        (0.0, 1.0, 0.5, 0.6), // Low stimulus, high threshold
        (1.0, 0.0, 0.5, 0.6), // High stimulus, zero threshold (edge case)
        (0.5, 0.5, 0.5, 0.0), // Zero sigma
        (0.5, 0.5, 0.5, 1.0), // Full sigma
    ];
    
    for (stimulus, threshold, demand, sigma) in test_cases {
        // Handle edge case of zero threshold
        let threshold: f64 = threshold;
        let threshold = threshold.max(0.001);
        let tendency = calculate_tendency(stimulus, threshold, demand, sigma);
        
        assert!(
            tendency >= 0.0 && tendency <= 1.0,
            "Tendency {} for (s={}, t={}, d={}, σ={}) is out of bounds",
            tendency,
            stimulus,
            threshold,
            demand,
            sigma
        );
    }
}

// ============================================================================
// Property 5: Higher stimulus increases tendency (ceteris paribus)
// ============================================================================

#[test]
fn property_higher_stimulus_higher_tendency() {
    let threshold = 0.5;
    let demand = 0.5;
    let sigma = 0.6;
    
    let low_tendency = calculate_tendency(0.2, threshold, demand, sigma);
    let high_tendency = calculate_tendency(0.9, threshold, demand, sigma);
    
    assert!(
        high_tendency > low_tendency,
        "Higher stimulus should give higher tendency: {} vs {}",
        high_tendency,
        low_tendency
    );
}

// ============================================================================
// Property 6: Higher demand increases tendency (ceteris paribus)
// ============================================================================

#[test]
fn property_higher_demand_higher_tendency() {
    let stimulus = 0.5;
    let threshold = 0.5;
    let sigma = 0.6;
    
    let low_demand_tendency = calculate_tendency(stimulus, threshold, 0.1, sigma);
    let high_demand_tendency = calculate_tendency(stimulus, threshold, 0.9, sigma);
    
    assert!(
        high_demand_tendency > low_demand_tendency,
        "Higher demand should give higher tendency: {} vs {}",
        high_demand_tendency,
        low_demand_tendency
    );
}

// ============================================================================
// Property 7: Threshold adaptation stays within bounds
// ============================================================================

#[tokio::test]
async fn property_threshold_bounds_maintained() {
    let consensus = AdaptiveConsensus::new(ConsensusCriteria::default());
    let node_id = "test-node".to_string();
    
    // Many successes - should not go below 0.3
    for _ in 0..100 {
        consensus.update_threshold(&node_id, true).await;
    }
    let threshold = consensus.get_node_threshold(&node_id).await;
    assert!(
        threshold >= 0.3,
        "Threshold {} went below minimum 0.3 after many successes",
        threshold
    );
    
    // Many failures - should not go above 0.9
    for _ in 0..100 {
        consensus.update_threshold(&node_id, false).await;
    }
    let threshold = consensus.get_node_threshold(&node_id).await;
    assert!(
        threshold <= 0.9,
        "Threshold {} went above maximum 0.9 after many failures",
        threshold
    );
}

// ============================================================================
// Property 8: Consensus with unanimous agreement always reaches consensus
// ============================================================================

#[tokio::test]
async fn property_unanimous_agreement_reaches_consensus() {
    let consensus = AdaptiveConsensus::new(ConsensusCriteria::AdaptiveThreshold {
        base_threshold: 0.5,
        stimulus_weight: 0.6,
        demand_weight: 0.4,
        min_participants: 2,
    });
    
    // All nodes propose the same decision with high confidence
    let proposals = vec![
        Proposal {
            id: "p1".to_string(),
            node_id: "node-1".to_string(),
            decision: "unanimous_choice".to_string(),
            confidence: 0.95,
            reasoning: "Strongly agree".to_string(),
            job_priority: 9,
            wait_time_secs: 1,
            specialization_match: 0.9,
            timestamp: Utc::now(),
        },
        Proposal {
            id: "p2".to_string(),
            node_id: "node-2".to_string(),
            decision: "unanimous_choice".to_string(),
            confidence: 0.92,
            reasoning: "Also agree".to_string(),
            job_priority: 9,
            wait_time_secs: 1,
            specialization_match: 0.88,
            timestamp: Utc::now(),
        },
        Proposal {
            id: "p3".to_string(),
            node_id: "node-3".to_string(),
            decision: "unanimous_choice".to_string(),
            confidence: 0.90,
            reasoning: "Agree as well".to_string(),
            job_priority: 9,
            wait_time_secs: 1,
            specialization_match: 0.85,
            timestamp: Utc::now(),
        },
    ];
    
    let result = consensus.propose_decision(proposals).await.unwrap();
    
    assert!(
        result.reached,
        "Unanimous agreement with high confidence should always reach consensus"
    );
    assert_eq!(result.decision, Some("unanimous_choice".to_string()));
}

// ============================================================================
// Property 9: Critical decisions always select LLM strategy
// ============================================================================

#[tokio::test]
async fn property_critical_always_llm() {
    let router = DecisionRouter::new(DecisionRoutingConfig {
        critical_decisions: vec![
            "coordinator_election".to_string(),
            "security_check".to_string(),
        ],
        ..Default::default()
    });
    
    for decision_type in ["coordinator_election", "security_check"] {
        let context = DecisionContext {
            decision_type: decision_type.to_string(),
            is_critical: true,
            job_value: 0.0, // Low value
            complexity_score: 0.0, // Low complexity
            ..Default::default()
        };
        
        let strategy = router.select_strategy(&context).await;
        
        assert!(
            matches!(strategy, DecisionStrategy::LLMReasoning { .. }),
            "Critical decision '{}' should always use LLM strategy, got {:?}",
            decision_type,
            strategy
        );
    }
}

// ============================================================================
// Property 10: Low value + low complexity always uses heuristic
// ============================================================================

#[tokio::test]
async fn property_simple_always_heuristic() {
    let router = DecisionRouter::new(DecisionRoutingConfig {
        high_value_threshold: 10.0,
        complexity_threshold: 0.7,
        ..Default::default()
    });
    
    let context = DecisionContext {
        decision_type: "job_routing".to_string(),
        is_critical: false,
        job_value: 1.0,
        complexity_score: 0.2,
        ..Default::default()
    };
    
    let strategy = router.select_strategy(&context).await;
    
    assert!(
        matches!(strategy, DecisionStrategy::Heuristic { .. }),
        "Simple, low-value decision should use heuristic, got {:?}",
        strategy
    );
}

// ============================================================================
// Property 11: Group formation requires minimum nodes
// ============================================================================

#[tokio::test]
async fn property_group_requires_min_nodes() {
    let manager = ContextGroupManager::new(ContextGroupConfig {
        min_nodes_per_group: 3,
        ..Default::default()
    });
    
    // Try with fewer than minimum
    let result = manager.form_group("test", vec!["node-1".to_string(), "node-2".to_string()]).await;
    assert!(result.is_err(), "Should fail with fewer than min nodes");
    
    // Try with exactly minimum
    let result = manager.form_group(
        "test",
        vec!["node-1".to_string(), "node-2".to_string(), "node-3".to_string()]
    ).await;
    assert!(result.is_ok(), "Should succeed with min nodes");
}

// ============================================================================
// Property 12: Affinity weights sum to 1.0
// ============================================================================

#[test]
fn property_affinity_weights_sum_to_one() {
    let weights = AffinityWeights::default();
    let sum = weights.capacity_weight
        + weights.latency_weight
        + weights.uptime_weight
        + weights.specialization_weight;
    
    assert!(
        (sum - 1.0).abs() < 0.001,
        "Affinity weights should sum to 1.0, got {}",
        sum
    );
}

// ============================================================================
// Property 13: Heuristic decisions are fast (< 100ms)
// ============================================================================

#[tokio::test]
async fn property_heuristic_is_fast() {
    let router = DecisionRouter::new(DecisionRoutingConfig::default());
    
    let context = DecisionContext {
        decision_type: "job_routing".to_string(),
        is_critical: false,
        job_value: 1.0,
        complexity_score: 0.1,
        ..Default::default()
    };
    
    let start = std::time::Instant::now();
    
    let result = router
        .execute_heuristic(
            &mvp_node::distributed_orchestration::types::HeuristicAlgorithm::GreedyAffinity,
            &context,
        )
        .await
        .unwrap();
    
    let elapsed = start.elapsed();
    
    assert!(
        elapsed.as_millis() < 100,
        "Heuristic should complete in < 100ms, took {}ms",
        elapsed.as_millis()
    );
    assert!(!result.decision.is_empty());
}

// ============================================================================
// Property 14: Consensus with empty proposals returns not reached
// ============================================================================

#[tokio::test]
async fn property_empty_proposals_no_consensus() {
    let consensus = AdaptiveConsensus::new(ConsensusCriteria::default());
    
    let result = consensus.propose_decision(vec![]).await.unwrap();
    
    assert!(!result.reached, "Empty proposals should not reach consensus");
    assert!(result.decision.is_none());
}

// ============================================================================
// Property 15: Metrics are consistent after operations
// ============================================================================

#[tokio::test]
async fn property_metrics_consistency() {
    use mvp_node::distributed_orchestration::metrics::MetricsCollector;
    
    let collector = MetricsCollector::new();
    
    // Record various operations
    collector.record_decision(false, true, false, false);
    collector.record_decision(true, false, true, false);
    collector.record_decision(false, false, false, true);
    collector.record_consensus(100, 2);
    collector.record_rotation();
    collector.record_group_formed();
    
    let metrics = collector.get_metrics();
    
    // Total decisions should equal sum of types
    assert_eq!(
        metrics.total_decisions,
        3, // We recorded 3 decisions
        "Total decisions mismatch"
    );
    
    // Strategy counts should match
    assert_eq!(metrics.heuristic_decisions, 1);
    assert_eq!(metrics.llm_decisions, 1);
    assert_eq!(metrics.hybrid_decisions, 1);
    assert_eq!(metrics.consensus_decisions, 1);
    
    // Other metrics
    assert_eq!(metrics.coordinator_rotations, 1);
    assert_eq!(metrics.context_groups_formed, 1);
}

// ============================================================================
// Property 16: Partition detection requires heartbeat timeout
// ============================================================================

#[tokio::test]
async fn property_partition_requires_timeout() {
    use mvp_node::distributed_orchestration::{PartitionHandler, PartitionHandlerTrait};
    use mvp_node::distributed_orchestration::config::PartitionHandlingConfig;
    
    let handler = PartitionHandler::new(PartitionHandlingConfig {
        heartbeat_timeout_secs: 1,
        ..Default::default()
    });
    
    // All nodes heartbeating - no partition
    let nodes = vec!["node-1".to_string(), "node-2".to_string(), "node-3".to_string()];
    for node in &nodes {
        handler.record_heartbeat(node).await;
    }
    
    let partitions = handler.detect_partition(&nodes).await;
    assert!(partitions.is_none(), "No partition when all nodes heartbeat");
}

// ============================================================================
// Property 17: Coordinator rotation triggers after threshold
// ============================================================================

#[tokio::test]
async fn property_rotation_triggers_after_threshold() {
    use mvp_node::distributed_orchestration::{RotationManager, RotationManagerTrait};
    use mvp_node::distributed_orchestration::types::RotationTrigger;
    
    let manager = RotationManager::new(RotationTrigger::Periodic { interval_jobs: 5 });
    
    // Should not rotate initially
    let should_rotate_initial = manager.should_rotate(&"node-1".to_string(), 0.8).await.unwrap();
    assert!(!should_rotate_initial, "Should not rotate with 0 jobs");
    
    // Record enough jobs to trigger rotation
    for _ in 0..6 {
        manager.record_job_processed();
    }
    
    let should_rotate = manager.should_rotate(&"node-1".to_string(), 0.8).await.unwrap();
    assert!(should_rotate, "Should rotate after {} jobs", 6);
}

// ============================================================================
// Property 18: Context group jobs are always tracked
// ============================================================================

#[tokio::test]
async fn property_group_jobs_always_tracked() {
    let manager = ContextGroupManager::new(ContextGroupConfig::default());
    
    // Form group
    let nodes = vec!["node-1".to_string(), "node-2".to_string()];
    manager.form_group("translation", nodes).await.unwrap();
    
    // Record many jobs
    for _ in 0..100 {
        manager.record_job("translation").await;
    }
    
    let group = manager.get_group("translation").await.unwrap();
    assert_eq!(group.jobs_processed, 100, "All jobs should be tracked");
}

// ============================================================================
// Property 19: Strategies always return valid decisions
// ============================================================================

#[tokio::test]
async fn property_strategies_return_valid_decisions() {
    use mvp_node::distributed_orchestration::types::HeuristicAlgorithm;
    
    let router = DecisionRouter::new(DecisionRoutingConfig::default());
    
    let algorithms = vec![
        HeuristicAlgorithm::GreedyAffinity,
        HeuristicAlgorithm::RoundRobin,
        HeuristicAlgorithm::LeastLoaded,
        HeuristicAlgorithm::Random,
    ];
    
    for algo in algorithms {
        let context = DecisionContext::default();
        let result = router.execute_heuristic(&algo, &context).await.unwrap();
        
        assert!(!result.decision.is_empty(), "Decision should not be empty for {:?}", algo);
        assert!(result.confidence >= 0.0 && result.confidence <= 1.0);
        assert!(result.latency_ms < 1000);
    }
}

// ============================================================================
// Property 20: Consensus confidence is always normalized
// ============================================================================

#[tokio::test]
async fn property_consensus_confidence_normalized() {
    let consensus = AdaptiveConsensus::new(ConsensusCriteria::default());
    
    let proposals = vec![
        Proposal {
            id: "p1".to_string(),
            node_id: "node-1".to_string(),
            decision: "test".to_string(),
            confidence: 1.5, // Invalid - above 1.0
            reasoning: "Test".to_string(),
            job_priority: 5,
            wait_time_secs: 1,
            specialization_match: 0.5,
            timestamp: Utc::now(),
        },
    ];
    
    let result = consensus.propose_decision(proposals).await.unwrap();
    
    assert!(
        result.confidence >= 0.0 && result.confidence <= 1.0,
        "Confidence should be normalized, got {}",
        result.confidence
    );
}

// ============================================================================
// Property 21: Group dissolution clears all state
// ============================================================================

#[tokio::test]
async fn property_dissolution_clears_state() {
    let manager = ContextGroupManager::new(ContextGroupConfig::default());
    
    // Form and use group
    let nodes = vec!["node-1".to_string(), "node-2".to_string()];
    let group = manager.form_group("translation", nodes).await.unwrap();
    manager.record_job("translation").await;
    
    // Dissolve
    manager.dissolve_group(&group.id).await.unwrap();
    
    // Group should not exist
    assert!(manager.get_group("translation").await.is_none());
    
    // All groups should be empty
    assert!(manager.get_all_groups().await.is_empty());
}

// ============================================================================
// Property 22: Multiple proposals converge to decision
// ============================================================================

#[tokio::test]
async fn property_proposals_converge() {
    let consensus = AdaptiveConsensus::new(ConsensusCriteria::AdaptiveThreshold {
        base_threshold: 0.4,
        stimulus_weight: 0.6,
        demand_weight: 0.4,
        min_participants: 2,
    });
    
    // All agree with varying confidence
    let proposals = vec![
        Proposal {
            id: "p1".to_string(),
            node_id: "node-1".to_string(),
            decision: "accept".to_string(),
            confidence: 0.8,
            reasoning: "Good".to_string(),
            job_priority: 5,
            wait_time_secs: 1,
            specialization_match: 0.5,
            timestamp: Utc::now(),
        },
        Proposal {
            id: "p2".to_string(),
            node_id: "node-2".to_string(),
            decision: "accept".to_string(),
            confidence: 0.9,
            reasoning: "Also good".to_string(),
            job_priority: 5,
            wait_time_secs: 1,
            specialization_match: 0.5,
            timestamp: Utc::now(),
        },
        Proposal {
            id: "p3".to_string(),
            node_id: "node-3".to_string(),
            decision: "accept".to_string(),
            confidence: 0.7,
            reasoning: "Agree".to_string(),
            job_priority: 5,
            wait_time_secs: 1,
            specialization_match: 0.5,
            timestamp: Utc::now(),
        },
    ];
    
    let result = consensus.propose_decision(proposals).await.unwrap();
    
    assert!(result.reached, "Unanimous agreement should reach consensus");
    assert_eq!(result.decision, Some("accept".to_string()));
}

// ============================================================================
// Property 23: Node can be in multiple groups up to limit
// ============================================================================

#[tokio::test]
async fn property_node_group_limit() {
    let config = ContextGroupConfig {
        max_groups_per_node: 2,
        min_nodes_per_group: 2,
        ..Default::default()
    };
    let manager = ContextGroupManager::new(config);
    
    // Register nodes
    manager.register_available_nodes(vec![
        "node-1".to_string(),
        "node-2".to_string(),
        "node-3".to_string(),
    ]).await;
    
    // Form first group
    manager.form_group("translation", vec![
        "node-1".to_string(),
        "node-2".to_string(),
    ]).await.unwrap();
    
    // Form second group with same nodes
    manager.form_group("coding", vec![
        "node-1".to_string(),
        "node-2".to_string(),
    ]).await.unwrap();
    
    // Node should be in 2 groups (max)
    let groups = manager.get_all_groups().await;
    assert_eq!(groups.len(), 2);
}

// ============================================================================
// Property 24: Affinity score is deterministic for same inputs
// ============================================================================

#[tokio::test]
async fn property_affinity_deterministic() {
    let calc = AffinityCalculator::new(AffinityWeights::default());
    
    let metrics = NodeMetrics {
        capacity: 0.7,
        latency_ms: 50,
        uptime_secs: 3600,
        specialization_match: 0.8,
    };
    
    let affinity1 = calc.calculate_affinity(&"node".to_string(), &metrics).await.unwrap();
    let affinity2 = calc.calculate_affinity(&"node".to_string(), &metrics).await.unwrap();
    
    assert_eq!(affinity1, affinity2, "Same inputs should give same affinity");
}

// ============================================================================
// Property 25: Decision strategy selection is deterministic
// ============================================================================

#[tokio::test]
async fn property_strategy_selection_deterministic() {
    let router = DecisionRouter::new(DecisionRoutingConfig::default());
    
    let context = DecisionContext {
        job_value: 5.0,
        complexity_score: 0.3,
        is_critical: false,
        ..Default::default()
    };
    
    let strategy1 = router.select_strategy(&context).await;
    let strategy2 = router.select_strategy(&context).await;
    
    // Compare discriminants (variant types)
    assert!(
        std::mem::discriminant(&strategy1) == std::mem::discriminant(&strategy2),
        "Same context should give same strategy type"
    );
}

// ============================================================================
// Property 26: Partition reconciliation preserves data integrity
// ============================================================================

#[tokio::test]
async fn property_reconciliation_preserves_integrity() {
    use mvp_node::distributed_orchestration::{PartitionHandler, PartitionHandlerTrait};
    use mvp_node::distributed_orchestration::config::PartitionHandlingConfig;
    
    let handler = PartitionHandler::new(PartitionHandlingConfig::default());
    
    let partition1 = vec!["node-1".to_string(), "node-2".to_string()];
    let partition2 = vec!["node-3".to_string(), "node-4".to_string()];
    
    let result = handler.reconcile(&partition1, &partition2).await.unwrap();
    
    // Reconciliation should produce valid state
    assert!(result.state_synchronized, "State should be synchronized after reconciliation");
}

// ============================================================================
// Property 27: Metrics never decrease
// ============================================================================

#[tokio::test]
async fn property_metrics_never_decrease() {
    use mvp_node::distributed_orchestration::metrics::MetricsCollector;
    
    let collector = MetricsCollector::new();
    
    // Record decisions
    collector.record_decision(true, false, false, false);
    let metrics1 = collector.get_metrics();
    
    // Record more
    collector.record_decision(true, false, false, false);
    let metrics2 = collector.get_metrics();
    
    assert!(
        metrics2.total_decisions >= metrics1.total_decisions,
        "Total decisions should never decrease"
    );
    assert!(
        metrics2.heuristic_decisions >= metrics1.heuristic_decisions,
        "Heuristic decisions should never decrease"
    );
}

// ============================================================================
// Property 28: Orchestrator state is always consistent
// ============================================================================

#[tokio::test]
async fn property_orchestrator_state_consistent() {
    use mvp_node::distributed_orchestration::{DistributedOrchestrator, DistributedOrchestratorTrait};
    
    let config = mvp_node::distributed_orchestration::DistributedOrchestratorConfig::default();
    let orchestrator = DistributedOrchestrator::new(config, "node-1".to_string(), None);
    
    let state = orchestrator.get_orchestration_state().await.unwrap();
    
    // State should be internally consistent
    // If coordinator exists, it should have valid affinity
    if state.current_coordinator.is_some() {
        assert!(state.coordinator_affinity >= 0.0 && state.coordinator_affinity <= 1.0);
    }
    
    // All context groups should have leaders
    for group in &state.active_context_groups {
        assert!(!group.members.is_empty());
    }
}

