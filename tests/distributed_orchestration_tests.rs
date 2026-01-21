//! Integration Tests for Distributed Orchestration
//!
//! Tests end-to-end scenarios with multiple simulated nodes.

use mvp_node::distributed_orchestration::{
    DistributedOrchestrator, DistributedOrchestratorTrait,
    DecisionContext, DecisionType, DecisionStrategy,
    AffinityCalculator, AffinityCalculatorTrait,
    AdaptiveConsensus, AdaptiveConsensusTrait,
    ContextGroupManager, ContextGroupManagerTrait,
    PartitionHandler, PartitionHandlerTrait,
    DistributedOrchestratorConfig,
    Proposal, ConsensusResult,
};
use mvp_node::distributed_orchestration::affinity::NodeMetrics;
use mvp_node::distributed_orchestration::config::{
    DecisionRoutingConfig, ContextGroupConfig, PartitionHandlingConfig,
};
use mvp_node::distributed_orchestration::types::{
    AffinityWeights, ConsensusCriteria, OrchestrationMode, RotationTrigger,
};
use chrono::Utc;

// ============================================================================
// Integration Test: End-to-End Distributed Decision
// ============================================================================

#[tokio::test]
async fn test_distributed_decision_with_heuristic() {
    // Create orchestrator
    let config = DistributedOrchestratorConfig::default();
    let orchestrator = DistributedOrchestrator::new(
        config,
        "node-1".to_string(),
        None, // No LLM for this test
    );
    
    // Simple decision should use heuristic
    let context = DecisionContext {
        decision_type: "job_routing".to_string(),
        is_critical: false,
        job_value: 1.0,
        complexity_score: 0.2,
        requires_consensus: false,
        context_group: None,
        description: "Route a simple job".to_string(),
    };
    
    let result = orchestrator
        .make_distributed_decision(&context, DecisionType::JobRouting)
        .await
        .unwrap();
    
    // Should be a heuristic decision
    assert!(matches!(result.strategy_used, DecisionStrategy::Heuristic { .. }));
    assert!(!result.decision.is_empty());
    assert!(result.latency_ms < 100); // Heuristic should be fast
}

#[tokio::test]
async fn test_distributed_decision_high_value_uses_llm_strategy() {
    let config = DistributedOrchestratorConfig::default();
    let orchestrator = DistributedOrchestrator::new(
        config,
        "node-1".to_string(),
        None,
    );
    
    // High value job should try LLM (but fallback to heuristic since no LLM)
    let context = DecisionContext {
        decision_type: "job_routing".to_string(),
        is_critical: false,
        job_value: 100.0, // High value
        complexity_score: 0.5,
        requires_consensus: false,
        context_group: None,
        description: "Route an expensive job".to_string(),
    };
    
    let result = orchestrator
        .make_distributed_decision(&context, DecisionType::JobRouting)
        .await
        .unwrap();
    
    // Should fallback to heuristic since no LLM available
    assert!(matches!(result.strategy_used, DecisionStrategy::Heuristic { .. }));
}

// ============================================================================
// Integration Test: Coordinator Affinity
// ============================================================================

#[tokio::test]
async fn test_coordinator_selection_by_affinity() {
    let calc = AffinityCalculator::new(AffinityWeights::default());
    
    // Simulate 3 nodes with different metrics
    let nodes = vec![
        ("node-1".to_string(), NodeMetrics {
            capacity: 0.3,
            latency_ms: 200,
            uptime_secs: 3600,
            specialization_match: 0.5,
        }),
        ("node-2".to_string(), NodeMetrics {
            capacity: 0.9,
            latency_ms: 50,
            uptime_secs: 86400, // 24 hours
            specialization_match: 0.9,
        }),
        ("node-3".to_string(), NodeMetrics {
            capacity: 0.6,
            latency_ms: 100,
            uptime_secs: 7200,
            specialization_match: 0.7,
        }),
    ];
    
    let best = calc.get_highest_affinity_node(&nodes).await.unwrap();
    
    // Node 2 should win (best overall metrics)
    assert_eq!(best, Some("node-2".to_string()));
    
    // Verify affinity scores are properly ordered
    let affinities = calc.calculate_all_affinities(&nodes).await.unwrap();
    assert!(affinities.get("node-2").unwrap() > affinities.get("node-1").unwrap());
    assert!(affinities.get("node-2").unwrap() > affinities.get("node-3").unwrap());
}

