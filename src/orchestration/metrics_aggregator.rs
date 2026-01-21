//! Metrics Aggregator for the LLM Orchestrator
//!
//! This module collects and summarizes metrics from the DefaultHealthMonitor
//! into a format optimized for LLM consumption.

use crate::monitoring::{
    DefaultHealthMonitor, HealthStatus, InferenceMetrics, JobMetrics, NetworkMetrics,
};
use crate::orchestration::error::{OrchestratorError, OrchestratorResult};
use crate::orchestration::types::{
    CapacityLevel, ContextState, HealthLevel, LLMState, ModelState, NetworkState, NodeState,
    RiskLevel,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, trace};

/// Trait for metrics aggregation
#[async_trait::async_trait]
pub trait MetricsAggregatorTrait: Send + Sync {
    /// Get the current state formatted for LLM consumption
    async fn get_llm_state(&self) -> OrchestratorResult<LLMState>;
    
    /// Get a hash of the current state for cache key generation
    async fn get_state_hash(&self) -> OrchestratorResult<u64>;
    
    /// Calculate capacity level from job metrics
    fn calculate_capacity(&self, metrics: &JobMetrics, max_queue: usize) -> CapacityLevel;
    
    /// Assess health level from health status
    fn assess_health(&self, status: &HealthStatus) -> HealthLevel;
    
    /// Assess partition risk from network metrics
    fn assess_partition_risk(&self, network: &NetworkMetrics) -> RiskLevel;
}

/// Metrics Aggregator implementation
pub struct MetricsAggregator {
    /// Reference to the health monitor
    health_monitor: Arc<DefaultHealthMonitor>,
    /// Node identifier
    node_id: String,
    /// Maximum queue size for capacity calculations
    max_queue_size: usize,
    /// Cached state with timestamp
    state_cache: Arc<RwLock<Option<(LLMState, Instant)>>>,
    /// Cache time-to-live
    cache_ttl: Duration,
    /// Minimum peers for low partition risk
    min_peers_low_risk: u64,
}

impl MetricsAggregator {
    /// Create a new metrics aggregator
    pub fn new(
        health_monitor: Arc<DefaultHealthMonitor>,
        node_id: String,
        max_queue_size: usize,
    ) -> Self {
        Self {
            health_monitor,
            node_id,
            max_queue_size,
            state_cache: Arc::new(RwLock::new(None)),
            cache_ttl: Duration::from_secs(5), // Cache for 5 seconds
            min_peers_low_risk: 3,
        }
    }
    
    /// Create with custom cache TTL
    pub fn with_cache_ttl(mut self, ttl: Duration) -> Self {
        self.cache_ttl = ttl;
        self
    }
    
    /// Check if cached state is still valid
    async fn is_cache_valid(&self) -> bool {
        let cache = self.state_cache.read().await;
        match cache.as_ref() {
            Some((_, timestamp)) => timestamp.elapsed() < self.cache_ttl,
            None => false,
        }
    }
    
    /// Invalidate the cache
    pub async fn invalidate_cache(&self) {
        let mut cache = self.state_cache.write().await;
        *cache = None;
        debug!("Metrics cache invalidated");
    }
    
    /// Collect all metrics from the health monitor
    async fn collect_metrics(&self) -> (JobMetrics, NetworkMetrics, InferenceMetrics, HealthStatus) {
        let job_metrics = self.health_monitor.get_job_metrics().await;
        let network_metrics = self.health_monitor.get_network_metrics().await;
        let inference_metrics = self.health_monitor.get_inference_metrics().await;
        let health_status = self.health_monitor.get_full_health_status().await;
        
        (job_metrics, network_metrics, inference_metrics, health_status)
    }
    
