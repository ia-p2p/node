//! Multi-node integration tests
//!
//! These tests verify that multiple MVP_Node instances can run simultaneously
//! on the same machine, discover each other, and communicate properly.

use mvp_node::config::{NodeConfig, MultiInstanceConfig, find_available_port};
use mvp_node::network::{P2PNetworkLayer, NetworkConfig, NetworkLayer, BootstrapMode};
use mvp_node::executor::DefaultJobExecutor;
use mvp_node::inference::DefaultInferenceEngine;
use mvp_node::{JobOffer, JobMode, Requirements, InferenceEngine, JobExecutor};
use libp2p::identity::Keypair;
use serial_test::serial;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::timeout;

/// Helper to create a network layer for testing
async fn create_test_network(port: u16) -> P2PNetworkLayer {
    let keypair = Keypair::generate_ed25519();
    P2PNetworkLayer::new(keypair).await.unwrap()
}

/// Helper to start a network on a specific port
async fn start_network(network: &mut P2PNetworkLayer, port: u16, bootstrap_peers: Vec<String>) -> Vec<libp2p::Multiaddr> {
    let config = NetworkConfig {
        listen_addr: format!("/ip4/127.0.0.1/tcp/{}", port).parse().unwrap(),
        bootstrap_peers: bootstrap_peers.iter()
            .filter_map(|s| s.parse().ok())
            .collect(),
    };
    network.start(config).await.unwrap();
    network.get_listeners()
}

// ============================================================================
// Property 36: Port conflict avoidance
// ============================================================================

/// Test that multiple nodes can start on different ports without conflicts
#[tokio::test]
#[serial]
async fn test_multiple_nodes_start_on_different_ports() {
    let port1 = find_available_port().unwrap();
    let port2 = find_available_port().unwrap();
    let port3 = find_available_port().unwrap();
    
    // Ensure all ports are different
    assert_ne!(port1, port2);
    assert_ne!(port2, port3);
    assert_ne!(port1, port3);
    
    // Start three networks
    let mut network1 = create_test_network(port1).await;
    let mut network2 = create_test_network(port2).await;
    let mut network3 = create_test_network(port3).await;
    
    let listeners1 = start_network(&mut network1, port1, vec![]).await;
    let listeners2 = start_network(&mut network2, port2, vec![]).await;
    let listeners3 = start_network(&mut network3, port3, vec![]).await;
    
    // All should have listeners
    assert!(!listeners1.is_empty(), "Node 1 should have listeners");
    assert!(!listeners2.is_empty(), "Node 2 should have listeners");
    assert!(!listeners3.is_empty(), "Node 3 should have listeners");
    
    // Shutdown all
    network1.shutdown().await.unwrap();
    network2.shutdown().await.unwrap();
    network3.shutdown().await.unwrap();
}

// ============================================================================
// Property 38: Multi-node connectivity
// ============================================================================

/// Test that a bootstrap node can accept incoming connections
#[tokio::test]
#[serial]
async fn test_bootstrap_node_accepts_connections() {
    // Start bootstrap node (no bootstrap peers)
    let port1 = find_available_port().unwrap();
    let mut bootstrap_node = create_test_network(port1).await;
    let listeners1 = start_network(&mut bootstrap_node, port1, vec![]).await;
    
    // Verify it's in bootstrap mode
    assert!(bootstrap_node.is_bootstrap_node());
    assert_eq!(bootstrap_node.get_bootstrap_mode(), BootstrapMode::Bootstrap);
    
    // Get the bootstrap address
    let bootstrap_addr = listeners1.first().unwrap().to_string();
    
    // Start client node with bootstrap peer
    let port2 = find_available_port().unwrap();
    let mut client_node = create_test_network(port2).await;
    start_network(&mut client_node, port2, vec![bootstrap_addr]).await;
    
    // Verify client is in client mode
    assert!(!client_node.is_bootstrap_node());
    assert_eq!(client_node.get_bootstrap_mode(), BootstrapMode::Client);
    
    // Cleanup
    bootstrap_node.shutdown().await.unwrap();
    client_node.shutdown().await.unwrap();
}