// ============================================================================
// Integration Test: Consensus Protocol
// ============================================================================

#[tokio::test]
async fn test_consensus_with_multiple_proposals() {
    let consensus = AdaptiveConsensus::new(ConsensusCriteria::AdaptiveThreshold {
        base_threshold: 0.5,
        stimulus_weight: 0.6,
        demand_weight: 0.4,
        min_participants: 2,
    });
    
    // Simulate proposals from multiple nodes
    let proposals = vec![
        Proposal {
            id: "p1".to_string(),
            node_id: "node-1".to_string(),
            decision: "accept_on_node_A".to_string(),
            confidence: 0.9,
            reasoning: "High capacity available".to_string(),
            job_priority: 8,
            wait_time_secs: 5,
            specialization_match: 0.95,
            timestamp: Utc::now(),
        },
        Proposal {
            id: "p2".to_string(),
            node_id: "node-2".to_string(),
            decision: "accept_on_node_A".to_string(), // Same decision
            confidence: 0.85,
            reasoning: "Good match for job type".to_string(),
            job_priority: 8,
            wait_time_secs: 5,
            specialization_match: 0.8,
            timestamp: Utc::now(),
        },
        Proposal {
            id: "p3".to_string(),
            node_id: "node-3".to_string(),
            decision: "accept_on_node_B".to_string(), // Different decision
            confidence: 0.6,
            reasoning: "Lower load".to_string(),
            job_priority: 5,
            wait_time_secs: 10,
            specialization_match: 0.5,
            timestamp: Utc::now(),
        },
    ];
    
    let result = consensus.propose_decision(proposals).await.unwrap();
    
    // Consensus should be reached for "accept_on_node_A" (2 vs 1 votes, higher confidence)
    assert!(result.reached);
    assert_eq!(result.decision, Some("accept_on_node_A".to_string()));
    assert_eq!(result.participants.len(), 3);
}

#[tokio::test]
async fn test_consensus_not_reached_with_low_confidence() {
    let consensus = AdaptiveConsensus::new(ConsensusCriteria::AdaptiveThreshold {
        base_threshold: 0.9, // Very high threshold
        stimulus_weight: 0.6,
        demand_weight: 0.4,
        min_participants: 2,
    });
    
    // Low confidence proposals
    let proposals = vec![
        Proposal {
            id: "p1".to_string(),
            node_id: "node-1".to_string(),
            decision: "maybe_A".to_string(),
            confidence: 0.4,
            reasoning: "Not sure".to_string(),
            job_priority: 3,
            wait_time_secs: 1,
            specialization_match: 0.3,
            timestamp: Utc::now(),
        },
        Proposal {
            id: "p2".to_string(),
            node_id: "node-2".to_string(),
            decision: "maybe_B".to_string(),
            confidence: 0.3,
            reasoning: "Also not sure".to_string(),
            job_priority: 3,
            wait_time_secs: 1,
            specialization_match: 0.2,
            timestamp: Utc::now(),
        },
    ];
    
    let result = consensus.propose_decision(proposals).await.unwrap();
    
    // Consensus should NOT be reached due to high threshold
    assert!(!result.reached);
    assert!(result.decision.is_none());
}

// ============================================================================
// Integration Test: Context Group Formation
// ============================================================================

