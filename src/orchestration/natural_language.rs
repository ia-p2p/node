//! Natural Language Interface for the LLM Orchestrator
//!
//! This module provides natural language command interpretation,
//! mapping user intents to structured CLI commands.

use crate::orchestration::error::{OrchestratorError, OrchestratorResult};
use crate::orchestration::llm_backend::LocalLLMBackend;
use crate::orchestration::prompts::PromptTemplates;
use crate::orchestration::types::{CliCommand, InterpretationResult};
use crate::orchestration::GenerationConfig;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Trait for natural language interface
#[async_trait::async_trait]
pub trait NaturalLanguageInterfaceTrait: Send + Sync {
    /// Interpret a natural language command
    async fn interpret_command(&self, input: &str) -> OrchestratorResult<InterpretationResult>;
    
    /// Execute commands with confirmation handling
    async fn execute_with_confirmation(
        &self,
        commands: Vec<CliCommand>,
        confirmed: bool,
    ) -> OrchestratorResult<ExecutionResult>;
    
    /// Format a response in natural language
    fn format_response(&self, result: &ExecutionResult) -> String;
}

/// Result of command execution
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Commands that were executed
    pub executed: Vec<CliCommand>,
    /// Commands that required confirmation
    pub pending_confirmation: Vec<CliCommand>,
    /// Commands that failed
    pub failed: Vec<(CliCommand, String)>,
    /// Overall success
    pub success: bool,
    /// Human-readable message
    pub message: String,
}

impl ExecutionResult {
    pub fn success(executed: Vec<CliCommand>, message: impl Into<String>) -> Self {
        Self {
            pending_confirmation: vec![],
            failed: vec![],
            success: true,
            message: message.into(),
            executed,
        }
    }
    
    pub fn needs_confirmation(pending: Vec<CliCommand>) -> Self {
        Self {
            executed: vec![],
            pending_confirmation: pending,
            failed: vec![],
            success: false,
            message: "Some commands require confirmation".to_string(),
        }
    }
    
    pub fn failure(failed: Vec<(CliCommand, String)>, message: String) -> Self {
        Self {
            executed: vec![],
            pending_confirmation: vec![],
            failed,
            success: false,
            message,
        }
    }
}

/// Command mapping rules
struct CommandMapper {
    /// Known command patterns
    patterns: Vec<CommandPattern>,
}

struct CommandPattern {
    /// Keywords that trigger this pattern
    keywords: Vec<String>,
    /// Base command
    command: String,
    /// Whether this is destructive
    is_destructive: bool,
    /// Description template
    description: String,
}

impl CommandMapper {
    fn new() -> Self {
        Self {
            patterns: vec![
                CommandPattern {
                    keywords: vec!["status".to_string(), "show".to_string(), "list".to_string()],
                    command: "status".to_string(),
                    is_destructive: false,
                    description: "Show status information".to_string(),
                },
                CommandPattern {
                    keywords: vec!["start".to_string(), "launch".to_string(), "run".to_string()],
                    command: "start".to_string(),
                    is_destructive: false,
                    description: "Start a component".to_string(),
                },
                CommandPattern {
                    keywords: vec!["stop".to_string(), "shutdown".to_string(), "kill".to_string()],
                    command: "stop".to_string(),
                    is_destructive: true,
                    description: "Stop a component".to_string(),
                },
                CommandPattern {
                    keywords: vec!["restart".to_string(), "reboot".to_string()],
                    command: "restart".to_string(),
                    is_destructive: true,
                    description: "Restart a component".to_string(),
                },
                CommandPattern {
                    keywords: vec!["peers".to_string(), "connections".to_string(), "network".to_string()],
                    command: "peers".to_string(),
                    is_destructive: false,
                    description: "Show network peers".to_string(),
                },
                CommandPattern {
                    keywords: vec!["metrics".to_string(), "stats".to_string(), "performance".to_string()],
                    command: "metrics".to_string(),
                    is_destructive: false,
                    description: "Show metrics".to_string(),
                },
                CommandPattern {
                    keywords: vec!["help".to_string(), "?".to_string()],
                    command: "help".to_string(),
                    is_destructive: false,
                    description: "Show help".to_string(),
                },
            ],
        }
    }
    
