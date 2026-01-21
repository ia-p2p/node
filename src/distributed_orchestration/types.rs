//! Core types for Distributed Orchestration
//!
//! Defines all data structures used across the distributed orchestration system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Node identifier type
pub type NodeId = String;

// ============================================================================
// Decision Types
// ============================================================================

/// Type of decision being made (reusing from orchestration but extended)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecisionType {
    /// Infrastructure decisions: conservative, stability-focused
    Infrastructure,
    /// Context decisions: moderate, semantics-focused
    Context,
    /// Mediation decisions: creative, conflict-resolution focused
    Mediation,
    /// Job routing decisions
    JobRouting,
    /// Coordinator election
    CoordinatorElection,
    /// Group formation
    GroupFormation,
}

/// Context for making a decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionContext {
    /// Type of decision
    pub decision_type: String,
    /// Whether this is a critical decision
    pub is_critical: bool,
    /// Economic value of the job (if applicable)
    pub job_value: f64,
    /// Complexity score (0.0-1.0)
    pub complexity_score: f64,
    /// Whether consensus is required
    pub requires_consensus: bool,
    /// Context group (if applicable)
    pub context_group: Option<String>,
    /// Human-readable description
    pub description: String,
}

impl Default for DecisionContext {
    fn default() -> Self {
        Self {
            decision_type: "general".to_string(),
            is_critical: false,
            job_value: 0.0,
            complexity_score: 0.0,
            requires_consensus: false,
            context_group: None,
            description: String::new(),
        }
    }
}

// ============================================================================
// Decision Strategies
// ============================================================================

/// Heuristic algorithms available
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeuristicAlgorithm {
    /// Select node with highest affinity
    GreedyAffinity,
    /// Round-robin selection
    RoundRobin,
    /// Select least loaded node
    LeastLoaded,
    /// Random selection
    Random,
}

/// Decision strategy to use
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecisionStrategy {
    /// Fast deterministic heuristic
    Heuristic {
        algorithm: HeuristicAlgorithm,
        expected_latency_ms: u64,
        cost_per_decision: f64,
    },
    /// Full LLM reasoning
    LLMReasoning {
        model: String,
        temperature: f32,
        expected_latency_ms: u64,
        cost_per_decision: f64,
    },
    /// Heuristic with LLM validation
    HybridValidation {
        heuristic: Box<DecisionStrategy>,
        llm_validator: Box<DecisionStrategy>,
    },
}

// ============================================================================
// Orchestration Modes
// ============================================================================

/// How coordination is managed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrchestrationMode {
    /// Coordinator emerges based on affinity
    EmergentCoordinator {
        rotation_trigger: RotationTrigger,
        affinity_recalc_interval_secs: u64,
    },
    /// Fixed coordinator (fallback mode)
    PermanentCoordinator {
        coordinator_id: NodeId,
    },
    /// No coordinator, consensus for every decision
    FullyDecentralized {
        consensus_for_every_decision: bool,
    },
}

impl Default for OrchestrationMode {
    fn default() -> Self {
        Self::EmergentCoordinator {
            rotation_trigger: RotationTrigger::Adaptive,
            affinity_recalc_interval_secs: 60,
        }
    }
}

impl OrchestrationMode {
    /// Get the rotation trigger for this mode
    pub fn rotation_trigger(&self) -> RotationTrigger {
        match self {
            Self::EmergentCoordinator { rotation_trigger, .. } => rotation_trigger.clone(),
            Self::PermanentCoordinator { .. } => RotationTrigger::EventDriven,
            Self::FullyDecentralized { .. } => RotationTrigger::EventDriven,
        }
    }
}

/// When to rotate coordinator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RotationTrigger {
    /// Rotate after N jobs
    Periodic { interval_jobs: u64 },
    /// Rotate on failure or load change
    EventDriven,
    /// Adaptive based on metrics
    Adaptive,
}

impl Default for RotationTrigger {
    fn default() -> Self {
        Self::Adaptive
    }
}