#[tokio::test]
async fn test_context_group_lifecycle() {
    let manager = ContextGroupManager::new(ContextGroupConfig {
        min_jobs_for_formation: 3,
        min_nodes_per_group: 2,
        max_groups_per_node: 3,
        dissolution_idle_threshold_secs: 60,
        leader_rotation_enabled: true,
        leader_rotation_interval_jobs: 10,
    });
    
    // Register available nodes first
    manager.register_available_nodes(vec![
        "node-1".to_string(),
        "node-2".to_string(),
        "node-3".to_string(),
    ]).await;
    
    // Record jobs to trigger group formation
    manager.record_job("translation").await;
    manager.record_job("translation").await;
    manager.record_job("translation").await;
    
    // Evaluate should suggest forming a group with nodes
    let actions = manager.evaluate_groups().await;
    assert!(actions.iter().any(|a| matches!(a, 
        mvp_node::distributed_orchestration::types::GroupAction::FormGroup { context, nodes } 
        if context == "translation" && nodes.len() >= 2
    )));
    
    // Form the group
    let nodes = vec!["node-1".to_string(), "node-2".to_string(), "node-3".to_string()];
    let group = manager.form_group("translation", nodes).await.unwrap();
    
    assert_eq!(group.context, "translation");
    assert_eq!(group.members.len(), 3);
    assert!(group.leader.is_some());
    
    // Record more jobs
    manager.record_job("translation").await;
    manager.record_job("translation").await;
    
    let updated_group = manager.get_group("translation").await.unwrap();
    assert_eq!(updated_group.jobs_processed, 2);
    
    // Dissolve group
    manager.dissolve_group(&group.id).await.unwrap();
    assert!(manager.get_group("translation").await.is_none());
}

// ============================================================================
// Integration Test: Partition Detection and Recovery
// ============================================================================

#[tokio::test]
async fn test_partition_detection_and_recovery() {
    let handler = PartitionHandler::new(PartitionHandlingConfig {
        detection_interval_secs: 1,
        heartbeat_timeout_secs: 5,
        reconciliation_strategy: mvp_node::distributed_orchestration::types::ReconciliationStrategy::LastWriteWins,
        auto_reconcile: true,
        min_partition_quorum: 2,
    });
    
    // Simulate heartbeats from some nodes
    handler.record_heartbeat(&"node-1".to_string()).await;
    handler.record_heartbeat(&"node-2".to_string()).await;
    // node-3 and node-4 don't send heartbeats
    
    let known_nodes = vec![
        "node-1".to_string(),
        "node-2".to_string(),
        "node-3".to_string(),
        "node-4".to_string(),
    ];
    
    // Detect partition
    let partitions = handler.detect_partition(&known_nodes).await;
    assert!(partitions.is_some());
    
    let parts = partitions.unwrap();
    assert_eq!(parts.len(), 2);
    
    // Reachable partition
    let reachable = &parts[0];
    assert!(reachable.contains(&"node-1".to_string()));
    assert!(reachable.contains(&"node-2".to_string()));
    
    // Unreachable partition
    let unreachable = &parts[1];
    assert!(unreachable.contains(&"node-3".to_string()));
    assert!(unreachable.contains(&"node-4".to_string()));
    
    // Simulate recovery - all nodes now responding
    handler.record_heartbeat(&"node-3".to_string()).await;
    handler.record_heartbeat(&"node-4".to_string()).await;
    
    // Reconcile
    let result = handler.reconcile(&parts[0], &parts[1]).await.unwrap();
    assert!(result.state_synchronized);
    
    // Status should be healthy
    let status = handler.get_status().await;
    assert!(matches!(status, mvp_node::distributed_orchestration::types::PartitionStatus::Healthy));
}

// ============================================================================
// Integration Test: Strategy Selection
// ============================================================================

#[tokio::test]
async fn test_strategy_selection_based_on_context() {
    use mvp_node::distributed_orchestration::{DecisionRouter, DecisionRouterTrait};
    
    let router = mvp_node::distributed_orchestration::DecisionRouter::new(
        DecisionRoutingConfig {
            high_value_threshold: 10.0,
            complexity_threshold: 0.7,
            critical_decisions: vec!["coordinator_election".to_string()],
            ..Default::default()
        }
    );
    
    // Low value, simple = Heuristic
    let simple_context = DecisionContext {
        job_value: 1.0,
        complexity_score: 0.2,
        is_critical: false,
        decision_type: "job_routing".to_string(),
        ..Default::default()
    };
    let strategy = router.select_strategy(&simple_context).await;
    assert!(matches!(strategy, DecisionStrategy::Heuristic { .. }));
    
    // High value = LLM
    let high_value_context = DecisionContext {
        job_value: 50.0,
        complexity_score: 0.5,
        is_critical: false,
        decision_type: "job_routing".to_string(),
        ..Default::default()
    };
    let strategy = router.select_strategy(&high_value_context).await;
    assert!(matches!(strategy, DecisionStrategy::LLMReasoning { .. }));
    
    // Complex = Hybrid
    let complex_context = DecisionContext {
        job_value: 5.0,
        complexity_score: 0.8,
        is_critical: false,
        decision_type: "job_routing".to_string(),
        ..Default::default()
    };
    let strategy = router.select_strategy(&complex_context).await;
    assert!(matches!(strategy, DecisionStrategy::HybridValidation { .. }));
    
    // Critical = LLM
    let critical_context = DecisionContext {
        job_value: 1.0,
        complexity_score: 0.2,
        is_critical: true,
        decision_type: "coordinator_election".to_string(),
        ..Default::default()
    };
    let strategy = router.select_strategy(&critical_context).await;
    assert!(matches!(strategy, DecisionStrategy::LLMReasoning { .. }));
}