    /// Build LLMState from collected metrics
    fn build_llm_state(
        &self,
        job_metrics: &JobMetrics,
        network_metrics: &NetworkMetrics,
        inference_metrics: &InferenceMetrics,
        health_status: &HealthStatus,
    ) -> LLMState {
        // Build NodeState
        let node = NodeState {
            id: self.node_id.clone(),
            capacity: self.calculate_capacity(job_metrics, self.max_queue_size),
            latency_avg: job_metrics.average_processing_time_ms as u64,
            uptime_seconds: health_status.uptime_seconds,
            error_count: health_status.error_count,
        };
        
        // Build ContextState
        let context = ContextState {
            id: format!("{}-ctx", self.node_id),
            health: self.assess_health(health_status),
            queue_size: health_status.queue_size,
            max_queue: self.max_queue_size,
            processing_jobs: if health_status.queue_size > 0 { 1 } else { 0 },
        };
        
        // Build NetworkState
        let network = NetworkState {
            partition_risk: self.assess_partition_risk(network_metrics),
            peers_available: network_metrics.active_peers,
            active_connections: network_metrics.active_peers,
            message_latency_avg: None, // Not tracked in current metrics
        };
        
        // Build ModelState
        let model = ModelState {
            loaded: inference_metrics.model_loaded,
            avg_inference_ms: inference_metrics.average_inference_time_ms as u64,
            memory_usage_mb: inference_metrics.peak_memory_mb,
            recent_failures: inference_metrics.inferences_failed,
        };
        
        LLMState::new(node, context, network, model)
    }
    
    /// Detect significant state changes that should invalidate cache
    pub async fn has_significant_change(&self, new_state: &LLMState) -> bool {
        let cache = self.state_cache.read().await;
        
        let old_state = match cache.as_ref() {
            Some((state, _)) => state,
            None => return true, // No old state, definitely changed
        };
        
        // Check for significant changes
        // Capacity level changed
        if old_state.node.capacity != new_state.node.capacity {
            return true;
        }
        
        // Health level changed
        if old_state.context.health != new_state.context.health {
            return true;
        }
        
        // Partition risk changed
        if old_state.network.partition_risk != new_state.network.partition_risk {
            return true;
        }
        
        // Model load state changed
        if old_state.model.loaded != new_state.model.loaded {
            return true;
        }
        
        // Queue size changed significantly (>20%)
        if old_state.context.queue_size > 0 {
            let ratio = (new_state.context.queue_size as f64 - old_state.context.queue_size as f64).abs()
                / old_state.context.queue_size as f64;
            if ratio > 0.2 {
                return true;
            }
        }
        
        // Error count increased significantly
        if new_state.node.error_count > old_state.node.error_count + 5 {
            return true;
        }
        
        false
    }
}

#[async_trait::async_trait]
impl MetricsAggregatorTrait for MetricsAggregator {
    async fn get_llm_state(&self) -> OrchestratorResult<LLMState> {
        // Check cache first
        if self.is_cache_valid().await {
            let cache = self.state_cache.read().await;
            if let Some((state, _)) = cache.as_ref() {
                trace!("Returning cached LLM state");
                return Ok(state.clone());
            }
        }
        
        // Collect fresh metrics
        let (job_metrics, network_metrics, inference_metrics, health_status) = 
            self.collect_metrics().await;
        
        // Build state
        let state = self.build_llm_state(
            &job_metrics,
            &network_metrics,
            &inference_metrics,
            &health_status,
        );
        
        // Update cache
        {
            let mut cache = self.state_cache.write().await;
            *cache = Some((state.clone(), Instant::now()));
        }
        
        debug!(
            node_id = %state.node.id,
            capacity = %state.node.capacity,
            health = %state.context.health,
            queue_size = state.context.queue_size,
            "Collected LLM state"
        );
        
        Ok(state)
    }
    
    async fn get_state_hash(&self) -> OrchestratorResult<u64> {
        let state = self.get_llm_state().await?;
        Ok(state.calculate_hash())
    }
    
    fn calculate_capacity(&self, metrics: &JobMetrics, max_queue: usize) -> CapacityLevel {
        // Use pending jobs (submitted - completed - failed)
        let pending = metrics.jobs_submitted
            .saturating_sub(metrics.jobs_completed)
            .saturating_sub(metrics.jobs_failed) as usize;
        
        CapacityLevel::from_utilization(pending, max_queue)
    }
    
