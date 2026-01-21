//! Core data types for the LLM Orchestrator
//!
//! This module defines all the data structures used by the orchestrator,
//! including state representations, decisions, actions, and training data.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ============================================================================
// State Representations
// ============================================================================

/// Summarized state for LLM consumption
/// Optimized to provide essential context without overwhelming the model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMState {
    /// Node state information
    pub node: NodeState,
    /// Context/job queue state
    pub context: ContextState,
    /// Network state
    pub network: NetworkState,
    /// Model/inference state
    pub model: ModelState,
    /// Timestamp of state capture
    pub timestamp: DateTime<Utc>,
}

impl LLMState {
    /// Create a new LLMState with current timestamp
    pub fn new(
        node: NodeState,
        context: ContextState,
        network: NetworkState,
        model: ModelState,
    ) -> Self {
        Self {
            node,
            context,
            network,
            model,
            timestamp: Utc::now(),
        }
    }
    
    /// Calculate a hash for cache key generation
    pub fn calculate_hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        
        // Hash key state components
        self.node.capacity.to_string().hash(&mut hasher);
        self.context.health.to_string().hash(&mut hasher);
        self.context.queue_size.hash(&mut hasher);
        self.network.partition_risk.to_string().hash(&mut hasher);
        self.model.loaded.hash(&mut hasher);
        
        hasher.finish()
    }
    
    /// Format state as a human-readable summary for LLM prompt
    pub fn to_summary(&self) -> String {
        format!(
            "Node '{}': capacity={}, uptime={}s, errors={}\n\
             Context: health={}, queue={}/{}, processing={}\n\
             Network: partition_risk={}, peers={}, connections={}\n\
             Model: loaded={}, avg_inference={}ms",
            self.node.id,
            self.node.capacity,
            self.node.uptime_seconds,
            self.node.error_count,
            self.context.health,
            self.context.queue_size,
            self.context.max_queue,
            self.context.processing_jobs,
            self.network.partition_risk,
            self.network.peers_available,
            self.network.active_connections,
            self.model.loaded,
            self.model.avg_inference_ms,
        )
    }
}

/// Node state information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeState {
    /// Node identifier
    pub id: String,
    /// Current capacity level
    pub capacity: CapacityLevel,
    /// Average latency in milliseconds
    pub latency_avg: u64,
    /// Uptime in seconds
    pub uptime_seconds: u64,
    /// Error count since start
    pub error_count: u64,
}

/// Context/queue state information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextState {
    /// Context identifier
    pub id: String,
    /// Health level
    pub health: HealthLevel,
    /// Current queue size
    pub queue_size: usize,
    /// Maximum queue capacity
    pub max_queue: usize,
    /// Number of jobs currently processing
    pub processing_jobs: usize,
}

/// Network state information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkState {
    /// Risk of network partition
    pub partition_risk: RiskLevel,
    /// Number of peers available
    pub peers_available: u64,
    /// Number of active connections
    pub active_connections: u64,
    /// Average message latency (if known)
    pub message_latency_avg: Option<u64>,
}

/// Model/inference state information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelState {
    /// Whether a model is loaded
    pub loaded: bool,
    /// Average inference time in milliseconds
    pub avg_inference_ms: u64,
    /// Memory usage in MB
    pub memory_usage_mb: u64,
    /// Recent failure count
    pub recent_failures: u64,
}

// ============================================================================
// Level Enums
// ============================================================================

/// Capacity level classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CapacityLevel {
    Low,
    Medium,
    High,
    Overloaded,
}

impl std::fmt::Display for CapacityLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
            Self::Overloaded => write!(f, "overloaded"),
        }
    }
}

impl CapacityLevel {
    /// Calculate capacity level from queue utilization
    pub fn from_utilization(current: usize, max: usize) -> Self {
        if max == 0 {
            return Self::Low;
        }
        let ratio = current as f64 / max as f64;
        if ratio < 0.3 {
            Self::Low
        } else if ratio < 0.6 {
            Self::Medium
        } else if ratio < 0.9 {
            Self::High
        } else {
            Self::Overloaded
        }
    }
}

/// Health level classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthLevel {
    Healthy,
    Degraded,
    Unhealthy,
    Critical,
}

