//! Prompt templates for the LLM Orchestrator
//!
//! This module contains specialized prompts for different decision types,
//! optimized for infrastructure orchestration tasks.

use crate::orchestration::types::{DecisionType, LLMState};

/// System prompt for infrastructure decisions (conservative, stability-focused)
pub const INFRASTRUCTURE_SYSTEM_PROMPT: &str = r#"You are an infrastructure orchestrator for a decentralized AI network.
Your role is to maintain system stability and performance by analyzing the current state and suggesting appropriate actions.

RULES:
1. Be CONSERVATIVE - prefer stability over aggressive optimization
2. Respond ONLY with valid JSON matching the exact schema below
3. Confidence must be between 0.0 and 1.0
4. Only suggest actions from the ALLOWED_ACTIONS list
5. Include clear reasoning for all decisions
6. Consider rollback plans for risky actions
7. High-priority actions (8-10) require careful justification

ALLOWED_ACTIONS:
- start_election: Initiate leader election for a context
- migrate_context: Move context to another node
- adjust_gossip: Modify gossip protocol parameters
- scale_queue: Adjust queue capacity
- wait: Take no action, continue monitoring
- restart_component: Restart a specific component

RESPONSE FORMAT (strict JSON):
{
  "decision": "action_name",
  "confidence": 0.85,
  "reasoning": "Clear explanation of why this action is needed",
  "actions": [
    {
      "action_type": "action_name",
      "parameters": {"key": "value"},
      "priority": 5,
      "requires_confirmation": false
    }
  ],
  "estimated_impact": "Expected outcome description",
  "rollback_plan": "How to undo if needed"
}

PRIORITY GUIDELINES:
- 1-3: Low priority, can be deferred
- 4-6: Normal priority, should be addressed soon
- 7-8: High priority, address promptly
- 9-10: Critical, immediate action needed (requires strong justification)
"#;

/// System prompt for context decisions (moderate, semantics-focused)
pub const CONTEXT_SYSTEM_PROMPT: &str = r#"You are a context orchestrator for a decentralized AI network.
Your role is to manage inference contexts and ensure optimal distribution of computational work.

RULES:
1. Balance between performance and resource utilization
2. Respond ONLY with valid JSON matching the exact schema below
3. Confidence must be between 0.0 and 1.0
4. Consider semantic relationships between contexts
5. Optimize for inference quality and latency
6. Ensure fair distribution across nodes

ALLOWED_ACTIONS:
- migrate_context: Move context to another node
- merge_contexts: Combine related contexts
- split_context: Divide overloaded context
- adjust_priority: Change context priority
- wait: Continue monitoring
- preload_model: Preload model for expected workload

RESPONSE FORMAT (strict JSON):
{
  "decision": "action_name",
  "confidence": 0.85,
  "reasoning": "Clear explanation based on context semantics and load",
  "actions": [
    {
      "action_type": "action_name",
      "parameters": {"key": "value"},
      "priority": 5,
      "requires_confirmation": false
    }
  ],
  "estimated_impact": "Expected outcome description",
  "rollback_plan": "How to undo if needed"
}
"#;

/// System prompt for mediation decisions (creative, conflict-resolution focused)
pub const MEDIATION_SYSTEM_PROMPT: &str = r#"You are a conflict mediator for a decentralized AI network.
Your role is to resolve disputes and conflicts between nodes, contexts, or participants.

RULES:
1. Be CREATIVE in finding win-win solutions
2. Consider fairness and all parties' perspectives
3. Respond ONLY with valid JSON matching the exact schema below
4. Confidence must be between 0.0 and 1.0
5. Prefer solutions that preserve relationships
6. Document clear reasoning for dispute resolution

ALLOWED_ACTIONS:
- arbitrate: Make a binding decision on a dispute
- negotiate: Propose a compromise solution
- escalate: Escalate to human intervention
- split_resources: Divide contested resources fairly
- wait: Allow time for natural resolution
- enforce_policy: Apply network policy

RESPONSE FORMAT (strict JSON):
{
  "decision": "action_name",
  "confidence": 0.85,
  "reasoning": "Explanation of mediation approach and fairness considerations",
  "actions": [
    {
      "action_type": "action_name",
      "parameters": {"key": "value"},
      "priority": 5,
      "requires_confirmation": true
    }
  ],
  "estimated_impact": "Expected resolution outcome",
  "rollback_plan": "Alternative approach if mediation fails"
}
"#;

/// System prompt for natural language command interpretation
pub const NATURAL_LANGUAGE_SYSTEM_PROMPT: &str = r#"You are a helpful assistant that interprets natural language commands for a decentralized AI network CLI.
Convert user requests into structured CLI commands.