// ============================================================================
// Consensus Types
// ============================================================================

/// Criteria for reaching consensus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusCriteria {
    /// Simple majority vote
    SimpleMajority {
        required_percentage: f64,
        min_participants: usize,
    },
    /// Minimum confidence threshold
    ConfidenceThreshold {
        min_confidence: f64,
        min_participants: usize,
        aggregate_method: AggregateMethod,
    },
    /// Adaptive threshold based on Response Threshold Model
    AdaptiveThreshold {
        base_threshold: f64,
        stimulus_weight: f64,
        demand_weight: f64,
        min_participants: usize,
    },
}

impl Default for ConsensusCriteria {
    fn default() -> Self {
        Self::AdaptiveThreshold {
            base_threshold: 0.6,
            stimulus_weight: 0.6,
            demand_weight: 0.4,
            min_participants: 2,
        }
    }
}

/// How to aggregate votes/proposals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AggregateMethod {
    Average,
    WeightedAverage,
    Median,
}

/// A proposal in the consensus protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    /// Unique proposal ID
    pub id: String,
    /// Node that made the proposal
    pub node_id: NodeId,
    /// The proposed decision
    pub decision: String,
    /// Confidence level (0.0-1.0)
    pub confidence: f64,
    /// Reasoning behind the proposal
    pub reasoning: String,
    /// Job priority (if applicable)
    pub job_priority: u8,
    /// How long the job has been waiting
    pub wait_time_secs: u64,
    /// How well the proposer matches the job requirements
    pub specialization_match: f64,
    /// When the proposal was made
    pub timestamp: DateTime<Utc>,
}

/// Result of a consensus round
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusResult {
    /// Whether consensus was reached
    pub reached: bool,
    /// The decision (if reached)
    pub decision: Option<String>,
    /// Aggregate confidence
    pub confidence: f64,
    /// Nodes that participated
    pub participants: Vec<NodeId>,
    /// Number of rounds taken
    pub rounds: usize,
    /// Summary of reasoning
    pub reasoning: String,
}

// ============================================================================
// Distributed Decision
// ============================================================================

/// Result of a distributed decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedDecision {
    /// The decision made
    pub decision: String,
    /// Confidence level
    pub confidence: f64,
    /// Reasoning
    pub reasoning: String,
    /// Strategy that was used
    pub strategy_used: DecisionStrategy,
    /// Consensus result (if consensus was used)
    pub consensus_result: Option<ConsensusResult>,
    /// Coordinator that made/coordinated the decision
    pub coordinator: Option<NodeId>,
    /// Context group (if applicable)
    pub context_group: Option<String>,
    /// Time taken
    pub latency_ms: u64,
    /// Cost incurred
    pub cost: f64,
    /// When the decision was made
    pub timestamp: DateTime<Utc>,
}

// ============================================================================
// Context Groups
// ============================================================================

/// A group of nodes specialized in a context/domain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextGroup {
    /// Group ID
    pub id: String,
    /// Context/domain name
    pub context: String,
    /// Current leader (if any)
    pub leader: Option<NodeId>,
    /// Member nodes
    pub members: Vec<NodeId>,
    /// Specializations of this group
    pub specializations: Vec<String>,
    /// When the group was formed
    pub formed_at: DateTime<Utc>,
    /// Jobs processed by this group
    pub jobs_processed: u64,
    /// Average latency
    pub avg_latency_ms: f64,
}

/// Actions for group management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GroupAction {
    FormGroup { context: String, nodes: Vec<NodeId> },
    DissolveGroup { group_id: String, reason: String },
    AddMember { group_id: String, node_id: NodeId },
    RemoveMember { group_id: String, node_id: NodeId },
    RotateLeader { group_id: String },
}

// ============================================================================
// Partition Handling
// ============================================================================

/// Current partition status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PartitionStatus {
    /// Network is healthy
    Healthy,
    /// Network is partitioned
    Partitioned { partitions: Vec<Vec<NodeId>> },
    /// Currently reconciling after partition
    Reconciling,
}