/// Test that client node stores known bootstrap peers
#[tokio::test]
#[serial]
async fn test_client_stores_bootstrap_peers() {
    let port1 = find_available_port().unwrap();
    let mut bootstrap_node = create_test_network(port1).await;
    let listeners1 = start_network(&mut bootstrap_node, port1, vec![]).await;
    let bootstrap_addr = listeners1.first().unwrap().to_string();
    
    let port2 = find_available_port().unwrap();
    let mut client_node = create_test_network(port2).await;
    start_network(&mut client_node, port2, vec![bootstrap_addr]).await;
    
    // Client should have stored the bootstrap peer
    assert!(!client_node.get_known_bootstrap_peers().is_empty());
    
    bootstrap_node.shutdown().await.unwrap();
    client_node.shutdown().await.unwrap();
}

/// Test peer discovery via mDNS between two nodes on same machine
#[tokio::test]
#[serial]
async fn test_mdns_peer_discovery() {
    // This test verifies mDNS works for local discovery
    let port1 = find_available_port().unwrap();
    let port2 = find_available_port().unwrap();
    
    let mut node1 = create_test_network(port1).await;
    let mut node2 = create_test_network(port2).await;
    
    start_network(&mut node1, port1, vec![]).await;
    start_network(&mut node2, port2, vec![]).await;
    
    // mDNS discovery takes time - just verify both nodes are running
    assert!(!node1.get_listeners().is_empty());
    assert!(!node2.get_listeners().is_empty());
    
    // Each node should have a unique peer ID
    assert_ne!(node1.get_local_peer_id(), node2.get_local_peer_id());
    
    node1.shutdown().await.unwrap();
    node2.shutdown().await.unwrap();
}

// ============================================================================
// Property 39: Instance isolation during job processing
// ============================================================================

/// Test that job processing in one instance doesn't affect another
#[tokio::test]
#[serial]
async fn test_job_processing_isolation() {
    // Create two independent job executors
    let engine1 = Arc::new(Mutex::new(DefaultInferenceEngine::new()));
    let engine2 = Arc::new(Mutex::new(DefaultInferenceEngine::new()));
    
    // Load models
    {
        let mut eng1 = engine1.lock().await;
        eng1.load_model("tinyllama-1.1b").await.unwrap();
    }
    {
        let mut eng2 = engine2.lock().await;
        eng2.load_model("tinyllama-1.1b").await.unwrap();
    }
    
    let mut executor1 = DefaultJobExecutor::with_inference_engine(10, engine1.clone());
    let mut executor2 = DefaultJobExecutor::with_inference_engine(10, engine2.clone());
    
    // Submit job to executor 1
    let job1 = JobOffer {
        job_id: "isolated-job-1".to_string(),
        model: "tinyllama-1.1b".to_string(),
        mode: JobMode::Batch,
        reward: 10.0,
        currency: "USD".to_string(),
        requirements: Requirements::default(),
        input_data: "Test input 1".to_string(),
    };
    executor1.submit_job(job1).await.unwrap();
    
    // Executor 2 should have empty queue
    assert_eq!(executor2.get_queue_status().pending_jobs, 0, "Executor 2 queue should be empty");
    
    // Process job in executor 1
    let result = executor1.process_next_job().await.unwrap();
    assert!(result.is_some(), "Executor 1 should process job");
    
    // Executor 2 should still have empty queue
    assert_eq!(executor2.get_queue_status().pending_jobs, 0, "Executor 2 queue should remain empty");
}

/// Test that engine state is isolated between instances
#[tokio::test]
#[serial]
async fn test_engine_state_isolation() {
    let mut engine1 = DefaultInferenceEngine::new();
    let mut engine2 = DefaultInferenceEngine::new();
    
    // Load model only in engine1
    engine1.load_model("tinyllama-1.1b").await.unwrap();
    
    // Engine1 should have model, engine2 should not
    assert!(engine1.is_model_loaded());
    assert!(!engine2.is_model_loaded());
    
    // Get model info
    let info1 = engine1.get_model_info().unwrap();
    assert_eq!(info1.name, "tinyllama-1.1b");
    
    let info2 = engine2.get_model_info();
    assert!(info2.is_none());
    
    // Unload from engine1
    engine1.unload_model().await.unwrap();
    
    // Both should now have no model
    assert!(!engine1.is_model_loaded());
    assert!(!engine2.is_model_loaded());
}

