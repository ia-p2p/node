//! Property tests for the Inference Engine
//!
//! Implements property tests for:
//! - Task 5.1: Model loading (Property 3)
//! - Task 5.2: Model size constraint (Property 16)
//! - Task 5.3: Inference timeout (Property 18)

use proptest::prelude::*;
use mvp_node::inference::DefaultInferenceEngine;
use mvp_node::InferenceEngine;
use serial_test::serial;
use std::time::Duration;

/// Maximum allowed parameters (7 billion)
const MAX_PARAMETERS: u64 = 7_000_000_000;

/// Default inference timeout in seconds
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// **Feature: mvp-node, Property 3: Model loading on startup**
/// **Feature: mvp-node, Property 16: Model size constraint**
/// **Feature: mvp-node, Property 18: Inference timeout compliance**
#[cfg(test)]
mod inference_property_tests {
    use super::*;

    // Generator for valid model paths (small models <=7B)
    fn arb_valid_model_path() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("tinyllama-1.1b".to_string()),
            Just("phi-2".to_string()),
            Just("mistral-7b".to_string()),
            Just("llama-3b".to_string()),
            Just("gpt2-small".to_string()),
        ]
    }

    // Generator for invalid model paths (models > 7B)
    fn arb_invalid_model_path() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("llama-70b".to_string()),
            Just("llama-13b".to_string()),
            Just("llama-8b".to_string()),
            Just("mixtral-8x7b".to_string()),
        ]
    }

    // Generator for valid inference inputs
    fn arb_valid_input() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 .,!?-]{10,500}".prop_map(|s| s)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(20))]

        /// Property 3: Model loading on startup
        /// For any valid model path, the engine should successfully load the model
        #[test]
        fn model_loading_succeeds_for_valid_models(model_path in arb_valid_model_path()) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(async {
                let mut engine = DefaultInferenceEngine::new();
                let load_result = engine.load_model(&model_path).await;
                (load_result.is_ok(), engine.is_model_loaded(), engine.get_model_info().is_some())
            });
            
            prop_assert!(result.0, "Should load valid model '{}'", model_path);
            prop_assert!(result.1, "Model should be marked as loaded");
            prop_assert!(result.2, "Model info should be available");
        }

        /// Property 16: Model size constraint
        /// For any model with >7B parameters, loading should fail
        #[test]
        fn model_loading_fails_for_large_models(model_path in arb_invalid_model_path()) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(async {
                let mut engine = DefaultInferenceEngine::new();
                let load_result = engine.load_model(&model_path).await;
                (load_result.is_err(), load_result.err().map(|e| e.to_string()))
            });
            
            prop_assert!(result.0, "Should reject model > 7B: '{}'", model_path);
            
            if let Some(err_msg) = result.1 {
                prop_assert!(err_msg.contains("exceeds") || err_msg.contains("maximum"),
                    "Error should mention parameter limit: {}", err_msg);
            }
        }

        /// Property 16 (continued): Valid size models should load
        #[test]
        fn model_loading_accepts_valid_size_models(model_path in arb_valid_model_path()) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(async {
                let mut engine = DefaultInferenceEngine::new();
                let load_result = engine.load_model(&model_path).await;
                if load_result.is_ok() {
                    Some(engine.get_model_info().unwrap().parameters)
                } else {
                    None
                }
            });
            
            prop_assert!(result.is_some(), "Should accept model <= 7B: '{}'", model_path);
            prop_assert!(result.unwrap() <= MAX_PARAMETERS,
                "Loaded model should have <= 7B params");
        }

        /// Property 18: Inference timeout compliance
        /// Inference should complete within the timeout period for normal inputs
        #[test]
        fn inference_completes_within_timeout(input in arb_valid_input()) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(async {
                let mut engine = DefaultInferenceEngine::new();
                engine.load_model("tinyllama-1.1b").await.unwrap();
                
                let start = std::time::Instant::now();
                let infer_result = engine.infer(&input).await;
                let elapsed = start.elapsed();
                
                (
                    infer_result.is_ok(),
                    elapsed,
                    infer_result.map(|r| r.tokens_processed).unwrap_or(0)
                )
            });
            
            prop_assert!(result.0, "Inference should succeed");
            prop_assert!(result.1 < Duration::from_secs(DEFAULT_TIMEOUT_SECS),
                "Inference should complete within timeout: {:?}", result.1);
            prop_assert!(result.2 > 0, "Should process some tokens");
        }
    }

    /// Property 3 (continued): Invalid paths should fail
    #[tokio::test]
    #[serial]
    async fn model_loading_fails_for_empty_path() {
        let mut engine = DefaultInferenceEngine::new();
        
        let result = engine.load_model("").await;
        
        assert!(result.is_err(), "Should fail for empty path");
        assert!(!engine.is_model_loaded(), "Model should not be loaded on failure");
    }

    /// Property 18 (continued): Empty input should fail validation
    #[tokio::test]
    #[serial]
    async fn inference_fails_for_empty_input() {
        let mut engine = DefaultInferenceEngine::new();
        engine.load_model("tinyllama-1.1b").await.unwrap();
        
        let result = engine.infer("").await;
        
        assert!(result.is_err(), "Should fail for empty input");
    }

    /// Integration test: Model unloading frees resources
    #[tokio::test]
    #[serial]
    async fn test_model_unload_releases_resources() {
        let mut engine = DefaultInferenceEngine::new();
        
        // Load model
        engine.load_model("tinyllama-1.1b").await.unwrap();
        assert!(engine.is_model_loaded());
        assert!(engine.get_model_info().is_some());
        
        // Unload model
        engine.unload_model().await.unwrap();
        assert!(!engine.is_model_loaded());
        assert!(engine.get_model_info().is_none());
        
        // Inference should fail after unload
        let result = engine.infer("test").await;
        assert!(result.is_err(), "Inference should fail after unload");
    }

    /// Integration test: Model reload after unload
    #[tokio::test]
    #[serial]
    async fn test_model_reload_after_unload() {
        let mut engine = DefaultInferenceEngine::new();
        
        // First load
        engine.load_model("phi-2").await.unwrap();
        let info1 = engine.get_model_info().unwrap();
        
        // Unload
        engine.unload_model().await.unwrap();
        
        // Reload with different model
        engine.load_model("tinyllama-1.1b").await.unwrap();
        let info2 = engine.get_model_info().unwrap();
        
        assert_ne!(info1.name, info2.name, "Different model should be loaded");
        assert_eq!(info2.parameters, 1_100_000_000);
    }

    /// Integration test: Custom timeout configuration
    #[tokio::test]
    #[serial]
    async fn test_custom_timeout_configuration() {
        let mut engine = DefaultInferenceEngine::with_config(60, 10_000_000_000);
        
        // Should accept 8B model with higher limit
        engine.load_model("llama-8b").await.unwrap();
        assert!(engine.is_model_loaded());
        
        let info = engine.get_model_info().unwrap();
        assert_eq!(info.parameters, 8_000_000_000);
    }

    /// Integration test: Model info accuracy
    #[tokio::test]
    #[serial]
    async fn test_model_info_accuracy() {
        let mut engine = DefaultInferenceEngine::new();
        
        engine.load_model("mistral-7b-instruct-q4_0").await.unwrap();
        let info = engine.get_model_info().unwrap();
        
        assert_eq!(info.parameters, 7_000_000_000);
        assert_eq!(info.quantization, Some("Q4_0".to_string()));
        assert!(info.supported_tasks.contains(&"text-generation".to_string()));
    }

    /// Integration test: Inference result contains expected fields
    #[tokio::test]
    #[serial]
    async fn test_inference_result_completeness() {
        let mut engine = DefaultInferenceEngine::new();
        engine.load_model("tinyllama-1.1b").await.unwrap();
        
        let result = engine.infer("Hello, how are you today?").await.unwrap();
        
        assert!(result.tokens_processed > 0);
        assert!(result.duration_ms >= 0);
        assert!(result.memory_used_mb > 0);
        assert!(!result.output.is_empty());
    }

    /// Property test: Multiple sequential inferences
    #[tokio::test]
    #[serial]
    async fn test_multiple_sequential_inferences() {
        let mut engine = DefaultInferenceEngine::new();
        engine.load_model("phi-2").await.unwrap();
        
        for i in 0..5 {
            let input = format!("Test input number {} for inference", i);
            let result = engine.infer(&input).await;
            assert!(result.is_ok(), "Inference {} should succeed", i);
        }
    }

    /// Property test: Model size boundary (exactly 7B should work)
    #[tokio::test]
    #[serial]
    async fn test_model_size_boundary() {
        let mut engine = DefaultInferenceEngine::new();
        
        // Exactly 7B should work
        let result = engine.load_model("mistral-7b").await;
        assert!(result.is_ok(), "7B model should be accepted");
        
        let info = engine.get_model_info().unwrap();
        assert_eq!(info.parameters, MAX_PARAMETERS);
    }
}
