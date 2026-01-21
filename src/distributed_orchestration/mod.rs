//! Distributed Orchestration Module
//!
//! This module implements distributed orchestration inspired by Swarm Intelligence,
//! transforming the centralized LLM Orchestrator into a truly P2P architecture.
//!
//! ## Key Concepts
//!
//! 1. **Emergent Coordination**: Coordinators emerge naturally based on affinity scores
//! 2. **Adaptive Consensus**: Collective decisions using Response Threshold Model
//! 3. **Decision Routing**: Automatic choice between heuristic, LLM, or hybrid strategies
//!
//! ## Architecture
//!
//! The system operates in two modes:
//! - **Deterministic (Heuristic)**: Fast, cheap decisions for low-value/simple cases
//! - **Non-Deterministic (LLM-to-LLM)**: LLM reasoning with consensus for critical cases

pub mod types;
pub mod config;
pub mod affinity;
pub mod rotation;
pub mod consensus;
pub mod threshold;
pub mod decision_router;
pub mod strategies;
pub mod context_groups;
pub mod partition;
pub mod protocols;
pub mod metrics;
pub mod p2p_adapter;
pub mod network_integration;

// Re-exports
pub use types::*;
pub use config::DistributedOrchestratorConfig;
pub use affinity::{AffinityCalculator, AffinityCalculatorTrait};
pub use rotation::{RotationManager, RotationManagerTrait};
pub use consensus::{AdaptiveConsensus, AdaptiveConsensusTrait};
pub use threshold::{ThresholdAdapter, ThresholdAdapterTrait};
pub use decision_router::{DecisionRouter, DecisionRouterTrait};
pub use context_groups::{ContextGroupManager, ContextGroupManagerTrait};
pub use partition::{PartitionHandler, PartitionHandlerTrait};
pub use types::DistributedMetrics;
pub use metrics::MetricsCollector;
pub use p2p_adapter::{P2PAdapter, OrchestrationNetworkMessage};
pub use network_integration::{OrchestrationNetworkBridge, create_orchestration_network_integration};

use crate::orchestration::{LLMOrchestrator, Orchestrator, Decision, DecisionType as OrchestratorDecisionType};
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;

/// P2P message sender for distributed communication
pub type P2PSender = tokio::sync::mpsc::Sender<P2POutboundMessage>;

/// Outbound P2P message types
#[derive(Debug, Clone)]
pub enum P2POutboundMessage {
    /// Broadcast a consensus proposal
    BroadcastProposal(protocols::ConsensusMessage),
    /// Send a direct message to a peer
    DirectMessage(NodeId, protocols::ConsensusMessage),
    /// Broadcast group message
    BroadcastGroupMessage(protocols::GroupMessage),
    /// Send heartbeat
    Heartbeat(protocols::HeartbeatMessage),
}

/// Main Distributed Orchestrator
/// 
/// Coordinates all components for distributed decision-making.
/// Integrates with existing LLMOrchestrator for local LLM capabilities.
pub struct DistributedOrchestrator {
    /// Operating mode
    mode: OrchestrationMode,
    
    /// Affinity calculator for coordinator selection
    affinity_calculator: Arc<AffinityCalculator>,
    
    /// Rotation manager for coordinator changes
    rotation_manager: Arc<RotationManager>,
    
    /// Adaptive consensus engine
    consensus_manager: Arc<AdaptiveConsensus>,
    
    /// Decision strategy router
    decision_router: Arc<DecisionRouter>,
    
    /// Context group manager
    context_group_manager: Arc<ContextGroupManager>,
    
    /// Partition handler
    partition_handler: Arc<PartitionHandler>,
    
    /// Local LLM orchestrator (integration with existing)
    local_llm_orchestrator: Option<Arc<LLMOrchestrator>>,
    
    /// Configuration
    config: DistributedOrchestratorConfig,
    
    /// Current coordinator (if any)
    current_coordinator: Arc<RwLock<Option<NodeId>>>,
    
    /// Metrics
    metrics: Arc<RwLock<DistributedMetrics>>,
    
    /// This node's ID
    node_id: NodeId,
    
    /// P2P message sender (set after network initialization)
    p2p_sender: Option<P2PSender>,
}

