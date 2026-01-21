//! Property-based tests for status and metrics reporting
//!
//! Validates:
//! - Property 24: Health status reporting
//! - Property 25: Metrics exposure
//! - Property 29: Queue status reporting

use mvp_node::monitoring::{
    DefaultHealthMonitor, MonitoringEvent, JobMetrics, NetworkMetrics, InferenceMetrics,
    HealthStatus, NodeStatus, NodeState, ComponentHealth, ComponentStatus,
    DetailedQueueStatus, PerformanceMetrics, MetricsCollector, ResourceUsage,
};
use proptest::prelude::*;
use std::time::Duration;

// ============================================================================
// Property 24: Health status reporting tests
// ============================================================================

/// Property: Health queries should always return valid status
#[tokio::test]
async fn test_health_status_always_valid() {
    let monitor = DefaultHealthMonitor::new();
    
    let status = monitor.get_full_health_status().await;
    
    // Status should have valid uptime
    assert!(status.uptime_seconds >= 0);
    // Error count should be non-negative
    assert!(status.error_count >= 0);
}

/// Property: Health status should include all required fields
#[tokio::test]
async fn test_health_status_complete_fields() {
    let monitor = DefaultHealthMonitor::new();
    
    monitor.set_model_loaded(true).await;
    monitor.set_network_connected(true).await;
    monitor.set_queue_size(5).await;
    
    let status = monitor.get_full_health_status().await;
    
    // All required fields should be present
    assert!(status.model_loaded);
    assert!(status.network_connected);
    assert_eq!(status.queue_size, 5);
}

/// Property: Health status reflects recorded errors
#[tokio::test]
async fn test_health_status_reflects_errors() {
    let monitor = DefaultHealthMonitor::new();
    
    // Record some errors
    monitor.record_error("test", "Error 1").await;
    monitor.record_error("test", "Error 2").await;
    monitor.record_error("test", "Error 3").await;
    
    let status = monitor.get_full_health_status().await;
    
    assert_eq!(status.error_count, 3);
    assert!(status.last_error.is_some());
    assert!(status.last_error.unwrap().contains("Error 3"));
}

/// Property: Node status includes version information
#[test]
fn test_node_status_includes_version() {
    let status = NodeStatus::new("test-node");
    
    assert!(!status.version.is_empty());
    assert_eq!(status.node_id, "test-node");
}

/// Property: Component health reflects individual component status
#[test]
fn test_component_health_individual_status() {
    let mut health = ComponentHealth::default();
    
    // Initially all healthy
    assert!(health.is_healthy());
    
    // Mark one component unhealthy
    health.network.is_healthy = false;
    
    assert!(!health.is_healthy());
    assert_eq!(health.unhealthy_components(), vec!["network"]);
}

/// Property: Node status reflects operational state
#[test]
fn test_node_status_reflects_state() {
    let mut status = NodeStatus::new("test-node");
    
    assert_eq!(status.state, NodeState::Initializing);
    
    status.state = NodeState::Running;
    assert!(status.is_healthy()); // Running state is healthy
    
    status.state = NodeState::Error;
    assert!(!status.is_healthy()); // Error state is not healthy
}

// ============================================================================
// Property 25: Metrics exposure tests
// ============================================================================

/// Property: Metrics should always be non-negative
#[tokio::test]
async fn test_metrics_non_negative() {
    let monitor = DefaultHealthMonitor::new();
    
    // Record some activity
    monitor.record_job_submitted().await;
    monitor.record_job_completed(100, 50).await;
    monitor.record_peer_connected().await;
    
    let job_metrics = monitor.get_job_metrics().await;
    let network_metrics = monitor.get_network_metrics().await;
    
    assert!(job_metrics.jobs_submitted >= 0);
    assert!(job_metrics.jobs_completed >= 0);
    assert!(job_metrics.total_tokens_processed >= 0);
    assert!(network_metrics.peer_connections >= 0);
    assert!(network_metrics.active_peers >= 0);
}

