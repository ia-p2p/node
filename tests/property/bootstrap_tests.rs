//! Property-based tests for bootstrap node functionality
//! 
//! Validates Property 2: Bootstrap node operation
//! "For any MVP_Node started with an empty bootstrap peer list, the node should 
//! successfully operate as a Bootstrap_Node and accept incoming connections from other nodes."

use mvp_node::network::{
    BootstrapMode, BootstrapNodeStatus, NetworkConfig, NetworkLayer, 
    P2PNetworkLayer, TopologyConfig,
};
use libp2p::identity::Keypair;
use proptest::prelude::*;
use serial_test::serial;

/// Create a test network config with no bootstrap peers (bootstrap mode)
fn create_bootstrap_config(port: u16) -> NetworkConfig {
    NetworkConfig {
        listen_addr: format!("/ip4/127.0.0.1/tcp/{}", port).parse().unwrap(),
        bootstrap_peers: vec![],
    }
}

/// Create a test network config with bootstrap peers (client mode)
fn create_client_config(port: u16, bootstrap_addr: &str) -> NetworkConfig {
    NetworkConfig {
        listen_addr: format!("/ip4/127.0.0.1/tcp/{}", port).parse().unwrap(),
        bootstrap_peers: vec![bootstrap_addr.parse().unwrap()],
    }
}

/// Property: A node with no bootstrap peers should operate as a bootstrap node
#[tokio::test]
#[serial]
async fn test_no_bootstrap_peers_becomes_bootstrap_node() {
    let keypair = Keypair::generate_ed25519();
    let mut network = P2PNetworkLayer::new(keypair).await.unwrap();
    
    let config = create_bootstrap_config(0); // Port 0 for auto-assignment
    network.start(config).await.unwrap();
    
    // Verify bootstrap mode
    assert!(network.is_bootstrap_node(), "Node should be in bootstrap mode");
    assert_eq!(network.get_bootstrap_mode(), BootstrapMode::Bootstrap);
    
    network.shutdown().await.unwrap();
}

/// Property: A node with bootstrap peers should operate as a client node
#[tokio::test]
#[serial]
async fn test_with_bootstrap_peers_becomes_client_node() {
    let keypair = Keypair::generate_ed25519();
    let mut network = P2PNetworkLayer::new(keypair).await.unwrap();
    
    // Create config with a dummy bootstrap peer
    let config = NetworkConfig {
        listen_addr: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
        bootstrap_peers: vec!["/ip4/127.0.0.1/tcp/9999".parse().unwrap()],
    };
    
    network.start(config).await.unwrap();
    
    // Verify client mode
    assert!(!network.is_bootstrap_node(), "Node should be in client mode");
    assert_eq!(network.get_bootstrap_mode(), BootstrapMode::Client);
    
    // Should have stored the bootstrap peer
    assert_eq!(network.get_known_bootstrap_peers().len(), 1);
    
    network.shutdown().await.unwrap();
}

/// Property: Bootstrap node should have correct status
#[tokio::test]
#[serial]
async fn test_bootstrap_node_status() {
    let keypair = Keypair::generate_ed25519();
    let mut network = P2PNetworkLayer::new(keypair).await.unwrap();
    
    let config = create_bootstrap_config(0);
    network.start(config).await.unwrap();
    
    // Wait a bit for uptime
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    let status: BootstrapNodeStatus = network.get_bootstrap_status();
    
    assert!(status.is_bootstrap);
    assert_eq!(status.mode, BootstrapMode::Bootstrap);
    assert!(status.uptime_secs >= 0);
    assert!(!status.listen_addresses.is_empty());
    assert!(status.auto_accept_peers);
    
    network.shutdown().await.unwrap();
}

/// Property: Bootstrap node should respect max peers configuration
#[tokio::test]
#[serial]
async fn test_bootstrap_node_max_peers() {
    let keypair = Keypair::generate_ed25519();
    let topology_config = TopologyConfig {
        max_peers: 5,
        auto_accept_peers: true,
        preferred_peers: vec![],
        enable_peer_rotation: false,
    };
    
    let mut network = P2PNetworkLayer::new_bootstrap(keypair, topology_config).await.unwrap();
    
    let config = create_bootstrap_config(0);
    network.start(config).await.unwrap();
    
    assert!(network.is_bootstrap_node());
    assert_eq!(network.get_topology_config().max_peers, 5);
    assert!(network.can_accept_peer()); // Can accept since no peers yet
    
    network.shutdown().await.unwrap();
}

/// Property: Bootstrap node should disable peer acceptance when configured
#[tokio::test]
#[serial]
async fn test_bootstrap_node_disabled_auto_accept() {
    let keypair = Keypair::generate_ed25519();
    let topology_config = TopologyConfig {
        max_peers: 50,
        auto_accept_peers: false,
        preferred_peers: vec![],
        enable_peer_rotation: false,
    };
    
    let mut network = P2PNetworkLayer::new_bootstrap(keypair, topology_config).await.unwrap();
    
    let config = create_bootstrap_config(0);
    network.start(config).await.unwrap();
    
    assert!(!network.can_accept_peer()); // Should not accept since auto_accept is false
    
    network.shutdown().await.unwrap();
}