/// Trait for Distributed Orchestrator operations
#[async_trait::async_trait]
pub trait DistributedOrchestratorTrait: Send + Sync {
    /// Make a distributed decision
    async fn make_distributed_decision(
        &self,
        context: &DecisionContext,
        decision_type: DecisionType,
    ) -> Result<DistributedDecision>;
    
    /// Get the current coordinator
    async fn get_current_coordinator(&self) -> Result<Option<NodeId>>;
    
    /// Get all context groups
    async fn get_context_groups(&self) -> Result<Vec<ContextGroup>>;
    
    /// Get current orchestration state
    async fn get_orchestration_state(&self) -> Result<OrchestrationState>;
    
    /// Get metrics
    fn get_metrics(&self) -> DistributedMetrics;
}

impl DistributedOrchestrator {
    /// Create a new Distributed Orchestrator
    pub fn new(
        config: DistributedOrchestratorConfig,
        node_id: NodeId,
        local_llm_orchestrator: Option<Arc<LLMOrchestrator>>,
    ) -> Self {
        let affinity_calculator = Arc::new(AffinityCalculator::new(config.affinity_weights.clone()));
        let rotation_manager = Arc::new(RotationManager::new(config.mode.rotation_trigger()));
        let consensus_manager = Arc::new(AdaptiveConsensus::new(config.consensus_criteria.clone()));
        let decision_router = Arc::new(DecisionRouter::new(config.decision_routing.clone()));
        let context_group_manager = Arc::new(ContextGroupManager::new(config.context_groups.clone()));
        let partition_handler = Arc::new(PartitionHandler::new(config.partition_handling.clone()));
        
        Self {
            mode: config.mode.clone(),
            affinity_calculator,
            rotation_manager,
            consensus_manager,
            decision_router,
            context_group_manager,
            partition_handler,
            local_llm_orchestrator,
            config,
            current_coordinator: Arc::new(RwLock::new(None)),
            metrics: Arc::new(RwLock::new(DistributedMetrics::default())),
            node_id,
            p2p_sender: None,
        }
    }
    
    /// Set the P2P sender for distributed communication
    pub fn set_p2p_sender(&mut self, sender: P2PSender) {
        self.p2p_sender = Some(sender);
    }
    
    /// Check if P2P communication is available
    pub fn has_p2p(&self) -> bool {
        self.p2p_sender.is_some()
    }
    
    /// Check if this node is the current coordinator
    pub async fn is_coordinator(&self) -> bool {
        let coordinator = self.current_coordinator.read().await;
        coordinator.as_ref().map(|c| c == &self.node_id).unwrap_or(false)
    }
    
    /// Get this node's ID
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }
    
    /// Get the operating mode
    pub fn mode(&self) -> &OrchestrationMode {
        &self.mode
    }
}

#[async_trait::async_trait]
impl DistributedOrchestratorTrait for DistributedOrchestrator {
    async fn make_distributed_decision(
        &self,
        context: &DecisionContext,
        decision_type: DecisionType,
    ) -> Result<DistributedDecision> {
        // Select strategy based on context
        let strategy = self.decision_router.select_strategy(context).await;
        
        // Execute based on strategy
        let result = match &strategy {
            DecisionStrategy::Heuristic { algorithm, .. } => {
                // Fast local decision
                self.decision_router.execute_heuristic(algorithm, context).await?
            }
            DecisionStrategy::LLMReasoning { .. } => {
                // LLM + consensus if needed
                if context.requires_consensus {
                    self.execute_llm_consensus(context, decision_type).await?
                } else {
                    self.execute_local_llm(context, decision_type).await?
                }
            }
            DecisionStrategy::HybridValidation { .. } => {
                // Heuristic + LLM validation
                self.execute_hybrid(context, decision_type).await?
            }
        };
        
        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.total_decisions += 1;
            match &strategy {
                DecisionStrategy::Heuristic { .. } => metrics.heuristic_decisions += 1,
                DecisionStrategy::LLMReasoning { .. } => metrics.llm_decisions += 1,
                DecisionStrategy::HybridValidation { .. } => metrics.hybrid_decisions += 1,
            }
        }
        
