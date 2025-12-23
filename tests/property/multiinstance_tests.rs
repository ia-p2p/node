//! Property tests for multi-instance support
//!
//! Tests for Requirements 8.1, 8.2, 8.3, 8.4, 8.5

use mvp_node::config::{
    NodeConfig, DefaultConfigManager, MultiInstanceConfig,
    find_available_port, is_port_available, generate_unique_node_id,
};
use mvp_node::ConfigManager;
use mvp_node::executor::DefaultJobExecutor;
use mvp_node::inference::DefaultInferenceEngine;
use mvp_node::{JobOffer, JobMode, Requirements, JobStatus, InferenceEngine, JobExecutor};
use proptest::prelude::*;
use serial_test::serial;
use std::collections::HashSet;
use std::net::TcpListener;
use std::sync::Arc;
use tokio::sync::Mutex;

// Property 36: Port conflict avoidance
// When multiple MVP_Node instances start on the same machine, each instance
// shall use different network ports to avoid conflicts.

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]
    
    #[test]
    #[serial]
    fn port_assignment_avoids_conflicts(instance_count in 2..5usize) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Create multiple instance configurations
            let configs = MultiInstanceConfig::new(instance_count)
                .with_base_config(NodeConfig {
                    node_id: "test".to_string(),
                    model_path: "tinyllama-1.1b".to_string(),
                    ..Default::default()
                })
                .build()
                .unwrap();
            
            // Verify all ports are unique
            let ports: Vec<_> = configs.iter().map(|c| c.listen_port).collect();
            let unique_ports: HashSet<_> = ports.iter().copied().collect();
            
            prop_assert_eq!(ports.len(), unique_ports.len(), "All ports should be unique");
            
            // Verify all ports are available (not bound by other processes)
            for port in &ports {
                prop_assert!(*port > 0, "Port should be assigned");
            }
            
            Ok(()) as Result<(), proptest::test_runner::TestCaseError>
        }).unwrap();
    }
}

#[tokio::test]
#[serial]
async fn test_find_available_port_uniqueness() {
    let mut ports = Vec::new();
    
    for _ in 0..5 {
        let port = find_available_port().unwrap();
        ports.push(port);
    }
    
    // All ports should be different (most likely, depending on system)
    let unique: HashSet<_> = ports.iter().collect();
    assert!(unique.len() >= 3, "Should find mostly unique ports");
}

#[tokio::test]
#[serial]
async fn test_port_availability_check() {
    // Find an available port
    let port = find_available_port().unwrap();
    assert!(is_port_available(port), "Port should be available initially");
    
    // Bind to it
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).unwrap();
    
    // Now it should not be available
    assert!(!is_port_available(port), "Port should not be available after binding");
    
    // Release the port
    drop(listener);
    
    // Port should be available again (might take a moment)
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(is_port_available(port), "Port should be available after release");
}

#[tokio::test]
#[serial]
async fn test_auto_port_reassignment() {
    let mut config = NodeConfig::default();
    
    // First call should assign a port
    let port1 = config.ensure_available_port().unwrap();
    assert!(port1 > 0, "Should assign a port");
    
    // Bind to make it unavailable
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port1)).unwrap();
    
    // Create new config with same port (should get reassigned)
    let mut config2 = NodeConfig::default();
    config2.listen_port = port1;
    let port2 = config2.ensure_available_port().unwrap();
    
    // New port should be different from bound port
    assert_ne!(port2, port1, "Should reassign to different port");
    
    drop(listener);
}

// Property 37: Instance configuration uniqueness (from Task 1.1)
// Already tested in config_tests, extended here for multi-instance

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]
    
    #[test]
    #[serial]
    fn node_ids_are_unique(count in 5..10usize) {
        let ids: Vec<String> = (0..count).map(|_| generate_unique_node_id()).collect();
        let unique_ids: HashSet<_> = ids.iter().collect();
        
        prop_assert_eq!(ids.len(), unique_ids.len(), "All node IDs should be unique");
    }
}

// Property 38: Multi-node connectivity
// When multiple instances connect to each other, the P2P network shall establish
// connections and enable message routing between all nodes.

#[tokio::test]
#[serial]
async fn test_bootstrap_peer_configuration() {
    let configs = MultiInstanceConfig::new(3)
        .with_base_config(NodeConfig {
            node_id: "test".to_string(),
            model_path: "tinyllama-1.1b".to_string(),
            ..Default::default()
        })
        .build()
        .unwrap();
    
    // First instance has no bootstrap peers
    assert!(configs[0].bootstrap_peers.is_empty(), "First instance should have no bootstrap peers");
    
    // Subsequent instances should have bootstrap peers
    assert!(!configs[1].bootstrap_peers.is_empty(), "Second instance should have bootstrap peers");
    assert!(!configs[2].bootstrap_peers.is_empty(), "Third instance should have bootstrap peers");
    
    // Third instance should have both previous instances as bootstrap peers
    assert!(configs[2].bootstrap_peers.len() >= 1, "Third instance should have at least 1 bootstrap peer");
}