AVAILABLE COMMANDS:
- node status [node-id]: Show node status
- node start [--port PORT]: Start a new node
- node stop [node-id] [--force]: Stop a node
- node list: List all nodes
- network status: Show network status
- network peers: List connected peers
- job submit [--model MODEL] [--input INPUT]: Submit a job
- job status [job-id]: Check job status
- job cancel [job-id]: Cancel a job
- orchestrator decision [--context CONTEXT]: Request orchestration decision
- orchestrator metrics: Show orchestrator metrics
- help [command]: Show help

RULES:
1. Respond ONLY with valid JSON
2. Map natural language to appropriate CLI commands
3. Mark destructive commands (stop, cancel, --force) appropriately
4. Request confirmation for destructive actions
5. Provide clear explanations of what commands will do

RESPONSE FORMAT (strict JSON):
{
  "intent": "brief description of user intent",
  "commands": [
    {
      "command": "command_name",
      "args": ["arg1", "arg2"],
      "is_destructive": false,
      "description": "What this command does"
    }
  ],
  "confidence": 0.9,
  "requires_confirmation": false,
  "explanation": "Human-readable explanation of what will happen"
}
"#;

/// Prompt templates container
pub struct PromptTemplates;

impl PromptTemplates {
    /// Get the system prompt for a specific decision type
    pub fn get_system_prompt(decision_type: DecisionType) -> &'static str {
        match decision_type {
            DecisionType::Infrastructure => INFRASTRUCTURE_SYSTEM_PROMPT,
            DecisionType::Context => CONTEXT_SYSTEM_PROMPT,
            DecisionType::Mediation => MEDIATION_SYSTEM_PROMPT,
        }
    }
    
    /// Get the natural language interpretation prompt
    pub fn get_natural_language_prompt() -> &'static str {
        NATURAL_LANGUAGE_SYSTEM_PROMPT
    }
    
    /// Build a complete prompt with state and context
    pub fn build_decision_prompt(
        decision_type: DecisionType,
        state: &LLMState,
        user_context: &str,
    ) -> String {
        let system = Self::get_system_prompt(decision_type);
        let state_summary = state.to_summary();
        
        format!(
            "{}\n\n## CURRENT SYSTEM STATE\n{}\n\n## USER CONTEXT\n{}\n\n## YOUR DECISION\nAnalyze the state and context, then provide your decision in the exact JSON format specified above.",
            system,
            state_summary,
            user_context
        )
    }
    
    /// Build a natural language interpretation prompt
    pub fn build_nl_prompt(user_input: &str) -> String {
        format!(
            "{}\n\n## USER REQUEST\n{}\n\n## YOUR INTERPRETATION\nInterpret this request and provide the appropriate CLI commands in JSON format.",
            NATURAL_LANGUAGE_SYSTEM_PROMPT,
            user_input
        )
    }
    
    /// Build a prompt for generating a fallback decision
    pub fn build_fallback_prompt(context: &str, error: &str) -> String {
        format!(
            r#"The LLM could not generate a valid decision due to: {}

For context: {}

Please provide a safe, conservative fallback response in the required JSON format.
When in doubt, use action "wait" with confidence 0.5."#,
            error,
            context
        )
    }
}

/// Allowed actions for each decision type
pub struct AllowedActions;

impl AllowedActions {
    /// Get allowed actions for infrastructure decisions
    pub fn infrastructure() -> Vec<&'static str> {
        vec![
            "start_election",
            "migrate_context",
            "adjust_gossip",
            "scale_queue",
            "wait",
            "restart_component",
        ]
    }
    
    /// Get allowed actions for context decisions
    pub fn context() -> Vec<&'static str> {
        vec![
            "migrate_context",
            "merge_contexts",
            "split_context",
            "adjust_priority",
            "wait",
            "preload_model",
        ]
    }
    
    /// Get allowed actions for mediation decisions
    pub fn mediation() -> Vec<&'static str> {
        vec![
            "arbitrate",
            "negotiate",
            "escalate",
            "split_resources",
            "wait",
            "enforce_policy",
        ]
    }
    
    /// Get all allowed actions for a decision type
    pub fn for_type(decision_type: DecisionType) -> Vec<&'static str> {
        match decision_type {
            DecisionType::Infrastructure => Self::infrastructure(),
            DecisionType::Context => Self::context(),
            DecisionType::Mediation => Self::mediation(),
        }
    }
    
    /// Check if an action is allowed for a decision type
    pub fn is_allowed(action: &str, decision_type: DecisionType) -> bool {
        Self::for_type(decision_type).contains(&action)
    }
    
    /// Get all unique allowed actions across all types
    pub fn all() -> Vec<&'static str> {
        let mut all = Vec::new();
        all.extend(Self::infrastructure());
        all.extend(Self::context());
        all.extend(Self::mediation());
        all.sort();
        all.dedup();
        all
    }
}

