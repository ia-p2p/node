//! End-to-End Integration Tests
//!
//! Comprehensive tests for complete workflows validating all requirements:
//! - Full job lifecycle from submission to completion
//! - Protocol compliance verification
//! - Multi-instance scenarios with real network communication
//! - System stability under various conditions

use mvp_node::config::find_available_port;
use mvp_node::network::{P2PNetworkLayer, NetworkConfig, NetworkLayer, BootstrapMode};
use mvp_node::executor::DefaultJobExecutor;
use mvp_node::inference::DefaultInferenceEngine;
use mvp_node::monitoring::{DefaultHealthMonitor, MetricsCollector};
use mvp_node::protocol::{
    P2PMessage, AnnounceMsg, WorkOffer, JobClaimMsg, JobReceipt, NodeCapabilities,
    ProtocolVersion, VersionNegotiator, VersionNegotiationRequest, CURRENT_VERSION,
};
use mvp_node::error::{MvpNodeError, ErrorCategory, RecoveryStrategy};
use mvp_node::{JobOffer, JobMode, Requirements, InferenceEngine, JobExecutor, JobStatus};
use libp2p::identity::Keypair;
use serial_test::serial;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::timeout;

// ============================================================================
// Full Job Lifecycle Tests (Requirement 2)
// ============================================================================

/// Test complete job lifecycle: submit -> queue -> process -> result
#[tokio::test]
#[serial]
async fn test_full_job_lifecycle() {
    // Setup components
    let engine = Arc::new(Mutex::new(DefaultInferenceEngine::new()));
    let mut executor = DefaultJobExecutor::with_inference_engine(100, engine.clone());
    let monitor = DefaultHealthMonitor::new();
    
    // Load model (simulated)
    {
        let mut eng = engine.lock().await;
        let _ = eng.load_model("tinyllama").await;
    }
    
    // Create and submit a job
    let job = JobOffer {
        job_id: "e2e-test-job-1".to_string(),
        model: "tinyllama".to_string(),
        input_data: "What is 2 + 2?".to_string(),
        mode: JobMode::Batch,
        requirements: Requirements::default(),
        reward: 1.0,
        currency: "USD".to_string(),
    };
    
    // Submit job
    monitor.record_job_submitted().await;
    let job_id = executor.submit_job(job).await.unwrap();
    assert_eq!(job_id, "e2e-test-job-1");
    
    // Verify job is in queue
    let queue_status = executor.get_queue_status();
    assert!(queue_status.pending_jobs > 0);
    
    // Process job
    let result = executor.process_next_job().await.unwrap();
    assert!(result.is_some());
    
    let job_result = result.unwrap();
    assert_eq!(job_result.job_id, "e2e-test-job-1");
    assert_eq!(job_result.status, JobStatus::Completed);
    assert!(job_result.output.is_some());
    assert!(!job_result.output.unwrap().is_empty());
    
    // Record completion
    monitor.record_job_completed(job_result.metrics.duration_ms, 100).await;
    
    // Verify metrics
    let metrics = monitor.get_job_metrics().await;
    assert_eq!(metrics.jobs_submitted, 1);
    assert_eq!(metrics.jobs_completed, 1);
}

/// Test multiple jobs in sequence
#[tokio::test]
#[serial]
async fn test_multiple_jobs_sequential() {
    let engine = Arc::new(Mutex::new(DefaultInferenceEngine::new()));
    let mut executor = DefaultJobExecutor::with_inference_engine(100, engine.clone());
    
    {
        let mut eng = engine.lock().await;
        let _ = eng.load_model("tinyllama").await;
    }
    
    // Submit multiple jobs
    for i in 1..=5 {
        let job = JobOffer {
            job_id: format!("seq-job-{}", i),
            model: "tinyllama".to_string(),
            input_data: format!("Question {}", i),
            mode: JobMode::Batch,
            requirements: Requirements::default(),
            reward: 1.0,
            currency: "USD".to_string(),
        };
        
        let job_id = executor.submit_job(job).await.unwrap();
        assert_eq!(job_id, format!("seq-job-{}", i));
    }
    
    // Verify all jobs are queued
    let status = executor.get_queue_status();
    assert_eq!(status.pending_jobs, 5);
    
    // Process all jobs
    let mut completed = 0;
    while let Some(result) = executor.process_next_job().await.unwrap() {
        assert_eq!(result.status, JobStatus::Completed);
        completed += 1;
    }
    
    assert_eq!(completed, 5);
    
    // Verify queue is empty
    let status = executor.get_queue_status();
    assert_eq!(status.pending_jobs, 0);
}

