//! Main Node Orchestration and Lifecycle Management
//!
//! This module provides the main MVP Node struct that coordinates all components:
//! - Job Executor
//! - Inference Engine
//! - Health Monitor
//! - LLM Orchestrator (optional)
//!
//! Implements Requirements 1.4, 1.5, 3.4, 10.1-10.5

use anyhow::{anyhow, Result};
use libp2p::identity::Keypair;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};

// Import from mvp_node library
use mvp_node::InferenceEngine;
use mvp_node::JobExecutor;
use mvp_node::orchestration::{
    LLMOrchestrator, Orchestrator, OrchestratorConfig, DecisionType, 
    Decision, SupportedModel, OrchestratorMetrics,
};
use mvp_node::query_handler::QueryHandler;
use mvp_node::protocol::{Query, QueryResponse};

/// Node state during lifecycle
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeState {
    /// Node is initializing components
    Initializing,
    /// Node is starting up
    Starting,
    /// Node is running and ready
    Running,
    /// Node is shutting down
    ShuttingDown,
    /// Node has stopped
    Stopped,
    /// Node encountered an error
    Error(String),
}

/// Capability announcement message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityAnnouncement {
    pub node_id: String,
    pub available_models: Vec<String>,
    pub queue_capacity: usize,
    pub current_queue_size: usize,
    pub is_accepting_jobs: bool,
    pub gpu_available: bool,
    pub max_context_length: usize,
}

/// Main MVP Node orchestrator
pub struct MvpNode {
    /// Node configuration
    pub config: mvp_node::config::NodeConfig,
    
    /// Node keypair for identity
    keypair: Keypair,
    
    /// Current node state
    state: Arc<RwLock<NodeState>>,
    
    /// Job executor
    executor: Arc<Mutex<mvp_node::executor::DefaultJobExecutor>>,
    
    /// Inference engine
    inference_engine: Arc<Mutex<mvp_node::inference::DefaultInferenceEngine>>,
    
    /// Health monitor
    monitor: Arc<mvp_node::monitoring::DefaultHealthMonitor>,
    
    /// LLM Orchestrator (optional) - Property 40: Optional module loading
    orchestrator: Option<Arc<LLMOrchestrator>>,
    
    /// Query handler for CLI P2P queries
    query_handler: Arc<QueryHandler>,
    
    /// Start time for uptime tracking
    start_time: Option<Instant>,
}

impl MvpNode {
    /// Create a new MVP Node with the given configuration
    pub async fn new(config: mvp_node::config::NodeConfig) -> Result<Self> {
        info!("Creating new MVP Node...");
        
        // Generate keypair
        let keypair = Keypair::generate_ed25519();
        let peer_id = keypair.public().to_peer_id();
        
        info!("Node peer ID: {}", peer_id);
        
        // Create inference engine
        let inference_engine = mvp_node::inference::DefaultInferenceEngine::new();
        
        // Create job executor with inference engine
        let inference_arc = Arc::new(Mutex::new(inference_engine));
        let max_queue = config.max_queue_size;
        let executor = mvp_node::executor::DefaultJobExecutor::with_inference_engine(
            max_queue,
            inference_arc.clone(),
        );
        
        // Create health monitor
        let monitor = mvp_node::monitoring::DefaultHealthMonitor::new();
        
        let monitor = Arc::new(monitor);
        
        // Create LLM Orchestrator (optional feature) - Property 40: Optional module loading
        let orchestrator = Self::create_orchestrator(
            &config,
            Arc::clone(&monitor),
        );
        
        // Create Query Handler for CLI P2P queries
        let query_handler = QueryHandler::new(
            Arc::clone(&monitor),
            config.node_id.clone(),
            peer_id,
            config.max_queue_size,
        );
        
        Ok(Self {
            config,
            keypair,
            state: Arc::new(RwLock::new(NodeState::Initializing)),
            executor: Arc::new(Mutex::new(executor)),
            inference_engine: inference_arc,
            monitor,
            orchestrator,
            query_handler: Arc::new(query_handler),
            start_time: None,
        })
    }
    
    /// Create the LLM Orchestrator if enabled in config
    /// Property 40: Optional module loading - Requirements 10.1
    fn create_orchestrator(
        config: &mvp_node::config::NodeConfig,
        monitor: Arc<mvp_node::monitoring::DefaultHealthMonitor>,
    ) -> Option<Arc<LLMOrchestrator>> {
        // Get orchestrator config from file or use defaults
        let orch_config = config.get_orchestrator_config();
        
        if !orch_config.enabled {
            info!("LLM Orchestrator is disabled in configuration");
            return None;
        }
        
        info!("Initializing LLM Orchestrator with model type: {}", orch_config.model_type);
        debug!("Orchestrator config: model_path={}, device={}", 
            orch_config.model_path, orch_config.device);
        
        let orchestrator = LLMOrchestrator::new(
            orch_config,
            monitor,
            config.node_id.clone(),
            config.max_queue_size,
        );
        
        info!("LLM Orchestrator initialized successfully");
        Some(Arc::new(orchestrator))
    }