    /// Try to map input to a command using simple pattern matching
    fn try_simple_map(&self, input: &str) -> Option<CliCommand> {
        let input_lower = input.to_lowercase();
        
        for pattern in &self.patterns {
            if pattern.keywords.iter().any(|k| input_lower.contains(k)) {
                return Some(CliCommand {
                    command: pattern.command.clone(),
                    args: vec![],
                    is_destructive: pattern.is_destructive,
                    description: pattern.description.clone(),
                });
            }
        }
        
        None
    }
    
    /// Extract numbers from input (for "start 3 nodes" patterns)
    fn extract_numbers(input: &str) -> Vec<i32> {
        input
            .split_whitespace()
            .filter_map(|word| word.parse::<i32>().ok())
            .collect()
    }
    
    /// Check if input mentions nodes
    fn mentions_nodes(input: &str) -> bool {
        let input_lower = input.to_lowercase();
        input_lower.contains("node") || input_lower.contains("nodes") || input_lower.contains("instance")
    }
}

/// Natural Language Interface implementation
pub struct NaturalLanguageInterface {
    /// LLM backend for interpretation
    llm_backend: Arc<LocalLLMBackend>,
    /// Command mapper for fallback
    command_mapper: CommandMapper,
    /// Whether to use LLM for interpretation
    use_llm: bool,
    /// Generation config for NL tasks
    generation_config: GenerationConfig,
}

impl NaturalLanguageInterface {
    /// Create a new natural language interface
    pub fn new(llm_backend: Arc<LocalLLMBackend>) -> Self {
        Self {
            llm_backend,
            command_mapper: CommandMapper::new(),
            use_llm: true,
            generation_config: GenerationConfig {
                max_tokens: 256,
                temperature: 0.3, // Low temperature for accurate mapping
                top_p: 0.9,
                repetition_penalty: 1.1,
            },
        }
    }
    
    /// Create without LLM (uses pattern matching only)
    pub fn without_llm() -> Self {
        Self {
            llm_backend: Arc::new(LocalLLMBackend::new(&crate::orchestration::OrchestratorConfig::default())),
            command_mapper: CommandMapper::new(),
            use_llm: false,
            generation_config: GenerationConfig::default(),
        }
    }
    
    /// Try to interpret using LLM
    async fn interpret_with_llm(&self, input: &str) -> OrchestratorResult<InterpretationResult> {
        let prompt = PromptTemplates::build_nl_prompt(input);
        
        let response = self.llm_backend
            .generate(&prompt, Some(&self.generation_config))
            .await?;
        
        // Parse response
        self.parse_nl_response(&response)
    }
    
    /// Parse LLM response into InterpretationResult
    fn parse_nl_response(&self, response: &str) -> OrchestratorResult<InterpretationResult> {
        // Try to extract JSON from response
        let json_str = self.extract_json(response)?;
        
        serde_json::from_str(&json_str).map_err(|e| {
            OrchestratorError::IntentInterpretationError {
                message: format!("Failed to parse interpretation: {}", e),
                input: Some(response.to_string()),
            }
        })
    }
    
    /// Extract JSON from a response that might have extra text
    fn extract_json(&self, text: &str) -> OrchestratorResult<String> {
        // Find JSON object boundaries
        let start = text.find('{').ok_or_else(|| {
            OrchestratorError::IntentInterpretationError {
                message: "No JSON object found in response".to_string(),
                input: Some(text.to_string()),
            }
        })?;
        
        // Find matching closing brace
        let mut depth = 0;
        let mut end = start;
        
        for (i, c) in text[start..].chars().enumerate() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = start + i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        
        if depth != 0 {
            return Err(OrchestratorError::IntentInterpretationError {
                message: "Unbalanced JSON braces".to_string(),
                input: Some(text.to_string()),
            });
        }
        
        Ok(text[start..end].to_string())
    }
    
