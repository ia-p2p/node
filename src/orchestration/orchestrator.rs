//! Core Orchestrator for the LLM-based decision making
//!
//! This module provides the main orchestrator that coordinates all components
//! to analyze system state and generate validated decisions.

use crate::monitoring::DefaultHealthMonitor;
use crate::orchestration::cache::DecisionCache;
use crate::orchestration::decision_validator::{DecisionValidator, DecisionValidatorTrait, ValidationResult};
use crate::orchestration::error::{OrchestratorError, OrchestratorResult};
use crate::orchestration::llm_backend::{LocalLLMBackend, SupportedModel};
use crate::orchestration::metrics_aggregator::{MetricsAggregator, MetricsAggregatorTrait};
use crate::orchestration::natural_language::{ExecutionResult, NaturalLanguageInterface, NaturalLanguageInterfaceTrait};
use crate::orchestration::prompts::PromptTemplates;
use crate::orchestration::training_collector::{TrainingDataCollector, TrainingDataCollectorTrait};
use crate::orchestration::types::{
    CliCommand, DatasetFormat, Decision, DecisionType, InterpretationResult, LLMState,
    OrchestratorMetrics,
};
use crate::orchestration::{GenerationConfig, OrchestratorConfig};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Trait for the main orchestrator interface
#[async_trait::async_trait]
pub trait Orchestrator: Send + Sync {
    /// Make a decision based on context
    async fn make_decision(
        &self,
        context: &str,
        decision_type: DecisionType,
    ) -> OrchestratorResult<Decision>;
    
    /// Interpret natural language input
    async fn interpret_natural_language(
        &self,
        input: &str,
    ) -> OrchestratorResult<InterpretationResult>;
    
    /// Get current orchestrator metrics
    fn get_metrics(&self) -> OrchestratorMetrics;
    
    /// Export training data
    async fn export_training_data(
        &self,
        path: &str,
        format: DatasetFormat,
    ) -> OrchestratorResult<usize>;
}

/// LLM Orchestrator - Main implementation
pub struct LLMOrchestrator {
    /// LLM backend for inference
    backend: Arc<LocalLLMBackend>,
    /// Metrics aggregator
    metrics_aggregator: Arc<dyn MetricsAggregatorTrait>,
    /// Decision validator
    decision_validator: Arc<DecisionValidator>,
    /// Training data collector
    data_collector: Arc<TrainingDataCollector>,
    /// Natural language interface
    nl_interface: Arc<NaturalLanguageInterface>,
    /// Decision cache
    decision_cache: Arc<DecisionCache>,
    /// Configuration
    config: OrchestratorConfig,
    /// Metrics
    metrics: Arc<RwLock<OrchestratorMetrics>>,
    /// Enabled state
    enabled: bool,
}

impl LLMOrchestrator {
    /// Create a new LLM Orchestrator
    pub fn new(
        config: OrchestratorConfig,
        health_monitor: Arc<DefaultHealthMonitor>,
        node_id: String,
        max_queue_size: usize,
    ) -> Self {
        let backend = Arc::new(LocalLLMBackend::new(&config));
        let metrics_aggregator = Arc::new(MetricsAggregator::new(
            health_monitor,
            node_id,
            max_queue_size,
        ));
        let decision_validator = Arc::new(DecisionValidator::new());
        let data_collector = Arc::new(
            TrainingDataCollector::new()
                .with_auto_export(
                    config.training.auto_export_threshold,
                    config.training.export_path.clone(),
                ),
        );
        let nl_interface = Arc::new(NaturalLanguageInterface::new(Arc::clone(&backend)));
        let decision_cache = Arc::new(DecisionCache::new(
            config.cache.max_entries,
            config.cache.ttl_seconds,
        ));
        
        Self {
            backend,
            metrics_aggregator,
            decision_validator,
            data_collector,
            nl_interface,
            decision_cache,
            enabled: config.enabled,
            config,
            metrics: Arc::new(RwLock::new(OrchestratorMetrics::default())),
        }
    }
    