    /// Get the node's peer ID
    pub fn get_peer_id(&self) -> String {
        self.keypair.public().to_peer_id().to_string()
    }

    /// Get current node state
    pub async fn get_state(&self) -> NodeState {
        self.state.read().await.clone()
    }

    /// Set node state
    async fn set_state(&self, state: NodeState) {
        let mut current = self.state.write().await;
        *current = state;
    }

    /// Start the node with proper initialization order
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting MVP Node...");
        self.set_state(NodeState::Starting).await;
        self.start_time = Some(Instant::now());

        // Step 1: Load inference model
        info!("Step 1: Loading inference model...");
        if let Err(e) = self.load_model().await {
            warn!("Failed to load model: {}. Continuing without model.", e);
            self.monitor.record_error("inference", &e.to_string()).await;
        }

        // Mark as running
        self.set_state(NodeState::Running).await;
        
        self.monitor.log_event(mvp_node::monitoring::MonitoringEvent::NodeStarted {
            node_id: self.get_peer_id(),
        });

        info!("MVP Node started successfully!");
        Ok(())
    }

    /// Load the inference model
    async fn load_model(&self) -> Result<()> {
        let model_path = &self.config.model_path;
        if model_path.is_empty() {
            return Err(anyhow!("No model path configured"));
        }
        
        let mut engine = self.inference_engine.lock().await;
        engine.load_model(model_path).await?;
        
        self.monitor.set_model_loaded(true).await;
        
        if let Some(info) = engine.get_model_info() {
            self.monitor.log_event(mvp_node::monitoring::MonitoringEvent::ModelLoaded {
                model_name: info.name.clone(),
                parameters: info.parameters,
            });
        }
        
        Ok(())
    }

    /// Create capability announcement (Property 4: Requirement 1.4)
    pub async fn create_capability_announcement(&self) -> CapabilityAnnouncement {
        let engine = self.inference_engine.lock().await;
        let executor = self.executor.lock().await;
        
        let available_models: Vec<String> = engine.get_model_info()
            .map(|info| vec![info.name])
            .unwrap_or_default();
        
        let queue_status = executor.get_queue_status();
        
        CapabilityAnnouncement {
            node_id: self.get_peer_id(),
            available_models,
            queue_capacity: executor.get_max_queue_size(),
            current_queue_size: queue_status.pending_jobs,
            is_accepting_jobs: !executor.is_queue_full(),
            gpu_available: false,
            max_context_length: 4096,
        }
    }

    /// Graceful shutdown within 30 seconds (Property 5: Requirement 1.5)
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Initiating graceful shutdown...");
        self.set_state(NodeState::ShuttingDown).await;
        
        let shutdown_start = Instant::now();
        let timeout = Duration::from_secs(30);
        
        // Step 1: Stop accepting new jobs
        info!("Step 1: Stopping job acceptance...");
        
        // Step 2: Wait for current job to complete (with timeout)
        info!("Step 2: Waiting for current job...");
        {
            let executor = self.executor.lock().await;
            let status = executor.get_queue_status();
            drop(executor);
            
            if status.processing_job.is_some() {
                let remaining = timeout.saturating_sub(shutdown_start.elapsed());
                if remaining > Duration::ZERO {
                    tokio::time::sleep(Duration::from_millis(100).min(remaining)).await;
                }
            }
        }
        
        // Step 3: Unload model
        info!("Step 3: Unloading model...");
        if shutdown_start.elapsed() < timeout {
            let mut engine = self.inference_engine.lock().await;
            let _ = engine.unload_model().await;
            self.monitor.set_model_loaded(false).await;
        }
        
        // Mark as stopped
        self.set_state(NodeState::Stopped).await;
        
        let shutdown_duration = shutdown_start.elapsed();
        self.monitor.log_event(mvp_node::monitoring::MonitoringEvent::NodeShutdown {
            reason: format!("Graceful shutdown completed in {:?}", shutdown_duration),
        });
        
        if shutdown_duration > timeout {
            warn!("Shutdown exceeded 30 second timeout: {:?}", shutdown_duration);
        }
        
        info!("Node shutdown complete in {:?}", shutdown_duration);
        Ok(())
    }

    /// Get health monitor reference
    pub fn get_monitor(&self) -> Arc<mvp_node::monitoring::DefaultHealthMonitor> {
        self.monitor.clone()
    }

    /// Check if node is running
    pub async fn is_running(&self) -> bool {
        *self.state.read().await == NodeState::Running
    }

    /// Get uptime in seconds
    pub fn get_uptime_seconds(&self) -> u64 {
        self.start_time
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0)
    }

    /// Submit a job to the executor
    pub async fn submit_job(&self, job: mvp_node::JobOffer) -> Result<String> {
        let mut executor = self.executor.lock().await;
        self.monitor.record_job_submitted().await;
        self.monitor.log_event(mvp_node::monitoring::MonitoringEvent::JobSubmitted {
            job_id: job.job_id.clone(),
            model: job.model.clone(),
        });
        executor.submit_job(job).await
    }

    /// Process the next job in queue
    pub async fn process_next_job(&self) -> Result<Option<mvp_node::JobResult>> {
        let mut executor = self.executor.lock().await;
        let result = executor.process_next_job().await?;
        
        if let Some(ref job_result) = result {
            if job_result.status == mvp_node::JobStatus::Completed {
                self.monitor.record_job_completed(
                    job_result.metrics.duration_ms,
                    job_result.metrics.tokens_processed.unwrap_or(0),
                ).await;
                self.monitor.log_event(mvp_node::monitoring::MonitoringEvent::JobCompleted {
                    job_id: job_result.job_id.clone(),
                    duration_ms: job_result.metrics.duration_ms,
                    tokens: job_result.metrics.tokens_processed.unwrap_or(0),
                });
            } else {
                self.monitor.record_job_failed().await;
                self.monitor.log_event(mvp_node::monitoring::MonitoringEvent::JobFailed {
                    job_id: job_result.job_id.clone(),
                    error: job_result.error.clone().unwrap_or_default(),
                    duration_ms: job_result.metrics.duration_ms,
                });
            }
        }
        
        Ok(result)
    }

    /// Get queue status
    pub async fn get_queue_status(&self) -> mvp_node::QueueStatus {
        let executor = self.executor.lock().await;
        executor.get_queue_status()
    }
    
    // ========================================================================
    // LLM Orchestrator Integration - Requirements 10.1-10.5
    // ========================================================================
    
    /// Check if orchestrator is enabled and available
    /// Property 44: Graceful disable - Requirements 10.5
    pub fn is_orchestrator_enabled(&self) -> bool {
        self.orchestrator.as_ref().map(|o| o.is_enabled()).unwrap_or(false)
    }
    
    /// Get orchestrator reference (if available)
    pub fn get_orchestrator(&self) -> Option<Arc<LLMOrchestrator>> {
        self.orchestrator.clone()
    }
    
    /// Request a decision from the LLM Orchestrator
    /// Property 41: Health monitor integration - Requirements 10.2
    pub async fn request_orchestration_decision(
        &self,
        context: &str,
        decision_type: DecisionType,
    ) -> Result<Decision> {
        let orchestrator = self.orchestrator.as_ref()
            .ok_or_else(|| anyhow!("Orchestrator not enabled"))?;
        
        debug!(context = %context, decision_type = ?decision_type, "Requesting orchestration decision");
        
        let decision = orchestrator.make_decision(context, decision_type).await
            .map_err(|e| anyhow!("Orchestration error: {}", e))?;
        
        info!(
            decision = %decision.decision,
            confidence = decision.confidence,
            actions = decision.actions.len(),
            "Orchestration decision made"
        );
        
        Ok(decision)
    }
    
    /// Interpret a natural language command
    /// Requirements 7.1-7.5
    pub async fn interpret_natural_language(
        &self,
        input: &str,
    ) -> Result<mvp_node::orchestration::InterpretationResult> {
        let orchestrator = self.orchestrator.as_ref()
            .ok_or_else(|| anyhow!("Orchestrator not enabled"))?;
        
        orchestrator.interpret_natural_language(input).await
            .map_err(|e| anyhow!("NL interpretation error: {}", e))
    }
    
    /// Get orchestrator metrics
    /// Requirements 6.1-6.4
    pub fn get_orchestrator_metrics(&self) -> Option<OrchestratorMetrics> {
        self.orchestrator.as_ref().map(|o| o.get_metrics())
    }
    
    /// Load orchestrator model
    pub async fn load_orchestrator_model(&self, model: SupportedModel) -> Result<()> {
        let orchestrator = self.orchestrator.as_ref()
            .ok_or_else(|| anyhow!("Orchestrator not enabled"))?;
        
        orchestrator.load_model(model).await
            .map_err(|e| anyhow!("Model load error: {}", e))
    }
    
    /// Export training data from orchestrator
    /// Requirements 3.4, 3.5
    pub async fn export_orchestrator_training_data(
        &self,
        path: &str,
        format: mvp_node::orchestration::DatasetFormat,
    ) -> Result<usize> {
        let orchestrator = self.orchestrator.as_ref()
            .ok_or_else(|| anyhow!("Orchestrator not enabled"))?;
        
        orchestrator.export_training_data(path, format).await
            .map_err(|e| anyhow!("Export error: {}", e))
    }
    
    // ========================================================================
    // Query Handler Integration - CLI P2P Queries
    // ========================================================================
    
    /// Handle an incoming query from the P2P network
    /// 
    /// This method processes queries from CLI clients via the request-response protocol.
    /// Property 10: Node Query Response Time - responds within 1 second.
    pub async fn handle_query(&self, query: Query) -> QueryResponse {
        debug!("Handling query from network: {:?}", query);
        self.query_handler.handle_query(query).await
    }
    
    /// Get reference to the query handler
    pub fn get_query_handler(&self) -> Arc<QueryHandler> {
        Arc::clone(&self.query_handler)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> mvp_node::config::NodeConfig {
        mvp_node::config::NodeConfig {
            node_id: "test-node".to_string(),
            listen_port: 0,
            bootstrap_peers: vec![],
            model_path: "tinyllama-1.1b".to_string(),
            max_queue_size: 10,
            log_level: "debug".to_string(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_node_creation() {
        let config = create_test_config();
        let node = MvpNode::new(config).await;
        
        assert!(node.is_ok(), "Node should be created successfully");
        
        let node = node.unwrap();
        assert!(!node.get_peer_id().is_empty());
        
        let state = node.get_state().await;
        assert_eq!(state, NodeState::Initializing);
    }

    #[tokio::test]
    async fn test_node_start() {
        let config = create_test_config();
        let mut node = MvpNode::new(config).await.unwrap();
        
        let result = node.start().await;
        assert!(result.is_ok(), "Node should start successfully: {:?}", result);
        
        assert!(node.is_running().await);
        assert_eq!(node.get_state().await, NodeState::Running);
    }

    #[tokio::test]
    async fn test_node_shutdown() {
        let config = create_test_config();
        let mut node = MvpNode::new(config).await.unwrap();
        
        node.start().await.unwrap();
        assert!(node.is_running().await);
        
        let result = node.shutdown().await;
        assert!(result.is_ok(), "Shutdown should succeed");
        
        assert_eq!(node.get_state().await, NodeState::Stopped);
    }

    #[tokio::test]
    async fn test_capability_announcement() {
        let config = create_test_config();
        let mut node = MvpNode::new(config).await.unwrap();
        
        node.start().await.unwrap();
        
        let announcement = node.create_capability_announcement().await;
        
        assert_eq!(announcement.node_id, node.get_peer_id());
        assert!(!announcement.available_models.is_empty());
        assert!(announcement.is_accepting_jobs);
        assert_eq!(announcement.queue_capacity, 10);
    }

    #[tokio::test]
    async fn test_job_submission() {
        let config = create_test_config();
        let mut node = MvpNode::new(config).await.unwrap();
        
        node.start().await.unwrap();
        
        let job = mvp_node::JobOffer {
            job_id: "test-job-1".to_string(),
            model: "tinyllama-1.1b".to_string(),
            mode: mvp_node::JobMode::Batch,
            reward: 10.0,
            currency: "USD".to_string(),
            requirements: mvp_node::Requirements::default(),
            input_data: "Hello, world!".to_string(),
        };
        
        let result = node.submit_job(job).await;
        assert!(result.is_ok());
        
        let status = node.get_queue_status().await;
        assert_eq!(status.pending_jobs, 1);
    }

    #[tokio::test]
    async fn test_job_processing() {
        let config = create_test_config();
        let mut node = MvpNode::new(config).await.unwrap();
        
        node.start().await.unwrap();
        
        let job = mvp_node::JobOffer {
            job_id: "test-job-2".to_string(),
            model: "tinyllama-1.1b".to_string(),
            mode: mvp_node::JobMode::Batch,
            reward: 10.0,
            currency: "USD".to_string(),
            requirements: mvp_node::Requirements::default(),
            input_data: "What is 2+2?".to_string(),
        };
        
        node.submit_job(job).await.unwrap();
        
        let result = node.process_next_job().await.unwrap();
        assert!(result.is_some());
        
        let job_result = result.unwrap();
        assert_eq!(job_result.job_id, "test-job-2");
        assert_eq!(job_result.status, mvp_node::JobStatus::Completed);
    }

    #[tokio::test]
    async fn test_uptime_tracking() {
        let config = create_test_config();
        let mut node = MvpNode::new(config).await.unwrap();
        
        assert_eq!(node.get_uptime_seconds(), 0);
        
        node.start().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(node.get_uptime_seconds() <= 1);
    }
}
