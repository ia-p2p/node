use proptest::prelude::*;
use mvp_node::executor::DefaultJobExecutor;
use mvp_node::{JobExecutor, JobOffer, JobMode, Requirements};
use serial_test::serial;

/// **Feature: mvp-node, Property 6: Valid job acceptance**
/// 
/// For any valid inference job submitted to the MVP_Node, the job should be 
/// accepted and added to the Job_Queue.
/// 
/// **Validates: Requirements 2.1**
#[cfg(test)]
mod executor_property_tests {
    use super::*;

    // Generator for valid JobOffer instances
    fn arb_job_offer() -> impl Strategy<Value = JobOffer> {
        (
            "[a-zA-Z0-9_-]{8,32}",  // job_id
            "[a-zA-Z0-9_/-]{1,50}",  // model
            prop_oneof![
                Just(JobMode::Batch),
                Just(JobMode::Session)
            ],
            1.0f64..1000.0f64,       // reward
            "[A-Z]{3}",              // currency (e.g., USD, BTC)
            "[a-zA-Z0-9 .,!?-]{10,100}", // input_data
        ).prop_map(|(job_id, model, mode, reward, currency, input_data)| {
            JobOffer {
                job_id,
                model,
                mode,
                reward,
                currency,
                requirements: Requirements::default(),
                input_data,
            }
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]
        
        /// Property 6: Valid job acceptance
        /// For any valid JobOffer, the executor should accept it
        #[test]
        #[serial]
        fn valid_job_is_accepted(job in arb_job_offer()) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut executor = DefaultJobExecutor::new(100);
                
                let job_id = job.job_id.clone();
                let result = executor.submit_job(job).await;
                
                prop_assert!(result.is_ok(), "Valid job should be accepted: {:?}", result);
                prop_assert_eq!(result.unwrap(), job_id, "Should return correct job ID");
                
                let status = executor.get_queue_status();
                prop_assert_eq!(status.pending_jobs, 1, "Job should be in queue");
                
                Ok(())
            })?
        }
        
        /// Property 6 (continued): Invalid jobs are rejected
        /// Jobs with empty required fields should be rejected
        #[test]
        #[serial]
        fn invalid_job_is_rejected(mut job in arb_job_offer()) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut executor = DefaultJobExecutor::new(100);
                
                // Make job invalid by clearing job_id
                job.job_id = String::new();
                
                let result = executor.submit_job(job).await;
                
                prop_assert!(result.is_err(), "Invalid job should be rejected");
                prop_assert!(result.unwrap_err().to_string().contains("Job ID"), 
                           "Error should mention Job ID");
                
                let status = executor.get_queue_status();
                prop_assert_eq!(status.pending_jobs, 0, "Invalid job should not be queued");
                
                Ok(())
            })?
        }
        
        /// Property 26: Concurrent job queuing
        /// For any set of jobs arriving simultaneously, all should be queued
        #[test]
        #[serial]
        fn concurrent_jobs_are_queued(
            jobs in prop::collection::vec(arb_job_offer(), 2..10)
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut executor = DefaultJobExecutor::new(100);
                let job_count = jobs.len();
                
                // Submit all jobs
                for job in jobs {
                    let result = executor.submit_job(job).await;
                    prop_assert!(result.is_ok(), "Each job should be accepted: {:?}", result);
                }
                
                // Verify all jobs are queued
                let status = executor.get_queue_status();
                prop_assert_eq!(status.pending_jobs, job_count, 
                               "All jobs should be in queue");
                
                Ok(())
            })?
        }
        
        /// Property 27: Fair job scheduling (FIFO order)
        /// Jobs should be processed in the order they were submitted
        #[test]
        #[serial]
        fn jobs_processed_in_fifo_order(
            jobs in prop::collection::vec(arb_job_offer(), 3..5)
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut executor = DefaultJobExecutor::new(100);
                let mut job_ids = Vec::new();
                
                // Submit all jobs
                for job in jobs {
                    job_ids.push(job.job_id.clone());
                    executor.submit_job(job).await.unwrap();
                }
                
                // Process jobs and verify order
                for expected_id in job_ids {
                    let result = executor.process_next_job().await.unwrap();
                    prop_assert!(result.is_some(), "Should have a job to process");
                    
                    let job_result = result.unwrap();
                    prop_assert_eq!(job_result.job_id, expected_id,
                                   "Jobs should be processed in FIFO order");
                }
                
                Ok(())
            })?
        }
        
        /// Property 28: Queue overflow handling
        /// When queue is full, new jobs should be rejected with error
        #[test]
        #[serial]
        fn queue_overflow_rejects_jobs(
            jobs in prop::collection::vec(arb_job_offer(), 5..10)
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // Create executor with small queue
                let queue_size = 3;
                let mut executor = DefaultJobExecutor::new(queue_size);
                
                let mut accepted = 0;
                let mut rejected = 0;
                
                // Try to submit all jobs
                for job in jobs {
                    let result = executor.submit_job(job).await;
                    if result.is_ok() {
                        accepted += 1;
                    } else {
                        rejected += 1;
                        // Verify error message mentions queue is full
                        prop_assert!(result.unwrap_err().to_string().contains("full"),
                                   "Error should mention queue is full");
                    }
                }
                
                // Verify correct number accepted
                prop_assert_eq!(accepted, queue_size, 
                               "Should accept exactly queue_size jobs");
                prop_assert!(rejected > 0, "Should reject overflow jobs");
                
                let status = executor.get_queue_status();
                prop_assert_eq!(status.pending_jobs, queue_size,
                               "Queue should be at max capacity");
                
                Ok(())
            })?
        }
        
        /// Property 29: Queue status reporting
        /// Queue status should accurately reflect current state
        #[test]
        #[serial]
        fn queue_status_is_accurate(
            jobs in prop::collection::vec(arb_job_offer(), 1..10)
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut executor = DefaultJobExecutor::new(100);
                let job_count = jobs.len();
                
                // Initially empty
                let status = executor.get_queue_status();
                prop_assert_eq!(status.pending_jobs, 0, "Should start empty");
                prop_assert_eq!(status.completed_jobs, 0, "No completed jobs yet");
                prop_assert_eq!(status.failed_jobs, 0, "No failed jobs yet");
                
                // Submit jobs
                for job in jobs {
                    executor.submit_job(job).await.unwrap();
                }
                
                let status = executor.get_queue_status();
                prop_assert_eq!(status.pending_jobs, job_count,
                               "Should show all pending jobs");
                
                // Process one job
                executor.process_next_job().await.unwrap();
                
                let status = executor.get_queue_status();
                prop_assert_eq!(status.pending_jobs, job_count - 1,
                               "Should decrease pending count");
                prop_assert_eq!(status.completed_jobs, 1,
                               "Should increase completed count");
                
                Ok(())
            })?
        }
    }

    /// Integration test: Job cancellation
    #[tokio::test]
    #[serial]
    async fn test_job_cancellation() {
        let mut executor = DefaultJobExecutor::new(10);
        
        let job1 = JobOffer {
            job_id: "job1".to_string(),
            model: "test-model".to_string(),
            mode: JobMode::Batch,
            reward: 10.0,
            currency: "USD".to_string(),
            requirements: Requirements::default(),
            input_data: "test input 1".to_string(),
        };
        
        let job2 = JobOffer {
            job_id: "job2".to_string(),
            model: "test-model".to_string(),
            mode: JobMode::Batch,
            reward: 10.0,
            currency: "USD".to_string(),
            requirements: Requirements::default(),
            input_data: "test input 2".to_string(),
        };
        
        executor.submit_job(job1).await.unwrap();
        executor.submit_job(job2).await.unwrap();
        
        assert_eq!(executor.get_queue_status().pending_jobs, 2);
        
        // Cancel job1
        let result = executor.cancel_job("job1").await;
        assert!(result.is_ok(), "Should cancel successfully");
        
        assert_eq!(executor.get_queue_status().pending_jobs, 1);
        
        // Try to cancel non-existent job
        let result = executor.cancel_job("job3").await;
        assert!(result.is_err(), "Should fail for non-existent job");
    }

    /// Integration test: Sequential processing
    #[tokio::test]
    #[serial]
    async fn test_sequential_processing() {
        let mut executor = DefaultJobExecutor::new(10);
        
        for i in 1..=5 {
            let job = JobOffer {
                job_id: format!("job{}", i),
                model: "test-model".to_string(),
                mode: JobMode::Batch,
                reward: 10.0,
                currency: "USD".to_string(),
                requirements: Requirements::default(),
                input_data: format!("test input {}", i),
            };
            executor.submit_job(job).await.unwrap();
        }
        
        assert_eq!(executor.get_queue_status().pending_jobs, 5);
        
        // Process all jobs
        for i in 1..=5 {
            let result = executor.process_next_job().await.unwrap();
            assert!(result.is_some());
            let job_result = result.unwrap();
            assert_eq!(job_result.job_id, format!("job{}", i));
        }
        
        // Queue should be empty
        let result = executor.process_next_job().await.unwrap();
        assert!(result.is_none(), "Queue should be empty");
        
        let status = executor.get_queue_status();
        assert_eq!(status.pending_jobs, 0);
        assert_eq!(status.completed_jobs, 5);
    }

    /// Integration test: Max queue size enforcement
    #[tokio::test]
    #[serial]
    async fn test_max_queue_size() {
        let max_size = 5;
        let mut executor = DefaultJobExecutor::new(max_size);
        
        assert_eq!(executor.get_max_queue_size(), max_size);
        assert!(!executor.is_queue_full());
        
        // Fill the queue
        for i in 1..=max_size {
            let job = JobOffer {
                job_id: format!("job{}", i),
                model: "test-model".to_string(),
                mode: JobMode::Batch,
                reward: 10.0,
                currency: "USD".to_string(),
                requirements: Requirements::default(),
                input_data: format!("test input {}", i),
            };
            executor.submit_job(job).await.unwrap();
        }
        
        assert!(executor.is_queue_full());
        
        // Try to add one more
        let overflow_job = JobOffer {
            job_id: "overflow".to_string(),
            model: "test-model".to_string(),
            mode: JobMode::Batch,
            reward: 10.0,
            currency: "USD".to_string(),
            requirements: Requirements::default(),
            input_data: "overflow input".to_string(),
        };
        
        let result = executor.submit_job(overflow_job).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("full"));
    }
}