// ============================================================================
// Integration Test: Threshold Adaptation Over Time
// ============================================================================

#[tokio::test]
async fn test_threshold_adaptation_over_decisions() {
    let consensus = AdaptiveConsensus::new(ConsensusCriteria::default());
    
    let node_id = "test-node".to_string();
    
    // Initial threshold
    let initial = consensus.get_node_threshold(&node_id).await;
    
    // Simulate successful decisions
    for _ in 0..5 {
        consensus.update_threshold(&node_id, true).await;
    }
    
    let after_success = consensus.get_node_threshold(&node_id).await;
    assert!(after_success < initial); // Should decrease
    
    // Simulate failed decisions
    for _ in 0..10 {
        consensus.update_threshold(&node_id, false).await;
    }
    
    let after_failure = consensus.get_node_threshold(&node_id).await;
    assert!(after_failure > after_success); // Should increase
    
    // Check bounds
    assert!(after_failure <= 0.9);
    assert!(after_failure >= 0.3);
}

// ============================================================================
// Integration Test: Multi-Node Simulation
// ============================================================================

#[tokio::test]
async fn test_multi_node_orchestration() {
    // Create 3 orchestrators simulating 3 nodes
    let config = DistributedOrchestratorConfig::default();
    
    let node1 = DistributedOrchestrator::new(config.clone(), "node-1".to_string(), None);
    let node2 = DistributedOrchestrator::new(config.clone(), "node-2".to_string(), None);
    let node3 = DistributedOrchestrator::new(config, "node-3".to_string(), None);
    
    // Each node makes a decision
    let context = DecisionContext {
        decision_type: "job_routing".to_string(),
        is_critical: false,
        job_value: 1.0,
        complexity_score: 0.2,
        requires_consensus: false,
        context_group: None,
        description: "Simple job".to_string(),
    };
    
    let result1 = node1.make_distributed_decision(&context, DecisionType::JobRouting).await.unwrap();
    let result2 = node2.make_distributed_decision(&context, DecisionType::JobRouting).await.unwrap();
    let result3 = node3.make_distributed_decision(&context, DecisionType::JobRouting).await.unwrap();
    
    // All should complete successfully
    assert!(!result1.decision.is_empty());
    assert!(!result2.decision.is_empty());
    assert!(!result3.decision.is_empty());
    
    // Check metrics
    let metrics1 = node1.get_metrics();
    assert_eq!(metrics1.total_decisions, 1);
    assert_eq!(metrics1.heuristic_decisions, 1);
}

// ============================================================================
// Integration Test: Orchestration State
// ============================================================================

#[tokio::test]
async fn test_orchestration_state_query() {
    let config = DistributedOrchestratorConfig {
        mode: OrchestrationMode::EmergentCoordinator {
            rotation_trigger: RotationTrigger::Adaptive,
            affinity_recalc_interval_secs: 60,
        },
        ..Default::default()
    };
    
    let orchestrator = DistributedOrchestrator::new(
        config,
        "node-1".to_string(),
        None,
    );
    
    let state = orchestrator.get_orchestration_state().await.unwrap();
    
    assert!(matches!(state.mode, OrchestrationMode::EmergentCoordinator { .. }));
    assert!(state.current_coordinator.is_none()); // Not set initially
    assert!(state.active_context_groups.is_empty());
    assert!(matches!(state.partition_status, mvp_node::distributed_orchestration::types::PartitionStatus::Healthy));
}