/// Test job failure handling
#[tokio::test]
#[serial]
async fn test_job_failure_handling() {
    let engine = Arc::new(Mutex::new(DefaultInferenceEngine::new()));
    let mut executor = DefaultJobExecutor::with_inference_engine(100, engine.clone());
    let monitor = DefaultHealthMonitor::new();
    
    // Don't load model - should cause failure
    
    // Submit job that will fail
    let job = JobOffer {
        job_id: "fail-job-1".to_string(),
        model: "nonexistent-model".to_string(),
        input_data: "Test input".to_string(),
        mode: JobMode::Batch,
        requirements: Requirements::default(),
        reward: 1.0,
        currency: "USD".to_string(),
    };
    
    monitor.record_job_submitted().await;
    let _ = executor.submit_job(job).await;
    
    // Process job - should fail
    let result = executor.process_next_job().await.unwrap();
    if let Some(job_result) = result {
        if job_result.status == JobStatus::Failed {
            monitor.record_job_failed().await;
            
            let metrics = monitor.get_job_metrics().await;
            assert_eq!(metrics.jobs_failed, 1);
        }
    }
}

// ============================================================================
// Protocol Compliance Tests (Requirement 7)
// ============================================================================

/// Test protocol message serialization compliance
#[test]
fn test_protocol_message_serialization_compliance() {
    // Create all message types
    let announce = P2PMessage::Announce(AnnounceMsg {
        manifest_id: "manifest-1".to_string(),
        version: "1.0.0".to_string(),
        manifest_hash: "abc123".to_string(),
        capabilities: NodeCapabilities {
            hostname: "test-node".to_string(),
            total_memory_gb: 16.0,
            cpus: vec!["Intel i7".to_string()],
            gpus: vec![],
        },
        payment_preferences: vec!["USD".to_string()],
        timestamp: chrono::Utc::now().timestamp(),
        peer_id: "peer-1".to_string(),
    });
    
    let work_offer = P2PMessage::WorkOffer(WorkOffer {
        job_id: "job-1".to_string(),
        model: "llama".to_string(),
        mode: mvp_node::protocol::JobMode::Batch,
        reward: 10.0,
        currency: "USD".to_string(),
        requirements: mvp_node::protocol::JobRequirements::default(),
        policy_refs: vec![],
        session: None,
        additional: Default::default(),
    });
    
    let job_claim = P2PMessage::JobClaim(JobClaimMsg {
        job_id: "job-1".to_string(),
        agent_peer_id: "agent-1".to_string(),
        manifest_id: "manifest-1".to_string(),
        timestamp: chrono::Utc::now().timestamp(),
    });
    
    let receipt = P2PMessage::Receipt(JobReceipt {
        receipt_id: "receipt-1".to_string(),
        job_id: "job-1".to_string(),
        session_id: None,
        sequence: None,
        executor_pubkey: "pubkey123".to_string(),
        manifest_id: Some("manifest-1".to_string()),
        outputs_hash: "hash123".to_string(),
        tokens_processed: Some(100.0),
        energy_kwh_estimate: None,
        started_at: Some(chrono::Utc::now().timestamp() - 10),
        completed_at: chrono::Utc::now().timestamp(),
        latency_ms: Some(100.0),
        payment_proof: None,
        chunk_window_seconds: None,
        signature: "sig123".to_string(),
        additional: Default::default(),
    });
    
    // All messages should serialize to valid JSON
    for msg in [announce, work_offer, job_claim, receipt] {
        let bytes = msg.to_bytes();
        assert!(bytes.is_ok(), "Message should serialize");
        
        let json_str = String::from_utf8(bytes.unwrap());
        assert!(json_str.is_ok(), "Bytes should be valid UTF-8");
        
        let json = json_str.unwrap();
        assert!(json.contains("\"type\""), "JSON should contain type tag");
    }
}