/// Property: Metrics exposure includes all categories
#[test]
fn test_metrics_includes_all_categories() {
    let monitor = DefaultHealthMonitor::new();
    
    let metrics = monitor.get_metrics();
    
    // Should include job metrics
    assert!(metrics.contains_key("jobs_submitted"));
    assert!(metrics.contains_key("jobs_completed"));
    
    // Should include network metrics
    assert!(metrics.contains_key("peer_connections"));
    assert!(metrics.contains_key("active_peers"));
    
    // Should include inference metrics
    assert!(metrics.contains_key("inferences_executed"));
    
    // Should include uptime
    assert!(metrics.contains_key("uptime_seconds"));
}

/// Property: Metrics accurately reflect recorded events
#[tokio::test]
async fn test_metrics_accuracy() {
    let monitor = DefaultHealthMonitor::new();
    
    // Record specific events
    monitor.record_job_submitted().await;
    monitor.record_job_submitted().await;
    monitor.record_job_submitted().await;
    monitor.record_job_completed(100, 25).await;
    monitor.record_job_completed(200, 30).await;
    monitor.record_job_failed().await;
    
    let metrics = monitor.get_job_metrics().await;
    
    assert_eq!(metrics.jobs_submitted, 3);
    assert_eq!(metrics.jobs_completed, 2);
    assert_eq!(metrics.jobs_failed, 1);
    assert_eq!(metrics.total_tokens_processed, 55);
    assert_eq!(metrics.total_processing_time_ms, 300);
    assert_eq!(metrics.average_processing_time_ms, 150.0);
}

/// Property: Network metrics track peer activity
#[tokio::test]
async fn test_network_metrics_tracking() {
    let monitor = DefaultHealthMonitor::new();
    
    monitor.record_peer_connected().await;
    monitor.record_peer_connected().await;
    monitor.record_peer_disconnected().await;
    monitor.record_message_sent(100).await;
    monitor.record_message_received(200).await;
    
    let metrics = monitor.get_network_metrics().await;
    
    assert_eq!(metrics.peer_connections, 2);
    assert_eq!(metrics.peer_disconnections, 1);
    assert_eq!(metrics.active_peers, 1);
    assert_eq!(metrics.bytes_sent, 100);
    assert_eq!(metrics.bytes_received, 200);
}

/// Property: Inference metrics track execution
#[tokio::test]
async fn test_inference_metrics_tracking() {
    let monitor = DefaultHealthMonitor::new();
    
    monitor.record_inference_completed(100).await;
    monitor.record_inference_completed(200).await;
    monitor.record_inference_failed().await;
    
    let metrics = monitor.get_inference_metrics().await;
    
    assert_eq!(metrics.inferences_executed, 2);
    assert_eq!(metrics.inferences_failed, 1);
    assert_eq!(metrics.average_inference_time_ms, 150.0);
}

/// Property: Custom metrics can be recorded and retrieved
#[test]
fn test_custom_metrics() {
    let monitor = DefaultHealthMonitor::new();
    
    monitor.record_metric("custom_value", 42.5);
    monitor.record_metric("another_metric", 100.0);
    
    let metrics = monitor.get_metrics();
    
    assert_eq!(metrics.get("custom_value"), Some(&42.5));
    assert_eq!(metrics.get("another_metric"), Some(&100.0));
}

// ============================================================================
// Property 29: Queue status reporting tests
// ============================================================================

/// Property: Queue status includes capacity information
#[test]
fn test_queue_status_capacity() {
    let mut status = DetailedQueueStatus::default();
    status.pending_jobs = 10;
    status.max_capacity = 100;
    
    status.calculate_utilization();
    
    assert_eq!(status.utilization_percent, 10.0);
}

/// Property: Queue status can_accept reflects capacity
#[test]
fn test_queue_status_can_accept() {
    let mut status = DetailedQueueStatus::default();
    status.max_capacity = 10;
    
    status.pending_jobs = 5;
    assert!(status.can_accept());
    
    status.pending_jobs = 10;
    assert!(!status.can_accept());
    
    status.pending_jobs = 5;
    status.accepting_jobs = false;
    assert!(!status.can_accept());
}

/// Property: Queue status estimates wait time
#[test]
fn test_queue_status_wait_time() {
    let mut status = DetailedQueueStatus::default();
    status.pending_jobs = 5;
    status.avg_processing_time_ms = 1000.0; // 1 second per job
    
    status.estimate_wait_time();
    
    assert_eq!(status.estimated_wait_secs, 5.0);
}