    /// Interpret using simple pattern matching (fallback)
    fn interpret_with_patterns(&self, input: &str) -> OrchestratorResult<InterpretationResult> {
        let input_lower = input.to_lowercase();
        let mut commands = Vec::new();
        let mut intent = String::new();
        
        // Handle "start N nodes" pattern
        if input_lower.contains("start") && CommandMapper::mentions_nodes(&input_lower) {
            let numbers = CommandMapper::extract_numbers(input);
            let count = numbers.first().copied().unwrap_or(1);
            
            intent = format!("Start {} node(s)", count);
            
            for i in 0..count {
                commands.push(CliCommand {
                    command: "node".to_string(),
                    args: vec!["start".to_string(), format!("--port={}", 5000 + i)],
                    is_destructive: false,
                    description: format!("Start node on port {}", 5000 + i),
                });
            }
        }
        // Handle "stop" pattern
        else if input_lower.contains("stop") || input_lower.contains("shutdown") {
            intent = "Stop node(s)".to_string();
            commands.push(CliCommand {
                command: "node".to_string(),
                args: vec!["stop".to_string()],
                is_destructive: true,
                description: "Stop node".to_string(),
            });
        }
        // Handle "status" pattern
        else if input_lower.contains("status") || input_lower.contains("show") {
            intent = "Show status".to_string();
            
            if CommandMapper::mentions_nodes(&input_lower) {
                commands.push(CliCommand::new("node", vec!["status".to_string()])
                    .with_description("Show node status"));
            } else if input_lower.contains("network") {
                commands.push(CliCommand::new("network", vec!["status".to_string()])
                    .with_description("Show network status"));
            } else {
                commands.push(CliCommand::new("status", vec![])
                    .with_description("Show overall status"));
            }
        }
        // Handle "help" pattern
        else if input_lower.contains("help") {
            intent = "Show help".to_string();
            commands.push(CliCommand::new("help", vec![])
                .with_description("Show available commands"));
        }
        // Default: try simple mapping
        else if let Some(cmd) = self.command_mapper.try_simple_map(input) {
            intent = cmd.description.clone();
            commands.push(cmd);
        }
        else {
            return Err(OrchestratorError::IntentInterpretationError {
                message: "Could not interpret command".to_string(),
                input: Some(input.to_string()),
            });
        }
        
        let requires_confirmation = commands.iter().any(|c| c.is_destructive);
        
        Ok(InterpretationResult {
            intent,
            commands,
            confidence: 0.7, // Lower confidence for pattern matching
            requires_confirmation,
            explanation: format!(
                "Interpreted from: \"{}\"{}",
                input,
                if requires_confirmation { " (requires confirmation)" } else { "" }
            ),
        })
    }
    
    /// Generate confirmation prompt for destructive actions
    pub fn generate_confirmation_prompt(&self, result: &InterpretationResult) -> String {
        let mut prompt = format!("🤖 I understand you want to: {}\n\n", result.intent);
        prompt.push_str("This will:\n");
        
        for cmd in &result.commands {
            prompt.push_str(&format!("  • {}\n", cmd.description));
        }
        
        if result.requires_confirmation {
            prompt.push_str("\n⚠️  This action may affect running services.\n");
            prompt.push_str("\nProceed? (y/N): ");
        }
        
        prompt
    }
    
    /// Generate success response
    pub fn generate_success_response(&self, result: &ExecutionResult) -> String {
        let mut response = String::new();
        
        if result.success {
            response.push_str("✅ ");
            response.push_str(&result.message);
            response.push('\n');
            
            for cmd in &result.executed {
                response.push_str(&format!("   • {}\n", cmd.description));
            }
        } else {
            response.push_str("❌ ");
            response.push_str(&result.message);
            response.push('\n');
            
            for (cmd, error) in &result.failed {
                response.push_str(&format!("   • {} - {}\n", cmd.command, error));
            }
        }
        
        response
    }
}

#[async_trait::async_trait]
impl NaturalLanguageInterfaceTrait for NaturalLanguageInterface {
    async fn interpret_command(&self, input: &str) -> OrchestratorResult<InterpretationResult> {
        debug!(input = %input, use_llm = %self.use_llm, "Interpreting natural language command");
        
        // Try LLM first if available
        if self.use_llm && self.llm_backend.is_model_loaded().await {
            match self.interpret_with_llm(input).await {
                Ok(result) => {
                    info!(
                        intent = %result.intent,
                        commands = result.commands.len(),
                        confidence = result.confidence,
                        "LLM interpretation successful"
                    );
                    return Ok(result);
                }
                Err(e) => {
                    warn!(error = %e, "LLM interpretation failed, falling back to patterns");
                }
            }
        }
        
        // Fallback to pattern matching
        self.interpret_with_patterns(input)
    }
    
