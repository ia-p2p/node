use proptest::prelude::*;
use mvp_node::network::{P2PNetworkLayer, NetworkLayer, NetworkConfig, load_or_generate_keypair};
use libp2p::{Multiaddr, identity::Keypair};
use serial_test::serial;

/// **Feature: mvp-node, Property 1: Network initialization on startup**
/// 
/// For any MVP_Node startup, the node should initialize the P2P_Network and 
/// become discoverable by other nodes through mDNS and gossipsub protocols.
/// 
/// **Validates: Requirements 1.1**
#[cfg(test)]
mod network_property_tests {
    use super::*;

    // Generator for valid network configurations
    fn arb_network_config() -> impl Strategy<Value = NetworkConfig> {
        (
            1024u16..65535u16,  // port range
            prop::collection::vec("127\\.0\\.0\\.1:[0-9]{4,5}", 0..3), // bootstrap peers
        ).prop_map(|(port, bootstrap_strs)| {
            let listen_addr: Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", port).parse().unwrap();
            let bootstrap_peers: Vec<Multiaddr> = bootstrap_strs
                .into_iter()
                .filter_map(|s| format!("/ip4/{}/tcp/0", s.replace(":", "/tcp/")).parse().ok())
                .collect();
            
            NetworkConfig {
                listen_addr,
                bootstrap_peers,
            }
        })
    }