impl std::fmt::Display for HealthLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => write!(f, "healthy"),
            Self::Degraded => write!(f, "degraded"),
            Self::Unhealthy => write!(f, "unhealthy"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

impl HealthLevel {
    /// Calculate health level from error count and latency
    pub fn from_metrics(error_count: u64, latency_avg_ms: u64) -> Self {
        if error_count > 50 || latency_avg_ms > 5000 {
            Self::Critical
        } else if error_count > 20 || latency_avg_ms > 2000 {
            Self::Unhealthy
        } else if error_count > 5 || latency_avg_ms > 1000 {
            Self::Degraded
        } else {
            Self::Healthy
        }
    }
}

/// Risk level classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

impl RiskLevel {
    /// Calculate partition risk from peer count
    pub fn from_peer_count(peers: u64, min_required: u64) -> Self {
        if peers < min_required {
            Self::Critical
        } else if peers < min_required * 2 {
            Self::High
        } else if peers < min_required * 3 {
            Self::Medium
        } else {
            Self::Low
        }
    }
}

// ============================================================================
// Decision Types
// ============================================================================

/// Type of decision being requested
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecisionType {
    /// Infrastructure decisions: conservative, stability-focused
    Infrastructure,
    /// Context decisions: moderate, semantics-focused  
    Context,
    /// Mediation decisions: creative, conflict-resolution focused
    Mediation,
}

impl DecisionType {
    /// Get recommended temperature for this decision type
    pub fn recommended_temperature(&self) -> f64 {
        match self {
            Self::Infrastructure => 0.3,
            Self::Context => 0.5,
            Self::Mediation => 0.7,
        }
    }
    
    /// Get description for logging
    pub fn description(&self) -> &'static str {
        match self {
            Self::Infrastructure => "conservative, stability-focused",
            Self::Context => "moderate, semantics-focused",
            Self::Mediation => "creative, conflict-resolution focused",
        }
    }
}

impl std::fmt::Display for DecisionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Infrastructure => write!(f, "infrastructure"),
            Self::Context => write!(f, "context"),
            Self::Mediation => write!(f, "mediation"),
        }
    }
}

/// Structured decision from the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    /// Unique decision identifier
    #[serde(default = "generate_decision_id")]
    pub id: String,
    /// The decision name/type
    pub decision: String,
    /// Confidence level (0.0 to 1.0)
    pub confidence: f64,
    /// Reasoning behind the decision
    pub reasoning: String,
    /// List of actions to take
    pub actions: Vec<Action>,
    /// Estimated impact of the decision
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_impact: Option<String>,
    /// Rollback plan if decision fails
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollback_plan: Option<String>,
    /// Timestamp of decision
    #[serde(default = "Utc::now")]
    pub timestamp: DateTime<Utc>,
}

fn generate_decision_id() -> String {
    Uuid::new_v4().to_string()
}

impl Decision {
    /// Create a new decision with default ID and timestamp
    pub fn new(
        decision: String,
        confidence: f64,
        reasoning: String,
        actions: Vec<Action>,
    ) -> Self {
        Self {
            id: generate_decision_id(),
            decision,
            confidence,
            reasoning,
            actions,
            estimated_impact: None,
            rollback_plan: None,
            timestamp: Utc::now(),
        }
    }
    
    /// Check if any action requires confirmation
    pub fn requires_confirmation(&self) -> bool {
        self.actions.iter().any(|a| a.requires_confirmation)
    }
    
    /// Get highest priority action
    pub fn highest_priority_action(&self) -> Option<&Action> {
        self.actions.iter().max_by_key(|a| a.priority)
    }
}

/// An action to be taken as part of a decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    /// Action type/name
    pub action_type: String,
    /// Action parameters
    #[serde(default)]
    pub parameters: HashMap<String, serde_json::Value>,
    /// Priority (1-10, higher = more urgent)
    pub priority: u8,
    /// Whether confirmation is required before execution
    #[serde(default)]
    pub requires_confirmation: bool,
}

impl Action {
    /// Create a new action
    pub fn new(action_type: &str, priority: u8) -> Self {
        Self {
            action_type: action_type.to_string(),
            parameters: HashMap::new(),
            priority,
            requires_confirmation: false,
        }
    }
    
    /// Add a parameter
    pub fn with_param(mut self, key: &str, value: serde_json::Value) -> Self {
        self.parameters.insert(key.to_string(), value);
        self
    }
    
    /// Set requires confirmation
    pub fn with_confirmation(mut self) -> Self {
        self.requires_confirmation = true;
        self
    }
}

// ============================================================================
// Training Data
// ============================================================================

