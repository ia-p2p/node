//! Error types for the LLM Orchestrator module
//!
//! This module defines comprehensive error types for all orchestrator operations,
//! following the principle of typed errors with recovery strategies.

use std::fmt;
use thiserror::Error;
use serde::{Deserialize, Serialize};

/// Result type alias for orchestrator operations
pub type OrchestratorResult<T> = Result<T, OrchestratorError>;

/// Main error type for LLM Orchestrator operations
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum OrchestratorError {
    // ========================================================================
    // Model Errors
    // ========================================================================
    
    /// Model loading failed
    #[error("Model load error: {message}")]
    ModelLoadError {
        model_name: String,
        message: String,
    },
    
    /// Model inference failed
    #[error("Inference error: {message}")]
    InferenceError {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        context: Option<String>,
    },
    
    /// Model not loaded
    #[error("No model loaded")]
    ModelNotLoaded,
    
    /// Model integrity check failed
    #[error("Model integrity error: {message}")]
    ModelIntegrityError {
        model_path: String,
        message: String,
    },
    
    /// Model format not supported
    #[error("Unsupported model format: {format}")]
    UnsupportedModelFormat {
        format: String,
    },
    
    /// Insufficient resources for model
    #[error("Insufficient resources: {message}")]
    InsufficientResources {
        message: String,
        required_mb: Option<u64>,
        available_mb: Option<u64>,
    },

    // ========================================================================
    // Validation Errors
    // ========================================================================
    
    /// Invalid JSON format
    #[error("Invalid JSON format: {message}")]
    InvalidJsonFormat {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        raw_output: Option<String>,
    },
    
    /// Decision schema validation failed
    #[error("Schema validation error: {message}")]
    SchemaValidationError {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        field: Option<String>,
    },
    
    /// Action not allowed
    #[error("Action not allowed: {action}")]
    ActionNotAllowed {
        action: String,
        allowed_actions: Vec<String>,
    },
    
    /// Parameter out of bounds
    #[error("Parameter out of bounds: {param_name} = {value}, expected {min} - {max}")]
    ParameterOutOfBounds {
        param_name: String,
        value: String,
        min: String,
        max: String,
    },
    
    /// Invalid confidence value
    #[error("Invalid confidence: {value}, must be between 0.0 and 1.0")]
    InvalidConfidence {
        value: f64,
    },

    // ========================================================================
    // Cache Errors
    // ========================================================================
    
    /// Cache operation failed
    #[error("Cache error: {message}")]
    CacheError {
        message: String,
    },
    
    /// Cache entry corrupted
    #[error("Cache corruption detected: {key}")]
    CacheCorruption {
        key: String,
    },

    // ========================================================================
    // Integration Errors
    // ========================================================================
    
    /// Health monitor unavailable
    #[error("Health monitor unavailable: {message}")]
    HealthMonitorUnavailable {
        message: String,
    },
    
    /// Logging system error
    #[error("Logging error: {message}")]
    LoggingError {
        message: String,
    },
    
    /// Configuration error
    #[error("Configuration error: {message}")]
    ConfigurationError {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        field: Option<String>,
    },

    // ========================================================================
    // Resource Errors
    // ========================================================================
    
    /// Resource exhausted
    #[error("Resource exhausted: {resource}")]
    ResourceExhausted {
        resource: String,
    },
    
    /// Timeout error
    #[error("Timeout: {operation} exceeded {timeout_ms}ms")]
    Timeout {
        operation: String,
        timeout_ms: u64,
    },

    // ========================================================================
    // Training Data Errors
    // ========================================================================
    
    /// Training data storage error
    #[error("Training data error: {message}")]
    TrainingDataError {
        message: String,
    },
    
    /// Export failed
    #[error("Export failed: {message}")]
    ExportError {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },
    
    /// Decision not found for feedback
    #[error("Decision not found: {id}")]
    DecisionNotFound {
        id: String,
    },

    // ========================================================================
    // Natural Language Errors
    // ========================================================================
    
    /// Intent interpretation failed
    #[error("Could not interpret intent: {message}")]
    IntentInterpretationError {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        input: Option<String>,
    },
    
    /// Command mapping failed
    #[error("Command mapping failed: {message}")]
    CommandMappingError {
        message: String,
    },

    // ========================================================================
    // Internal Errors
    // ========================================================================
    
    /// Internal error
    #[error("Internal error: {message}")]
    InternalError {
        message: String,
    },
    
    /// Orchestrator disabled
    #[error("Orchestrator is disabled")]
    Disabled,
    
    /// Fallback activated
    #[error("Fallback activated: {reason}")]
    FallbackActivated {
        reason: String,
    },
}