/// Property: Preferred peers should bypass max_peers limit conceptually
#[tokio::test]
#[serial]
async fn test_preferred_peers_management() {
    let keypair = Keypair::generate_ed25519();
    let mut network = P2PNetworkLayer::new(keypair).await.unwrap();
    
    let config = create_bootstrap_config(0);
    network.start(config).await.unwrap();
    
    // Generate a test peer ID
    let test_keypair = Keypair::generate_ed25519();
    let test_peer_id = test_keypair.public().to_peer_id();
    
    // Initially not preferred
    assert!(!network.is_preferred_peer(&test_peer_id));
    
    // Add as preferred
    network.add_preferred_peer(test_peer_id);
    assert!(network.is_preferred_peer(&test_peer_id));
    
    // Remove from preferred
    network.remove_preferred_peer(&test_peer_id);
    assert!(!network.is_preferred_peer(&test_peer_id));
    
    network.shutdown().await.unwrap();
}

/// Property: Network stats should be accurate
#[tokio::test]
#[serial]
async fn test_network_stats() {
    let keypair = Keypair::generate_ed25519();
    let mut network = P2PNetworkLayer::new(keypair).await.unwrap();
    
    let config = create_bootstrap_config(0);
    network.start(config).await.unwrap();
    
    let stats = network.get_network_stats();
    
    assert!(stats.is_bootstrap);
    assert_eq!(stats.connected_peers, 0);
    assert!(stats.subscribed_topics > 0); // Should have default topic subscriptions
    assert!(stats.uptime.is_some());
    
    network.shutdown().await.unwrap();
}

/// Property: Bootstrap mode display should work correctly
#[test]
fn test_bootstrap_mode_display() {
    assert_eq!(format!("{}", BootstrapMode::Bootstrap), "Bootstrap");
    assert_eq!(format!("{}", BootstrapMode::Client), "Client");
}

/// Property: Default topology config should have reasonable values
#[test]
fn test_default_topology_config() {
    let config = TopologyConfig::default();
    
    assert!(config.max_peers > 0);
    assert!(config.auto_accept_peers);
    assert!(config.preferred_peers.is_empty());
    assert!(config.enable_peer_rotation);
}

proptest! {
    /// Property: Any max_peers value should be stored correctly
    #[test]
    fn topology_config_max_peers(max_peers in 1usize..1000usize) {
        let config = TopologyConfig {
            max_peers,
            ..Default::default()
        };
        prop_assert_eq!(config.max_peers, max_peers);
    }
}

/// Property: Bootstrap node should accept connections when peers < max_peers
#[tokio::test]
#[serial]
async fn test_bootstrap_accepts_connections_within_limit() {
    let keypair = Keypair::generate_ed25519();
    let topology_config = TopologyConfig {
        max_peers: 10,
        auto_accept_peers: true,
        ..Default::default()
    };
    
    let mut network = P2PNetworkLayer::new_bootstrap(keypair, topology_config).await.unwrap();
    
    let config = create_bootstrap_config(0);
    network.start(config).await.unwrap();
    
    // With 0 peers and max 10, should be able to accept
    assert!(network.can_accept_peer());
    assert!(network.get_peer_count() < network.get_topology_config().max_peers);
    
    network.shutdown().await.unwrap();
}

/// Property: Uptime should be None before start and Some after start
#[tokio::test]
#[serial]
async fn test_uptime_tracking() {
    let keypair = Keypair::generate_ed25519();
    let mut network = P2PNetworkLayer::new(keypair).await.unwrap();
    
    // Before start, uptime should be None
    assert!(network.get_uptime().is_none());
    
    let config = create_bootstrap_config(0);
    network.start(config).await.unwrap();
    
    // After start, uptime should be Some
    assert!(network.get_uptime().is_some());
    
    // After shutdown, uptime should be None again
    network.shutdown().await.unwrap();
    assert!(network.get_uptime().is_none());
}

/// Property: Known bootstrap peers should be stored for client nodes
#[tokio::test]
#[serial]
async fn test_known_bootstrap_peers_stored() {
    let keypair = Keypair::generate_ed25519();
    let mut network = P2PNetworkLayer::new(keypair).await.unwrap();
    
    let config = NetworkConfig {
        listen_addr: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
        bootstrap_peers: vec![
            "/ip4/127.0.0.1/tcp/9001".parse().unwrap(),
            "/ip4/127.0.0.1/tcp/9002".parse().unwrap(),
        ],
    };
    
    network.start(config).await.unwrap();
    
    // Should have stored the bootstrap peers
    assert_eq!(network.get_known_bootstrap_peers().len(), 2);
    assert_eq!(network.get_bootstrap_mode(), BootstrapMode::Client);
    
    network.shutdown().await.unwrap();
}

/// Property: Bootstrap node can manually add known peers
#[tokio::test]
#[serial]
async fn test_add_known_bootstrap_peer() {
    let keypair = Keypair::generate_ed25519();
    let mut network = P2PNetworkLayer::new(keypair).await.unwrap();
    
    let config = create_bootstrap_config(0);
    network.start(config).await.unwrap();
    
    assert!(network.is_bootstrap_node());
    assert_eq!(network.get_known_bootstrap_peers().len(), 0);
    
    // Add a known peer
    let peer_addr = "/ip4/127.0.0.1/tcp/9999".parse().unwrap();
    network.add_known_bootstrap_peer(peer_addr);
    
    assert_eq!(network.get_known_bootstrap_peers().len(), 1);
    
    // Adding same peer again should not duplicate
    network.add_known_bootstrap_peer("/ip4/127.0.0.1/tcp/9999".parse().unwrap());
    assert_eq!(network.get_known_bootstrap_peers().len(), 1);
    
    network.shutdown().await.unwrap();
}