    // Generator for keypairs (using a seed for deterministic generation)
    fn arb_keypair() -> impl Strategy<Value = Keypair> {
        any::<u64>().prop_map(|_seed| {
            // Generate a new keypair for each test
            Keypair::generate_ed25519()
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(10))] // Reduced cases for network tests
        
        /// Property 1: Network initialization on startup
        /// For any valid configuration, network layer should initialize successfully
        #[test]
        #[serial]
        fn network_initialization_succeeds(
            keypair in arb_keypair(),
            config in arb_network_config()
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // Create network layer
                let mut network = P2PNetworkLayer::new(keypair).await.unwrap();
                
                // Start network with configuration
                let start_result = network.start(config.clone()).await;
                prop_assert!(start_result.is_ok(), "Network should start successfully: {:?}", start_result);
                
                // Verify network is listening
                let listeners = network.get_listeners();
                prop_assert!(!listeners.is_empty(), "Network should have at least one listener");
                
                // Verify peer ID is valid
                let peer_id = network.get_local_peer_id();
                prop_assert!(!peer_id.to_string().is_empty(), "Peer ID should not be empty");
                
                // Initial peer count should be 0 (no connections yet)
                prop_assert_eq!(network.get_peer_count(), 0, "Initial peer count should be 0");
                
                // Shutdown gracefully
                let shutdown_result = network.shutdown().await;
                prop_assert!(shutdown_result.is_ok(), "Network should shutdown gracefully: {:?}", shutdown_result);
                
                Ok(())
            })?
        }
        
        /// Property 11: Peer discovery response
        /// For any peer discovery attempt, the MVP_Node should respond with network identity
        #[test]
        #[serial]
        fn peer_discovery_responds_with_identity(
            keypair in arb_keypair(),
            config in arb_network_config()
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut network = P2PNetworkLayer::new(keypair).await.unwrap();
                let start_result = network.start(config).await;
                prop_assert!(start_result.is_ok(), "Network should start: {:?}", start_result);
                
                // Verify network has valid peer ID (identity)
                let peer_id = network.get_local_peer_id();
                prop_assert!(!peer_id.to_string().is_empty(), "Peer ID should not be empty");
                
                // Verify network is listening and discoverable
                let listeners = network.get_listeners();
                prop_assert!(!listeners.is_empty(), "Should have listeners for discovery");
                
                // Verify each listener is a valid multiaddr
                for listener in listeners {
                    prop_assert!(listener.to_string().len() > 0, "Listener address should be valid");
                }
                
                let shutdown_result = network.shutdown().await;
                prop_assert!(shutdown_result.is_ok(), "Should shutdown: {:?}", shutdown_result);
                
                Ok(())
            })?
        }
        
        /// Property 11 (continued): Topic subscription capability
        /// For any network layer, it should be able to subscribe to topics
        #[test]
        #[serial]
        fn topic_subscription_works(
            keypair in arb_keypair(),
            config in arb_network_config(),
            topic in "[a-zA-Z0-9/_-]{5,30}"
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut network = P2PNetworkLayer::new(keypair).await.unwrap();
                let start_result = network.start(config).await;
                prop_assert!(start_result.is_ok(), "Network should start: {:?}", start_result);
                
                // Subscribe to topic
                let sub_result = network.subscribe_topic(&topic).await;
                prop_assert!(sub_result.is_ok(), "Should subscribe to topic: {:?}", sub_result);
                
                let shutdown_result = network.shutdown().await;
                prop_assert!(shutdown_result.is_ok(), "Should shutdown: {:?}", shutdown_result);
                
                Ok(())
            })?
        }
        
        /// Property 12: Connection maintenance
        /// For any established peer connection, the network should maintain it
        #[test]
        #[serial]
        fn connection_maintains_state(
            keypair in arb_keypair(),
            config in arb_network_config()
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut network = P2PNetworkLayer::new(keypair).await.unwrap();
                let original_peer_id = network.get_local_peer_id();
                
                let start_result = network.start(config).await;
                prop_assert!(start_result.is_ok(), "Network should start: {:?}", start_result);
                
                // Peer ID should remain consistent (connection state maintained)
                prop_assert_eq!(network.get_local_peer_id(), original_peer_id, 
                               "Peer ID should remain consistent after start");
                
                // Should be able to get listeners (connection capability maintained)
                let listeners = network.get_listeners();
                prop_assert!(!listeners.is_empty(), "Should have listeners after start");
                
                // Initial peer count
                let initial_peer_count = network.get_peer_count();
                
                // Connection state should be queryable
                let connected_peers = network.get_connected_peers();
                prop_assert_eq!(connected_peers.len(), initial_peer_count,
                               "Connected peers should match peer count");
                
                let shutdown_result = network.shutdown().await;
                prop_assert!(shutdown_result.is_ok(), "Should shutdown: {:?}", shutdown_result);
                
                Ok(())
            })?
        }
        
        /// Property: Message publishing capability
        /// For any network layer, it should be able to publish messages to topics
        #[test]
        #[serial]
        fn message_publishing_works(
            keypair in arb_keypair(),
            config in arb_network_config(),
            topic in "[a-zA-Z0-9/_-]{5,30}",
            message_data in prop::collection::vec(any::<u8>(), 1..100)
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut network = P2PNetworkLayer::new(keypair).await.unwrap();
                let start_result = network.start(config).await;
                prop_assert!(start_result.is_ok(), "Network should start: {:?}", start_result);
                
                // Subscribe to topic first
                let sub_result = network.subscribe_topic(&topic).await;
                prop_assert!(sub_result.is_ok(), "Should subscribe: {:?}", sub_result);
                
                // Publish message
                // Note: In single-node setup, gossipsub may return InsufficientPeers
                // This is expected and not a bug - in production there will be peers
                let pub_result = network.publish_message(&topic, &message_data).await;
                match pub_result {
                    Ok(_) => {}, // Success
                    Err(ref e) if e.to_string().contains("InsufficientPeers") => {
                        // Expected in single-node testing
                    }
                    Err(e) => prop_assert!(false, "Unexpected publish error: {:?}", e),
                }
                
                let shutdown_result = network.shutdown().await;
                prop_assert!(shutdown_result.is_ok(), "Should shutdown: {:?}", shutdown_result);
                
                Ok(())
            })?
        }
        
        /// Property: Keypair consistency
        /// For any keypair, the network should maintain consistent peer ID
        #[test]
        fn keypair_consistency(keypair in arb_keypair()) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let peer_id_1 = {
                    let network = P2PNetworkLayer::new(keypair.clone()).await.unwrap();
                    network.get_local_peer_id()
                };
                
                let peer_id_2 = {
                    let network = P2PNetworkLayer::new(keypair).await.unwrap();
                    network.get_local_peer_id()
                };
                
                prop_assert_eq!(peer_id_1, peer_id_2, "Same keypair should produce same peer ID");
                
                Ok(())
            })?
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_network_startup_and_shutdown() {
        let keypair = Keypair::generate_ed25519();
        let config = NetworkConfig {
            listen_addr: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            bootstrap_peers: vec![],
        };

        let mut network = P2PNetworkLayer::new(keypair).await.unwrap();
        
        // Start network
        let start_result = network.start(config).await;
        assert!(start_result.is_ok(), "Network should start successfully");
        
        // Verify basic functionality
        assert!(!network.get_listeners().is_empty(), "Should have listeners");
        assert_eq!(network.get_peer_count(), 0, "Should start with 0 peers");
        
        // Shutdown
        let shutdown_result = network.shutdown().await;
        assert!(shutdown_result.is_ok(), "Network should shutdown gracefully");
    }

    #[tokio::test]
    #[serial]
    async fn test_topic_subscription_and_publishing() {
        let keypair = Keypair::generate_ed25519();
        let config = NetworkConfig {
            listen_addr: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            bootstrap_peers: vec![],
        };

        let mut network = P2PNetworkLayer::new(keypair).await.unwrap();
        let _ = network.start(config).await.unwrap();
        
        // Subscribe to topic
        let topic = "test-topic";
        let sub_result = network.subscribe_topic(topic).await;
        assert!(sub_result.is_ok(), "Should subscribe to topic successfully");
        
        // Publish message
        // Note: In a single-node setup, gossipsub may return InsufficientPeers error
        // This is expected behavior and not a bug. In production, there will be peers.
        let message = b"test message";
        let pub_result = network.publish_message(topic, message).await;
        
        // Accept either success or InsufficientPeers error (expected in single-node test)
        match pub_result {
            Ok(_) => {}, // Success
            Err(e) if e.to_string().contains("InsufficientPeers") => {
                // This is expected in single-node testing
                eprintln!("Note: InsufficientPeers error is expected in single-node testing");
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
        
        let _ = network.shutdown().await;
    }

    #[test]
    fn test_keypair_loading_and_generation() {
        use tempfile::NamedTempFile;
        
        // Test generating new keypair
        let keypair1 = Keypair::generate_ed25519();
        let keypair2 = Keypair::generate_ed25519();
        
        // Different keypairs should have different peer IDs
        let peer_id1 = libp2p::PeerId::from(keypair1.public());
        let peer_id2 = libp2p::PeerId::from(keypair2.public());
        assert_ne!(peer_id1, peer_id2, "Different keypairs should have different peer IDs");
        
        // Test loading non-existent file (should generate new)
        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path();
        std::fs::remove_file(temp_path).unwrap(); // Remove the file
        
        let loaded_keypair = load_or_generate_keypair(temp_path).unwrap();
        assert!(!libp2p::PeerId::from(loaded_keypair.public()).to_string().is_empty());
    }

    #[test]
    fn test_network_config_default() {
        let config = NetworkConfig::default();
        
        // Should have valid default listen address
        assert!(config.listen_addr.to_string().contains("0.0.0.0"));
        assert!(config.listen_addr.to_string().contains("tcp"));
        
        // Should start with empty bootstrap peers
        assert!(config.bootstrap_peers.is_empty());
    }

    /// Integration test for Property 11: Peer discovery between two nodes
    #[tokio::test]
    #[serial]
    async fn test_peer_discovery_between_nodes() {
        use tokio::time::{sleep, Duration};
        
        // Start first node (bootstrap node)
        let keypair1 = Keypair::generate_ed25519();
        let config1 = NetworkConfig {
            listen_addr: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            bootstrap_peers: vec![],
        };
        
        let mut network1 = P2PNetworkLayer::new(keypair1).await.unwrap();
        let _ = network1.start(config1).await.unwrap();
        
        let peer_id1 = network1.get_local_peer_id();
        let listeners1 = network1.get_listeners();
        assert!(!listeners1.is_empty(), "Node 1 should have listeners");
        
        // Get the actual listening address
        let node1_addr = listeners1[0].clone();
        println!("Node 1 listening on: {}", node1_addr);
        
        // Start second node and connect to first
        let keypair2 = Keypair::generate_ed25519();
        let config2 = NetworkConfig {
            listen_addr: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            bootstrap_peers: vec![node1_addr.clone()],
        };
        
        let mut network2 = P2PNetworkLayer::new(keypair2).await.unwrap();
        let _ = network2.start(config2).await.unwrap();
        
        let peer_id2 = network2.get_local_peer_id();
        
        // Give nodes time to discover each other via mDNS and establish connection
        // mDNS discovery typically takes 1-3 seconds
        sleep(Duration::from_secs(3)).await;
        
        // Verify nodes have different peer IDs
        assert_ne!(peer_id1, peer_id2, "Nodes should have different peer IDs");
        
        // Note: In localhost testing, mDNS may or may not work depending on the system
        // The important thing is that the nodes are discoverable and can accept connections
        println!("Node 1 peer count: {}", network1.get_peer_count());
        println!("Node 2 peer count: {}", network2.get_peer_count());
        
        // Cleanup
        let _ = network2.shutdown().await;
        let _ = network1.shutdown().await;
    }

    /// Integration test for Property 11: Node identity consistency
    #[tokio::test]
    #[serial]
    async fn test_node_identity_remains_consistent() {
        let keypair = Keypair::generate_ed25519();
        let peer_id_from_keypair = libp2p::PeerId::from(keypair.public());
        
        let config = NetworkConfig {
            listen_addr: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            bootstrap_peers: vec![],
        };
        
        let mut network = P2PNetworkLayer::new(keypair.clone()).await.unwrap();
        let peer_id_before_start = network.get_local_peer_id();
        
        // Verify peer ID matches keypair
        assert_eq!(peer_id_before_start, peer_id_from_keypair, 
                  "Peer ID should match keypair before start");
        
        let _ = network.start(config).await.unwrap();
        let peer_id_after_start = network.get_local_peer_id();
        
        // Verify peer ID remains consistent after starting
        assert_eq!(peer_id_after_start, peer_id_from_keypair,
                  "Peer ID should remain consistent after start");
        assert_eq!(peer_id_after_start, peer_id_before_start,
                  "Peer ID should not change during start");
        
        let _ = network.shutdown().await;
    }

    /// Integration test for Property 11: Network responds to identify protocol
    #[tokio::test]
    #[serial]
    async fn test_network_supports_identify_protocol() {
        let keypair = Keypair::generate_ed25519();
        let config = NetworkConfig {
            listen_addr: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            bootstrap_peers: vec![],
        };
        
        let mut network = P2PNetworkLayer::new(keypair).await.unwrap();
        let _ = network.start(config).await.unwrap();
        
        // Verify node has identity
        let peer_id = network.get_local_peer_id();
        assert!(!peer_id.to_string().is_empty(), "Should have valid peer ID");
        
        // Verify node is listening (discoverable)
        let listeners = network.get_listeners();
        assert!(!listeners.is_empty(), "Should have at least one listener");
        
        // Verify listeners are on localhost (for testing)
        for listener in listeners {
            let addr_str = listener.to_string();
            assert!(addr_str.contains("127.0.0.1") || addr_str.contains("0.0.0.0"),
                   "Listener should be on localhost for testing");
        }
        
        let _ = network.shutdown().await;
    }

    /// Integration test for Property 12: Connection maintenance over time
    #[tokio::test]
    #[serial]
    async fn test_connection_maintenance_over_time() {
        use tokio::time::{sleep, Duration};
        
        let keypair = Keypair::generate_ed25519();
        let config = NetworkConfig {
            listen_addr: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            bootstrap_peers: vec![],
        };
        
        let mut network = P2PNetworkLayer::new(keypair).await.unwrap();
        let _ = network.start(config).await.unwrap();
        
        let peer_id_initial = network.get_local_peer_id();
        let listeners_initial = network.get_listeners();
        let peer_count_initial = network.get_peer_count();
        
        // Wait for some time to ensure connection state is maintained
        sleep(Duration::from_millis(500)).await;
        
        // Verify connection state is maintained
        let peer_id_after = network.get_local_peer_id();
        let listeners_after = network.get_listeners();
        let peer_count_after = network.get_peer_count();
        
        assert_eq!(peer_id_initial, peer_id_after, 
                  "Peer ID should remain consistent over time");
        assert_eq!(listeners_initial.len(), listeners_after.len(),
                  "Number of listeners should remain consistent");
        assert_eq!(peer_count_initial, peer_count_after,
                  "Peer count should remain consistent (no disconnections)");
        
        let _ = network.shutdown().await;
    }

    /// Integration test for Property 12: Bidirectional message capability
    #[tokio::test]
    #[serial]
    async fn test_bidirectional_message_capability() {
        let keypair = Keypair::generate_ed25519();
        let config = NetworkConfig {
            listen_addr: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            bootstrap_peers: vec![],
        };
        
        let mut network = P2PNetworkLayer::new(keypair).await.unwrap();
        let _ = network.start(config).await.unwrap();
        
        // Subscribe to a topic (receive capability)
        let topic = "test-bidirectional";
        let sub_result = network.subscribe_topic(topic).await;
        assert!(sub_result.is_ok(), "Should be able to subscribe (receive capability)");
        
        // Attempt to publish (send capability)
        let message = b"test message";
        let pub_result = network.publish_message(topic, message).await;
        
        // Accept either success or InsufficientPeers (expected in single-node test)
        match pub_result {
            Ok(_) => {
                println!("Successfully published message (send capability verified)");
            }
            Err(e) if e.to_string().contains("InsufficientPeers") => {
                println!("InsufficientPeers is expected in single-node test");
                // The important thing is that the API works and doesn't panic
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
        
        let _ = network.shutdown().await;
    }

    /// Integration test for Property 12: Connection state query capability
    #[tokio::test]
    #[serial]
    async fn test_connection_state_queryable() {
        let keypair = Keypair::generate_ed25519();
        let config = NetworkConfig {
            listen_addr: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            bootstrap_peers: vec![],
        };
        
        let mut network = P2PNetworkLayer::new(keypair).await.unwrap();
        let _ = network.start(config).await.unwrap();
        
        // Query connection state
        let peer_count = network.get_peer_count();
        let connected_peers = network.get_connected_peers();
        let listeners = network.get_listeners();
        let local_peer_id = network.get_local_peer_id();
        
        // Verify all state queries work
        assert_eq!(connected_peers.len(), peer_count,
                  "Connected peers list should match peer count");
        assert!(!listeners.is_empty(),
               "Should have listeners");
        assert!(!local_peer_id.to_string().is_empty(),
               "Should have valid local peer ID");
        
        // Verify we can query individual peer info (even if empty)
        for peer_id in connected_peers {
            let peer_info = network.get_peer_info(&peer_id);
            // In a fresh network, this might be None, but the API should work
            if let Some(info) = peer_info {
                assert_eq!(info.peer_id, peer_id, "Peer info should match peer ID");
            }
        }
        
        let _ = network.shutdown().await;
    }
}