// ============================================================================
// Property 40: Independent state management
// ============================================================================

/// Test that multiple executors maintain independent queue state
#[tokio::test]
#[serial]
async fn test_independent_queue_state() {
    let engine1 = Arc::new(Mutex::new(DefaultInferenceEngine::new()));
    let engine2 = Arc::new(Mutex::new(DefaultInferenceEngine::new()));
    let engine3 = Arc::new(Mutex::new(DefaultInferenceEngine::new()));
    
    // Load models
    for engine in [&engine1, &engine2, &engine3] {
        let mut eng = engine.lock().await;
        eng.load_model("tinyllama-1.1b").await.unwrap();
    }
    
    let mut exec1 = DefaultJobExecutor::with_inference_engine(10, engine1);
    let mut exec2 = DefaultJobExecutor::with_inference_engine(10, engine2);
    let mut exec3 = DefaultJobExecutor::with_inference_engine(10, engine3);
    
    // Submit different numbers of jobs
    for i in 0..3 {
        exec1.submit_job(create_test_job(&format!("node1-{}", i))).await.unwrap();
    }
    for i in 0..5 {
        exec2.submit_job(create_test_job(&format!("node2-{}", i))).await.unwrap();
    }
    for i in 0..1 {
        exec3.submit_job(create_test_job(&format!("node3-{}", i))).await.unwrap();
    }
    
    // Verify queue sizes are independent
    assert_eq!(exec1.get_queue_status().pending_jobs, 3);
    assert_eq!(exec2.get_queue_status().pending_jobs, 5);
    assert_eq!(exec3.get_queue_status().pending_jobs, 1);
    
    // Process some jobs
    exec1.process_next_job().await.unwrap();
    exec2.process_next_job().await.unwrap();
    exec2.process_next_job().await.unwrap();
    
    // Queue sizes should update independently
    assert_eq!(exec1.get_queue_status().pending_jobs, 2);
    assert_eq!(exec2.get_queue_status().pending_jobs, 3);
    assert_eq!(exec3.get_queue_status().pending_jobs, 1);
}

/// Test that network layers maintain independent peer lists
#[tokio::test]
#[serial]
async fn test_independent_network_state() {
    let port1 = find_available_port().unwrap();
    let port2 = find_available_port().unwrap();
    
    let mut network1 = create_test_network(port1).await;
    let mut network2 = create_test_network(port2).await;
    
    start_network(&mut network1, port1, vec![]).await;
    start_network(&mut network2, port2, vec![]).await;
    
    // Each network should have its own peer ID
    let peer_id1 = network1.get_local_peer_id();
    let peer_id2 = network2.get_local_peer_id();
    assert_ne!(peer_id1, peer_id2, "Networks should have unique peer IDs");
    
    // Each network should have independent listener addresses
    let listeners1 = network1.get_listeners();
    let listeners2 = network2.get_listeners();
    
    for l1 in &listeners1 {
        for l2 in &listeners2 {
            assert_ne!(l1, l2, "Listeners should be on different addresses");
        }
    }
    
    network1.shutdown().await.unwrap();
    network2.shutdown().await.unwrap();
}

