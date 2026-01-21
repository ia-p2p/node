//! Adaptive Consensus
//!
//! Implements consensus using the Response Threshold Model from social insects.
//! Nodes have adaptive thresholds that change based on decision outcomes.

use super::types::{ConsensusCriteria, ConsensusResult, NodeId, Proposal};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Implements adaptive consensus
pub struct AdaptiveConsensus {
    /// Consensus criteria
    criteria: ConsensusCriteria,
    
    /// Per-node thresholds
    node_thresholds: Arc<RwLock<HashMap<NodeId, f64>>>,
    
    /// Default threshold for new nodes
    default_threshold: f64,
}

/// Trait for consensus operations
#[async_trait::async_trait]
pub trait AdaptiveConsensusTrait: Send + Sync {
    /// Propose a decision and gather consensus
    async fn propose_decision(
        &self,
        proposals: Vec<Proposal>,
    ) -> Result<ConsensusResult>;
    
    /// Evaluate a single proposal (calculate tendency)
    async fn evaluate_proposal(&self, proposal: &Proposal, node_id: &NodeId) -> Result<f64>;
    
    /// Update threshold for a node based on outcome
    async fn update_threshold(&self, node_id: &NodeId, success: bool);
    
    /// Get threshold for a node
    async fn get_node_threshold(&self, node_id: &NodeId) -> f64;
    
    /// Get all thresholds
    async fn get_all_thresholds(&self) -> HashMap<NodeId, f64>;
}

impl AdaptiveConsensus {
    /// Create a new AdaptiveConsensus
    pub fn new(criteria: ConsensusCriteria) -> Self {
        let default_threshold = match &criteria {
            ConsensusCriteria::AdaptiveThreshold { base_threshold, .. } => *base_threshold,
            _ => 0.5,
        };
        
        Self {
            criteria,
            node_thresholds: Arc::new(RwLock::new(HashMap::new())),
            default_threshold,
        }
    }
}

/// Calculate tendency using Response Threshold Model
///
/// The formula is:
/// tendency = σ * (s² / (s² + θ²)) + (1-σ) * d
///
/// Where:
/// - s = stimulus (urgency/importance)
/// - θ = threshold (node's internal threshold)
/// - d = demand (fraction of nodes that already agreed)
/// - σ = weight between response and demand (typically 0.6)
pub fn calculate_tendency(stimulus: f64, threshold: f64, demand: f64, sigma: f64) -> f64 {
    let response = (stimulus.powi(2)) / (stimulus.powi(2) + threshold.powi(2));
    let demand_factor = demand;
    sigma * response + (1.0 - sigma) * demand_factor
}

/// Calculate stimulus from proposal characteristics
pub fn calculate_stimulus(proposal: &Proposal) -> f64 {
    // Combine confidence, priority, wait time, and specialization
    let confidence_factor = proposal.confidence;
    let priority_factor = (proposal.job_priority as f64) / 10.0;
    let wait_factor = (proposal.wait_time_secs as f64 / 300.0).min(1.0); // 5 min = 1.0
    let specialization_factor = proposal.specialization_match;
    
    // Weighted average
    let stimulus = 0.4 * confidence_factor
        + 0.2 * priority_factor
        + 0.2 * wait_factor
        + 0.2 * specialization_factor;
    
    stimulus.clamp(0.0, 1.0)
}

#[async_trait::async_trait]
impl AdaptiveConsensusTrait for AdaptiveConsensus {
    async fn propose_decision(
        &self,
        proposals: Vec<Proposal>,
    ) -> Result<ConsensusResult> {
        if proposals.is_empty() {
            return Ok(ConsensusResult {
                reached: false,
                decision: None,
                confidence: 0.0,
                participants: vec![],
                rounds: 0,
                reasoning: "No proposals provided".to_string(),
            });
        }
        
        // Group proposals by decision
        let mut decision_votes: HashMap<String, Vec<&Proposal>> = HashMap::new();
        for proposal in &proposals {
            decision_votes
                .entry(proposal.decision.clone())
                .or_default()
                .push(proposal);
        }
        
        // Calculate tendency for each decision
        let mut decision_tendencies: HashMap<String, f64> = HashMap::new();
        let total_proposals = proposals.len() as f64;
        
        for (decision, voters) in &decision_votes {
            let demand = voters.len() as f64 / total_proposals;
            
            // Calculate average tendency for this decision
            let mut total_tendency = 0.0;
            for proposal in voters {
                let stimulus = calculate_stimulus(proposal);
                let threshold = self.get_node_threshold(&proposal.node_id).await;
                let tendency = calculate_tendency(stimulus, threshold, demand, 0.6);
                total_tendency += tendency;
            }
            
            let avg_tendency = total_tendency / voters.len() as f64;
            decision_tendencies.insert(decision.clone(), avg_tendency);
        }
        
        // Find winning decision based on criteria
        let (winning_decision, winning_tendency) = decision_tendencies
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(d, t)| (d.clone(), *t))
            .unwrap_or((String::new(), 0.0));
        
        // Check if consensus is reached based on criteria
        let (reached, min_threshold) = match &self.criteria {
            ConsensusCriteria::SimpleMajority { required_percentage, min_participants } => {
                let voter_count = decision_votes.get(&winning_decision).map(|v| v.len()).unwrap_or(0);
                let percentage = voter_count as f64 / total_proposals;
                (percentage >= *required_percentage && proposals.len() >= *min_participants, *required_percentage)
            }
            ConsensusCriteria::ConfidenceThreshold { min_confidence, min_participants, .. } => {
                (winning_tendency >= *min_confidence && proposals.len() >= *min_participants, *min_confidence)
            }
            ConsensusCriteria::AdaptiveThreshold { base_threshold, min_participants, .. } => {
                (winning_tendency >= *base_threshold && proposals.len() >= *min_participants, *base_threshold)
            }
        };
        