    /// Create a disabled orchestrator
    pub fn disabled() -> Self {
        Self {
            backend: Arc::new(LocalLLMBackend::new(&OrchestratorConfig::default())),
            metrics_aggregator: Arc::new(
                crate::orchestration::metrics_aggregator::MockMetricsAggregator::with_default_state()
            ),
            decision_validator: Arc::new(DecisionValidator::new()),
            data_collector: Arc::new(TrainingDataCollector::new()),
            nl_interface: Arc::new(NaturalLanguageInterface::without_llm()),
            decision_cache: Arc::new(DecisionCache::disabled()),
            config: OrchestratorConfig::default(),
            metrics: Arc::new(RwLock::new(OrchestratorMetrics::default())),
            enabled: false,
        }
    }
    
    /// Check if orchestrator is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
    
    /// Load a model
    pub async fn load_model(&self, model: SupportedModel) -> OrchestratorResult<()> {
        if !self.enabled {
            return Err(OrchestratorError::Disabled);
        }
        
        self.backend.load_model(model).await
    }
    
    /// Load model from config
    pub async fn load_model_from_config(&self) -> OrchestratorResult<()> {
        let model = SupportedModel::from_str(&self.config.model_type, &self.config.model_path)
            .ok_or_else(|| OrchestratorError::ConfigurationError {
                message: format!("Unknown model type: {}", self.config.model_type),
                field: Some("model_type".to_string()),
            })?;
        
        self.load_model(model).await
    }
    
    /// Check if model is loaded
    pub async fn is_model_loaded(&self) -> bool {
        self.backend.is_model_loaded().await
    }
    
    /// Get generation config for decision type
    fn get_generation_config(&self, decision_type: DecisionType) -> GenerationConfig {
        let mut config = self.config.generation.clone();
        config.temperature = decision_type.recommended_temperature();
        config
    }
    
    /// Generate a decision using LLM
    async fn generate_decision(
        &self,
        state: &LLMState,
        context: &str,
        decision_type: DecisionType,
    ) -> OrchestratorResult<Decision> {
        // Build prompt
        let prompt = PromptTemplates::build_decision_prompt(decision_type, state, context);
        
        // Get appropriate generation config
        let gen_config = self.get_generation_config(decision_type);
        
        // Generate response
        let start = Instant::now();
        let response = self.backend.generate(&prompt, Some(&gen_config)).await?;
        let inference_time = start.elapsed();
        
        debug!(
            inference_ms = inference_time.as_millis() as u64,
            response_len = response.len(),
            "LLM inference completed"
        );
        
        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            let count = metrics.decisions_made + 1;
            metrics.avg_inference_latency_ms = 
                (metrics.avg_inference_latency_ms * metrics.decisions_made as f64 
                 + inference_time.as_millis() as f64) / count as f64;
        }
        
        // Parse response
        let decision = self.decision_validator.validate_json(&response)?;
        
