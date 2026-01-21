//! Metrics for Distributed Orchestration
//!
//! Collects and exposes metrics for monitoring the distributed orchestration system.

use super::types::DistributedMetrics;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Metrics collector for distributed orchestration
pub struct MetricsCollector {
    /// Total decisions made
    total_decisions: AtomicU64,
    
    /// Decisions requiring consensus
    consensus_decisions: AtomicU64,
    
    /// Heuristic-only decisions
    heuristic_decisions: AtomicU64,
    
    /// LLM-based decisions
    llm_decisions: AtomicU64,
    
    /// Hybrid (heuristic + LLM validation) decisions
    hybrid_decisions: AtomicU64,
    
    /// Total consensus latency (for averaging)
    total_consensus_latency_ms: AtomicU64,
    
    /// Total consensus rounds (for averaging)
    total_consensus_rounds: AtomicU64,
    
    /// Coordinator rotations
    coordinator_rotations: AtomicU64,
    
    /// Context groups formed
    context_groups_formed: AtomicU64,
    
    /// Context groups dissolved
    context_groups_dissolved: AtomicU64,
    
    /// Cross-context jobs
    cross_context_jobs: AtomicU64,
    
    /// Partition events
    partition_events: AtomicU64,
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsCollector {
    /// Create a new MetricsCollector
    pub fn new() -> Self {
        Self {
            total_decisions: AtomicU64::new(0),
            consensus_decisions: AtomicU64::new(0),
            heuristic_decisions: AtomicU64::new(0),
            llm_decisions: AtomicU64::new(0),
            hybrid_decisions: AtomicU64::new(0),
            total_consensus_latency_ms: AtomicU64::new(0),
            total_consensus_rounds: AtomicU64::new(0),
            coordinator_rotations: AtomicU64::new(0),
            context_groups_formed: AtomicU64::new(0),
            context_groups_dissolved: AtomicU64::new(0),
            cross_context_jobs: AtomicU64::new(0),
            partition_events: AtomicU64::new(0),
        }
    }
    
    /// Record a decision
    pub fn record_decision(&self, is_consensus: bool, is_heuristic: bool, is_llm: bool, is_hybrid: bool) {
        self.total_decisions.fetch_add(1, Ordering::Relaxed);
        
        if is_consensus {
            self.consensus_decisions.fetch_add(1, Ordering::Relaxed);
        }
        if is_heuristic {
            self.heuristic_decisions.fetch_add(1, Ordering::Relaxed);
        }
        if is_llm {
            self.llm_decisions.fetch_add(1, Ordering::Relaxed);
        }
        if is_hybrid {
            self.hybrid_decisions.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    /// Record consensus metrics
    pub fn record_consensus(&self, latency_ms: u64, rounds: u64) {
        self.total_consensus_latency_ms.fetch_add(latency_ms, Ordering::Relaxed);
        self.total_consensus_rounds.fetch_add(rounds, Ordering::Relaxed);
    }
    
    /// Record coordinator rotation
    pub fn record_rotation(&self) {
        self.coordinator_rotations.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record group formation
    pub fn record_group_formed(&self) {
        self.context_groups_formed.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record group dissolution
    pub fn record_group_dissolved(&self) {
        self.context_groups_dissolved.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record cross-context job
    pub fn record_cross_context_job(&self) {
        self.cross_context_jobs.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record partition event
    pub fn record_partition_event(&self) {
        self.partition_events.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Get current metrics
    pub fn get_metrics(&self) -> DistributedMetrics {
        let total = self.total_decisions.load(Ordering::Relaxed);
        let consensus_count = self.consensus_decisions.load(Ordering::Relaxed);
        
        let avg_consensus_latency = if consensus_count > 0 {
            self.total_consensus_latency_ms.load(Ordering::Relaxed) as f64 / consensus_count as f64
        } else {
            0.0
        };
        
        let avg_consensus_rounds = if consensus_count > 0 {
            self.total_consensus_rounds.load(Ordering::Relaxed) as f64 / consensus_count as f64
        } else {
            0.0
        };
        
        DistributedMetrics {
            total_decisions: total,
            consensus_decisions: consensus_count,
            heuristic_decisions: self.heuristic_decisions.load(Ordering::Relaxed),
            llm_decisions: self.llm_decisions.load(Ordering::Relaxed),
            hybrid_decisions: self.hybrid_decisions.load(Ordering::Relaxed),
            avg_consensus_latency_ms: avg_consensus_latency,
            avg_consensus_rounds: avg_consensus_rounds,
            coordinator_rotations: self.coordinator_rotations.load(Ordering::Relaxed),
            context_groups_formed: self.context_groups_formed.load(Ordering::Relaxed),
            context_groups_dissolved: self.context_groups_dissolved.load(Ordering::Relaxed),
            cross_context_jobs: self.cross_context_jobs.load(Ordering::Relaxed),
            partition_events: self.partition_events.load(Ordering::Relaxed),
        }
    }
    
    /// Reset all metrics
    pub fn reset(&self) {
        self.total_decisions.store(0, Ordering::Relaxed);
        self.consensus_decisions.store(0, Ordering::Relaxed);
        self.heuristic_decisions.store(0, Ordering::Relaxed);
        self.llm_decisions.store(0, Ordering::Relaxed);
        self.hybrid_decisions.store(0, Ordering::Relaxed);
        self.total_consensus_latency_ms.store(0, Ordering::Relaxed);
        self.total_consensus_rounds.store(0, Ordering::Relaxed);
        self.coordinator_rotations.store(0, Ordering::Relaxed);
        self.context_groups_formed.store(0, Ordering::Relaxed);
        self.context_groups_dissolved.store(0, Ordering::Relaxed);
        self.cross_context_jobs.store(0, Ordering::Relaxed);
        self.partition_events.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_record_decisions() {
        let collector = MetricsCollector::new();
        
        collector.record_decision(false, true, false, false); // Heuristic
        collector.record_decision(true, false, true, false);  // LLM + Consensus
        collector.record_decision(false, false, false, true); // Hybrid
        
        let metrics = collector.get_metrics();
        
        assert_eq!(metrics.total_decisions, 3);
        assert_eq!(metrics.heuristic_decisions, 1);
        assert_eq!(metrics.llm_decisions, 1);
        assert_eq!(metrics.hybrid_decisions, 1);
        assert_eq!(metrics.consensus_decisions, 1);
    }
    
    #[test]
    fn test_consensus_averages() {
        let collector = MetricsCollector::new();
        
        collector.record_decision(true, false, true, false);
        collector.record_consensus(100, 2);
        
        collector.record_decision(true, false, true, false);
        collector.record_consensus(200, 3);
        
        let metrics = collector.get_metrics();
        
        assert_eq!(metrics.consensus_decisions, 2);
        assert!((metrics.avg_consensus_latency_ms - 150.0).abs() < 0.001);
        assert!((metrics.avg_consensus_rounds - 2.5).abs() < 0.001);
    }
    
    #[test]
    fn test_group_metrics() {
        let collector = MetricsCollector::new();
        
        collector.record_group_formed();
        collector.record_group_formed();
        collector.record_group_dissolved();
        
        let metrics = collector.get_metrics();
        
        assert_eq!(metrics.context_groups_formed, 2);
        assert_eq!(metrics.context_groups_dissolved, 1);
    }
    
    #[test]
    fn test_reset() {
        let collector = MetricsCollector::new();
        
        collector.record_decision(true, true, false, false);
        collector.record_rotation();
        collector.record_partition_event();
        
        collector.reset();
        
        let metrics = collector.get_metrics();
        
        assert_eq!(metrics.total_decisions, 0);
        assert_eq!(metrics.coordinator_rotations, 0);
        assert_eq!(metrics.partition_events, 0);
    }
}

