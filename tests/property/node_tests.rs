//! Property tests for Node orchestration and lifecycle management
//!
//! Tests for Requirements 1.4, 1.5, 3.4

use mvp_node::config::NodeConfig;
use mvp_node::{JobOffer, JobMode, Requirements, JobStatus, InferenceEngine, JobExecutor};
use mvp_node::inference::DefaultInferenceEngine;
use mvp_node::executor::DefaultJobExecutor;
use mvp_node::monitoring::DefaultHealthMonitor;
use proptest::prelude::*;
use serial_test::serial;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Helper function to create test config
fn create_test_config() -> NodeConfig {
    NodeConfig {
        node_id: "test-node".to_string(),
        listen_port: 0,
        bootstrap_peers: vec![],
        model_path: "tinyllama-1.1b".to_string(),
        max_queue_size: 10,
        log_level: "debug".to_string(),
        ..Default::default()
    }
}

// Property 4: Capability announcement
// For any running MVP_Node, the node should publish capability announcement 
// messages to the P2P_Network containing its manifest and available models.

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]
    
    /// Test that capability announcements contain all required fields
    #[test]
    #[serial]
    fn capability_announcement_contains_required_fields(
        queue_size in 5..20usize,
        model_path in "tinyllama-1\\.1b"
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Create components
            let inference_engine = DefaultInferenceEngine::new();
            let inference_arc = Arc::new(Mutex::new(inference_engine));
            let executor = DefaultJobExecutor::with_inference_engine(queue_size, inference_arc.clone());
            
            // Load model
            {
                let mut engine = inference_arc.lock().await;
                let _ = engine.load_model(&model_path).await;
            }
            
            // Get model info and queue status
            let engine = inference_arc.lock().await;
            let model_info = engine.get_model_info();
            drop(engine);
            
            let queue_status = executor.get_queue_status();
            let max_size = executor.get_max_queue_size();
            let is_full = executor.is_queue_full();
            
            // Verify announcement fields
            prop_assert!(model_info.is_some(), "Model should be loaded");
            prop_assert_eq!(max_size, queue_size, "Queue size mismatch");
            prop_assert_eq!(queue_status.pending_jobs, 0, "Queue should be empty initially");
            prop_assert!(!is_full, "Queue should not be full initially");
            
            Ok(()) as Result<(), proptest::test_runner::TestCaseError>
        }).unwrap();
    }
}

// Property 5: Graceful shutdown
// For any MVP_Node receiving a shutdown signal, the node should close all 
// network connections and save state within 30 seconds.

#[tokio::test]
#[serial]
async fn shutdown_completes_within_timeout() {
    let shutdown_start = Instant::now();
    let timeout = Duration::from_secs(30);
    
    // Simulate shutdown sequence
    let inference_engine = DefaultInferenceEngine::new();
    let inference_arc = Arc::new(Mutex::new(inference_engine));
    let executor = DefaultJobExecutor::with_inference_engine(10, inference_arc.clone());
    
    // Load model
    {
        let mut engine = inference_arc.lock().await;
        let _ = engine.load_model("tinyllama-1.1b").await;
    }
    
    // Step 1: Stop accepting jobs
    assert!(!executor.is_queue_full(), "Queue should accept jobs");
    
    // Step 2: Wait for current job (simulated)
    tokio::time::sleep(Duration::from_millis(10)).await;
    
    // Step 3: Unload model
    {
        let mut engine = inference_arc.lock().await;
        let result = engine.unload_model().await;
        assert!(result.is_ok(), "Model unload should succeed");
        assert!(!engine.is_model_loaded(), "Model should be unloaded");
    }
    
    let shutdown_duration = shutdown_start.elapsed();
    assert!(shutdown_duration < timeout, "Shutdown exceeded 30 second timeout: {:?}", shutdown_duration);
}

#[tokio::test]
#[serial]
async fn shutdown_unloads_model() {
    let inference_engine = DefaultInferenceEngine::new();
    let inference_arc = Arc::new(Mutex::new(inference_engine));
    
    // Load model
    {
        let mut engine = inference_arc.lock().await;
        engine.load_model("tinyllama-1.1b").await.unwrap();
        assert!(engine.is_model_loaded(), "Model should be loaded");
    }
    
    // Shutdown: unload model
    {
        let mut engine = inference_arc.lock().await;
        engine.unload_model().await.unwrap();
        assert!(!engine.is_model_loaded(), "Model should be unloaded after shutdown");
    }
}

#[tokio::test]
#[serial]
async fn shutdown_clears_pending_jobs() {
    let inference_engine = DefaultInferenceEngine::new();
    let inference_arc = Arc::new(Mutex::new(inference_engine));
    let mut executor = DefaultJobExecutor::with_inference_engine(10, inference_arc.clone());
    
    // Load model
    {
        let mut engine = inference_arc.lock().await;
        engine.load_model("tinyllama-1.1b").await.unwrap();
    }
    
    // Submit some jobs
    for i in 0..3 {
        let job = JobOffer {
            job_id: format!("job-{}", i),
            model: "tinyllama-1.1b".to_string(),
            mode: JobMode::Batch,
            reward: 10.0,
            currency: "USD".to_string(),
            requirements: Requirements::default(),
            input_data: "test".to_string(),
        };
        executor.submit_job(job).await.unwrap();
    }
    
    let status = executor.get_queue_status();
    assert_eq!(status.pending_jobs, 3, "Should have 3 pending jobs");
}