impl OrchestratorError {
    /// Check if the error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::ModelNotLoaded => true,
            Self::InferenceError { .. } => true,
            Self::CacheError { .. } => true,
            Self::Timeout { .. } => true,
            Self::FallbackActivated { .. } => true,
            Self::HealthMonitorUnavailable { .. } => true,
            Self::ResourceExhausted { .. } => true,
            Self::Disabled => false,
            Self::InternalError { .. } => false,
            Self::ModelLoadError { .. } => false,
            Self::ModelIntegrityError { .. } => false,
            _ => false,
        }
    }
    
    /// Get the fallback strategy for this error
    pub fn fallback_strategy(&self) -> FallbackStrategy {
        match self {
            Self::ModelNotLoaded | Self::ModelLoadError { .. } => {
                FallbackStrategy::UseHeuristics
            }
            Self::InferenceError { .. } | Self::Timeout { .. } => {
                FallbackStrategy::UseHeuristics
            }
            Self::InsufficientResources { .. } => {
                FallbackStrategy::RetryWithSmallerModel
            }
            Self::CacheError { .. } | Self::CacheCorruption { .. } => {
                FallbackStrategy::ClearCacheAndRetry
            }
            Self::Disabled => {
                FallbackStrategy::DisableFeature
            }
            _ => FallbackStrategy::UseHeuristics,
        }
    }
    
    /// Check if retry might help
    pub fn should_retry(&self) -> bool {
        match self {
            Self::Timeout { .. } => true,
            Self::InferenceError { .. } => true,
            Self::CacheError { .. } => true,
            Self::ResourceExhausted { .. } => true,
            _ => false,
        }
    }
    
    /// Get error category for logging
    pub fn category(&self) -> ErrorCategory {
        match self {
            Self::ModelLoadError { .. } 
            | Self::InferenceError { .. }
            | Self::ModelNotLoaded
            | Self::ModelIntegrityError { .. }
            | Self::UnsupportedModelFormat { .. } => ErrorCategory::Model,
            
            Self::InvalidJsonFormat { .. }
            | Self::SchemaValidationError { .. }
            | Self::ActionNotAllowed { .. }
            | Self::ParameterOutOfBounds { .. }
            | Self::InvalidConfidence { .. } => ErrorCategory::Validation,
            
            Self::CacheError { .. }
            | Self::CacheCorruption { .. } => ErrorCategory::Cache,
            
            Self::HealthMonitorUnavailable { .. }
            | Self::LoggingError { .. }
            | Self::ConfigurationError { .. } => ErrorCategory::Integration,
            
            Self::InsufficientResources { .. }
            | Self::ResourceExhausted { .. }
            | Self::Timeout { .. } => ErrorCategory::Resource,
            
            Self::TrainingDataError { .. }
            | Self::ExportError { .. }
            | Self::DecisionNotFound { .. } => ErrorCategory::TrainingData,
            
            Self::IntentInterpretationError { .. }
            | Self::CommandMappingError { .. } => ErrorCategory::NaturalLanguage,
            
            Self::InternalError { .. }
            | Self::Disabled
            | Self::FallbackActivated { .. } => ErrorCategory::Internal,
        }
    }
}

/// Error category for classification and logging
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCategory {
    Model,
    Validation,
    Cache,
    Integration,
    Resource,
    TrainingData,
    NaturalLanguage,
    Internal,
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Model => write!(f, "model"),
            Self::Validation => write!(f, "validation"),
            Self::Cache => write!(f, "cache"),
            Self::Integration => write!(f, "integration"),
            Self::Resource => write!(f, "resource"),
            Self::TrainingData => write!(f, "training_data"),
            Self::NaturalLanguage => write!(f, "natural_language"),
            Self::Internal => write!(f, "internal"),
        }
    }
}

/// Fallback strategy when errors occur
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FallbackStrategy {
    /// Use deterministic heuristics instead of LLM
    UseHeuristics,
    /// Retry with a smaller model
    RetryWithSmallerModel,
    /// Clear cache and retry
    ClearCacheAndRetry,
    /// Disable the feature gracefully
    DisableFeature,
}

impl fmt::Display for FallbackStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UseHeuristics => write!(f, "use_heuristics"),
            Self::RetryWithSmallerModel => write!(f, "retry_with_smaller_model"),
            Self::ClearCacheAndRetry => write!(f, "clear_cache_and_retry"),
            Self::DisableFeature => write!(f, "disable_feature"),
        }
    }
}

/// Validation error details for logging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationErrorDetails {
    /// The validation that failed
    pub validation_type: String,
    /// Field that failed validation (if applicable)
    pub field: Option<String>,
    /// Expected value or constraint
    pub expected: String,
    /// Actual value received
    pub actual: String,
    /// The raw LLM output that caused the error
    pub raw_output: Option<String>,
}

impl ValidationErrorDetails {
    pub fn new(validation_type: &str, expected: &str, actual: &str) -> Self {
        Self {
            validation_type: validation_type.to_string(),
            field: None,
            expected: expected.to_string(),
            actual: actual.to_string(),
            raw_output: None,
        }
    }
    
    pub fn with_field(mut self, field: &str) -> Self {
        self.field = Some(field.to_string());
        self
    }
    
    pub fn with_raw_output(mut self, output: &str) -> Self {
        self.raw_output = Some(output.to_string());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_is_recoverable() {
        assert!(OrchestratorError::ModelNotLoaded.is_recoverable());
        assert!(OrchestratorError::Timeout { 
            operation: "test".to_string(), 
            timeout_ms: 1000 
        }.is_recoverable());
        assert!(!OrchestratorError::Disabled.is_recoverable());
    }

    #[test]
    fn test_fallback_strategy() {
        let err = OrchestratorError::ModelNotLoaded;
        assert_eq!(err.fallback_strategy(), FallbackStrategy::UseHeuristics);
        
        let err = OrchestratorError::InsufficientResources {
            message: "test".to_string(),
            required_mb: Some(1000),
            available_mb: Some(500),
        };
        assert_eq!(err.fallback_strategy(), FallbackStrategy::RetryWithSmallerModel);
    }

    #[test]
    fn test_error_category() {
        let err = OrchestratorError::InvalidJsonFormat {
            message: "test".to_string(),
            raw_output: None,
        };
        assert_eq!(err.category(), ErrorCategory::Validation);
        
        let err = OrchestratorError::CacheError {
            message: "test".to_string(),
        };
        assert_eq!(err.category(), ErrorCategory::Cache);
    }

    #[test]
    fn test_validation_error_details() {
        let details = ValidationErrorDetails::new("action_allowlist", "allowed", "forbidden")
            .with_field("action_type")
            .with_raw_output("{\"action_type\": \"forbidden\"}");
        
        assert_eq!(details.field, Some("action_type".to_string()));
        assert!(details.raw_output.is_some());
    }
}