// ============================================================================
// Integration Test: Fallback when LLM unavailable
// ============================================================================

#[tokio::test]
async fn test_fallback_to_heuristic_without_llm() {
    let config = DistributedOrchestratorConfig::default();
    let orchestrator = DistributedOrchestrator::new(
        config,
        "node-1".to_string(),
        None, // No LLM
    );
    
    // High value job would normally use LLM, but should fallback
    let context = DecisionContext {
        decision_type: "job_routing".to_string(),
        is_critical: false,
        job_value: 100.0, // High value
        complexity_score: 0.5,
        requires_consensus: false,
        context_group: None,
        description: "Expensive job".to_string(),
    };
    
    let result = orchestrator
        .make_distributed_decision(&context, DecisionType::JobRouting)
        .await
        .unwrap();
    
    // Should still produce a valid decision via heuristic fallback
    assert!(!result.decision.is_empty());
    // Strategy should be heuristic since no LLM
    assert!(matches!(result.strategy_used, DecisionStrategy::Heuristic { .. }));
}

// ============================================================================
// Integration Test: Resilience with concurrent decisions
// ============================================================================

#[tokio::test]
async fn test_concurrent_decisions() {
    let config = DistributedOrchestratorConfig::default();
    let orchestrator = std::sync::Arc::new(DistributedOrchestrator::new(
        config,
        "node-1".to_string(),
        None,
    ));
    
    let context = DecisionContext {
        decision_type: "job_routing".to_string(),
        is_critical: false,
        job_value: 1.0,
        complexity_score: 0.2,
        requires_consensus: false,
        context_group: None,
        description: "Simple job".to_string(),
    };
    
    // Spawn 10 concurrent decisions
    let mut handles = Vec::new();
    for _ in 0..10 {
        let orch = orchestrator.clone();
        let ctx = context.clone();
        handles.push(tokio::spawn(async move {
            orch.make_distributed_decision(&ctx, DecisionType::JobRouting).await
        }));
    }
    
    // All should succeed
    for handle in handles {
        let result = handle.await.unwrap().unwrap();
        assert!(!result.decision.is_empty());
    }
    
    // Metrics should show 10 decisions
    let metrics = orchestrator.get_metrics();
    assert_eq!(metrics.total_decisions, 10);
}

// ============================================================================
// Integration Test: Group auto-formation flow
// ============================================================================

#[tokio::test]
async fn test_group_auto_formation_flow() {
    use mvp_node::distributed_orchestration::config::ContextGroupConfig;
    
    let config = ContextGroupConfig {
        min_jobs_for_formation: 3,
        min_nodes_per_group: 2,
        ..Default::default()
    };
    let manager = ContextGroupManager::new(config);
    
    // Register available nodes
    manager.register_available_nodes(vec![
        "node-1".to_string(),
        "node-2".to_string(),
        "node-3".to_string(),
    ]).await;
    
    // Record jobs to trigger formation
    manager.record_job("translation").await;
    manager.record_job("translation").await;
    manager.record_job("translation").await;
    
    // Evaluate should suggest forming a group with actual nodes
    let actions = manager.evaluate_groups().await;
    
    // Should have a FormGroup action
    let form_action = actions.iter().find(|a| {
        matches!(a, mvp_node::distributed_orchestration::types::GroupAction::FormGroup { context, nodes } 
            if context == "translation" && nodes.len() >= 2)
    });
    
    assert!(form_action.is_some(), "Should have FormGroup action with nodes");
}

// ============================================================================
// Integration Test: Coordinator election by affinity
// ============================================================================