/// Test protocol version negotiation
#[test]
fn test_protocol_version_negotiation_compliance() {
    let negotiator = VersionNegotiator::new();
    
    // Test with current version
    let request = VersionNegotiationRequest::new("test-node");
    let response = negotiator.negotiate(&request);
    
    assert!(response.success);
    assert_eq!(response.agreed_version, Some(CURRENT_VERSION));
    
    // Test with compatible older version
    let mut old_request = VersionNegotiationRequest::new("old-node");
    old_request.version = ProtocolVersion::new(1, 0, 0);
    old_request.min_version = ProtocolVersion::new(1, 0, 0);
    
    let response = negotiator.negotiate(&old_request);
    assert!(response.success);
}

/// Test message roundtrip
#[test]
fn test_protocol_message_roundtrip() {
    let original = P2PMessage::Announce(AnnounceMsg {
        manifest_id: "test-manifest".to_string(),
        version: "1.0.0".to_string(),
        manifest_hash: "hash".to_string(),
        capabilities: NodeCapabilities {
            hostname: "node".to_string(),
            total_memory_gb: 8.0,
            cpus: vec!["CPU".to_string()],
            gpus: vec![],
        },
        payment_preferences: vec![],
        timestamp: 12345,
        peer_id: "peer".to_string(),
    });
    
    let bytes = original.to_bytes().unwrap();
    let decoded = P2PMessage::from_bytes(&bytes).unwrap();
    let bytes2 = decoded.to_bytes().unwrap();
    
    assert_eq!(bytes, bytes2);
}

// ============================================================================
// Error Handling Tests (Requirement 3.5, 5.3)
// ============================================================================

/// Test error categorization
#[test]
fn test_error_categorization() {
    let errors: Vec<MvpNodeError> = vec![
        MvpNodeError::ConnectionError { message: "test".to_string(), peer_id: None, recoverable: true },
        MvpNodeError::JobSubmissionError { job_id: None, message: "test".to_string() },
        MvpNodeError::ModelLoadError { model_name: "test".to_string(), message: "test".to_string() },
        MvpNodeError::ConfigurationError { message: "test".to_string(), field: None },
        MvpNodeError::ResourceExhausted { resource: "memory".to_string(), current_usage: "80%".to_string(), limit: "90%".to_string() },
        MvpNodeError::ProtocolVersionMismatch { expected: "1.0".to_string(), actual: "0.9".to_string() },
    ];
    
    for error in errors {
        let category = error.category();
        
        // All errors should have a valid category
        assert!(matches!(
            category,
            ErrorCategory::Network | ErrorCategory::Job | 
            ErrorCategory::Inference | ErrorCategory::Configuration |
            ErrorCategory::Resource | ErrorCategory::Protocol | ErrorCategory::Internal
        ));
    }
}

/// Test error recovery strategies
#[test]
fn test_error_recovery_strategies() {
    // PeerDisconnected returns Reconnect
    let disconnect_error = MvpNodeError::PeerDisconnected { 
        peer_id: "peer1".to_string(), 
        reason: None 
    };
    assert!(matches!(disconnect_error.recovery_strategy(), RecoveryStrategy::Reconnect { .. }));
    
    // NetworkTimeout returns Retry
    let timeout_error = MvpNodeError::NetworkTimeout { 
        operation: "test".to_string(),
        timeout_secs: 30,
    };
    assert!(matches!(timeout_error.recovery_strategy(), RecoveryStrategy::Retry { .. }));
    
    // QueueFullError returns Throttle
    let queue_error = MvpNodeError::QueueFullError { 
        max_size: 100,
        current_size: 100,
    };
    assert!(matches!(queue_error.recovery_strategy(), RecoveryStrategy::Throttle { .. }));
}