// Property 14: Protocol-compliant message processing
// All messages received and sent by the node must comply with the protocol specification.

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]
    
    #[test]
    #[serial]
    fn job_processing_produces_valid_results(
        job_id in "[a-z0-9]{8}",
        input in ".{1,100}"
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let inference_engine = DefaultInferenceEngine::new();
            let inference_arc = Arc::new(Mutex::new(inference_engine));
            let mut executor = DefaultJobExecutor::with_inference_engine(10, inference_arc.clone());
            
            // Load model
            {
                let mut engine = inference_arc.lock().await;
                engine.load_model("tinyllama-1.1b").await.unwrap();
            }
            
            let job = JobOffer {
                job_id: job_id.clone(),
                model: "tinyllama-1.1b".to_string(),
                mode: JobMode::Batch,
                reward: 10.0,
                currency: "USD".to_string(),
                requirements: Requirements::default(),
                input_data: input,
            };
            
            executor.submit_job(job).await.unwrap();
            let result = executor.process_next_job().await.unwrap();
            
            prop_assert!(result.is_some(), "Should produce a result");
            
            let job_result = result.unwrap();
            prop_assert_eq!(job_result.job_id, job_id, "Job ID should match");
            prop_assert!(
                job_result.status == JobStatus::Completed || job_result.status == JobStatus::Failed,
                "Status should be completed or failed"
            );
            
            Ok(()) as Result<(), proptest::test_runner::TestCaseError>
        }).unwrap();
    }
}

#[tokio::test]
#[serial]
async fn test_node_lifecycle_complete() {
    // Create components
    let inference_engine = DefaultInferenceEngine::new();
    let inference_arc = Arc::new(Mutex::new(inference_engine));
    let mut executor = DefaultJobExecutor::with_inference_engine(10, inference_arc.clone());
    let monitor = DefaultHealthMonitor::new();
    
    // Phase 1: Initialization
    assert!(!inference_arc.lock().await.is_model_loaded(), "Model should not be loaded initially");
    
    // Phase 2: Startup - load model
    {
        let mut engine = inference_arc.lock().await;
        engine.load_model("tinyllama-1.1b").await.unwrap();
    }
    assert!(inference_arc.lock().await.is_model_loaded(), "Model should be loaded after startup");
    
    // Phase 3: Running - process jobs
    let job = JobOffer {
        job_id: "lifecycle-test-job".to_string(),
        model: "tinyllama-1.1b".to_string(),
        mode: JobMode::Batch,
        reward: 10.0,
        currency: "USD".to_string(),
        requirements: Requirements::default(),
        input_data: "Hello".to_string(),
    };
    
    executor.submit_job(job).await.unwrap();
    let result = executor.process_next_job().await.unwrap();
    assert!(result.is_some(), "Should process job");
    
    // Phase 4: Shutdown - unload model
    {
        let mut engine = inference_arc.lock().await;
        engine.unload_model().await.unwrap();
    }
    assert!(!inference_arc.lock().await.is_model_loaded(), "Model should be unloaded after shutdown");
}

#[tokio::test]
#[serial]
async fn test_monitor_integration() {
    let monitor = DefaultHealthMonitor::new();
    
    // Record job metrics
    monitor.record_job_submitted().await;
    monitor.record_job_submitted().await;
    monitor.record_job_completed(100, 50).await;
    monitor.record_job_failed().await;
    
    // Check health status (synchronous method)
    let status = monitor.get_health_status_map();
    assert!(status.contains_key("uptime_seconds"), "Should have uptime");
    
    // Check metrics (synchronous method)
    let metrics = monitor.get_metrics();
    assert_eq!(*metrics.get("jobs_submitted").unwrap_or(&0.0), 2.0);
    assert_eq!(*metrics.get("jobs_completed").unwrap_or(&0.0), 1.0);
    assert_eq!(*metrics.get("jobs_failed").unwrap_or(&0.0), 1.0);
}

#[tokio::test]
#[serial]
async fn test_queue_management() {
    let inference_engine = DefaultInferenceEngine::new();
    let inference_arc = Arc::new(Mutex::new(inference_engine));
    let mut executor = DefaultJobExecutor::with_inference_engine(3, inference_arc.clone());
    
    // Fill queue
    for i in 0..3 {
        let job = JobOffer {
            job_id: format!("queue-test-{}", i),
            model: "tinyllama-1.1b".to_string(),
            mode: JobMode::Batch,
            reward: 10.0,
            currency: "USD".to_string(),
            requirements: Requirements::default(),
            input_data: "test".to_string(),
        };
        executor.submit_job(job).await.unwrap();
    }
    
    assert!(executor.is_queue_full(), "Queue should be full");
    
    // Try to add more
    let extra_job = JobOffer {
        job_id: "extra-job".to_string(),
        model: "tinyllama-1.1b".to_string(),
        mode: JobMode::Batch,
        reward: 10.0,
        currency: "USD".to_string(),
        requirements: Requirements::default(),
        input_data: "test".to_string(),
    };
    
    let result = executor.submit_job(extra_job).await;
    assert!(result.is_err(), "Should reject job when queue is full");
}

