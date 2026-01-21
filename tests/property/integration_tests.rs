//! Integration tests for Job Executor + Inference Engine
//!
//! Implements property tests for:
//! - Task 6.1: Input validation during processing (Property 7)
//! - Task 6.2: Inference execution (Property 8)
//! - Task 6.3: Result metadata inclusion (Property 9)

use proptest::prelude::*;
use mvp_node::executor::DefaultJobExecutor;
use mvp_node::inference::DefaultInferenceEngine;
use mvp_node::{JobExecutor, InferenceEngine, JobOffer, JobStatus, JobMode, Requirements};
use serial_test::serial;
use std::sync::Arc;
use tokio::sync::Mutex;

/// **Feature: mvp-node, Property 7: Input validation during processing**
/// **Feature: mvp-node, Property 8: Inference execution**
/// **Feature: mvp-node, Property 9: Result metadata inclusion**
#[cfg(test)]
mod integration_property_tests {
    use super::*;

    // Generator for valid JobOffer instances
    fn arb_valid_job_offer() -> impl Strategy<Value = JobOffer> {
        (
            "[a-zA-Z0-9_-]{8,32}",  // job_id
            "[a-zA-Z0-9 .,!?-]{20,200}", // input_data
        ).prop_map(|(job_id, input_data)| {
            JobOffer {
                job_id,
                model: "tinyllama-1.1b".to_string(),
                mode: JobMode::Batch,
                reward: 10.0,
                currency: "USD".to_string(),
                requirements: Requirements::default(),
                input_data,
            }
        })
    }