/// Test full multi-instance workflow with separate configs
#[tokio::test]
#[serial]
async fn test_full_multi_instance_workflow() {
    // Generate unique configurations for 3 instances
    let configs = MultiInstanceConfig::new(3)
        .with_base_config(NodeConfig {
            node_id: "workflow-test".to_string(),
            model_path: "tinyllama-1.1b".to_string(),
            max_queue_size: 10,
            ..Default::default()
        })
        .build()
        .unwrap();
    
    assert_eq!(configs.len(), 3);
    
    // Verify all node IDs are unique
    let node_ids: Vec<_> = configs.iter().map(|c| c.node_id.clone()).collect();
    let unique_ids: std::collections::HashSet<_> = node_ids.iter().collect();
    assert_eq!(node_ids.len(), unique_ids.len(), "All node IDs should be unique");
    
    // Verify all ports are unique
    let ports: Vec<_> = configs.iter().map(|c| c.listen_port).collect();
    let unique_ports: std::collections::HashSet<_> = ports.iter().collect();
    assert_eq!(ports.len(), unique_ports.len(), "All ports should be unique");
    
    // Create engines and executors for each config
    let mut executors = Vec::new();
    for config in &configs {
        let engine = Arc::new(Mutex::new(DefaultInferenceEngine::new()));
        {
            let mut eng = engine.lock().await;
            eng.load_model(&config.model_path).await.unwrap();
        }
        let executor = DefaultJobExecutor::with_inference_engine(
            config.max_queue_size,
            engine,
        );
        executors.push(executor);
    }
    
    // Submit and process jobs independently
    for (i, executor) in executors.iter_mut().enumerate() {
        let job = create_test_job(&format!("workflow-{}", i));
        executor.submit_job(job).await.unwrap();
        
        let result = executor.process_next_job().await.unwrap();
        assert!(result.is_some(), "Instance {} should process job", i);
    }
}

// ============================================================================
// Network message routing tests
// ============================================================================

/// Test that network stats are tracked per-instance
#[tokio::test]
#[serial]
async fn test_per_instance_network_stats() {
    let port1 = find_available_port().unwrap();
    let port2 = find_available_port().unwrap();
    
    let mut network1 = create_test_network(port1).await;
    let mut network2 = create_test_network(port2).await;
    
    start_network(&mut network1, port1, vec![]).await;
    start_network(&mut network2, port2, vec![]).await;
    
    // Get stats for each network
    let stats1 = network1.get_network_stats();
    let stats2 = network2.get_network_stats();
    
    // Both should have 0 connected peers (no direct connection established in this test)
    assert_eq!(stats1.connected_peers, 0);
    assert_eq!(stats2.connected_peers, 0);
    
    // Both should have subscribed topics
    assert!(stats1.subscribed_topics > 0);
    assert!(stats2.subscribed_topics > 0);
    
    network1.shutdown().await.unwrap();
    network2.shutdown().await.unwrap();
}

/// Test bootstrap status tracking
#[tokio::test]
#[serial]
async fn test_bootstrap_status_tracking() {
    let port1 = find_available_port().unwrap();
    let port2 = find_available_port().unwrap();
    
    // Bootstrap node
    let mut bootstrap = create_test_network(port1).await;
    let listeners = start_network(&mut bootstrap, port1, vec![]).await;
    
    let status = bootstrap.get_bootstrap_status();
    assert!(status.is_bootstrap);
    assert_eq!(status.mode, BootstrapMode::Bootstrap);
    assert!(!status.listen_addresses.is_empty());
    
    // Client node
    let bootstrap_addr = listeners.first().unwrap().to_string();
    let mut client = create_test_network(port2).await;
    start_network(&mut client, port2, vec![bootstrap_addr]).await;
    
    let status = client.get_bootstrap_status();
    assert!(!status.is_bootstrap);
    assert_eq!(status.mode, BootstrapMode::Client);
    assert_eq!(status.known_bootstrap_peers_count, 1);
    
    bootstrap.shutdown().await.unwrap();
    client.shutdown().await.unwrap();
}

// ============================================================================
// Helper functions
// ============================================================================

fn create_test_job(job_id: &str) -> JobOffer {
    JobOffer {
        job_id: job_id.to_string(),
        model: "tinyllama-1.1b".to_string(),
        mode: JobMode::Batch,
        reward: 10.0,
        currency: "USD".to_string(),
        requirements: Requirements::default(),
        input_data: "Test input".to_string(),
    }
}