        Ok(decision)
    }
    
    /// Record a decision in training data
    async fn record_decision(
        &self,
        state: &LLMState,
        context: &str,
        decision_type: DecisionType,
        decision: &Decision,
    ) {
        if self.config.training.collect_data {
            let id = self.data_collector
                .record_decision(state, context, decision_type, decision)
                .await;
            
            debug!(decision_id = %id, "Recorded decision for training");
            
            // Update metrics
            {
                let mut metrics = self.metrics.write().await;
                metrics.training_examples_collected += 1;
            }
        }
    }
    
    /// Get fallback decision using heuristics
    fn get_heuristic_decision(
        &self,
        state: &LLMState,
        context: &str,
        decision_type: DecisionType,
    ) -> Decision {
        // Simple heuristics based on state
        let decision_name;
        let reasoning;
        let actions;
        let confidence;
        
        // Check for critical conditions
        if state.context.health == crate::orchestration::types::HealthLevel::Critical {
            decision_name = "restart_component";
            reasoning = "Critical health level detected, component restart recommended";
            actions = vec![crate::orchestration::types::Action::new("restart_component", 9)
                .with_confirmation()];
            confidence = 0.7;
        }
        // Check for overload
        else if state.node.capacity == crate::orchestration::types::CapacityLevel::Overloaded {
            decision_name = "scale_queue";
            reasoning = "Node is overloaded, queue scaling recommended";
            actions = vec![crate::orchestration::types::Action::new("scale_queue", 7)
                .with_param("new_size", serde_json::json!(state.context.max_queue * 2))];
            confidence = 0.6;
        }
        // Check for high partition risk
        else if state.network.partition_risk == crate::orchestration::types::RiskLevel::Critical {
            decision_name = "adjust_gossip";
            reasoning = "High partition risk, adjusting gossip protocol for faster discovery";
            actions = vec![crate::orchestration::types::Action::new("adjust_gossip", 6)
                .with_param("interval_ms", serde_json::json!(500))];
            confidence = 0.6;
        }
        // Default: wait
        else {
            decision_name = "wait";
            reasoning = "No immediate action required based on current state";
            actions = vec![crate::orchestration::types::Action::new("wait", 3)];
            confidence = 0.8;
        }
        
        Decision::new(
            decision_name.to_string(),
            confidence,
            format!("{} (Heuristic fallback for {} decision)", reasoning, decision_type),
            actions,
        )
    }
    
    /// Execute natural language commands
    pub async fn execute_nl_commands(
        &self,
        commands: Vec<CliCommand>,
        confirmed: bool,
    ) -> OrchestratorResult<ExecutionResult> {
        self.nl_interface.execute_with_confirmation(commands, confirmed).await
    }
    
    /// Add feedback to a decision
    pub async fn add_feedback(
        &self,
        decision_id: &str,
        feedback: crate::orchestration::types::Feedback,
    ) -> OrchestratorResult<()> {
        self.data_collector.add_feedback(decision_id, feedback).await
    }
    
    /// Add outcome to a decision
    pub async fn add_outcome(
        &self,
        decision_id: &str,
        outcome: crate::orchestration::types::Outcome,
    ) -> OrchestratorResult<()> {
        self.data_collector.add_outcome(decision_id, outcome.clone()).await?;
        
        // Update success/failure metrics
        {
            let mut metrics = self.metrics.write().await;
            match &outcome {
                crate::orchestration::types::Outcome::Success { .. } => {
                    metrics.decisions_successful += 1;
                }
                crate::orchestration::types::Outcome::Failure { .. } => {
                    metrics.decisions_failed += 1;
                }
                _ => {}
            }
        }
        
        Ok(())
    }
    
    /// Get current LLM state
    pub async fn get_current_state(&self) -> OrchestratorResult<LLMState> {
        self.metrics_aggregator.get_llm_state().await
    }
    
    /// Clear decision cache
    pub async fn clear_cache(&self) {
        self.decision_cache.clear().await;
    }
    
    /// Get cache statistics
    pub async fn get_cache_stats(&self) -> crate::orchestration::cache::CacheStats {
        self.decision_cache.get_stats().await
    }
    
    /// Get training data count
    pub async fn get_training_data_count(&self) -> usize {
        self.data_collector.count().await
    }
}