/// Training example for fine-tuning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingExample {
    /// Unique example identifier
    pub id: String,
    /// State at time of decision
    pub state: LLMState,
    /// User-provided context
    pub context: String,
    /// Type of decision
    pub decision_type: String,
    /// The LLM's decision
    pub llm_decision: Decision,
    /// Human feedback (if provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub human_feedback: Option<Feedback>,
    /// Outcome of the decision
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<Outcome>,
    /// Metrics before decision
    #[serde(default)]
    pub metrics_before: HashMap<String, f64>,
    /// Metrics after decision (if known)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics_after: Option<HashMap<String, f64>>,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

impl TrainingExample {
    /// Create a new training example
    pub fn new(
        state: LLMState,
        context: String,
        decision_type: DecisionType,
        llm_decision: Decision,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            state,
            context,
            decision_type: decision_type.to_string(),
            llm_decision,
            human_feedback: None,
            outcome: None,
            metrics_before: HashMap::new(),
            metrics_after: None,
            timestamp: Utc::now(),
        }
    }
    
    /// Check if example is valid for training (has positive outcome)
    pub fn is_valid_for_training(&self) -> bool {
        match &self.outcome {
            Some(Outcome::Success { .. }) => true,
            Some(Outcome::Neutral { .. }) => true,
            _ => false,
        }
    }
    
    /// Check if example has human approval
    pub fn has_human_approval(&self) -> bool {
        matches!(self.human_feedback, Some(Feedback::Approved))
    }
}

/// Human feedback on a decision
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Feedback {
    /// Decision was approved
    Approved,
    /// Decision was rejected
    Rejected {
        reason: String,
    },
    /// Decision was modified
    Modified {
        new_decision: Decision,
        reason: String,
    },
}

/// Outcome of an executed decision
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Outcome {
    /// Decision was successful
    Success {
        /// Improvement as a ratio (e.g., 0.25 = 25% improvement)
        improvement: f64,
        description: String,
    },
    /// Decision failed
    Failure {
        /// Degradation as a ratio (e.g., 0.1 = 10% degradation)
        degradation: f64,
        description: String,
    },
    /// Decision had neutral effect
    Neutral {
        description: String,
    },
}

// ============================================================================
// CLI Command Types
// ============================================================================

/// CLI command interpreted from natural language
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliCommand {
    /// Command name
    pub command: String,
    /// Command arguments
    pub args: Vec<String>,
    /// Whether this is a destructive command
    pub is_destructive: bool,
    /// Human-readable description
    pub description: String,
}

impl CliCommand {
    pub fn new(command: &str, args: Vec<String>) -> Self {
        let is_destructive = Self::check_destructive(command, &args);
        Self {
            command: command.to_string(),
            args,
            is_destructive,
            description: String::new(),
        }
    }
    
    fn check_destructive(command: &str, args: &[String]) -> bool {
        matches!(command, "stop" | "shutdown" | "delete" | "remove" | "kill")
            || args.iter().any(|a| a.contains("--force"))
    }
    
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }
}

/// Result of natural language interpretation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterpretationResult {
    /// Interpreted intent
    pub intent: String,
    /// Mapped CLI commands
    pub commands: Vec<CliCommand>,
    /// Confidence in interpretation
    pub confidence: f64,
    /// Whether any command requires confirmation
    pub requires_confirmation: bool,
    /// Human-readable explanation
    pub explanation: String,
}

// ============================================================================
// Metrics Types
// ============================================================================

/// Orchestrator performance metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrchestratorMetrics {
    /// Total decisions made
    pub decisions_made: u64,
    /// Successful decisions
    pub decisions_successful: u64,
    /// Failed decisions
    pub decisions_failed: u64,
    /// Validation failures
    pub validation_failures: u64,
    /// Fallback activations
    pub fallback_activations: u64,
    /// Average inference latency in ms
    pub avg_inference_latency_ms: f64,
    /// Cache hit count
    pub cache_hits: u64,
    /// Cache miss count
    pub cache_misses: u64,
    /// Training examples collected
    pub training_examples_collected: u64,
}

impl OrchestratorMetrics {
    /// Calculate decision success rate
    pub fn decision_success_rate(&self) -> f64 {
        let total = self.decisions_successful + self.decisions_failed;
        if total == 0 {
            return 1.0;
        }
        self.decisions_successful as f64 / total as f64
    }
    
    /// Calculate validation success rate
    pub fn validation_success_rate(&self) -> f64 {
        let total = self.decisions_made + self.validation_failures;
        if total == 0 {
            return 1.0;
        }
        self.decisions_made as f64 / total as f64
    }
    