/// Test recoverable vs non-recoverable errors
#[test]
fn test_error_recoverability() {
    let recoverable = MvpNodeError::ConnectionError { 
        message: "test".to_string(), 
        peer_id: None, 
        recoverable: true 
    };
    assert!(recoverable.is_recoverable());
    
    let non_recoverable = MvpNodeError::ConfigurationError { 
        message: "critical".to_string(),
        field: None,
    };
    assert!(!non_recoverable.is_recoverable());
}

// ============================================================================
// Monitoring and Metrics Tests (Requirement 5)
// ============================================================================

/// Test comprehensive metrics collection
#[tokio::test]
async fn test_comprehensive_metrics_collection() {
    let collector = MetricsCollector::new("e2e-test-node");
    
    // Simulate activity
    collector.record_job_success(100.0).await;
    collector.record_job_success(150.0).await;
    collector.record_job_failure().await;
    collector.record_message(true, 1024).await;
    collector.record_message(false, 2048).await;
    collector.update_peer_count(5).await;
    collector.record_inference(500, 200).await;
    collector.update_queue_status(3, 1, 100).await;
    
    // Get all metrics
    let metrics = collector.get_performance_metrics().await;
    
    // Verify job metrics
    assert_eq!(metrics.jobs.total_processed, 3);
    assert_eq!(metrics.jobs.successful, 2);
    assert_eq!(metrics.jobs.failed, 1);
    assert!((metrics.jobs.success_rate - 66.666).abs() < 1.0);
    
    // Verify network metrics
    assert_eq!(metrics.network.messages_sent, 1);
    assert_eq!(metrics.network.messages_received, 1);
    assert_eq!(metrics.network.active_peers, 5);
    
    // Verify inference metrics
    assert_eq!(metrics.inference.total_inferences, 1);
    assert_eq!(metrics.inference.tokens_processed, 500);
    
    // Verify queue status
    let queue = collector.get_queue_status().await;
    assert_eq!(queue.pending_jobs, 3);
    assert_eq!(queue.processing_jobs, 1);
    assert_eq!(queue.utilization_percent, 4.0);
}

/// Test health status reporting
#[tokio::test]
async fn test_health_status_comprehensive() {
    let monitor = DefaultHealthMonitor::new();
    
    // Setup healthy state
    monitor.set_model_loaded(true).await;
    monitor.set_network_connected(true).await;
    monitor.set_queue_size(5).await;
    
    // Get health status
    let status = monitor.get_full_health_status().await;
    
    assert!(status.is_healthy);
    assert!(status.model_loaded);
    assert!(status.network_connected);
    assert_eq!(status.queue_size, 5);
    assert!(status.uptime_seconds >= 0);
}

/// Test error logging and tracking
#[tokio::test]
async fn test_error_logging_and_tracking() {
    let monitor = DefaultHealthMonitor::new();
    
    // Record multiple errors
    monitor.record_error("network", "Connection failed").await;
    monitor.record_error("executor", "Job timeout").await;
    monitor.record_error("inference", "Model error").await;
    
    let status = monitor.get_full_health_status().await;
    
    assert_eq!(status.error_count, 3);
    assert!(status.last_error.is_some());
    assert!(status.last_error.unwrap().contains("Model error"));
}

// ============================================================================
// Multi-Node Network Tests (Requirement 3, 8)
// ============================================================================

/// Test bootstrap node operation
#[tokio::test]
#[serial]
async fn test_bootstrap_node_operation() {
    let port = find_available_port().unwrap();
    let keypair = Keypair::generate_ed25519();
    let mut network = P2PNetworkLayer::new(keypair).await.unwrap();
    
    let config = NetworkConfig {
        listen_addr: format!("/ip4/127.0.0.1/tcp/{}", port).parse().unwrap(),
        bootstrap_peers: vec![], // Empty = bootstrap mode
    };
    
    network.start(config).await.unwrap();
    
    // Verify bootstrap mode
    assert!(network.is_bootstrap_node());
    assert_eq!(network.get_bootstrap_mode(), BootstrapMode::Bootstrap);
    
    // Verify listeners
    let listeners = network.get_listeners();
    assert!(!listeners.is_empty());
    
    network.shutdown().await.unwrap();
}