/// Property: Queue status includes timestamp
#[test]
fn test_queue_status_has_timestamp() {
    let status = DetailedQueueStatus::default();
    
    assert!(status.timestamp > 0);
}

// ============================================================================
// MetricsCollector tests
// ============================================================================

/// Property: Metrics collector tracks latency percentiles
#[tokio::test]
async fn test_metrics_collector_percentiles() {
    let collector = MetricsCollector::new("test-node");
    
    // Record latencies
    for i in 1..=100 {
        collector.record_latency(i as f64).await;
    }
    
    let metrics = collector.get_performance_metrics().await;
    
    // P50 should be around 50
    assert!(metrics.jobs.p50_latency_ms >= 45.0 && metrics.jobs.p50_latency_ms <= 55.0);
    // P95 should be around 95
    assert!(metrics.jobs.p95_latency_ms >= 90.0 && metrics.jobs.p95_latency_ms <= 100.0);
    // P99 should be around 99
    assert!(metrics.jobs.p99_latency_ms >= 95.0 && metrics.jobs.p99_latency_ms <= 100.0);
}

/// Property: Metrics collector tracks job success rate
#[tokio::test]
async fn test_metrics_collector_success_rate() {
    let collector = MetricsCollector::new("test-node");
    
    collector.record_job_success(100.0).await;
    collector.record_job_success(100.0).await;
    collector.record_job_success(100.0).await;
    collector.record_job_failure().await;
    
    let metrics = collector.get_performance_metrics().await;
    
    assert_eq!(metrics.jobs.total_processed, 4);
    assert_eq!(metrics.jobs.successful, 3);
    assert_eq!(metrics.jobs.failed, 1);
    assert_eq!(metrics.jobs.success_rate, 75.0);
}

/// Property: Metrics collector tracks network messages
#[tokio::test]
async fn test_metrics_collector_network() {
    let collector = MetricsCollector::new("test-node");
    
    collector.record_message(true, 100).await;
    collector.record_message(true, 200).await;
    collector.record_message(false, 150).await;
    
    let metrics = collector.get_performance_metrics().await;
    
    assert_eq!(metrics.network.messages_sent, 2);
    assert_eq!(metrics.network.bytes_sent, 300);
    assert_eq!(metrics.network.messages_received, 1);
    assert_eq!(metrics.network.bytes_received, 150);
}

/// Property: Metrics collector tracks peer counts
#[tokio::test]
async fn test_metrics_collector_peers() {
    let collector = MetricsCollector::new("test-node");
    
    collector.update_peer_count(5).await;
    collector.update_peer_count(10).await;
    collector.update_peer_count(8).await;
    
    let metrics = collector.get_performance_metrics().await;
    
    assert_eq!(metrics.network.active_peers, 8);
    assert_eq!(metrics.network.peak_peers, 10);
}

/// Property: Metrics collector tracks inference
#[tokio::test]
async fn test_metrics_collector_inference() {
    let collector = MetricsCollector::new("test-node");
    
    collector.record_inference(100, 100).await;
    collector.record_inference(200, 200).await;
    
    let metrics = collector.get_performance_metrics().await;
    
    assert_eq!(metrics.inference.total_inferences, 2);
    assert_eq!(metrics.inference.tokens_processed, 300);
    assert_eq!(metrics.inference.avg_inference_time_ms, 150.0);
}

/// Property: Metrics collector updates queue status
#[tokio::test]
async fn test_metrics_collector_queue() {
    let collector = MetricsCollector::new("test-node");
    
    collector.update_queue_status(10, 2, 100).await;
    
    let status = collector.get_queue_status().await;
    
    assert_eq!(status.pending_jobs, 10);
    assert_eq!(status.processing_jobs, 2);
    assert_eq!(status.max_capacity, 100);
    assert_eq!(status.utilization_percent, 12.0);
}

/// Property: Node status reflects current state
#[tokio::test]
async fn test_metrics_collector_node_status() {
    let collector = MetricsCollector::new("test-node");
    
    // Initially idle
    let status = collector.get_node_status().await;
    assert_eq!(status.state, NodeState::Idle);
    
    // After processing jobs
    collector.record_job_success(100.0).await;
    let status = collector.get_node_status().await;
    assert_eq!(status.state, NodeState::Running);
}