    /// Calculate cache hit rate
    pub fn cache_hit_rate(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            return 0.0;
        }
        self.cache_hits as f64 / total as f64
    }
}

// ============================================================================
// Dataset Export Types
// ============================================================================

/// Format for exporting training datasets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatasetFormat {
    /// JSON Lines format (one JSON object per line)
    Jsonl,
    /// Standard JSON array
    Json,
    /// CSV format
    Csv,
}

impl std::fmt::Display for DatasetFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Jsonl => write!(f, "jsonl"),
            Self::Json => write!(f, "json"),
            Self::Csv => write!(f, "csv"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capacity_level_from_utilization() {
        assert_eq!(CapacityLevel::from_utilization(10, 100), CapacityLevel::Low);
        assert_eq!(CapacityLevel::from_utilization(50, 100), CapacityLevel::Medium);
        assert_eq!(CapacityLevel::from_utilization(70, 100), CapacityLevel::High);
        assert_eq!(CapacityLevel::from_utilization(95, 100), CapacityLevel::Overloaded);
    }

    #[test]
    fn test_health_level_from_metrics() {
        assert_eq!(HealthLevel::from_metrics(0, 100), HealthLevel::Healthy);
        assert_eq!(HealthLevel::from_metrics(10, 1500), HealthLevel::Degraded);
        assert_eq!(HealthLevel::from_metrics(30, 500), HealthLevel::Unhealthy);
        assert_eq!(HealthLevel::from_metrics(100, 100), HealthLevel::Critical);
    }

    #[test]
    fn test_decision_type_temperature() {
        assert!(DecisionType::Infrastructure.recommended_temperature() < 0.5);
        assert!(DecisionType::Mediation.recommended_temperature() > 0.5);
    }

    #[test]
    fn test_decision_creation() {
        let decision = Decision::new(
            "scale_queue".to_string(),
            0.85,
            "Queue is overloaded".to_string(),
            vec![Action::new("scale_queue", 8)],
        );
        
        assert!(!decision.id.is_empty());
        assert_eq!(decision.confidence, 0.85);
    }

    #[test]
    fn test_action_builder() {
        let action = Action::new("migrate_context", 5)
            .with_param("target", serde_json::json!("node-2"))
            .with_confirmation();
        
        assert!(action.requires_confirmation);
        assert!(action.parameters.contains_key("target"));
    }

    #[test]
    fn test_training_example_validation() {
        let state = LLMState::new(
            NodeState {
                id: "test".to_string(),
                capacity: CapacityLevel::Medium,
                latency_avg: 100,
                uptime_seconds: 3600,
                error_count: 0,
            },
            ContextState {
                id: "ctx".to_string(),
                health: HealthLevel::Healthy,
                queue_size: 10,
                max_queue: 100,
                processing_jobs: 2,
            },
            NetworkState {
                partition_risk: RiskLevel::Low,
                peers_available: 5,
                active_connections: 3,
                message_latency_avg: Some(50),
            },
            ModelState {
                loaded: true,
                avg_inference_ms: 200,
                memory_usage_mb: 4000,
                recent_failures: 0,
            },
        );
        
        let decision = Decision::new(
            "wait".to_string(),
            0.9,
            "System is stable".to_string(),
            vec![],
        );
        
        let mut example = TrainingExample::new(
            state,
            "Check system status".to_string(),
            DecisionType::Infrastructure,
            decision,
        );
        
        // Without outcome, not valid for training
        assert!(!example.is_valid_for_training());
        
        // With success outcome, valid for training
        example.outcome = Some(Outcome::Success {
            improvement: 0.1,
            description: "Stability maintained".to_string(),
        });
        assert!(example.is_valid_for_training());
    }

    #[test]
    fn test_cli_command_destructive_detection() {
        let cmd = CliCommand::new("stop", vec!["node-1".to_string()]);
        assert!(cmd.is_destructive);
        
        let cmd = CliCommand::new("status", vec![]);
        assert!(!cmd.is_destructive);
        
        let cmd = CliCommand::new("restart", vec!["--force".to_string()]);
        assert!(cmd.is_destructive);
    }

    #[test]
    fn test_orchestrator_metrics() {
        let mut metrics = OrchestratorMetrics::default();
        metrics.decisions_successful = 90;
        metrics.decisions_failed = 10;
        metrics.cache_hits = 60;
        metrics.cache_misses = 40;
        
        assert!((metrics.decision_success_rate() - 0.9).abs() < 0.001);
        assert!((metrics.cache_hit_rate() - 0.6).abs() < 0.001);
    }
}

