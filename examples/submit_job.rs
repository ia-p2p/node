//! Example: Submit a job to a local MVP Node
//!
//! This example demonstrates how to programmatically submit a job
//! and process it using the MVP Node components.
//!
//! Run with: cargo run --example submit_job

use mvp_node::executor::DefaultJobExecutor;
use mvp_node::inference::DefaultInferenceEngine;
use mvp_node::{JobOffer, JobMode, Requirements, InferenceEngine, JobExecutor};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    println!("=== MVP Node Job Submission Example ===\n");

    // Create inference engine
    println!("1. Creating inference engine...");
    let engine = Arc::new(Mutex::new(DefaultInferenceEngine::new()));
    
    // Load model (using simulated model for demo)
    println!("2. Loading model (tinyllama)...");
    {
        let mut eng = engine.lock().await;
        eng.load_model("tinyllama").await?;
        
        if let Some(info) = eng.get_model_info() {
            println!("   Model loaded: {}", info.name);
            println!("   Parameters: {}B", info.parameters / 1_000_000_000);
        }
    }

    // Create job executor
    println!("3. Creating job executor...");
    let mut executor = DefaultJobExecutor::with_inference_engine(100, engine.clone());

    // Create a job with timestamp-based ID
    println!("4. Creating job offer...");
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    
    let job = JobOffer {
        job_id: format!("job-{}", timestamp),
        model: "tinyllama".to_string(),
        input_data: "What is the capital of France? Answer briefly.".to_string(),
        mode: JobMode::Batch,
        requirements: Requirements::default(),
        reward: 1.0,
        currency: "USD".to_string(),
    };
    
    println!("   Job ID: {}", job.job_id);
    println!("   Input: {}", job.input_data);

    // Submit job
    println!("\n5. Submitting job to queue...");
    let job_id = executor.submit_job(job).await?;
    println!("   Job submitted: {}", job_id);

    // Check queue status
    let status = executor.get_queue_status();
    println!("   Queue status: {} pending jobs", status.pending_jobs);

    // Process job
    println!("\n6. Processing job...");
    let start = std::time::Instant::now();
    
    match executor.process_next_job().await? {
        Some(result) => {
            let duration = start.elapsed();
            
            println!("\n=== Job Result ===");
            println!("   Job ID: {}", result.job_id);
            println!("   Status: {:?}", result.status);
            println!("   Duration: {:?}", duration);
            println!("   Processing time: {}ms", result.metrics.duration_ms);
            
            if let Some(output) = result.output {
                println!("\n   Output:");
                println!("   {}", output);
            }
            
            if let Some(error) = result.error {
                println!("\n   Error: {}", error);
            }
        }
        None => {
            println!("   No job to process (queue empty)");
        }
    }

    println!("\n=== Done ===");
    Ok(())
}