#[async_trait::async_trait]
impl Orchestrator for LLMOrchestrator {
    async fn make_decision(
        &self,
        context: &str,
        decision_type: DecisionType,
    ) -> OrchestratorResult<Decision> {
        if !self.enabled {
            return Err(OrchestratorError::Disabled);
        }
        
        let start = Instant::now();
        
        // Get current state
        let state = self.metrics_aggregator.get_llm_state().await?;
        
        // Check cache
        if let Some(cached) = self.decision_cache.get(&state, decision_type).await {
            debug!(
                decision = %cached.decision,
                "Cache hit for decision"
            );
            
            // Update metrics
            {
                let mut metrics = self.metrics.write().await;
                metrics.cache_hits += 1;
            }
            
            return Ok(cached);
        }
        
        // Update cache miss metric
        {
            let mut metrics = self.metrics.write().await;
            metrics.cache_misses += 1;
        }
        
        // Generate decision
        let decision = if self.backend.is_model_loaded().await {
            match self.generate_decision(&state, context, decision_type).await {
                Ok(decision) => decision,
                Err(e) => {
                    warn!(error = %e, "LLM generation failed, using heuristic fallback");
                    
                    // Update fallback metric
                    {
                        let mut metrics = self.metrics.write().await;
                        metrics.fallback_activations += 1;
                    }
                    
                    self.get_heuristic_decision(&state, context, decision_type)
                }
            }
        } else {
            debug!("No model loaded, using heuristic fallback");
            
            // Update fallback metric
            {
                let mut metrics = self.metrics.write().await;
                metrics.fallback_activations += 1;
            }
            
            self.get_heuristic_decision(&state, context, decision_type)
        };
        
        // Validate decision
        let validation = self.decision_validator.validate_decision(&decision, decision_type);
        
        let final_decision = match validation {
            ValidationResult::Valid => {
                info!(
                    decision = %decision.decision,
                    confidence = decision.confidence,
                    actions = decision.actions.len(),
                    duration_ms = start.elapsed().as_millis() as u64,
                    "Decision generated and validated"
                );
                
                // Update decisions made metric
                {
                    let mut metrics = self.metrics.write().await;
                    metrics.decisions_made += 1;
                }
                
                decision
            }
            ValidationResult::Invalid { error, .. } => {
                warn!(error = %error, "Decision validation failed, using fallback");
                
                // Update validation failure metric
                {
                    let mut metrics = self.metrics.write().await;
                    metrics.validation_failures += 1;
                    metrics.fallback_activations += 1;
                }
                
                self.decision_validator.get_fallback_decision(context, decision_type)
            }
        };
        
        // Cache the decision
        self.decision_cache
            .put(&state, decision_type, context.to_string(), final_decision.clone())
            .await;
        
        // Record for training
        self.record_decision(&state, context, decision_type, &final_decision).await;
        
        Ok(final_decision)
    }
    
    async fn interpret_natural_language(
        &self,
        input: &str,
    ) -> OrchestratorResult<InterpretationResult> {
        if !self.enabled {
            return Err(OrchestratorError::Disabled);
        }
        
        self.nl_interface.interpret_command(input).await
    }
    
    fn get_metrics(&self) -> OrchestratorMetrics {
        // Use try_read to avoid blocking
        self.metrics
            .try_read()
            .map(|m| m.clone())
            .unwrap_or_default()
    }
    