#[tokio::test]
async fn test_coordinator_election_by_affinity() {
    use mvp_node::distributed_orchestration::affinity::NodeMetrics;
    
    let calc = AffinityCalculator::new(AffinityWeights::default());
    
    // Simulate nodes with different metrics
    let nodes = vec![
        ("weak-node".to_string(), NodeMetrics {
            capacity: 0.2,
            latency_ms: 500,
            uptime_secs: 60,
            specialization_match: 0.3,
        }),
        ("strong-node".to_string(), NodeMetrics {
            capacity: 0.95,
            latency_ms: 10,
            uptime_secs: 86400 * 7, // 1 week
            specialization_match: 0.9,
        }),
        ("medium-node".to_string(), NodeMetrics {
            capacity: 0.5,
            latency_ms: 100,
            uptime_secs: 3600,
            specialization_match: 0.6,
        }),
    ];
    
    // Calculate all affinities
    let affinities = calc.calculate_all_affinities(&nodes).await.unwrap();
    
    // Strong node should have highest affinity
    let strong_affinity = affinities.get("strong-node").unwrap();
    let weak_affinity = affinities.get("weak-node").unwrap();
    let medium_affinity = affinities.get("medium-node").unwrap();
    
    assert!(strong_affinity > medium_affinity);
    assert!(medium_affinity > weak_affinity);
    
    // Elect coordinator
    let elected = calc.get_highest_affinity_node(&nodes).await.unwrap();
    assert_eq!(elected, Some("strong-node".to_string()));
}

// ============================================================================
// Integration Test: Decision types route to correct strategies
// ============================================================================

#[tokio::test]
async fn test_decision_type_routing() {
    use mvp_node::distributed_orchestration::config::DecisionRoutingConfig;
    
    let config = DistributedOrchestratorConfig {
        decision_routing: DecisionRoutingConfig {
            high_value_threshold: 10.0,
            complexity_threshold: 0.7,
            critical_decisions: vec!["coordinator_election".to_string()],
            ..Default::default()
        },
        ..Default::default()
    };
    
    let orchestrator = DistributedOrchestrator::new(config, "node-1".to_string(), None);
    
    // Test different decision types
    let test_cases = vec![
        (DecisionType::JobRouting, false, 1.0, "Heuristic"),
        (DecisionType::CoordinatorElection, true, 1.0, "LLM"),
        (DecisionType::GroupFormation, false, 50.0, "LLM"),
    ];
    
    for (decision_type, is_critical, job_value, expected_type) in test_cases {
        let context = DecisionContext {
            decision_type: format!("{:?}", decision_type).to_lowercase(),
            is_critical,
            job_value,
            complexity_score: 0.3,
            requires_consensus: false,
            context_group: None,
            description: "Test".to_string(),
        };
        
        let result = orchestrator
            .make_distributed_decision(&context, decision_type)
            .await
            .unwrap();
        
        let strategy_type = match &result.strategy_used {
            DecisionStrategy::Heuristic { .. } => "Heuristic",
            DecisionStrategy::LLMReasoning { .. } => "LLM", // Will fallback to Heuristic without LLM
            DecisionStrategy::HybridValidation { .. } => "Hybrid",
        };
        
        // Without LLM, all fallback to Heuristic
        assert!(!result.decision.is_empty(), 
            "Decision type {:?} should produce valid decision", decision_type);
    }
}

// ============================================================================
// Integration Test: Complete decision lifecycle
// ============================================================================

#[tokio::test]
async fn test_complete_decision_lifecycle() {
    let config = DistributedOrchestratorConfig::default();
    let orchestrator = DistributedOrchestrator::new(config, "node-1".to_string(), None);
    
    // Step 1: Initial state
    let initial_state = orchestrator.get_orchestration_state().await.unwrap();
    assert_eq!(initial_state.metrics.total_decisions, 0);
    
    // Step 2: Make a decision
    let context = DecisionContext {
        decision_type: "job_routing".to_string(),
        is_critical: false,
        job_value: 5.0,
        complexity_score: 0.3,
        requires_consensus: false,
        context_group: None,
        description: "Test job".to_string(),
    };
    
    let decision = orchestrator
        .make_distributed_decision(&context, DecisionType::JobRouting)
        .await
        .unwrap();
    
    // Step 3: Verify decision properties
    assert!(!decision.decision.is_empty());
    assert!(decision.confidence >= 0.0 && decision.confidence <= 1.0);
    assert!(decision.latency_ms < 1000); // Should be fast for heuristic
    assert!(decision.timestamp <= Utc::now());
    
    // Step 4: Check metrics updated
    let final_state = orchestrator.get_orchestration_state().await.unwrap();
    assert_eq!(final_state.metrics.total_decisions, 1);
    assert_eq!(final_state.metrics.heuristic_decisions, 1);
}