// ============================================================================
// Property tests with proptest
// ============================================================================

proptest! {
    /// Property: Job metrics accurately track counts
    #[test]
    fn job_metrics_accurate(
        submitted in 0u64..100u64,
        completed in 0u64..100u64,
        failed in 0u64..100u64
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let monitor = DefaultHealthMonitor::new();
            
            for _ in 0..submitted {
                monitor.record_job_submitted().await;
            }
            for _ in 0..completed {
                monitor.record_job_completed(100, 10).await;
            }
            for _ in 0..failed {
                monitor.record_job_failed().await;
            }
            
            let metrics = monitor.get_job_metrics().await;
            
            prop_assert_eq!(metrics.jobs_submitted, submitted);
            prop_assert_eq!(metrics.jobs_completed, completed);
            prop_assert_eq!(metrics.jobs_failed, failed);
            
            Ok(())
        }).unwrap();
    }
    
    /// Property: Network bytes are accurately tracked
    #[test]
    fn network_bytes_accurate(
        sent_bytes in proptest::collection::vec(1u64..1000u64, 0..10),
        recv_bytes in proptest::collection::vec(1u64..1000u64, 0..10)
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let monitor = DefaultHealthMonitor::new();
            
            let expected_sent: u64 = sent_bytes.iter().sum();
            let expected_recv: u64 = recv_bytes.iter().sum();
            
            for bytes in sent_bytes {
                monitor.record_message_sent(bytes).await;
            }
            for bytes in recv_bytes {
                monitor.record_message_received(bytes).await;
            }
            
            let metrics = monitor.get_network_metrics().await;
            
            prop_assert_eq!(metrics.bytes_sent, expected_sent);
            prop_assert_eq!(metrics.bytes_received, expected_recv);
            
            Ok(())
        }).unwrap();
    }
    
    /// Property: Queue utilization correctly calculated
    #[test]
    fn queue_utilization_correct(
        pending in 0usize..100usize,
        processing in 0usize..10usize,
        capacity in 1usize..200usize
    ) {
        let mut status = DetailedQueueStatus::default();
        status.pending_jobs = pending;
        status.processing_jobs = processing;
        status.max_capacity = capacity;
        
        status.calculate_utilization();
        
        let expected = (pending + processing) as f64 / capacity as f64 * 100.0;
        prop_assert!((status.utilization_percent - expected).abs() < 0.01);
    }
}

/// Property: Event logging does not panic
#[test]
fn test_event_logging_no_panic() {
    let monitor = DefaultHealthMonitor::new();
    
    let events = vec![
        MonitoringEvent::JobSubmitted { job_id: "job1".to_string(), model: "model1".to_string() },
        MonitoringEvent::JobStarted { job_id: "job1".to_string() },
        MonitoringEvent::JobCompleted { job_id: "job1".to_string(), duration_ms: 100, tokens: 50 },
        MonitoringEvent::JobFailed { job_id: "job2".to_string(), error: "Error".to_string(), duration_ms: 50 },
        MonitoringEvent::PeerConnected { peer_id: "peer1".to_string() },
        MonitoringEvent::PeerDisconnected { peer_id: "peer1".to_string(), reason: Some("timeout".to_string()) },
        MonitoringEvent::MessageSent { topic: "test".to_string(), size_bytes: 100 },
        MonitoringEvent::MessageReceived { topic: "test".to_string(), peer_id: "peer1".to_string(), size_bytes: 200 },
        MonitoringEvent::ModelLoaded { model_name: "model".to_string(), parameters: 1000 },
        MonitoringEvent::NodeStarted { node_id: "node1".to_string() },
        MonitoringEvent::ErrorOccurred { component: "test".to_string(), error: "Error".to_string() },
    ];
    
    for event in events {
        monitor.log_event(event); // Should not panic
    }
}

/// Property: Health status map includes required keys
#[test]
fn test_health_status_map_keys() {
    let monitor = DefaultHealthMonitor::new();
    
    let status = monitor.get_health_status_map();
    
    assert!(status.contains_key("uptime_seconds"));
    assert!(status.contains_key("error_count"));
    assert!(status.contains_key("is_healthy"));
}