    async fn export_training_data(
        &self,
        path: &str,
        format: DatasetFormat,
    ) -> OrchestratorResult<usize> {
        self.data_collector.export_dataset(path, format).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitoring::DefaultHealthMonitor;

    fn create_test_orchestrator() -> LLMOrchestrator {
        let config = OrchestratorConfig::default();
        let health_monitor = Arc::new(DefaultHealthMonitor::new());
        
        LLMOrchestrator::new(config, health_monitor, "test-node".to_string(), 100)
    }

    #[tokio::test]
    async fn test_orchestrator_creation() {
        let orchestrator = create_test_orchestrator();
        assert!(orchestrator.is_enabled());
        assert!(!orchestrator.is_model_loaded().await);
    }

    #[tokio::test]
    async fn test_disabled_orchestrator() {
        let orchestrator = LLMOrchestrator::disabled();
        assert!(!orchestrator.is_enabled());
        
        let result = orchestrator.make_decision("test", DecisionType::Infrastructure).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OrchestratorError::Disabled));
    }

    #[tokio::test]
    async fn test_make_decision_with_fallback() {
        let mut config = OrchestratorConfig::default();
        config.enabled = true;
        config.cache.enabled = false; // Disable cache for test
        
        let health_monitor = Arc::new(DefaultHealthMonitor::new());
        let orchestrator = LLMOrchestrator::new(
            config,
            health_monitor,
            "test-node".to_string(),
            100,
        );
        
        // Without model loaded, should use heuristic fallback
        let decision = orchestrator
            .make_decision("test context", DecisionType::Infrastructure)
            .await
            .unwrap();
        
        assert!(!decision.decision.is_empty());
        assert!(decision.confidence >= 0.0 && decision.confidence <= 1.0);
        
        let metrics = orchestrator.get_metrics();
        assert_eq!(metrics.fallback_activations, 1);
    }

    #[tokio::test]
    async fn test_cache_hit() {
        let config = OrchestratorConfig::default();
        let health_monitor = Arc::new(DefaultHealthMonitor::new());
        let orchestrator = LLMOrchestrator::new(
            config,
            health_monitor,
            "test-node".to_string(),
            100,
        );
        
        // First call
        let decision1 = orchestrator
            .make_decision("test context", DecisionType::Infrastructure)
            .await
            .unwrap();
        
        // Second call should hit cache
        let decision2 = orchestrator
            .make_decision("test context", DecisionType::Infrastructure)
            .await
            .unwrap();
        
        // Decisions should be the same
        assert_eq!(decision1.decision, decision2.decision);
        
        let metrics = orchestrator.get_metrics();
        assert!(metrics.cache_hits > 0);
    }

    #[tokio::test]
    async fn test_natural_language_interpretation() {
        let orchestrator = create_test_orchestrator();
        
        let result = orchestrator
            .interpret_natural_language("show status")
            .await
            .unwrap();
        
        assert!(!result.commands.is_empty());
        assert!(!result.intent.is_empty());
    }

    #[tokio::test]
    async fn test_training_data_collection() {
        let mut config = OrchestratorConfig::default();
        config.training.collect_data = true;
        
        let health_monitor = Arc::new(DefaultHealthMonitor::new());
        let orchestrator = LLMOrchestrator::new(
            config,
            health_monitor,
            "test-node".to_string(),
            100,
        );
        
        // Make a decision
        let _ = orchestrator
            .make_decision("test context", DecisionType::Infrastructure)
            .await
            .unwrap();
        
        // Should have collected training data
        assert_eq!(orchestrator.get_training_data_count().await, 1);
        
        let metrics = orchestrator.get_metrics();
        assert_eq!(metrics.training_examples_collected, 1);
    }

    #[tokio::test]
    async fn test_get_current_state() {
        let orchestrator = create_test_orchestrator();
        
        let state = orchestrator.get_current_state().await.unwrap();
        assert_eq!(state.node.id, "test-node");
    }

    #[tokio::test]
    async fn test_clear_cache() {
        let orchestrator = create_test_orchestrator();
        
        // Make a decision to populate cache
        let _ = orchestrator
            .make_decision("test", DecisionType::Infrastructure)
            .await
            .unwrap();
        
        // Clear cache
        orchestrator.clear_cache().await;
        
        // Cache should be empty now
        let stats = orchestrator.get_cache_stats().await;
        // After clearing, hits and misses are reset to default
    }

    #[test]
    fn test_heuristic_decision_critical_health() {
        let orchestrator = create_test_orchestrator();
        
        let state = LLMState::new(
            crate::orchestration::types::NodeState {
                id: "test".to_string(),
                capacity: crate::orchestration::types::CapacityLevel::Medium,
                latency_avg: 100,
                uptime_seconds: 3600,
                error_count: 0,
            },
            crate::orchestration::types::ContextState {
                id: "ctx".to_string(),
                health: crate::orchestration::types::HealthLevel::Critical,
                queue_size: 50,
                max_queue: 100,
                processing_jobs: 5,
            },
            crate::orchestration::types::NetworkState {
                partition_risk: crate::orchestration::types::RiskLevel::Low,
                peers_available: 5,
                active_connections: 3,
                message_latency_avg: Some(50),
            },
            crate::orchestration::types::ModelState {
                loaded: true,
                avg_inference_ms: 200,
                memory_usage_mb: 4000,
                recent_failures: 0,
            },
        );
        
        let decision = orchestrator.get_heuristic_decision(
            &state,
            "test",
            DecisionType::Infrastructure,
        );
        
        assert_eq!(decision.decision, "restart_component");
    }
}