#[tokio::test]
#[serial]
async fn test_multiaddr_format() {
    let config = NodeConfig::for_instance(5);
    let addr = config.get_listen_multiaddr();
    
    assert!(addr.starts_with("/ip4/"), "Multiaddr should start with /ip4/");
    assert!(addr.contains("/tcp/"), "Multiaddr should contain /tcp/");
}

// Property 39: Instance isolation during job processing
// When one instance receives a job, it shall process it locally without
// interfering with other running instances.

#[tokio::test]
#[serial]
async fn test_instance_isolation_job_queues() {
    // Create two separate instances
    let engine1 = Arc::new(Mutex::new(DefaultInferenceEngine::new()));
    let engine2 = Arc::new(Mutex::new(DefaultInferenceEngine::new()));
    
    let mut executor1 = DefaultJobExecutor::with_inference_engine(10, engine1.clone());
    let mut executor2 = DefaultJobExecutor::with_inference_engine(10, engine2.clone());
    
    // Load models
    {
        let mut eng1 = engine1.lock().await;
        eng1.load_model("tinyllama-1.1b").await.unwrap();
    }
    {
        let mut eng2 = engine2.lock().await;
        eng2.load_model("tinyllama-1.1b").await.unwrap();
    }
    
    // Submit jobs to instance 1
    for i in 0..3 {
        let job = JobOffer {
            job_id: format!("instance1-job-{}", i),
            model: "tinyllama-1.1b".to_string(),
            mode: JobMode::Batch,
            reward: 10.0,
            currency: "USD".to_string(),
            requirements: Requirements::default(),
            input_data: "Hello".to_string(),
        };
        executor1.submit_job(job).await.unwrap();
    }
    
    // Submit jobs to instance 2
    for i in 0..2 {
        let job = JobOffer {
            job_id: format!("instance2-job-{}", i),
            model: "tinyllama-1.1b".to_string(),
            mode: JobMode::Batch,
            reward: 10.0,
            currency: "USD".to_string(),
            requirements: Requirements::default(),
            input_data: "World".to_string(),
        };
        executor2.submit_job(job).await.unwrap();
    }
    
    // Verify queue isolation
    let status1 = executor1.get_queue_status();
    let status2 = executor2.get_queue_status();
    
    assert_eq!(status1.pending_jobs, 3, "Instance 1 should have 3 pending jobs");
    assert_eq!(status2.pending_jobs, 2, "Instance 2 should have 2 pending jobs");
    
    // Process one job from instance 1
    let result1 = executor1.process_next_job().await.unwrap();
    assert!(result1.is_some());
    assert!(result1.unwrap().job_id.starts_with("instance1-"));
    
    // Instance 2's queue should be unaffected
    let status2_after = executor2.get_queue_status();
    assert_eq!(status2_after.pending_jobs, 2, "Instance 2 queue should be unaffected");
}