impl Default for PartitionStatus {
    fn default() -> Self {
        Self::Healthy
    }
}

/// Strategy for reconciling after partition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReconciliationStrategy {
    /// Most recent write wins
    LastWriteWins,
    /// Use vector clocks
    VectorClock,
    /// Custom conflict resolution
    ConflictResolution { resolver: String },
}

impl Default for ReconciliationStrategy {
    fn default() -> Self {
        Self::LastWriteWins
    }
}

// ============================================================================
// Orchestration State
// ============================================================================

/// Complete state of the distributed orchestration system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationState {
    /// Current mode
    pub mode: OrchestrationMode,
    /// Current coordinator
    pub current_coordinator: Option<NodeId>,
    /// Coordinator's affinity score
    pub coordinator_affinity: f64,
    /// Active context groups
    pub active_context_groups: Vec<ContextGroup>,
    /// Thresholds for each node
    pub node_thresholds: HashMap<NodeId, f64>,
    /// Partition status
    pub partition_status: PartitionStatus,
    /// Metrics
    pub metrics: DistributedMetrics,
}

// ============================================================================
// Affinity Types
// ============================================================================

/// Weights for affinity calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffinityWeights {
    /// Weight for computational capacity
    pub capacity_weight: f64,
    /// Weight for network latency
    pub latency_weight: f64,
    /// Weight for uptime/stability
    pub uptime_weight: f64,
    /// Weight for specialization
    pub specialization_weight: f64,
}

impl Default for AffinityWeights {
    fn default() -> Self {
        Self {
            capacity_weight: 0.3,
            latency_weight: 0.25,
            uptime_weight: 0.25,
            specialization_weight: 0.2,
        }
    }
}

/// History of a node's affinity scores
#[derive(Debug, Clone)]
pub struct AffinityHistory {
    pub node_id: NodeId,
    pub recent_scores: std::collections::VecDeque<(std::time::Instant, f64)>,
    pub avg_score: f64,
    pub times_coordinated: u64,
}

// ============================================================================
// Rotation Types
// ============================================================================

/// Reason for coordinator rotation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RotationReason {
    PeriodicInterval,
    CoordinatorFailure,
    LoadImbalance,
    AffinityChange,
    ManualTrigger,
}

/// Record of a rotation event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationEvent {
    pub timestamp: DateTime<Utc>,
    pub from_node: NodeId,
    pub to_node: NodeId,
    pub reason: RotationReason,
    pub affinity_before: f64,
    pub affinity_after: f64,
}

// ============================================================================
// Metrics
// ============================================================================

/// Metrics for distributed orchestration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DistributedMetrics {
    pub total_decisions: u64,
    pub consensus_decisions: u64,
    pub heuristic_decisions: u64,
    pub llm_decisions: u64,
    pub hybrid_decisions: u64,
    pub avg_consensus_latency_ms: f64,
    pub avg_consensus_rounds: f64,
    pub coordinator_rotations: u64,
    pub context_groups_formed: u64,
    pub context_groups_dissolved: u64,
    pub cross_context_jobs: u64,
    pub partition_events: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_decision_context_default() {
        let ctx = DecisionContext::default();
        assert!(!ctx.is_critical);
        assert_eq!(ctx.job_value, 0.0);
    }
    
    #[test]
    fn test_affinity_weights_default() {
        let weights = AffinityWeights::default();
        let total = weights.capacity_weight 
            + weights.latency_weight 
            + weights.uptime_weight 
            + weights.specialization_weight;
        assert!((total - 1.0).abs() < 0.001);
    }
    
    #[test]
    fn test_consensus_criteria_default() {
        let criteria = ConsensusCriteria::default();
        if let ConsensusCriteria::AdaptiveThreshold { base_threshold, .. } = criteria {
            assert_eq!(base_threshold, 0.6);
        } else {
            panic!("Expected AdaptiveThreshold");
        }
    }
}