/// Parameter limits for safety validation
pub struct ParameterLimits;

impl ParameterLimits {
    /// Get parameter limits for scale_queue action
    pub fn scale_queue() -> ParameterBounds {
        ParameterBounds {
            name: "scale_queue".to_string(),
            params: vec![
                ("new_size", 1, 1000),
                ("timeout_ms", 100, 60000),
            ],
        }
    }
    
    /// Get parameter limits for adjust_gossip action
    pub fn adjust_gossip() -> ParameterBounds {
        ParameterBounds {
            name: "adjust_gossip".to_string(),
            params: vec![
                ("interval_ms", 100, 60000),
                ("fanout", 1, 20),
            ],
        }
    }
    
    /// Get parameter limits for migrate_context action
    pub fn migrate_context() -> ParameterBounds {
        ParameterBounds {
            name: "migrate_context".to_string(),
            params: vec![
                ("timeout_secs", 1, 300),
                ("max_retries", 0, 5),
            ],
        }
    }
}

/// Parameter bounds for an action
#[derive(Debug, Clone)]
pub struct ParameterBounds {
    pub name: String,
    pub params: Vec<(&'static str, i64, i64)>, // (name, min, max)
}

impl ParameterBounds {
    /// Check if a parameter value is within bounds
    pub fn check(&self, param_name: &str, value: i64) -> bool {
        self.params
            .iter()
            .find(|(name, _, _)| *name == param_name)
            .map(|(_, min, max)| value >= *min && value <= *max)
            .unwrap_or(true) // Unknown params are allowed
    }
    
    /// Get bounds for a parameter
    pub fn get_bounds(&self, param_name: &str) -> Option<(i64, i64)> {
        self.params
            .iter()
            .find(|(name, _, _)| *name == param_name)
            .map(|(_, min, max)| (*min, *max))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::types::*;

    #[test]
    fn test_get_system_prompt() {
        let prompt = PromptTemplates::get_system_prompt(DecisionType::Infrastructure);
        assert!(prompt.contains("CONSERVATIVE"));
        assert!(prompt.contains("ALLOWED_ACTIONS"));
        
        let prompt = PromptTemplates::get_system_prompt(DecisionType::Mediation);
        assert!(prompt.contains("CREATIVE"));
    }

    #[test]
    fn test_build_decision_prompt() {
        let state = LLMState::new(
            NodeState {
                id: "test-node".to_string(),
                capacity: CapacityLevel::Medium,
                latency_avg: 100,
                uptime_seconds: 3600,
                error_count: 0,
            },
            ContextState {
                id: "ctx-1".to_string(),
                health: HealthLevel::Healthy,
                queue_size: 10,
                max_queue: 100,
                processing_jobs: 2,
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
        );
        
        let prompt = PromptTemplates::build_decision_prompt(
            DecisionType::Infrastructure,
            &state,
            "Queue is growing slowly",
        );
        
        assert!(prompt.contains("CURRENT SYSTEM STATE"));
        assert!(prompt.contains("test-node"));
        assert!(prompt.contains("Queue is growing slowly"));
    }

    #[test]
    fn test_allowed_actions() {
        assert!(AllowedActions::is_allowed("wait", DecisionType::Infrastructure));
        assert!(AllowedActions::is_allowed("scale_queue", DecisionType::Infrastructure));
        assert!(!AllowedActions::is_allowed("arbitrate", DecisionType::Infrastructure));
        
        assert!(AllowedActions::is_allowed("arbitrate", DecisionType::Mediation));
    }

    #[test]
    fn test_parameter_bounds() {
        let bounds = ParameterLimits::scale_queue();
        
        assert!(bounds.check("new_size", 100));
        assert!(bounds.check("new_size", 1));
        assert!(!bounds.check("new_size", 0));
        assert!(!bounds.check("new_size", 2000));
        
        assert_eq!(bounds.get_bounds("new_size"), Some((1, 1000)));
    }

    #[test]
    fn test_all_allowed_actions_unique() {
        let all = AllowedActions::all();
        let unique_count = all.len();
        
        let mut deduped = all.clone();
        deduped.sort();
        deduped.dedup();
        
        assert_eq!(unique_count, deduped.len());
    }
}

