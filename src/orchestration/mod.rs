//! LLM Orchestrator Module
//!
//! This module provides cognitive orchestration capabilities using local LLMs
//! for infrastructure and context management in the decentralized AI network.
//!
//! The orchestrator follows the principle of separation between cognition (LLM decides WHAT)
//! and execution (infrastructure decides HOW), ensuring stability and auditability.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    LLM Orchestrator                         │
//! ├─────────────────────────────────────────────────────────────┤
//! │  Decision Cache  │  Natural Language  │  Model Manager     │
//! ├─────────────────────────────────────────────────────────────┤
//! │                    Core Orchestrator                        │
//! │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────┐ │
//! │  │ Decision Engine │  │ Training Data   │  │ Metrics     │ │
//! │  │                 │  │ Collector       │  │ Aggregator  │ │
//! │  └─────────────────┘  └─────────────────┘  └─────────────┘ │
//! ├─────────────────────────────────────────────────────────────┤
//! │                    Local LLM Backend                        │
//! │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────┐ │
//! │  │ Model Loader    │  │ Inference Engine│  │ Tokenizer   │ │
//! │  │ (Candle)        │  │                 │  │             │ │
//! │  └─────────────────┘  └─────────────────┘  └─────────────┘ │
//! ├─────────────────────────────────────────────────────────────┤
//! │                    Decision Validator                       │
//! │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────┐ │
//! │  │ Schema Validator│  │ Safety Checker  │  │ Audit Logger│ │
//! │  └─────────────────┘  └─────────────────┘  └─────────────┘ │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use mvp_node::orchestration::{LLMOrchestrator, Orchestrator, DecisionType};
//! use mvp_node::monitoring::DefaultHealthMonitor;
//! use std::sync::Arc;
//!
//! let config = OrchestratorConfig::default();
//! let monitor = Arc::new(DefaultHealthMonitor::new());
//! let orchestrator = LLMOrchestrator::new(config, monitor, "node-1".to_string(), 100);
//!
//! // Make a decision
//! let decision = orchestrator.make_decision(
//!     "High latency detected in queue",
//!     DecisionType::Infrastructure,
//! ).await?;
//!
//! println!("Decision: {} (confidence: {})", decision.decision, decision.confidence);
//! ```

pub mod cache;
pub mod decision_validator;
pub mod error;
pub mod llm_backend;
pub mod metrics_aggregator;
pub mod models;
pub mod natural_language;
pub mod orchestrator;
pub mod prompts;
pub mod training_collector;
pub mod types;

// Re-export main types and traits
pub use cache::DecisionCache;
pub use decision_validator::{DecisionValidator, DecisionValidatorTrait, ValidationResult};
pub use error::{OrchestratorError, OrchestratorResult};
pub use llm_backend::{LocalLLMBackend, LLMModel, SupportedModel, ModelInfo, BackendMetrics};
pub use metrics_aggregator::{MetricsAggregator, MetricsAggregatorTrait, MockMetricsAggregator};
pub use natural_language::{NaturalLanguageInterface, NaturalLanguageInterfaceTrait, ExecutionResult};
pub use orchestrator::{LLMOrchestrator, Orchestrator};
pub use training_collector::{TrainingDataCollector, TrainingDataCollectorTrait};
pub use types::*;

/// Configuration for the LLM Orchestrator
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OrchestratorConfig {
    /// Whether the orchestrator is enabled
    pub enabled: bool,
    
    /// Path to the model files
    pub model_path: String,
    
    /// Type of model to use
    pub model_type: String,
    
    /// Device to use for inference (auto, cpu, cuda, metal)
    pub device: String,
    
    /// Generation parameters
    pub generation: GenerationConfig,
    
    /// Cache configuration
    pub cache: CacheConfig,
    
    /// Training data collection configuration
    pub training: TrainingConfig,
    
    /// Safety configuration
    pub safety: SafetyConfig,
}

/// Generation parameters for the LLM
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GenerationConfig {
    pub max_tokens: usize,
    pub temperature: f64,
    pub top_p: f64,
    pub repetition_penalty: f64,
}

/// Cache configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CacheConfig {
    pub enabled: bool,
    pub max_entries: usize,
    pub ttl_seconds: u64,
}

/// Training data collection configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrainingConfig {
    pub collect_data: bool,
    pub export_path: String,
    pub auto_export_threshold: usize,
}

/// Safety configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SafetyConfig {
    pub allowed_actions: Vec<String>,
    pub max_confidence_threshold: f64,
    pub require_confirmation_above: f64,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            model_path: "./models/llama3-2-3b-instruct".to_string(),
            model_type: "llama3_2_3b".to_string(),
            device: "auto".to_string(),
            generation: GenerationConfig::default(),
            cache: CacheConfig::default(),
            training: TrainingConfig::default(),
            safety: SafetyConfig::default(),
        }
    }
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            max_tokens: 512,
            temperature: 0.7,
            top_p: 0.9,
            repetition_penalty: 1.1,
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_entries: 1000,
            ttl_seconds: 300,
        }
    }
}

impl Default for TrainingConfig {
    fn default() -> Self {
        Self {
            collect_data: true,
            export_path: "./training_data".to_string(),
            auto_export_threshold: 1000,
        }
    }
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            allowed_actions: vec![
                "start_election".to_string(),
                "migrate_context".to_string(),
                "adjust_gossip".to_string(),
                "scale_queue".to_string(),
                "wait".to_string(),
                "restart_component".to_string(),
            ],
            max_confidence_threshold: 0.95,
            require_confirmation_above: 0.8,
        }
    }
}