    fn assess_health(&self, status: &HealthStatus) -> HealthLevel {
        // Consider multiple factors
        if !status.is_healthy {
            return HealthLevel::Critical;
        }
        
        if status.error_count > 50 {
            return HealthLevel::Critical;
        }
        
        if status.error_count > 20 {
            return HealthLevel::Unhealthy;
        }
        
        if status.error_count > 5 {
            return HealthLevel::Degraded;
        }
        
        if !status.model_loaded || !status.network_connected {
            return HealthLevel::Degraded;
        }
        
        HealthLevel::Healthy
    }
    
    fn assess_partition_risk(&self, network: &NetworkMetrics) -> RiskLevel {
        RiskLevel::from_peer_count(network.active_peers, self.min_peers_low_risk)
    }
}

/// Mock metrics aggregator for testing
pub struct MockMetricsAggregator {
    state: LLMState,
}

impl MockMetricsAggregator {
    pub fn new(state: LLMState) -> Self {
        Self { state }
    }
    
    pub fn with_default_state() -> Self {
        let state = LLMState::new(
            NodeState {
                id: "mock-node".to_string(),
                capacity: CapacityLevel::Medium,
                latency_avg: 100,
                uptime_seconds: 3600,
                error_count: 0,
            },
            ContextState {
                id: "mock-ctx".to_string(),
                health: HealthLevel::Healthy,
                queue_size: 10,
                max_queue: 100,
                processing_jobs: 1,
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
        Self { state }
    }
    
    pub fn set_state(&mut self, state: LLMState) {
        self.state = state;
    }
}

#[async_trait::async_trait]
impl MetricsAggregatorTrait for MockMetricsAggregator {
    async fn get_llm_state(&self) -> OrchestratorResult<LLMState> {
        Ok(self.state.clone())
    }
    
    async fn get_state_hash(&self) -> OrchestratorResult<u64> {
        Ok(self.state.calculate_hash())
    }
    
    fn calculate_capacity(&self, _metrics: &JobMetrics, _max_queue: usize) -> CapacityLevel {
        self.state.node.capacity
    }
    
    fn assess_health(&self, _status: &HealthStatus) -> HealthLevel {
        self.state.context.health
    }
    
    fn assess_partition_risk(&self, _network: &NetworkMetrics) -> RiskLevel {
        self.state.network.partition_risk
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_mock_job_metrics(submitted: u64, completed: u64, failed: u64) -> JobMetrics {
        JobMetrics {
            jobs_submitted: submitted,
            jobs_completed: completed,
            jobs_failed: failed,
            total_processing_time_ms: 10000,
            average_processing_time_ms: 100.0,
            total_tokens_processed: 1000,
        }
    }

    fn create_mock_network_metrics(peers: u64) -> NetworkMetrics {
        NetworkMetrics {
            peer_connections: peers,
            peer_disconnections: 0,
            messages_sent: 100,
            messages_received: 100,
            bytes_sent: 10000,
            bytes_received: 10000,
            active_peers: peers,
        }
    }

    fn create_mock_health_status(healthy: bool, errors: u64) -> HealthStatus {
        HealthStatus {
            is_healthy: healthy,
            uptime_seconds: 3600,
            model_loaded: true,
            network_connected: true,
            queue_size: 10,
            error_count: errors,
            last_error: None,
        }
    }

    #[tokio::test]
    async fn test_metrics_aggregator_creation() {
        let monitor = Arc::new(DefaultHealthMonitor::new());
        let aggregator = MetricsAggregator::new(monitor, "test-node".to_string(), 100);
        
        let state = aggregator.get_llm_state().await.unwrap();
        assert_eq!(state.node.id, "test-node");
    }

    #[test]
    fn test_capacity_calculation() {
        let monitor = Arc::new(DefaultHealthMonitor::new());
        let aggregator = MetricsAggregator::new(monitor, "test".to_string(), 100);
        
        // Low capacity
        let metrics = create_mock_job_metrics(10, 5, 0);
        assert_eq!(aggregator.calculate_capacity(&metrics, 100), CapacityLevel::Low);
        
        // Medium capacity
        let metrics = create_mock_job_metrics(50, 10, 0);
        assert_eq!(aggregator.calculate_capacity(&metrics, 100), CapacityLevel::Medium);
        
        // High capacity
        let metrics = create_mock_job_metrics(80, 10, 0);
        assert_eq!(aggregator.calculate_capacity(&metrics, 100), CapacityLevel::High);
        
        // Overloaded
        let metrics = create_mock_job_metrics(100, 5, 0);
        assert_eq!(aggregator.calculate_capacity(&metrics, 100), CapacityLevel::Overloaded);
    }

    #[test]
    fn test_health_assessment() {
        let monitor = Arc::new(DefaultHealthMonitor::new());
        let aggregator = MetricsAggregator::new(monitor, "test".to_string(), 100);
        
        // Healthy
        let status = create_mock_health_status(true, 0);
        assert_eq!(aggregator.assess_health(&status), HealthLevel::Healthy);
        
        // Degraded
        let status = create_mock_health_status(true, 10);
        assert_eq!(aggregator.assess_health(&status), HealthLevel::Degraded);
        
        // Unhealthy
        let status = create_mock_health_status(true, 30);
        assert_eq!(aggregator.assess_health(&status), HealthLevel::Unhealthy);
        
        // Critical
        let status = create_mock_health_status(false, 0);
        assert_eq!(aggregator.assess_health(&status), HealthLevel::Critical);
    }

    #[test]
    fn test_partition_risk_assessment() {
        let monitor = Arc::new(DefaultHealthMonitor::new());
        let aggregator = MetricsAggregator::new(monitor, "test".to_string(), 100);
        
        // Low risk (many peers)
        let network = create_mock_network_metrics(10);
        assert_eq!(aggregator.assess_partition_risk(&network), RiskLevel::Low);
        
        // High risk (few peers)
        let network = create_mock_network_metrics(2);
        assert_eq!(aggregator.assess_partition_risk(&network), RiskLevel::Critical);
    }

    #[tokio::test]
    async fn test_state_caching() {
        let monitor = Arc::new(DefaultHealthMonitor::new());
        let aggregator = MetricsAggregator::new(monitor, "test-node".to_string(), 100)
            .with_cache_ttl(Duration::from_secs(10));
        
        // First call populates cache
        let state1 = aggregator.get_llm_state().await.unwrap();
        
        // Second call should use cache
        let state2 = aggregator.get_llm_state().await.unwrap();
        
        // States should be identical (from cache)
        assert_eq!(state1.node.id, state2.node.id);
    }

    #[tokio::test]
    async fn test_cache_invalidation() {
        let monitor = Arc::new(DefaultHealthMonitor::new());
        let aggregator = MetricsAggregator::new(monitor, "test-node".to_string(), 100);
        
        // Populate cache
        let _ = aggregator.get_llm_state().await.unwrap();
        assert!(aggregator.is_cache_valid().await);
        
        // Invalidate
        aggregator.invalidate_cache().await;
        assert!(!aggregator.is_cache_valid().await);
    }

    #[tokio::test]
    async fn test_mock_aggregator() {
        let mock = MockMetricsAggregator::with_default_state();
        
        let state = mock.get_llm_state().await.unwrap();
        assert_eq!(state.node.id, "mock-node");
        assert_eq!(state.node.capacity, CapacityLevel::Medium);
    }

    #[tokio::test]
    async fn test_state_hash() {
        let monitor = Arc::new(DefaultHealthMonitor::new());
        let aggregator = MetricsAggregator::new(monitor, "test-node".to_string(), 100);
        
        let hash1 = aggregator.get_state_hash().await.unwrap();
        let hash2 = aggregator.get_state_hash().await.unwrap();
        
        // Same state should produce same hash
        assert_eq!(hash1, hash2);
    }
}