        Ok(result)
    }
    
    async fn get_current_coordinator(&self) -> Result<Option<NodeId>> {
        let coordinator = self.current_coordinator.read().await;
        Ok(coordinator.clone())
    }
    
    async fn get_context_groups(&self) -> Result<Vec<ContextGroup>> {
        Ok(self.context_group_manager.get_all_groups().await)
    }
    
    async fn get_orchestration_state(&self) -> Result<OrchestrationState> {
        let coordinator = self.current_coordinator.read().await;
        let metrics = self.metrics.read().await;
        
        Ok(OrchestrationState {
            mode: self.mode.clone(),
            current_coordinator: coordinator.clone(),
            coordinator_affinity: coordinator
                .as_ref()
                .map(|c| self.affinity_calculator.get_cached_affinity(c))
                .flatten()
                .unwrap_or(0.0),
            active_context_groups: self.context_group_manager.get_all_groups().await,
            node_thresholds: self.consensus_manager.get_all_thresholds().await,
            partition_status: self.partition_handler.get_status().await,
            metrics: metrics.clone(),
        })
    }
    
    fn get_metrics(&self) -> DistributedMetrics {
        // Use try_read to avoid blocking, return default if locked
        self.metrics
            .try_read()
            .map(|m| m.clone())
            .unwrap_or_default()
    }
}

impl DistributedOrchestrator {
    /// Convert local DecisionType to orchestrator DecisionType
    fn convert_decision_type(decision_type: DecisionType) -> OrchestratorDecisionType {
        match decision_type {
            DecisionType::Infrastructure => OrchestratorDecisionType::Infrastructure,
            DecisionType::Context => OrchestratorDecisionType::Context,
            DecisionType::Mediation => OrchestratorDecisionType::Mediation,
            DecisionType::JobRouting => OrchestratorDecisionType::Context,
            DecisionType::CoordinatorElection => OrchestratorDecisionType::Infrastructure,
            DecisionType::GroupFormation => OrchestratorDecisionType::Context,
        }
    }
    
    /// Execute LLM-based decision with multi-round consensus
    async fn execute_llm_consensus(
        &self,
        context: &DecisionContext,
        decision_type: DecisionType,
    ) -> Result<DistributedDecision> {
        let start = std::time::Instant::now();
        let max_rounds = self.config.max_consensus_rounds;
        let timeout = std::time::Duration::from_millis(self.config.consensus_timeout_ms);
        
        // Phase 1: Generate local proposal using LLM
        let local_proposal = self.generate_local_proposal(context, decision_type).await?;
        
        // Phase 2: Broadcast and collect proposals from other nodes
        let mut all_proposals = vec![local_proposal.clone()];
        let peer_proposals = self.collect_peer_proposals(context, timeout).await;
        all_proposals.extend(peer_proposals);
        
        // Phase 3: Multi-round consensus
        let mut round = 0;
        let mut consensus_result: Option<ConsensusResult> = None;
        
        while round < max_rounds {
            round += 1;
            
            // Each node votes based on Response Threshold Model
            let result = self.consensus_manager.propose_decision(all_proposals.clone()).await?;
            
            if result.reached {
                consensus_result = Some(result);
                break;
            }
            
            // If no consensus, refine proposals for next round
            if round < max_rounds {
                // Nodes with low-confidence proposals can withdraw or update
                all_proposals = self.refine_proposals_for_next_round(all_proposals, &result).await;
            }
        }
        
        // Phase 4: Build final decision
        let latency_ms = start.elapsed().as_millis() as u64;
        
        if let Some(result) = consensus_result {
            // Update thresholds based on success
            for node_id in &result.participants {
                self.consensus_manager.update_threshold(node_id, true).await;
            }
            
            Ok(DistributedDecision {
                decision: result.decision.clone().unwrap_or_default(),
                confidence: result.confidence,
                reasoning: result.reasoning.clone(),
                strategy_used: DecisionStrategy::LLMReasoning {
                    model: "consensus".to_string(),
                    temperature: 0.7,
                    expected_latency_ms: latency_ms,
                    cost_per_decision: 0.0,
                },
                consensus_result: Some(result),
                coordinator: self.current_coordinator.read().await.clone(),
                context_group: context.context_group.clone(),
                latency_ms,
                cost: 0.0,
                timestamp: chrono::Utc::now(),
            })
        } else {
            // Fallback: use local LLM decision
            tracing::warn!(
                "Consensus not reached after {} rounds, falling back to local decision",
                max_rounds
            );
            
            // Update thresholds based on failure
            self.consensus_manager.update_threshold(&self.node_id, false).await;
            
            self.execute_local_llm(context, decision_type).await
        }
    }
    
