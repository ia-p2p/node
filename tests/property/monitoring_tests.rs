//! Property tests for Monitoring and Logging System
//!
//! Implements property tests for:
//! - Task 7.1: Job execution logging (Property 21)
//! - Task 7.2: Network event logging (Property 22)

use proptest::prelude::*;
use mvp_node::monitoring::{DefaultHealthMonitor, MonitoringEvent, HealthStatus};
use serial_test::serial;

/// **Feature: mvp-node, Property 21: Job execution logging**
/// **Feature: mvp-node, Property 22: Network event logging**
/// **Feature: mvp-node, Property 23: Error logging completeness**
/// **Feature: mvp-node, Property 24: Health status reporting**
/// **Feature: mvp-node, Property 25: Metrics exposure**
#[cfg(test)]
mod monitoring_property_tests {
    use super::*;

    // Generator for job IDs
    fn arb_job_id() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9_-]{8,32}".prop_map(|s| s)
    }

    // Generator for peer IDs
    fn arb_peer_id() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9]{16,52}".prop_map(|s| format!("12D3KooW{}", s))
    }

    // Generator for duration in milliseconds
    fn arb_duration_ms() -> impl Strategy<Value = u64> {
        1u64..30000u64
    }

    // Generator for token count
    fn arb_tokens() -> impl Strategy<Value = u64> {
        1u64..10000u64
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(30))]

        /// Property 21: Job execution logging
        /// For any job processed, execution details should be logged
        #[test]
        fn job_metrics_are_recorded(
            job_id in arb_job_id(),
            duration_ms in arb_duration_ms(),
            tokens in arb_tokens()
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(async {
                let monitor = DefaultHealthMonitor::new();
                
                // Record job events
                monitor.record_job_submitted().await;
                monitor.log_event(MonitoringEvent::JobSubmitted {
                    job_id: job_id.clone(),
                    model: "test-model".to_string(),
                });
                
                monitor.log_event(MonitoringEvent::JobStarted {
                    job_id: job_id.clone(),
                });
                
                monitor.record_job_completed(duration_ms, tokens).await;
                monitor.log_event(MonitoringEvent::JobCompleted {
                    job_id: job_id.clone(),
                    duration_ms,
                    tokens,
                });
                
                // Verify metrics
                let metrics = monitor.get_job_metrics().await;
                (metrics.jobs_submitted, metrics.jobs_completed, metrics.total_tokens_processed, tokens)
            });
            
            prop_assert_eq!(result.0, 1, "Should record submission");
            prop_assert_eq!(result.1, 1, "Should record completion");
            prop_assert_eq!(result.2, result.3, "Should record tokens");
        }

        /// Property 21 (continued): Failed job metrics
        #[test]
        fn failed_job_metrics_are_recorded(job_id in arb_job_id()) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(async {
                let monitor = DefaultHealthMonitor::new();
                
                monitor.record_job_submitted().await;
                monitor.record_job_failed().await;
                monitor.log_event(MonitoringEvent::JobFailed {
                    job_id,
                    error: "Test error".to_string(),
                    duration_ms: 100,
                });
                
                let metrics = monitor.get_job_metrics().await;
                (metrics.jobs_submitted, metrics.jobs_failed)
            });
            
            prop_assert_eq!(result.0, 1);
            prop_assert_eq!(result.1, 1);
        }

        /// Property 22: Network event logging
        /// For any network event, it should be logged with relevant details
        #[test]
        fn network_events_are_logged(peer_id in arb_peer_id()) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(async {
                let monitor = DefaultHealthMonitor::new();
                
                // Record connection
                monitor.record_peer_connected().await;
                monitor.log_event(MonitoringEvent::PeerConnected {
                    peer_id: peer_id.clone(),
                });
                
                // Record message
                monitor.record_message_sent(100).await;
                monitor.log_event(MonitoringEvent::MessageSent {
                    topic: "test-topic".to_string(),
                    size_bytes: 100,
                });
                
                // Record disconnection
                monitor.record_peer_disconnected().await;
                monitor.log_event(MonitoringEvent::PeerDisconnected {
                    peer_id,
                    reason: Some("Test disconnect".to_string()),
                });
                
                let metrics = monitor.get_network_metrics().await;
                (metrics.peer_connections, metrics.peer_disconnections, metrics.messages_sent, metrics.bytes_sent)
            });
            
            prop_assert_eq!(result.0, 1);
            prop_assert_eq!(result.1, 1);
            prop_assert_eq!(result.2, 1);
            prop_assert_eq!(result.3, 100);
        }

        /// Property 24: Health status reporting
        /// Health status should always be available
        #[test]
        fn health_status_always_available(
            jobs in 0u64..100u64,
            errors in 0u64..50u64
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(async {
                let monitor = DefaultHealthMonitor::new();
                
                // Record some jobs
                for _ in 0..jobs {
                    monitor.record_job_submitted().await;
                }
                
                // Record some errors
                for i in 0..errors {
                    monitor.record_error("test", &format!("Error {}", i)).await;
                }
                
                let status = monitor.get_full_health_status().await;
                (status.uptime_seconds, status.error_count, errors)
            });
            
            prop_assert!(result.0 >= 0);
            prop_assert_eq!(result.1, result.2);
        }

        /// Property 25: Metrics exposure
        /// All metrics should be accessible
        #[test]
        fn all_metrics_accessible(
            submitted in 1u64..50u64,
            completed in 1u64..50u64
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(async {
                let monitor = DefaultHealthMonitor::new();
                
                for _ in 0..submitted {
                    monitor.record_job_submitted().await;
                }
                for _ in 0..completed {
                    monitor.record_job_completed(100, 50).await;
                }
                
                let metrics = monitor.get_metrics();
                (
                    metrics.contains_key("jobs_submitted"),
                    metrics.contains_key("jobs_completed"),
                    metrics.contains_key("uptime_seconds"),
                    *metrics.get("jobs_submitted").unwrap() as u64,
                    *metrics.get("jobs_completed").unwrap() as u64,
                    submitted,
                    completed,
                )
            });
            
            prop_assert!(result.0);
            prop_assert!(result.1);
            prop_assert!(result.2);
            prop_assert_eq!(result.3, result.5);
            prop_assert_eq!(result.4, result.6);
        }
    }

    /// Property 23: Error logging completeness
    #[tokio::test]
    #[serial]
    async fn test_error_logging_completeness() {
        let monitor = DefaultHealthMonitor::new();
        
        // Record various errors
        monitor.record_error("network", "Connection timeout").await;
        monitor.record_error("inference", "Model not found").await;
        monitor.record_error("executor", "Queue overflow").await;
        
        let status = monitor.get_full_health_status().await;
        
        assert_eq!(status.error_count, 3);
        assert!(status.last_error.is_some());
        // Last error should be the most recent
        assert!(status.last_error.unwrap().contains("Queue overflow"));
    }

    /// Property 24: Health check accuracy
    #[tokio::test]
    #[serial]
    async fn test_health_check_accuracy() {
        let monitor = DefaultHealthMonitor::new();
        
        // Initially healthy
        assert!(monitor.is_healthy());
        
        // Record errors but still under threshold
        for i in 0..50 {
            monitor.record_error("test", &format!("Error {}", i)).await;
        }
        assert!(monitor.is_healthy(), "Should still be healthy with <100 errors");
        
        // Record more errors to exceed threshold
        for i in 50..100 {
            monitor.record_error("test", &format!("Error {}", i)).await;
        }
        assert!(!monitor.is_healthy(), "Should be unhealthy with >=100 errors");
    }

    /// Property 25: Metrics include inference data
    #[tokio::test]
    #[serial]
    async fn test_inference_metrics_included() {
        let monitor = DefaultHealthMonitor::new();
        
        monitor.record_inference_completed(500).await;
        monitor.record_inference_completed(300).await;
        monitor.record_inference_failed().await;
        
        let inference = monitor.get_inference_metrics().await;
        
        assert_eq!(inference.inferences_executed, 2);
        assert_eq!(inference.inferences_failed, 1);
        assert_eq!(inference.total_inference_time_ms, 800);
        assert_eq!(inference.average_inference_time_ms, 400.0);
    }

    /// Integration test: Full monitoring workflow
    #[tokio::test]
    #[serial]
    async fn test_full_monitoring_workflow() {
        let monitor = DefaultHealthMonitor::new();
        
        // Simulate node startup
        monitor.log_event(MonitoringEvent::NodeStarted {
            node_id: "test-node-1".to_string(),
        });
        
        // Simulate model loading
        monitor.set_model_loaded(true).await;
        monitor.log_event(MonitoringEvent::ModelLoaded {
            model_name: "test-model".to_string(),
            parameters: 1_000_000_000,
        });
        
        // Simulate network connection
        monitor.set_network_connected(true).await;
        monitor.record_peer_connected().await;
        monitor.log_event(MonitoringEvent::PeerConnected {
            peer_id: "peer-1".to_string(),
        });
        
        // Simulate job processing
        monitor.record_job_submitted().await;
        monitor.log_event(MonitoringEvent::JobSubmitted {
            job_id: "job-1".to_string(),
            model: "test-model".to_string(),
        });
        
        monitor.record_job_completed(250, 100).await;
        monitor.log_event(MonitoringEvent::JobCompleted {
            job_id: "job-1".to_string(),
            duration_ms: 250,
            tokens: 100,
        });
        
        // Verify full status
        let status = monitor.get_full_health_status().await;
        assert!(status.is_healthy);
        assert!(status.model_loaded);
        assert!(status.network_connected);
        
        let job_metrics = monitor.get_job_metrics().await;
        assert_eq!(job_metrics.jobs_submitted, 1);
        assert_eq!(job_metrics.jobs_completed, 1);
        
        let network_metrics = monitor.get_network_metrics().await;
        assert_eq!(network_metrics.active_peers, 1);
    }

    /// Test custom metrics
    #[tokio::test]
    #[serial]
    async fn test_custom_metrics() {
        let monitor = DefaultHealthMonitor::new();
        
        monitor.record_metric("custom_metric_1", 42.0);
        monitor.record_metric("custom_metric_2", 3.14);
        
        let metrics = monitor.get_metrics();
        
        assert_eq!(*metrics.get("custom_metric_1").unwrap(), 42.0);
        assert_eq!(*metrics.get("custom_metric_2").unwrap(), 3.14);
    }

    /// Test uptime tracking
    #[tokio::test]
    #[serial]
    async fn test_uptime_tracking() {
        let monitor = DefaultHealthMonitor::new();
        
        // Wait a small amount
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        let uptime = monitor.get_uptime_seconds();
        // Should be 0 or 1 second (depending on timing)
        assert!(uptime <= 2, "Uptime should be small");
    }
}