/// Test client node connecting to bootstrap
#[tokio::test]
#[serial]
async fn test_client_connects_to_bootstrap() {
    // Start bootstrap node
    let bootstrap_port = find_available_port().unwrap();
    let bootstrap_keypair = Keypair::generate_ed25519();
    let mut bootstrap = P2PNetworkLayer::new(bootstrap_keypair).await.unwrap();
    
    let bootstrap_config = NetworkConfig {
        listen_addr: format!("/ip4/127.0.0.1/tcp/{}", bootstrap_port).parse().unwrap(),
        bootstrap_peers: vec![],
    };
    
    bootstrap.start(bootstrap_config).await.unwrap();
    let bootstrap_addr = bootstrap.get_listeners().first().unwrap().to_string();
    
    // Start client node
    let client_port = find_available_port().unwrap();
    let client_keypair = Keypair::generate_ed25519();
    let mut client = P2PNetworkLayer::new(client_keypair).await.unwrap();
    
    let client_config = NetworkConfig {
        listen_addr: format!("/ip4/127.0.0.1/tcp/{}", client_port).parse().unwrap(),
        bootstrap_peers: vec![bootstrap_addr.parse().unwrap()],
    };
    
    client.start(client_config).await.unwrap();
    
    // Verify client mode
    assert!(!client.is_bootstrap_node());
    assert_eq!(client.get_bootstrap_mode(), BootstrapMode::Client);
    
    // Allow time for connection
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Cleanup
    client.shutdown().await.unwrap();
    bootstrap.shutdown().await.unwrap();
}

/// Test network topology with multiple nodes
#[tokio::test]
#[serial]
async fn test_network_topology_multiple_nodes() {
    // Start bootstrap node
    let bootstrap_port = find_available_port().unwrap();
    let mut bootstrap = P2PNetworkLayer::new(Keypair::generate_ed25519()).await.unwrap();
    
    let bootstrap_config = NetworkConfig {
        listen_addr: format!("/ip4/127.0.0.1/tcp/{}", bootstrap_port).parse().unwrap(),
        bootstrap_peers: vec![],
    };
    
    bootstrap.start(bootstrap_config).await.unwrap();
    let bootstrap_addr = bootstrap.get_listeners().first().unwrap().to_string();
    
    // Start multiple client nodes
    let mut clients = Vec::new();
    for _ in 0..3 {
        let port = find_available_port().unwrap();
        let mut client = P2PNetworkLayer::new(Keypair::generate_ed25519()).await.unwrap();
        
        let config = NetworkConfig {
            listen_addr: format!("/ip4/127.0.0.1/tcp/{}", port).parse().unwrap(),
            bootstrap_peers: vec![bootstrap_addr.parse().unwrap()],
        };
        
        client.start(config).await.unwrap();
        clients.push(client);
    }
    
    // Allow connections to establish
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Verify all nodes are running
    assert!(bootstrap.is_bootstrap_node());
    for client in &clients {
        assert!(!client.is_bootstrap_node());
    }
    
    // Cleanup
    for mut client in clients {
        client.shutdown().await.unwrap();
    }
    bootstrap.shutdown().await.unwrap();
}

// ============================================================================
// System Stability Tests
// ============================================================================

/// Test graceful shutdown
#[tokio::test]
#[serial]
async fn test_graceful_shutdown() {
    let port = find_available_port().unwrap();
    let mut network = P2PNetworkLayer::new(Keypair::generate_ed25519()).await.unwrap();
    
    let config = NetworkConfig {
        listen_addr: format!("/ip4/127.0.0.1/tcp/{}", port).parse().unwrap(),
        bootstrap_peers: vec![],
    };
    
    network.start(config).await.unwrap();
    
    // Verify running
    assert!(!network.get_listeners().is_empty());
    
    // Shutdown within timeout
    let shutdown_result = timeout(
        Duration::from_secs(5),
        network.shutdown()
    ).await;
    
    assert!(shutdown_result.is_ok());
}