    async fn execute_with_confirmation(
        &self,
        commands: Vec<CliCommand>,
        confirmed: bool,
    ) -> OrchestratorResult<ExecutionResult> {
        let destructive: Vec<_> = commands.iter().filter(|c| c.is_destructive).cloned().collect();
        let _safe: Vec<_> = commands.iter().filter(|c| !c.is_destructive).cloned().collect();
        
        // If there are destructive commands and not confirmed, return pending
        if !destructive.is_empty() && !confirmed {
            return Ok(ExecutionResult::needs_confirmation(destructive));
        }
        
        // Execute all commands (in a real implementation, this would call the actual CLI)
        let mut executed = Vec::new();
        let failed: Vec<(CliCommand, String)> = Vec::new();
        
        for cmd in commands {
            // Placeholder: In production, this would execute the actual command
            info!(
                command = %cmd.command,
                args = ?cmd.args,
                "Executing command (simulated)"
            );
            executed.push(cmd);
        }
        
        if failed.is_empty() {
            let count = executed.len();
            Ok(ExecutionResult::success(
                executed,
                format!("Successfully executed {} command(s)", count),
            ))
        } else {
            Ok(ExecutionResult::failure(
                failed,
                "Some commands failed".to_string(),
            ))
        }
    }
    
    fn format_response(&self, result: &ExecutionResult) -> String {
        self.generate_success_response(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_mapper_patterns() {
        let mapper = CommandMapper::new();
        
        // Status pattern
        let cmd = mapper.try_simple_map("show me the status");
        assert!(cmd.is_some());
        assert_eq!(cmd.unwrap().command, "status");
        
        // Stop pattern (destructive)
        let cmd = mapper.try_simple_map("stop everything");
        assert!(cmd.is_some());
        let cmd = cmd.unwrap();
        assert_eq!(cmd.command, "stop");
        assert!(cmd.is_destructive);
    }

    #[test]
    fn test_extract_numbers() {
        let nums = CommandMapper::extract_numbers("start 3 nodes");
        assert_eq!(nums, vec![3]);
        
        let nums = CommandMapper::extract_numbers("run 5 instances on port 8080");
        assert_eq!(nums, vec![5, 8080]);
        
        let nums = CommandMapper::extract_numbers("show status");
        assert!(nums.is_empty());
    }

    #[tokio::test]
    async fn test_interpret_start_nodes() {
        let nl = NaturalLanguageInterface::without_llm();
        
        let result = nl.interpret_command("start 3 nodes for testing").await;
        assert!(result.is_ok());
        
        let result = result.unwrap();
        assert_eq!(result.commands.len(), 3);
        assert!(!result.requires_confirmation);
    }

    #[tokio::test]
    async fn test_interpret_stop() {
        let nl = NaturalLanguageInterface::without_llm();
        
        let result = nl.interpret_command("stop the node").await;
        assert!(result.is_ok());
        
        let result = result.unwrap();
        assert!(result.requires_confirmation);
    }

    #[tokio::test]
    async fn test_interpret_status() {
        let nl = NaturalLanguageInterface::without_llm();
        
        let result = nl.interpret_command("show me the status").await;
        assert!(result.is_ok());
        
        let result = result.unwrap();
        assert!(!result.requires_confirmation);
        assert!(!result.commands.is_empty());
    }

    #[tokio::test]
    async fn test_confirmation_prompt() {
        let nl = NaturalLanguageInterface::without_llm();
        
        let result = nl.interpret_command("stop node").await.unwrap();
        let prompt = nl.generate_confirmation_prompt(&result);
        
        assert!(prompt.contains("Proceed?"));
        assert!(prompt.contains("⚠️"));
    }

    #[tokio::test]
    async fn test_execute_with_confirmation() {
        let nl = NaturalLanguageInterface::without_llm();
        
        let commands = vec![
            CliCommand {
                command: "stop".to_string(),
                args: vec![],
                is_destructive: true,
                description: "Stop node".to_string(),
            },
        ];
        
        // Without confirmation
        let result = nl.execute_with_confirmation(commands.clone(), false).await.unwrap();
        assert!(!result.pending_confirmation.is_empty());
        
        // With confirmation
        let result = nl.execute_with_confirmation(commands, true).await.unwrap();
        assert!(result.success);
        assert!(!result.executed.is_empty());
    }

    #[test]
    fn test_execution_result_formatting() {
        let nl = NaturalLanguageInterface::without_llm();
        
        let result = ExecutionResult::success(
            vec![CliCommand::new("status", vec![]).with_description("Show status")],
            "Command executed".to_string(),
        );
        
        let formatted = nl.format_response(&result);
        assert!(formatted.contains("✅"));
        assert!(formatted.contains("Show status"));
    }
}