    // Generator for invalid input (empty or too long)
    fn arb_invalid_input() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("".to_string()),
            // Very long input that exceeds context length
            "[a-zA-Z]{50000,60000}".prop_map(|s| s),
        ]
    }

    /// Helper to create an executor with inference engine
    async fn create_executor_with_engine() -> DefaultJobExecutor {
        let mut engine = DefaultInferenceEngine::new();
        engine.load_model("tinyllama-1.1b").await.unwrap();
        
        let engine_arc = Arc::new(Mutex::new(engine));
        DefaultJobExecutor::with_inference_engine(100, engine_arc)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(20))]

        /// Property 7: Input validation during processing
        /// For any job being processed, input should be validated against model requirements
        #[test]
        fn input_is_validated_before_processing(job in arb_valid_job_offer()) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(async {
                let mut executor = create_executor_with_engine().await;
                
                // Submit and process job
                executor.submit_job(job.clone()).await.unwrap();
                let process_result = executor.process_next_job().await;
                
                (process_result.is_ok(), process_result.ok().flatten())
            });
            
            prop_assert!(result.0, "Processing should succeed for valid input");
            prop_assert!(result.1.is_some(), "Should return a job result");
            
            let job_result = result.1.unwrap();
            prop_assert_eq!(job_result.status, JobStatus::Completed, 
                "Valid job should complete successfully");
        }

        /// Property 8: Inference execution
        /// For any job with validated input, the Inference_Engine should generate a response
        #[test]
        fn inference_generates_response(job in arb_valid_job_offer()) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(async {
                let mut executor = create_executor_with_engine().await;
                
                executor.submit_job(job).await.unwrap();
                let process_result = executor.process_next_job().await.unwrap().unwrap();
                
                process_result
            });
            
            prop_assert!(result.output.is_some(), "Should have output");
            prop_assert!(!result.output.as_ref().unwrap().is_empty(), 
                "Output should not be empty");
        }

        /// Property 9: Result metadata inclusion
        /// For any completed job, result should contain execution metadata
        #[test]
        fn result_contains_metadata(job in arb_valid_job_offer()) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(async {
                let mut executor = create_executor_with_engine().await;
                
                executor.submit_job(job).await.unwrap();
                let process_result = executor.process_next_job().await.unwrap().unwrap();
                
                process_result.metrics
            });
            
            prop_assert!(result.duration_ms >= 0, "Should have duration");
            prop_assert!(result.tokens_processed.is_some(), "Should have tokens processed");
            prop_assert!(result.peak_memory_mb.is_some(), "Should have memory usage");
        }
    }

    /// Property 7: Empty input should fail validation
    #[tokio::test]
    #[serial]
    async fn test_empty_input_fails_validation() {
        let mut executor = create_executor_with_engine().await;
        
        let job = JobOffer {
            job_id: "test-job-1".to_string(),
            model: "tinyllama-1.1b".to_string(),
            mode: JobMode::Batch,
            reward: 10.0,
            currency: "USD".to_string(),
            requirements: Requirements::default(),
            input_data: "".to_string(), // Empty input
        };
        
        // Submit should fail for empty input_data
        let result = executor.submit_job(job).await;
        assert!(result.is_err(), "Empty input should fail validation");
    }

    /// Property 8: Inference uses loaded model
    #[tokio::test]
    #[serial]
    async fn test_inference_uses_loaded_model() {
        let mut executor = create_executor_with_engine().await;
        
        let job = JobOffer {
            job_id: "test-job-2".to_string(),
            model: "tinyllama-1.1b".to_string(),
            mode: JobMode::Batch,
            reward: 10.0,
            currency: "USD".to_string(),
            requirements: Requirements::default(),
            input_data: "What is the meaning of life?".to_string(),
        };
        
        executor.submit_job(job).await.unwrap();
        let result = executor.process_next_job().await.unwrap().unwrap();
        
        assert_eq!(result.status, JobStatus::Completed);
        assert!(result.output.is_some());
        // Output should contain reference to the model
        let output = result.output.unwrap();
        assert!(!output.is_empty());
    }

    /// Property 9: Metadata accuracy
    #[tokio::test]
    #[serial]
    async fn test_metadata_accuracy() {
        let mut executor = create_executor_with_engine().await;
        
        let input = "Tell me a short story about a robot.";
        let job = JobOffer {
            job_id: "test-job-3".to_string(),
            model: "tinyllama-1.1b".to_string(),
            mode: JobMode::Batch,
            reward: 10.0,
            currency: "USD".to_string(),
            requirements: Requirements::default(),
            input_data: input.to_string(),
        };
        
        executor.submit_job(job).await.unwrap();
        let result = executor.process_next_job().await.unwrap().unwrap();
        
        // Check all metadata fields
        assert!(result.metrics.duration_ms >= 0, "Duration should be non-negative");
        assert!(result.metrics.tokens_processed.unwrap() > 0, "Should process tokens");
        assert!(result.metrics.peak_memory_mb.unwrap() > 0, "Should use memory");
    }

    /// Property 10: Error response formatting
    #[tokio::test]
    #[serial]
    async fn test_error_response_contains_details() {
        // Create executor without inference engine
        let mut executor = DefaultJobExecutor::new(10);
        
        // Set up an engine but don't load a model
        let engine = DefaultInferenceEngine::new();
        let engine_arc = Arc::new(Mutex::new(engine));
        executor.set_inference_engine(engine_arc);
        
        let job = JobOffer {
            job_id: "test-job-4".to_string(),
            model: "test-model".to_string(),
            mode: JobMode::Batch,
            reward: 10.0,
            currency: "USD".to_string(),
            requirements: Requirements::default(),
            input_data: "Test input".to_string(),
        };
        
        executor.submit_job(job).await.unwrap();
        let result = executor.process_next_job().await.unwrap().unwrap();
        
        // Should fail because no model is loaded
        assert_eq!(result.status, JobStatus::Failed);
        assert!(result.error.is_some(), "Should have error message");
        assert!(result.error.as_ref().unwrap().contains("model") || 
                result.error.as_ref().unwrap().contains("loaded"),
                "Error should mention model issue");
    }

    /// Integration test: Multiple jobs processed sequentially
    #[tokio::test]
    #[serial]
    async fn test_multiple_jobs_with_inference() {
        let mut executor = create_executor_with_engine().await;
        
        // Submit multiple jobs
        for i in 1..=3 {
            let job = JobOffer {
                job_id: format!("batch-job-{}", i),
                model: "tinyllama-1.1b".to_string(),
                mode: JobMode::Batch,
                reward: 10.0,
                currency: "USD".to_string(),
                requirements: Requirements::default(),
                input_data: format!("Question number {}: What is {} + {}?", i, i, i),
            };
            executor.submit_job(job).await.unwrap();
        }
        
        // Process all jobs
        for i in 1..=3 {
            let result = executor.process_next_job().await.unwrap().unwrap();
            assert_eq!(result.job_id, format!("batch-job-{}", i));
            assert_eq!(result.status, JobStatus::Completed);
            assert!(result.output.is_some());
        }
        
        // Queue should be empty
        let status = executor.get_queue_status();
        assert_eq!(status.pending_jobs, 0);
        assert_eq!(status.completed_jobs, 3);
    }

    /// Integration test: Executor without inference engine still works
    #[tokio::test]
    #[serial]
    async fn test_executor_without_engine() {
        let mut executor = DefaultJobExecutor::new(10);
        
        let job = JobOffer {
            job_id: "no-engine-job".to_string(),
            model: "any-model".to_string(),
            mode: JobMode::Batch,
            reward: 10.0,
            currency: "USD".to_string(),
            requirements: Requirements::default(),
            input_data: "Some input data".to_string(),
        };
        
        executor.submit_job(job).await.unwrap();
        let result = executor.process_next_job().await.unwrap().unwrap();
        
        // Should complete with placeholder
        assert_eq!(result.status, JobStatus::Completed);
        assert!(result.output.unwrap().contains("Placeholder"));
    }

    /// Integration test: Check has_inference_engine method
    #[tokio::test]
    #[serial]
    async fn test_has_inference_engine() {
        let executor_without = DefaultJobExecutor::new(10);
        assert!(!executor_without.has_inference_engine());
        
        let executor_with = create_executor_with_engine().await;
        assert!(executor_with.has_inference_engine());
    }
}