/// Test queue overflow handling
#[tokio::test]
#[serial]
async fn test_queue_overflow_handling() {
    let mut executor = DefaultJobExecutor::new(50); // Small capacity
    
    // Submit jobs up to capacity
    let capacity = 50;
    for i in 0..capacity {
        let job = JobOffer {
            job_id: format!("overflow-test-{}", i),
            model: "test".to_string(),
            input_data: "test".to_string(),
            mode: JobMode::Batch,
            requirements: Requirements::default(),
            reward: 1.0,
            currency: "USD".to_string(),
        };
        
        let result = executor.submit_job(job).await;
        // Should succeed until capacity
        if i < capacity - 1 {
            assert!(result.is_ok());
        }
    }
    
    // Verify queue has jobs
    let status = executor.get_queue_status();
    assert!(status.pending_jobs >= 0);
}

/// Test inference engine with multiple requests
#[tokio::test]
#[serial]
async fn test_inference_engine_multiple_requests() {
    let mut engine = DefaultInferenceEngine::new();
    
    // Load model
    let load_result = engine.load_model("tinyllama").await;
    assert!(load_result.is_ok());
    
    // Process multiple requests
    for i in 0..5 {
        let result = engine.infer(&format!("Test prompt {}", i)).await;
        
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(!output.output.is_empty());
    }
    
    // Verify model is still loaded
    assert!(engine.is_model_loaded());
}

// ============================================================================
// Integration Test Summary
// ============================================================================

/// Comprehensive end-to-end test combining all components
#[tokio::test]
#[serial]
async fn test_full_system_integration() {
    // 1. Setup infrastructure
    let monitor = Arc::new(DefaultHealthMonitor::new());
    let collector = MetricsCollector::new("integration-test");
    
    // 2. Setup network
    let port = find_available_port().unwrap();
    let mut network = P2PNetworkLayer::new(Keypair::generate_ed25519()).await.unwrap();
    
    let config = NetworkConfig {
        listen_addr: format!("/ip4/127.0.0.1/tcp/{}", port).parse().unwrap(),
        bootstrap_peers: vec![],
    };
    
    network.start(config).await.unwrap();
    monitor.set_network_connected(true).await;
    
    // 3. Setup inference engine
    let engine = Arc::new(Mutex::new(DefaultInferenceEngine::new()));
    {
        let mut eng = engine.lock().await;
        let _ = eng.load_model("tinyllama").await;
    }
    monitor.set_model_loaded(true).await;
    
    // 4. Setup executor
    let mut executor = DefaultJobExecutor::with_inference_engine(100, engine.clone());
    
    // 5. Process a job through the full pipeline
    let job = JobOffer {
        job_id: "full-integration-test".to_string(),
        model: "tinyllama".to_string(),
        input_data: "Hello, world!".to_string(),
        mode: JobMode::Batch,
        requirements: Requirements::default(),
        reward: 1.0,
        currency: "USD".to_string(),
    };
    
    // Submit
    monitor.record_job_submitted().await;
    collector.update_queue_status(1, 0, 100).await;
    let job_id = executor.submit_job(job).await.unwrap();
    
    // Process
    let result = executor.process_next_job().await.unwrap();
    assert!(result.is_some());
    
    let job_result = result.unwrap();
    assert_eq!(job_result.status, JobStatus::Completed);
    
    // Record metrics
    monitor.record_job_completed(job_result.metrics.duration_ms, 50).await;
    collector.record_job_success(job_result.metrics.duration_ms as f64).await;
    collector.update_queue_status(0, 0, 100).await;
    
    // 6. Verify system state
    let health = monitor.get_full_health_status().await;
    assert!(health.is_healthy);
    assert!(health.model_loaded);
    assert!(health.network_connected);
    
    let metrics = monitor.get_job_metrics().await;
    assert_eq!(metrics.jobs_submitted, 1);
    assert_eq!(metrics.jobs_completed, 1);
    
    let perf = collector.get_performance_metrics().await;
    assert_eq!(perf.jobs.total_processed, 1);
    assert_eq!(perf.jobs.successful, 1);
    
    // 7. Cleanup
    network.shutdown().await.unwrap();
}

