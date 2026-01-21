//! LLM Orchestrator Demo
//!
//! This example demonstrates the usage of the LLM Orchestrator
//! for cognitive orchestration of infrastructure decisions.
//!
//! Run with: cargo run --example orchestrator_demo

use std::sync::Arc;
use mvp_node::monitoring::DefaultHealthMonitor;
use mvp_node::orchestration::{
    LLMOrchestrator, OrchestratorConfig, Orchestrator,
    DecisionType, DatasetFormat, Decision,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🤖 LLM Orchestrator Demo");
    println!("========================");
    println!();

    // Create a health monitor (simulating node monitoring)
    let monitor = Arc::new(DefaultHealthMonitor::new());

    // Create orchestrator with default configuration
    let config = OrchestratorConfig::default();
    let orchestrator = LLMOrchestrator::new(
        config,
        monitor,
        "demo-node-1".to_string(),
        100, // max_queue_size
    );

    println!("✅ Orchestrator initialized");
    println!("   Enabled: {}", orchestrator.is_enabled());
    println!();

    // Demo 1: Infrastructure Decision
    println!("📊 Demo 1: Infrastructure Decision");
    println!("-----------------------------------");
    
    let context = "High CPU usage detected (85%), queue depth at 50/100, \
                   memory pressure increasing";
    
    match orchestrator.make_decision(context, DecisionType::Infrastructure).await {
        Ok(decision) => print_decision(&decision),
        Err(e) => println!("   Error: {}", e),
    }
    println!();

    // Demo 2: Context Management Decision
    println!("📊 Demo 2: Context Management Decision");
    println!("---------------------------------------");
    
    let context = "Context size approaching limit (90%), \
                   multiple large conversations active";
    
    match orchestrator.make_decision(context, DecisionType::Context).await {
        Ok(decision) => print_decision(&decision),
        Err(e) => println!("   Error: {}", e),
    }
    println!();

    // Demo 3: Natural Language Interpretation
    println!("💬 Demo 3: Natural Language Interpretation");
    println!("-------------------------------------------");
    
    let queries = [
        "show me the status of all nodes",
        "start a new election for coordinator",
        "what's the current queue depth?",
        "migrate context to a new node",
    ];
    
    for query in queries {
        println!("   Query: \"{}\"", query);
        match orchestrator.interpret_natural_language(query).await {
            Ok(result) => {
                println!("   Intent: {}", result.intent);
                println!("   Confidence: {:.1}%", result.confidence * 100.0);
                if !result.commands.is_empty() {
                    println!("   Commands: {:?}", result.commands[0].command);
                }
                if result.requires_confirmation {
                    println!("   ⚠️ Requires confirmation");
                }
            }
            Err(e) => println!("   Error: {}", e),
        }
        println!();
    }

    // Demo 4: Get Current State
    println!("📈 Demo 4: Current System State");
    println!("--------------------------------");
    
    match orchestrator.get_current_state().await {
        Ok(state) => {
            println!("   Node ID: {}", state.node.id);
            println!("   Capacity: {}", state.node.capacity);
            println!("   Health: {}", state.context.health);
            println!("   Queue: {}/{}", state.context.queue_size, state.context.max_queue);
            println!("   Model Loaded: {}", state.model.loaded);
            println!("   Connected Peers: {}", state.network.active_connections);
        }
        Err(e) => println!("   Error: {}", e),
    }
    println!();

    // Demo 5: Metrics
    println!("📊 Demo 5: Orchestrator Metrics");
    println!("--------------------------------");
    
    let metrics = orchestrator.get_metrics();
    println!("   Decisions Made: {}", metrics.decisions_made);
    println!("   Success Rate: {:.1}%", metrics.decision_success_rate() * 100.0);
    println!("   Avg Latency: {:.1}ms", metrics.avg_inference_latency_ms);
    println!("   Cache Hit Rate: {:.1}%", metrics.cache_hit_rate() * 100.0);
    println!("   Training Examples: {}", metrics.training_examples_collected);
    println!();

    // Demo 6: Training Data Export (if any collected)
    println!("📤 Demo 6: Training Data Export");
    println!("--------------------------------");
    
    match orchestrator.export_training_data("/tmp/training_demo.jsonl", DatasetFormat::Jsonl).await {
        Ok(count) => println!("   Exported {} examples", count),
        Err(e) => println!("   Export skipped: {}", e),
    }
    println!();

    println!("🎉 Demo complete!");
    println!();
    println!("Available CLI commands:");
    println!("  mvp-node orchestrator decision --context \"...\" --decision-type infrastructure");
    println!("  mvp-node orchestrator ask \"natural language query\"");
    println!("  mvp-node orchestrator metrics");
    println!("  mvp-node orchestrator status");
    println!("  mvp-node orchestrator export-data --output ./training.jsonl");

    Ok(())
}

fn print_decision(decision: &Decision) {
    println!("   Decision: {}", decision.decision);
    println!("   Confidence: {:.1}%", decision.confidence * 100.0);
    println!("   Reasoning: {}", decision.reasoning);
    
    if !decision.actions.is_empty() {
        println!("   Actions:");
        for action in &decision.actions {
            println!("     - {} (priority: {})", action.action_type, action.priority);
        }
    }
    
    if let Some(impact) = &decision.estimated_impact {
        println!("   Impact: {}", impact);
    }
}