    /// Generate a local proposal using this node's LLM
    async fn generate_local_proposal(
        &self,
        context: &DecisionContext,
        decision_type: DecisionType,
    ) -> Result<Proposal> {
        use chrono::Utc;
        
        if let Some(ref llm) = self.local_llm_orchestrator {
            let orch_type = Self::convert_decision_type(decision_type);
            let decision = llm.make_decision(&context.description, orch_type).await?;
            
            Ok(Proposal {
                id: uuid::Uuid::new_v4().to_string(),
                node_id: self.node_id.clone(),
                decision: decision.decision,
                confidence: decision.confidence,
                reasoning: decision.reasoning,
                job_priority: (context.job_value * 10.0) as u8,
                wait_time_secs: 0,
                specialization_match: context.complexity_score,
                timestamp: Utc::now(),
            })
        } else {
            // Generate heuristic proposal if no LLM
            let heuristic_result = self.decision_router
                .execute_heuristic(&HeuristicAlgorithm::GreedyAffinity, context)
                .await?;
            
            Ok(Proposal {
                id: uuid::Uuid::new_v4().to_string(),
                node_id: self.node_id.clone(),
                decision: heuristic_result.decision,
                confidence: heuristic_result.confidence,
                reasoning: heuristic_result.reasoning,
                job_priority: (context.job_value * 10.0) as u8,
                wait_time_secs: 0,
                specialization_match: context.complexity_score,
                timestamp: Utc::now(),
            })
        }
    }
    
    /// Collect proposals from peer nodes
    async fn collect_peer_proposals(
        &self,
        context: &DecisionContext,
        timeout: std::time::Duration,
    ) -> Vec<Proposal> {
        // In simulation mode, don't attempt P2P communication
        if self.config.simulation_mode {
            tracing::debug!("[SIMULATION] Skipping P2P proposal collection");
            return vec![];
        }
        
        // Send proposal request to known peers via P2P network
        let _ = (context, timeout);
        
        // In a real implementation, this would:
        // 1. Broadcast ProposalRequest via gossipsub
        // 2. Wait for responses up to timeout
        // 3. Collect and return all received proposals
        
        // Check if we have a P2P sender configured
        if let Some(ref sender) = self.p2p_sender {
            // Broadcast proposal request via P2P
            let proposal_msg = protocols::ConsensusMessage::proposal(
                self.node_id.clone(),
                "request_proposals".to_string(),
                1.0,
                format!("Requesting proposals for: {}", context.description),
                context.clone(),
                "local".to_string(),
            );
            
            if let Err(e) = sender.send(P2POutboundMessage::BroadcastProposal(proposal_msg)).await {
                tracing::warn!("Failed to broadcast proposal request: {}", e);
            }
            
            // Wait for responses (up to timeout)
            // In a real implementation, we'd collect responses from the network
            // For now, return empty as we need async message collection infrastructure
            tracing::debug!("Broadcast proposal request to peers (timeout: {:?})", timeout);
            vec![]
        } else {
            vec![]
        }
    }
    
    /// Check if in simulation mode
    pub fn is_simulation_mode(&self) -> bool {
        self.config.simulation_mode
    }
    
    /// Refine proposals for next consensus round
    async fn refine_proposals_for_next_round(
        &self,
        proposals: Vec<Proposal>,
        previous_result: &ConsensusResult,
    ) -> Vec<Proposal> {
        // Keep proposals with confidence above threshold
        let threshold = 0.5;
        let mut refined: Vec<Proposal> = proposals
            .into_iter()
            .filter(|p| p.confidence >= threshold)
            .collect();
        
        // If too few proposals remain, lower threshold
        if refined.len() < 2 {
            return refined;
        }
        
        // Boost confidence of proposals that align with majority
        if let Some(ref majority_decision) = previous_result.decision {
            for proposal in &mut refined {
                if proposal.decision == *majority_decision {
                    proposal.confidence = (proposal.confidence + 0.1).min(1.0);
                }
            }
        }
        
        refined
    }
    