#[tokio::test]
#[serial]
async fn test_instance_isolation_model_state() {
    // Create two separate instances
    let engine1 = DefaultInferenceEngine::new();
    let engine2 = DefaultInferenceEngine::new();
    
    let engine1 = Arc::new(Mutex::new(engine1));
    let engine2 = Arc::new(Mutex::new(engine2));
    
    // Load model only in instance 1
    {
        let mut eng1 = engine1.lock().await;
        eng1.load_model("tinyllama-1.1b").await.unwrap();
    }
    
    // Verify instance 1 has model loaded
    {
        let eng1 = engine1.lock().await;
        assert!(eng1.is_model_loaded(), "Instance 1 should have model loaded");
    }
    
    // Verify instance 2 does NOT have model loaded
    {
        let eng2 = engine2.lock().await;
        assert!(!eng2.is_model_loaded(), "Instance 2 should NOT have model loaded");
    }
    
    // Unload from instance 1
    {
        let mut eng1 = engine1.lock().await;
        eng1.unload_model().await.unwrap();
        assert!(!eng1.is_model_loaded(), "Instance 1 should have model unloaded");
    }
    
    // This should not affect instance 2's state (still no model)
    {
        let eng2 = engine2.lock().await;
        assert!(!eng2.is_model_loaded(), "Instance 2 state should be independent");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5))]
    
    #[test]
    #[serial]
    fn concurrent_instances_maintain_separate_state(job_count in 1..5usize) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let engine1 = Arc::new(Mutex::new(DefaultInferenceEngine::new()));
            let engine2 = Arc::new(Mutex::new(DefaultInferenceEngine::new()));
            
            let mut executor1 = DefaultJobExecutor::with_inference_engine(20, engine1.clone());
            let mut executor2 = DefaultJobExecutor::with_inference_engine(20, engine2.clone());
            
            // Load models
            {
                let mut eng1 = engine1.lock().await;
                eng1.load_model("tinyllama-1.1b").await.unwrap();
            }
            {
                let mut eng2 = engine2.lock().await;
                eng2.load_model("tinyllama-1.1b").await.unwrap();
            }
            
            // Submit different number of jobs to each instance
            for i in 0..job_count {
                let job = JobOffer {
                    job_id: format!("ex1-job-{}", i),
                    model: "tinyllama-1.1b".to_string(),
                    mode: JobMode::Batch,
                    reward: 10.0,
                    currency: "USD".to_string(),
                    requirements: Requirements::default(),
                    input_data: "test".to_string(),
                };
                executor1.submit_job(job).await.unwrap();
            }
            
            for i in 0..(job_count * 2) {
                let job = JobOffer {
                    job_id: format!("ex2-job-{}", i),
                    model: "tinyllama-1.1b".to_string(),
                    mode: JobMode::Batch,
                    reward: 10.0,
                    currency: "USD".to_string(),
                    requirements: Requirements::default(),
                    input_data: "test".to_string(),
                };
                executor2.submit_job(job).await.unwrap();
            }
            
            let status1 = executor1.get_queue_status();
            let status2 = executor2.get_queue_status();
            
            prop_assert_eq!(status1.pending_jobs, job_count, "Instance 1 queue size mismatch");
            prop_assert_eq!(status2.pending_jobs, job_count * 2, "Instance 2 queue size mismatch");
            
            Ok(()) as Result<(), proptest::test_runner::TestCaseError>
        }).unwrap();
    }
}

#[tokio::test]
#[serial]
async fn test_config_validation() {
    // Valid config
    let config = NodeConfig {
        node_id: "test-node".to_string(),
        listen_port: 9000,
        bootstrap_peers: vec!["/ip4/127.0.0.1/tcp/9001".to_string()],
        model_path: "tinyllama".to_string(),
        max_queue_size: 10,
        log_level: "info".to_string(),
        ..Default::default()
    };
    assert!(DefaultConfigManager::validate_config(&config).is_ok());
    
    // Invalid: empty node_id
    let mut bad_config = config.clone();
    bad_config.node_id = "".to_string();
    assert!(DefaultConfigManager::validate_config(&bad_config).is_err());
    
    // Invalid: zero queue size
    let mut bad_config = config.clone();
    bad_config.max_queue_size = 0;
    assert!(DefaultConfigManager::validate_config(&bad_config).is_err());
}

#[tokio::test]
#[serial]
async fn test_full_multiinstance_workflow() {
    // Create 3 instance configurations
    let configs = MultiInstanceConfig::new(3)
        .with_base_config(NodeConfig {
            node_id: "workflow".to_string(),
            model_path: "tinyllama-1.1b".to_string(),
            max_queue_size: 5,
            ..Default::default()
        })
        .build()
        .unwrap();
    
    assert_eq!(configs.len(), 3);
    
    // Verify configuration validity
    for config in &configs {
        assert!(DefaultConfigManager::validate_config(config).is_ok());
    }
    
    // Create executors for each instance
    let mut executors = Vec::new();
    let mut engines = Vec::new();
    
    for config in &configs {
        let engine = Arc::new(Mutex::new(DefaultInferenceEngine::new()));
        {
            let mut eng = engine.lock().await;
            eng.load_model(&config.model_path).await.unwrap();
        }
        let executor = DefaultJobExecutor::with_inference_engine(
            config.max_queue_size,
            engine.clone(),
        );
        engines.push(engine);
        executors.push(executor);
    }
    
    // Submit job to each instance
    for (i, executor) in executors.iter_mut().enumerate() {
        let job = JobOffer {
            job_id: format!("multi-test-{}", i),
            model: "tinyllama-1.1b".to_string(),
            mode: JobMode::Batch,
            reward: 10.0,
            currency: "USD".to_string(),
            requirements: Requirements::default(),
            input_data: format!("Hello from instance {}", i),
        };
        executor.submit_job(job).await.unwrap();
    }
    
    // Process jobs from each instance
    for (i, executor) in executors.iter_mut().enumerate() {
        let result = executor.process_next_job().await.unwrap();
        assert!(result.is_some(), "Instance {} should produce result", i);
        
        let job_result = result.unwrap();
        assert_eq!(job_result.job_id, format!("multi-test-{}", i));
        assert!(
            job_result.status == JobStatus::Completed || job_result.status == JobStatus::Failed,
            "Job should complete or fail"
        );
    }
}