        let participants: Vec<NodeId> = proposals.iter().map(|p| p.node_id.clone()).collect();
        
        let reasoning = if reached {
            format!(
                "Consensus reached for '{}' with tendency {:.2} (threshold: {:.2})",
                winning_decision, winning_tendency, min_threshold
            )
        } else {
            format!(
                "Consensus not reached. Best option '{}' had tendency {:.2} (needed: {:.2})",
                winning_decision, winning_tendency, min_threshold
            )
        };
        
        Ok(ConsensusResult {
            reached,
            decision: if reached { Some(winning_decision) } else { None },
            confidence: winning_tendency,
            participants,
            rounds: 1,
            reasoning,
        })
    }
    
    async fn evaluate_proposal(&self, proposal: &Proposal, node_id: &NodeId) -> Result<f64> {
        let stimulus = calculate_stimulus(proposal);
        let threshold = self.get_node_threshold(node_id).await;
        let demand = 0.5; // Default demand when evaluating single proposal
        
        Ok(calculate_tendency(stimulus, threshold, demand, 0.6))
    }
    
    async fn update_threshold(&self, node_id: &NodeId, success: bool) {
        let mut thresholds = self.node_thresholds.write().await;
        let current = thresholds.get(node_id).copied().unwrap_or(self.default_threshold);
        
        // Adaptation rate
        let rate = 0.05;
        let min_threshold = 0.3;
        let max_threshold = 0.9;
        
        let new_threshold = if success {
            // Success: lower threshold (more likely to accept)
            (current - rate).max(min_threshold)
        } else {
            // Failure: raise threshold (more selective)
            (current + rate).min(max_threshold)
        };
        
        thresholds.insert(node_id.clone(), new_threshold);
    }
    
    async fn get_node_threshold(&self, node_id: &NodeId) -> f64 {
        let thresholds = self.node_thresholds.read().await;
        thresholds.get(node_id).copied().unwrap_or(self.default_threshold)
    }
    
    async fn get_all_thresholds(&self) -> HashMap<NodeId, f64> {
        let thresholds = self.node_thresholds.read().await;
        thresholds.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    
    fn create_proposal(node_id: &str, decision: &str, confidence: f64) -> Proposal {
        Proposal {
            id: uuid::Uuid::new_v4().to_string(),
            node_id: node_id.to_string(),
            decision: decision.to_string(),
            confidence,
            reasoning: "Test reasoning".to_string(),
            job_priority: 5,
            wait_time_secs: 10,
            specialization_match: 0.8,
            timestamp: Utc::now(),
        }
    }
    
    #[test]
    fn test_calculate_tendency() {
        // High stimulus, low threshold = high tendency
        let tendency = calculate_tendency(0.9, 0.3, 0.5, 0.6);
        assert!(tendency > 0.7);
        
        // Low stimulus, high threshold = low tendency
        let tendency = calculate_tendency(0.2, 0.9, 0.1, 0.6);
        assert!(tendency < 0.3);
    }
    
    #[test]
    fn test_calculate_stimulus() {
        let proposal = create_proposal("node-1", "decision-A", 0.9);
        let stimulus = calculate_stimulus(&proposal);
        
        assert!(stimulus > 0.5);
        assert!(stimulus <= 1.0);
    }
    
    #[tokio::test]
    async fn test_consensus_reached() {
        let consensus = AdaptiveConsensus::new(ConsensusCriteria::AdaptiveThreshold {
            base_threshold: 0.5,
            stimulus_weight: 0.6,
            demand_weight: 0.4,
            min_participants: 2,
        });
        
        let proposals = vec![
            create_proposal("node-1", "decision-A", 0.9),
            create_proposal("node-2", "decision-A", 0.85),
            create_proposal("node-3", "decision-B", 0.6),
        ];
        
        let result = consensus.propose_decision(proposals).await.unwrap();
        
        assert!(result.reached);
        assert_eq!(result.decision, Some("decision-A".to_string()));
    }
    
    #[tokio::test]
    async fn test_threshold_adaptation() {
        let consensus = AdaptiveConsensus::new(ConsensusCriteria::default());
        
        let initial = consensus.get_node_threshold(&"node-1".to_string()).await;
        
        // Success should lower threshold
        consensus.update_threshold(&"node-1".to_string(), true).await;
        let after_success = consensus.get_node_threshold(&"node-1".to_string()).await;
        assert!(after_success < initial);
        
        // Failure should raise threshold
        consensus.update_threshold(&"node-1".to_string(), false).await;
        consensus.update_threshold(&"node-1".to_string(), false).await;
        let after_failure = consensus.get_node_threshold(&"node-1".to_string()).await;
        assert!(after_failure > after_success);
    }
    
    #[tokio::test]
    async fn test_threshold_bounds() {
        let consensus = AdaptiveConsensus::new(ConsensusCriteria::default());
        
        // Many successes should not go below 0.3
        for _ in 0..50 {
            consensus.update_threshold(&"node-1".to_string(), true).await;
        }
        let threshold = consensus.get_node_threshold(&"node-1".to_string()).await;
        assert!(threshold >= 0.3);
        
        // Many failures should not go above 0.9
        for _ in 0..50 {
            consensus.update_threshold(&"node-2".to_string(), false).await;
        }
        let threshold = consensus.get_node_threshold(&"node-2".to_string()).await;
        assert!(threshold <= 0.9);
    }
}

