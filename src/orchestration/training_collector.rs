//! Training Data Collector for the LLM Orchestrator
//!
//! This module collects decision data and outcomes for future fine-tuning,
//! including human feedback integration.

use crate::orchestration::error::{OrchestratorError, OrchestratorResult};
use crate::orchestration::types::{
    DatasetFormat, Decision, DecisionType, Feedback, LLMState, Outcome, TrainingExample,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Trait for training data collection
#[async_trait::async_trait]
pub trait TrainingDataCollectorTrait: Send + Sync {
    /// Record a decision with its context
    async fn record_decision(
        &self,
        state: &LLMState,
        context: &str,
        decision_type: DecisionType,
        decision: &Decision,
    ) -> String;
    
    /// Add human feedback to an existing decision
    async fn add_feedback(&self, id: &str, feedback: Feedback) -> OrchestratorResult<()>;
    
    /// Add outcome information to an existing decision
    async fn add_outcome(&self, id: &str, outcome: Outcome) -> OrchestratorResult<()>;
    
    /// Export the dataset in the specified format
    async fn export_dataset(&self, path: &str, format: DatasetFormat) -> OrchestratorResult<usize>;
    
    /// Get count of collected examples
    async fn count(&self) -> usize;
    
    /// Get count of examples valid for training
    async fn count_valid_for_training(&self) -> usize;
}

/// Filter for training examples
pub trait ExampleFilter: Send + Sync {
    /// Check if an example should be included in export
    fn should_include(&self, example: &TrainingExample) -> bool;
}

/// Filter that requires successful outcome
pub struct SuccessfulOutcomeFilter;

impl ExampleFilter for SuccessfulOutcomeFilter {
    fn should_include(&self, example: &TrainingExample) -> bool {
        matches!(
            &example.outcome,
            Some(Outcome::Success { .. }) | Some(Outcome::Neutral { .. })
        )
    }
}

/// Filter that requires human approval
pub struct HumanApprovedFilter;

impl ExampleFilter for HumanApprovedFilter {
    fn should_include(&self, example: &TrainingExample) -> bool {
        matches!(&example.human_feedback, Some(Feedback::Approved))
    }
}

/// Filter that requires minimum confidence
pub struct MinConfidenceFilter {
    min_confidence: f64,
}

impl MinConfidenceFilter {
    pub fn new(min: f64) -> Self {
        Self { min_confidence: min }
    }
}

impl ExampleFilter for MinConfidenceFilter {
    fn should_include(&self, example: &TrainingExample) -> bool {
        example.llm_decision.confidence >= self.min_confidence
    }
}

/// Training Data Collector implementation
pub struct TrainingDataCollector {
    /// Stored training examples
    examples: Arc<RwLock<HashMap<String, TrainingExample>>>,
    /// Filters for export
    export_filters: Vec<Box<dyn ExampleFilter>>,
    /// Maximum examples to store in memory
    max_in_memory: usize,
    /// Auto-export threshold
    auto_export_threshold: Option<usize>,
    /// Auto-export path
    auto_export_path: Option<String>,
    /// Metrics before cache (for delayed metrics_after recording)
    pending_metrics: Arc<RwLock<HashMap<String, HashMap<String, f64>>>>,
}

impl TrainingDataCollector {
    /// Create a new training data collector
    pub fn new() -> Self {
        Self {
            examples: Arc::new(RwLock::new(HashMap::new())),
            export_filters: vec![Box::new(SuccessfulOutcomeFilter)],
            max_in_memory: 10000,
            auto_export_threshold: None,
            auto_export_path: None,
            pending_metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Create with custom filters
    pub fn with_filters(mut self, filters: Vec<Box<dyn ExampleFilter>>) -> Self {
        self.export_filters = filters;
        self
    }
    
    /// Set auto-export configuration
    pub fn with_auto_export(mut self, threshold: usize, path: String) -> Self {
        self.auto_export_threshold = Some(threshold);
        self.auto_export_path = Some(path);
        self
    }
    
    /// Set maximum in-memory examples
    pub fn with_max_in_memory(mut self, max: usize) -> Self {
        self.max_in_memory = max;
        self
    }
    
    /// Record current metrics for a decision (to be compared with metrics_after)
    pub async fn record_metrics_before(&self, decision_id: &str, metrics: HashMap<String, f64>) {
        let mut pending = self.pending_metrics.write().await;
        pending.insert(decision_id.to_string(), metrics);
    }
    
    /// Get a specific example by ID
    pub async fn get_example(&self, id: &str) -> Option<TrainingExample> {
        let examples = self.examples.read().await;
        examples.get(id).cloned()
    }
    
    /// Get all examples
    pub async fn get_all_examples(&self) -> Vec<TrainingExample> {
        let examples = self.examples.read().await;
        examples.values().cloned().collect()
    }
    
    /// Clear all stored examples
    pub async fn clear(&self) {
        let mut examples = self.examples.write().await;
        examples.clear();
        
        let mut pending = self.pending_metrics.write().await;
        pending.clear();
        
        info!("Cleared all training examples");
    }
    
    /// Check if auto-export should be triggered
    async fn check_auto_export(&self) {
        let count = self.count().await;
        
        if let (Some(threshold), Some(path)) = (&self.auto_export_threshold, &self.auto_export_path) {
            if count >= *threshold {
                info!(
                    count = count,
                    threshold = threshold,
                    "Auto-export threshold reached"
                );
                
                if let Err(e) = self.export_dataset(path, DatasetFormat::Jsonl).await {
                    warn!(error = %e, "Auto-export failed");
                }
            }
        }
    }
    
    /// Filter examples for export
    fn filter_examples<'a>(&self, examples: &'a [TrainingExample]) -> Vec<&'a TrainingExample> {
        examples
            .iter()
            .filter(|ex| self.export_filters.iter().all(|f| f.should_include(ex)))
            .collect()
    }
    
    /// Format example for JSONL export
    fn format_for_jsonl(example: &TrainingExample) -> String {
        // Format for fine-tuning: instruction/input/output format
        let instruction = format!(
            "Analyze the following system state and context, then provide an orchestration decision.\n\n\
             State:\n{}\n\n\
             Context: {}",
            example.state.to_summary(),
            example.context
        );
        
        let output = serde_json::to_string(&example.llm_decision)
            .unwrap_or_else(|_| "{}".to_string());
        
        let training_row = serde_json::json!({
            "instruction": instruction,
            "input": "",
            "output": output,
            "decision_type": example.decision_type,
            "outcome": example.outcome,
            "feedback": example.human_feedback,
        });
        
        serde_json::to_string(&training_row).unwrap_or_default()
    }
    
    /// Export to JSONL format
    async fn export_jsonl(&self, path: &str, examples: &[&TrainingExample]) -> OrchestratorResult<usize> {
        use tokio::io::AsyncWriteExt;
        
        let mut file = tokio::fs::File::create(path).await.map_err(|e| {
            OrchestratorError::ExportError {
                message: format!("Failed to create file: {}", e),
                path: Some(path.to_string()),
            }
        })?;
        
        let mut count = 0;
        for example in examples {
            let line = Self::format_for_jsonl(example);
            file.write_all(line.as_bytes()).await.map_err(|e| {
                OrchestratorError::ExportError {
                    message: format!("Failed to write: {}", e),
                    path: Some(path.to_string()),
                }
            })?;
            file.write_all(b"\n").await.map_err(|e| {
                OrchestratorError::ExportError {
                    message: format!("Failed to write newline: {}", e),
                    path: Some(path.to_string()),
                }
            })?;
            count += 1;
        }
        
        file.flush().await.map_err(|e| {
            OrchestratorError::ExportError {
                message: format!("Failed to flush: {}", e),
                path: Some(path.to_string()),
            }
        })?;
        
        Ok(count)
    }
    
    /// Export to JSON array format
    async fn export_json(&self, path: &str, examples: &[&TrainingExample]) -> OrchestratorResult<usize> {
        let json = serde_json::to_string_pretty(&examples).map_err(|e| {
            OrchestratorError::ExportError {
                message: format!("Failed to serialize: {}", e),
                path: Some(path.to_string()),
            }
        })?;
        
        tokio::fs::write(path, json).await.map_err(|e| {
            OrchestratorError::ExportError {
                message: format!("Failed to write file: {}", e),
                path: Some(path.to_string()),
            }
        })?;
        
        Ok(examples.len())
    }
}

impl Default for TrainingDataCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl TrainingDataCollectorTrait for TrainingDataCollector {
    async fn record_decision(
        &self,
        state: &LLMState,
        context: &str,
        decision_type: DecisionType,
        decision: &Decision,
    ) -> String {
        let example = TrainingExample::new(
            state.clone(),
            context.to_string(),
            decision_type,
            decision.clone(),
        );
        
        let id = example.id.clone();
        
        // Check memory limit
        {
            let examples = self.examples.read().await;
            if examples.len() >= self.max_in_memory {
                warn!(
                    current = examples.len(),
                    max = self.max_in_memory,
                    "Training data collector at capacity, consider exporting"
                );
            }
        }
        
        // Store example
        {
            let mut examples = self.examples.write().await;
            examples.insert(id.clone(), example);
        }
        
        debug!(
            id = %id,
            decision = %decision.decision,
            "Recorded training example"
        );
        
        // Check auto-export
        self.check_auto_export().await;
        
        id
    }
    
    async fn add_feedback(&self, id: &str, feedback: Feedback) -> OrchestratorResult<()> {
        let mut examples = self.examples.write().await;
        
        let example = examples.get_mut(id).ok_or_else(|| {
            OrchestratorError::DecisionNotFound {
                id: id.to_string(),
            }
        })?;
        
        example.human_feedback = Some(feedback.clone());
        
        info!(
            id = %id,
            feedback = ?feedback,
            "Added feedback to training example"
        );
        
        Ok(())
    }
    
    async fn add_outcome(&self, id: &str, outcome: Outcome) -> OrchestratorResult<()> {
        let mut examples = self.examples.write().await;
        
        let example = examples.get_mut(id).ok_or_else(|| {
            OrchestratorError::DecisionNotFound {
                id: id.to_string(),
            }
        })?;
        
        example.outcome = Some(outcome.clone());
        
        // Check for pending metrics_after
        {
            let mut pending = self.pending_metrics.write().await;
            if let Some(metrics_before) = pending.remove(id) {
                example.metrics_before = metrics_before;
            }
        }
        
        info!(
            id = %id,
            outcome = ?outcome,
            "Added outcome to training example"
        );
        
        Ok(())
    }
    
    async fn export_dataset(&self, path: &str, format: DatasetFormat) -> OrchestratorResult<usize> {
        let examples = self.examples.read().await;
        let all_examples: Vec<TrainingExample> = examples.values().cloned().collect();
        drop(examples);
        
        let filtered = self.filter_examples(&all_examples);
        
        info!(
            total = all_examples.len(),
            filtered = filtered.len(),
            format = %format,
            path = %path,
            "Exporting training dataset"
        );
        
        let count = match format {
            DatasetFormat::Jsonl => self.export_jsonl(path, &filtered).await?,
            DatasetFormat::Json => self.export_json(path, &filtered).await?,
            DatasetFormat::Csv => {
                // CSV export not implemented yet
                return Err(OrchestratorError::ExportError {
                    message: "CSV format not yet implemented".to_string(),
                    path: Some(path.to_string()),
                });
            }
        };
        
        info!(
            exported = count,
            path = %path,
            "Successfully exported training dataset"
        );
        
        Ok(count)
    }
    
    async fn count(&self) -> usize {
        self.examples.read().await.len()
    }
    
    async fn count_valid_for_training(&self) -> usize {
        let examples = self.examples.read().await;
        examples.values().filter(|ex| ex.is_valid_for_training()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::types::*;

    fn create_test_state() -> LLMState {
        LLMState::new(
            NodeState {
                id: "test-node".to_string(),
                capacity: CapacityLevel::Medium,
                latency_avg: 100,
                uptime_seconds: 3600,
                error_count: 0,
            },
            ContextState {
                id: "ctx".to_string(),
                health: HealthLevel::Healthy,
                queue_size: 10,
                max_queue: 100,
                processing_jobs: 1,
            },
            NetworkState {
                partition_risk: RiskLevel::Low,
                peers_available: 5,
                active_connections: 3,
                message_latency_avg: Some(50),
            },
            ModelState {
                loaded: true,
                avg_inference_ms: 200,
                memory_usage_mb: 4000,
                recent_failures: 0,
            },
        )
    }

    fn create_test_decision() -> Decision {
        Decision::new(
            "wait".to_string(),
            0.9,
            "System is stable".to_string(),
            vec![Action::new("wait", 3)],
        )
    }

    #[tokio::test]
    async fn test_record_decision() {
        let collector = TrainingDataCollector::new();
        let state = create_test_state();
        let decision = create_test_decision();
        
        let id = collector.record_decision(
            &state,
            "Test context",
            DecisionType::Infrastructure,
            &decision,
        ).await;
        
        assert!(!id.is_empty());
        assert_eq!(collector.count().await, 1);
    }

    #[tokio::test]
    async fn test_add_feedback() {
        let collector = TrainingDataCollector::new();
        let state = create_test_state();
        let decision = create_test_decision();
        
        let id = collector.record_decision(
            &state,
            "Test",
            DecisionType::Infrastructure,
            &decision,
        ).await;
        
        let result = collector.add_feedback(&id, Feedback::Approved).await;
        assert!(result.is_ok());
        
        let example = collector.get_example(&id).await.unwrap();
        assert!(matches!(example.human_feedback, Some(Feedback::Approved)));
    }

    #[tokio::test]
    async fn test_add_outcome() {
        let collector = TrainingDataCollector::new();
        let state = create_test_state();
        let decision = create_test_decision();
        
        let id = collector.record_decision(
            &state,
            "Test",
            DecisionType::Infrastructure,
            &decision,
        ).await;
        
        let outcome = Outcome::Success {
            improvement: 0.2,
            description: "Latency reduced".to_string(),
        };
        
        let result = collector.add_outcome(&id, outcome).await;
        assert!(result.is_ok());
        
        assert_eq!(collector.count_valid_for_training().await, 1);
    }

    #[tokio::test]
    async fn test_feedback_not_found() {
        let collector = TrainingDataCollector::new();
        
        let result = collector.add_feedback("non-existent", Feedback::Approved).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            OrchestratorError::DecisionNotFound { .. }
        ));
    }

    #[tokio::test]
    async fn test_count_valid_for_training() {
        let collector = TrainingDataCollector::new();
        let state = create_test_state();
        let decision = create_test_decision();
        
        // Create example without outcome
        let id1 = collector.record_decision(
            &state,
            "Test 1",
            DecisionType::Infrastructure,
            &decision,
        ).await;
        
        // Create example with success outcome
        let id2 = collector.record_decision(
            &state,
            "Test 2",
            DecisionType::Infrastructure,
            &decision,
        ).await;
        
        collector.add_outcome(&id2, Outcome::Success {
            improvement: 0.1,
            description: "Good".to_string(),
        }).await.unwrap();
        
        assert_eq!(collector.count().await, 2);
        assert_eq!(collector.count_valid_for_training().await, 1);
    }

    #[tokio::test]
    async fn test_export_jsonl() {
        let collector = TrainingDataCollector::new();
        let state = create_test_state();
        let decision = create_test_decision();
        
        let id = collector.record_decision(
            &state,
            "Test",
            DecisionType::Infrastructure,
            &decision,
        ).await;
        
        collector.add_outcome(&id, Outcome::Success {
            improvement: 0.1,
            description: "Success".to_string(),
        }).await.unwrap();
        
        let temp_file = "/tmp/test_training_data.jsonl";
        let count = collector.export_dataset(temp_file, DatasetFormat::Jsonl).await.unwrap();
        
        assert_eq!(count, 1);
        
        // Verify file exists and has content
        let content = tokio::fs::read_to_string(temp_file).await.unwrap();
        assert!(content.contains("wait"));
        
        // Cleanup
        let _ = tokio::fs::remove_file(temp_file).await;
    }

    #[tokio::test]
    async fn test_clear() {
        let collector = TrainingDataCollector::new();
        let state = create_test_state();
        let decision = create_test_decision();
        
        collector.record_decision(&state, "Test", DecisionType::Infrastructure, &decision).await;
        assert_eq!(collector.count().await, 1);
        
        collector.clear().await;
        assert_eq!(collector.count().await, 0);
    }

    #[test]
    fn test_successful_outcome_filter() {
        let filter = SuccessfulOutcomeFilter;
        
        let mut example = TrainingExample::new(
            create_test_state(),
            "test".to_string(),
            DecisionType::Infrastructure,
            create_test_decision(),
        );
        
        // No outcome - should not include
        assert!(!filter.should_include(&example));
        
        // Success outcome - should include
        example.outcome = Some(Outcome::Success {
            improvement: 0.1,
            description: "Good".to_string(),
        });
        assert!(filter.should_include(&example));
        
        // Failure outcome - should not include
        example.outcome = Some(Outcome::Failure {
            degradation: 0.1,
            description: "Bad".to_string(),
        });
        assert!(!filter.should_include(&example));
    }

    #[test]
    fn test_min_confidence_filter() {
        let filter = MinConfidenceFilter::new(0.7);
        
        let mut example = TrainingExample::new(
            create_test_state(),
            "test".to_string(),
            DecisionType::Infrastructure,
            create_test_decision(), // confidence 0.9
        );
        
        assert!(filter.should_include(&example));
        
        example.llm_decision.confidence = 0.5;
        assert!(!filter.should_include(&example));
    }
}