    /// Execute local LLM decision
    async fn execute_local_llm(
        &self,
        context: &DecisionContext,
        decision_type: DecisionType,
    ) -> Result<DistributedDecision> {
        let start = std::time::Instant::now();
        
        if let Some(ref llm) = self.local_llm_orchestrator {
            let orch_decision_type = Self::convert_decision_type(decision_type);
            let decision = llm.make_decision(&context.description, orch_decision_type).await?;
            
            Ok(DistributedDecision {
                decision: decision.decision,
                confidence: decision.confidence,
                reasoning: decision.reasoning,
                strategy_used: DecisionStrategy::LLMReasoning {
                    model: "local".to_string(),
                    temperature: 0.7,
                    expected_latency_ms: 1000,
                    cost_per_decision: 0.0,
                },
                consensus_result: None,
                coordinator: Some(self.node_id.clone()),
                context_group: context.context_group.clone(),
                latency_ms: start.elapsed().as_millis() as u64,
                cost: 0.0,
                timestamp: chrono::Utc::now(),
            })
        } else {
            // Fallback to heuristic if no LLM
            self.decision_router
                .execute_heuristic(&HeuristicAlgorithm::GreedyAffinity, context)
                .await
        }
    }
    
    /// Execute hybrid decision (heuristic + LLM validation)
    async fn execute_hybrid(
        &self,
        context: &DecisionContext,
        decision_type: DecisionType,
    ) -> Result<DistributedDecision> {
        let start = std::time::Instant::now();
        
        // First, get heuristic decision
        let heuristic_result = self.decision_router
            .execute_heuristic(&HeuristicAlgorithm::GreedyAffinity, context)
            .await?;
        
        // Then validate with LLM if available
        if let Some(ref llm) = self.local_llm_orchestrator {
            let validation_prompt = format!(
                "Validate this decision: '{}'. Context: {}. Is this correct? (yes/no with reasoning)",
                heuristic_result.decision,
                context.description
            );
            
            let orch_decision_type = Self::convert_decision_type(decision_type);
            let llm_decision = llm.make_decision(&validation_prompt, orch_decision_type).await?;
            
            // If LLM disagrees (low confidence or explicit disagreement), use LLM decision
            if llm_decision.confidence < 0.5 || llm_decision.decision.to_lowercase().contains("no") {
                return Ok(DistributedDecision {
                    decision: llm_decision.decision,
                    confidence: llm_decision.confidence,
                    reasoning: format!("Hybrid: LLM overrode heuristic. {}", llm_decision.reasoning),
                    strategy_used: DecisionStrategy::HybridValidation {
                        heuristic: Box::new(DecisionStrategy::Heuristic {
                            algorithm: HeuristicAlgorithm::GreedyAffinity,
                            expected_latency_ms: 10,
                            cost_per_decision: 0.0,
                        }),
                        llm_validator: Box::new(DecisionStrategy::LLMReasoning {
                            model: "local".to_string(),
                            temperature: 0.3,
                            expected_latency_ms: 500,
                            cost_per_decision: 0.0,
                        }),
                    },
                    consensus_result: None,
                    coordinator: Some(self.node_id.clone()),
                    context_group: context.context_group.clone(),
                    latency_ms: start.elapsed().as_millis() as u64,
                    cost: 0.0,
                    timestamp: chrono::Utc::now(),
                });
            }
        }
        
        // Heuristic was validated (or no LLM available)
        Ok(DistributedDecision {
            reasoning: format!("Hybrid: Heuristic validated. {}", heuristic_result.reasoning),
            latency_ms: start.elapsed().as_millis() as u64,
            ..heuristic_result
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_distributed_orchestrator_creation() {
        let config = DistributedOrchestratorConfig::default();
        let orchestrator = DistributedOrchestrator::new(
            config,
            "test-node".to_string(),
            None,
        );
        
        assert_eq!(orchestrator.node_id(), "test-node");
    }
}

