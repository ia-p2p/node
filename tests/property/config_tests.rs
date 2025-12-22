use proptest::prelude::*;
use mvp_node::{NodeConfig, ConfigManager};
use mvp_node::config::DefaultConfigManager;
use tempfile::NamedTempFile;
use std::fs;

/// **Feature: mvp-node, Property 37: Instance configuration uniqueness**
/// 
/// For any Node_Instance startup, the MVP_Node should accept and apply 
/// configuration parameters that specify unique identity and network settings.
/// 
/// **Validates: Requirements 8.2**
#[cfg(test)]
mod config_property_tests {
    use super::*;

    // Generator for valid NodeConfig instances
    fn arb_node_config() -> impl Strategy<Value = NodeConfig> {
        (
            "[a-zA-Z0-9_-]{1,20}",  // node_id
            1024u16..65535u16,       // listen_port (valid port range)
            prop::collection::vec("(127\\.0\\.0\\.1|localhost):[0-9]{4,5}", 0..5), // bootstrap_peers
            "/tmp/test_model\\.gguf", // model_path (we'll create this file)
            1usize..1000usize,       // max_queue_size
            "(trace|debug|info|warn|error)", // log_level
        ).prop_map(|(node_id, listen_port, bootstrap_peers, model_path, max_queue_size, log_level)| {
            NodeConfig {
                node_id,
                listen_port,
                bootstrap_peers,
                model_path,
                max_queue_size,
                log_level,
            }
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        
        /// Property: Configuration round-trip consistency
        /// For any valid NodeConfig, saving and loading should preserve all settings
        #[test]
        fn config_roundtrip_preserves_settings(config in arb_node_config()) {
            // Create a temporary model file for validation
            let model_file = NamedTempFile::new().unwrap();
            fs::write(&model_file.path(), b"dummy model content").unwrap();
            
            // Update config to use the temporary model file
            let mut test_config = config;
            test_config.model_path = model_file.path().to_string_lossy().to_string();
            
            // Create temporary config file
            let config_file = NamedTempFile::new().unwrap();
            let config_path = config_file.path().to_string_lossy().to_string();
            
            // Save configuration
            DefaultConfigManager::save_config(&test_config, &config_path).unwrap();
            
            // Load configuration back
            let loaded_config = DefaultConfigManager::load_config(&config_path).unwrap();
            
            // Verify all fields are preserved
            prop_assert_eq!(loaded_config.node_id, test_config.node_id);
            prop_assert_eq!(loaded_config.listen_port, test_config.listen_port);
            prop_assert_eq!(loaded_config.bootstrap_peers, test_config.bootstrap_peers);
            prop_assert_eq!(loaded_config.model_path, test_config.model_path);
            prop_assert_eq!(loaded_config.max_queue_size, test_config.max_queue_size);
            prop_assert_eq!(loaded_config.log_level, test_config.log_level);
        }
        
        /// Property: Configuration validation consistency
        /// For any NodeConfig with valid parameters, validation should succeed
        #[test]
        fn valid_config_passes_validation(config in arb_node_config()) {
            // Create a temporary model file for validation
            let model_file = NamedTempFile::new().unwrap();
            fs::write(&model_file.path(), b"dummy model content").unwrap();
            
            // Update config to use the temporary model file
            let mut test_config = config;
            test_config.model_path = model_file.path().to_string_lossy().to_string();
            
            // Validation should succeed for valid config
            let result = DefaultConfigManager::validate_config(&test_config);
            prop_assert!(result.is_ok());
        }
        
        /// Property: Invalid configurations are rejected
        /// For any NodeConfig with max_queue_size = 0, validation should fail
        #[test]
        fn zero_queue_size_fails_validation(mut config in arb_node_config()) {
            config.max_queue_size = 0;
            
            let result = DefaultConfigManager::validate_config(&config);
            prop_assert!(result.is_err());
            prop_assert!(result.unwrap_err().to_string().contains("max_queue_size"));
        }
        
        /// Property: Unique identity generation
        /// For any two identity generation calls, the results should be different
        #[test]
        fn identity_generation_produces_unique_results(_seed in 0u64..1000u64) {
            let identity1 = DefaultConfigManager::generate_identity().unwrap();
            let identity2 = DefaultConfigManager::generate_identity().unwrap();
            
            // Peer IDs should be different
            prop_assert_ne!(identity1.peer_id, identity2.peer_id);
            
            // Manifest IDs should be different
            prop_assert_ne!(identity1.manifest_id, identity2.manifest_id);
        }
    }

    #[test]
    fn test_nonexistent_model_path_fails_validation() {
        let config = NodeConfig {
            node_id: "test".to_string(),
            listen_port: 8080,
            bootstrap_peers: vec![],
            model_path: "/nonexistent/path/model.gguf".to_string(),
            max_queue_size: 100,
            log_level: "info".to_string(),
        };
        
        let result = DefaultConfigManager::validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Model path does not exist"));
    }
}