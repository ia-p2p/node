//! Job executor implementation for MVP Node
//! 
//! This module implements the job queue and executor system for processing
//! inference jobs sequentially with proper concurrency control.

use crate::{
    JobExecutor, JobOffer, JobResult, JobStatus, QueueStatus, 
    ExecutionMetrics, InferenceEngine, Requirements, JobMode
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};

/// Internal job representation with metadata
#[derive(Debug, Clone)]
struct Job {
    pub offer: JobOffer,
    pub status: JobStatus,
    pub submitted_at: std::time::Instant,
}

/// Thread-safe job queue with size limits
#[derive(Debug)]
pub struct JobQueue {
    queue: VecDeque<Job>,
    max_size: usize,
    jobs_by_id: HashMap<String, Job>,
}

impl JobQueue {
    /// Create a new job queue with specified maximum size
    pub fn new(max_size: usize) -> Self {
        Self {
            queue: VecDeque::with_capacity(max_size),
            max_size,
            jobs_by_id: HashMap::new(),
        }
    }

    /// Check if the queue is full
    pub fn is_full(&self) -> bool {
        self.queue.len() >= self.max_size
    }

    /// Get the number of pending jobs
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Check if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Add a job to the queue
    /// Returns error if queue is full
    pub fn push(&mut self, offer: JobOffer) -> Result<()> {
        if self.is_full() {
            return Err(anyhow!(
                "Job queue is full (max size: {}). Cannot accept new jobs.",
                self.max_size
            ));
        }

        let job = Job {
            offer: offer.clone(),
            status: JobStatus::Completed, // Will be set properly when processing
            submitted_at: std::time::Instant::now(),
        };

        self.jobs_by_id.insert(offer.job_id.clone(), job.clone());
        self.queue.push_back(job);

        debug!("Job {} added to queue. Queue size: {}", offer.job_id, self.queue.len());
        Ok(())
    }

    /// Remove and return the next job from the queue
    pub fn pop(&mut self) -> Option<Job> {
        self.queue.pop_front()
    }

    /// Get job by ID
    pub fn get_job(&self, job_id: &str) -> Option<&Job> {
        self.jobs_by_id.get(job_id)
    }

    /// Remove a job by ID
    pub fn remove_job(&mut self, job_id: &str) -> Option<Job> {
        if let Some(job) = self.jobs_by_id.remove(job_id) {
            // Also remove from queue
            self.queue.retain(|j| j.offer.job_id != job_id);
            Some(job)
        } else {
            None
        }
    }

    /// Get all pending job IDs
    pub fn get_pending_job_ids(&self) -> Vec<String> {
        self.queue.iter().map(|j| j.offer.job_id.clone()).collect()
    }
}

/// Default job executor implementation
pub struct DefaultJobExecutor {
    /// Thread-safe job queue
    queue: Arc<Mutex<JobQueue>>,
    
    /// Currently processing job ID
    processing_job: Arc<RwLock<Option<String>>>,
    
    /// Completed jobs count
    completed_jobs: Arc<RwLock<usize>>,
    
    /// Failed jobs count
    failed_jobs: Arc<RwLock<usize>>,
    
    /// Maximum queue size
    max_queue_size: usize,
    
    /// Inference engine (optional for now, will be integrated in Task 6)
    inference_engine: Option<Arc<Mutex<dyn InferenceEngine>>>,
}

impl DefaultJobExecutor {
    /// Create a new job executor with specified queue size
    pub fn new(max_queue_size: usize) -> Self {
        Self {
            queue: Arc::new(Mutex::new(JobQueue::new(max_queue_size))),
            processing_job: Arc::new(RwLock::new(None)),
            completed_jobs: Arc::new(RwLock::new(0)),
            failed_jobs: Arc::new(RwLock::new(0)),
            max_queue_size,
            inference_engine: None,
        }
    }

    /// Create a new job executor with an inference engine
    pub fn with_inference_engine(
        max_queue_size: usize,
        engine: Arc<Mutex<dyn InferenceEngine>>,
    ) -> Self {
        Self {
            queue: Arc::new(Mutex::new(JobQueue::new(max_queue_size))),
            processing_job: Arc::new(RwLock::new(None)),
            completed_jobs: Arc::new(RwLock::new(0)),
            failed_jobs: Arc::new(RwLock::new(0)),
            max_queue_size,
            inference_engine: Some(engine),
        }
    }

    /// Set the inference engine
    pub fn set_inference_engine(&mut self, engine: Arc<Mutex<dyn InferenceEngine>>) {
        self.inference_engine = Some(engine);
    }

    /// Check if an inference engine is configured
    pub fn has_inference_engine(&self) -> bool {
        self.inference_engine.is_some()
    }

    /// Validate job offer
    fn validate_job(&self, job: &JobOffer) -> Result<()> {
        // Validate job_id is not empty
        if job.job_id.is_empty() {
            return Err(anyhow!("Job ID cannot be empty"));
        }

        // Validate model is specified
        if job.model.is_empty() {
            return Err(anyhow!("Model name cannot be empty"));
        }

        // Validate input_data is not empty
        if job.input_data.is_empty() {
            return Err(anyhow!("Input data cannot be empty"));
        }

        Ok(())
    }

    /// Validate job input against model requirements
    async fn validate_input_against_model(&self, job: &JobOffer) -> Result<()> {
        if let Some(ref engine) = self.inference_engine {
            let engine_guard = engine.lock().await;
            
            // Check if model is loaded
            if !engine_guard.is_model_loaded() {
                return Err(anyhow!("No model loaded for inference"));
            }

            // Validate input against model requirements
            engine_guard.validate_input(&job.input_data)?;

            // Check model compatibility if specified
            if let Some(ref model_info) = engine_guard.get_model_info() {
                // Verify the job's model requirement matches loaded model
                if !job.model.is_empty() && !model_info.name.to_lowercase().contains(&job.model.to_lowercase()) {
                    warn!(
                        "Job requests model '{}' but '{}' is loaded",
                        job.model, model_info.name
                    );
                }
            }
        }
        Ok(())
    }

    /// Process a single job using the inference engine
    async fn execute_job(&self, job: &JobOffer) -> JobResult {
        let start_time = std::time::Instant::now();
        
        info!("Processing job: {} with model: {}", job.job_id, job.model);
        
        // Validate input against model requirements (Property 7: Requirement 2.2)
        if let Err(e) = self.validate_input_against_model(job).await {
            warn!("Input validation failed for job {}: {}", job.job_id, e);
            return self.create_error_result(job, &e.to_string(), start_time);
        }

        // Execute inference using the inference engine (Property 8: Requirement 2.3)
        if let Some(ref engine) = self.inference_engine {
            let engine_guard = engine.lock().await;
            
            match engine_guard.infer(&job.input_data).await {
                Ok(inference_result) => {
                    let duration = start_time.elapsed().as_millis() as u64;
                    
                    // Return result with execution metadata (Property 9: Requirement 2.4)
                    JobResult {
                        job_id: job.job_id.clone(),
                        status: JobStatus::Completed,
                        output: Some(inference_result.output),
                        metrics: ExecutionMetrics {
                            duration_ms: duration,
                            tokens_processed: Some(inference_result.tokens_processed),
                            peak_memory_mb: Some(inference_result.memory_used_mb),
                        },
                        error: None,
                    }
                }
                Err(e) => {
                    // Return error response with failure details (Property 10: Requirement 2.5)
                    warn!("Inference failed for job {}: {}", job.job_id, e);
                    self.create_error_result(job, &e.to_string(), start_time)
                }
            }
        } else {
            // No inference engine configured - use placeholder (for testing)
            let duration = start_time.elapsed().as_millis() as u64;
            
            JobResult {
                job_id: job.job_id.clone(),
                status: JobStatus::Completed,
                output: Some(format!("Placeholder result for job {} (no inference engine)", job.job_id)),
                metrics: ExecutionMetrics {
                    duration_ms: duration,
                    tokens_processed: Some(job.input_data.len() as u64 / 4),
                    peak_memory_mb: Some(100),
                },
                error: None,
            }
        }
    }

    /// Create an error result with failure details
    fn create_error_result(&self, job: &JobOffer, error_msg: &str, start_time: std::time::Instant) -> JobResult {
        let duration = start_time.elapsed().as_millis() as u64;
        
        JobResult {
            job_id: job.job_id.clone(),
            status: JobStatus::Failed,
            output: None,
            metrics: ExecutionMetrics {
                duration_ms: duration,
                tokens_processed: Some(0),
                peak_memory_mb: Some(0),
            },
            error: Some(error_msg.to_string()),
        }
    }
}

#[async_trait]
impl JobExecutor for DefaultJobExecutor {
    /// Submit a job to the executor
    /// Returns the job ID on success, or error if queue is full or job is invalid
    async fn submit_job(&mut self, job: JobOffer) -> Result<String> {
        // Validate job
        self.validate_job(&job)?;
        
        let job_id = job.job_id.clone();
        
        // Add to queue
        let mut queue = self.queue.lock().await;
        queue.push(job)?;
        
        info!("Job {} submitted successfully", job_id);
        Ok(job_id)
    }

    /// Process the next job in the queue
    /// Returns None if queue is empty
    async fn process_next_job(&mut self) -> Result<Option<JobResult>> {
        // Check if already processing a job
        {
            let processing = self.processing_job.read().await;
            if processing.is_some() {
                return Err(anyhow!("Already processing a job: {:?}", processing));
            }
        }

        // Get next job from queue
        let job = {
            let mut queue = self.queue.lock().await;
            queue.pop()
        };

        if let Some(job) = job {
            // Mark as processing
            {
                let mut processing = self.processing_job.write().await;
                *processing = Some(job.offer.job_id.clone());
            }

            // Execute the job
            let result = self.execute_job(&job.offer).await;

            // Update counters based on result
            match result.status {
                JobStatus::Completed => {
                    let mut completed = self.completed_jobs.write().await;
                    *completed += 1;
                    info!("Job {} completed successfully", job.offer.job_id);
                }
                JobStatus::Failed | JobStatus::Timeout => {
                    let mut failed = self.failed_jobs.write().await;
                    *failed += 1;
                    warn!("Job {} failed: {:?}", job.offer.job_id, result.error);
                }
            }

            // Clear processing status
            {
                let mut processing = self.processing_job.write().await;
                *processing = None;
            }

            Ok(Some(result))
        } else {
            // No jobs in queue
            Ok(None)
        }
    }

    /// Get current queue status
    fn get_queue_status(&self) -> QueueStatus {
        // Use try_lock to avoid blocking in async context
        let pending_jobs = self.queue.try_lock()
            .map(|q| q.len())
            .unwrap_or(0);
        
        let processing_job = self.processing_job.try_read()
            .map(|p| p.clone())
            .ok()
            .flatten();
        
        let completed_jobs = self.completed_jobs.try_read()
            .map(|c| *c)
            .unwrap_or(0);
        
        let failed_jobs = self.failed_jobs.try_read()
            .map(|f| *f)
            .unwrap_or(0);

        QueueStatus {
            pending_jobs,
            processing_job,
            completed_jobs,
            failed_jobs,
        }
    }

    /// Cancel a specific job
    async fn cancel_job(&mut self, job_id: &str) -> Result<()> {
        let mut queue = self.queue.lock().await;
        
        if let Some(_job) = queue.remove_job(job_id) {
            info!("Job {} cancelled successfully", job_id);
            Ok(())
        } else {
            Err(anyhow!("Job {} not found in queue", job_id))
        }
    }

    /// Get maximum queue size
    fn get_max_queue_size(&self) -> usize {
        self.max_queue_size
    }

    /// Check if queue is full
    fn is_queue_full(&self) -> bool {
        self.queue.try_lock()
            .map(|q| q.is_full())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_job(id: &str) -> JobOffer {
        JobOffer {
            job_id: id.to_string(),
            model: "test-model".to_string(),
            mode: JobMode::Batch,
            reward: 10.0,
            currency: "USD".to_string(),
            requirements: Requirements::default(),
            input_data: "test input".to_string(),
        }
    }

    #[test]
    fn test_job_queue_creation() {
        let queue = JobQueue::new(10);
        assert_eq!(queue.len(), 0);
        assert!(queue.is_empty());
        assert!(!queue.is_full());
    }

    #[test]
    fn test_job_queue_push_pop() {
        let mut queue = JobQueue::new(10);
        let job = create_test_job("job1");
        
        assert!(queue.push(job.clone()).is_ok());
        assert_eq!(queue.len(), 1);
        
        let popped = queue.pop();
        assert!(popped.is_some());
        assert_eq!(popped.unwrap().offer.job_id, "job1");
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn test_job_queue_full() {
        let mut queue = JobQueue::new(2);
        
        assert!(queue.push(create_test_job("job1")).is_ok());
        assert!(queue.push(create_test_job("job2")).is_ok());
        assert!(queue.is_full());
        
        let result = queue.push(create_test_job("job3"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("full"));
    }

    #[tokio::test]
    async fn test_executor_submit_job() {
        let mut executor = DefaultJobExecutor::new(10);
        let job = create_test_job("job1");
        
        let result = executor.submit_job(job).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "job1");
        
        let status = executor.get_queue_status();
        assert_eq!(status.pending_jobs, 1);
    }

    #[tokio::test]
    async fn test_executor_rejects_invalid_job() {
        let mut executor = DefaultJobExecutor::new(10);
        let mut job = create_test_job("job1");
        job.job_id = String::new(); // Invalid: empty job_id
        
        let result = executor.submit_job(job).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Job ID"));
    }

    #[tokio::test]
    async fn test_executor_queue_overflow() {
        let mut executor = DefaultJobExecutor::new(2);
        
        assert!(executor.submit_job(create_test_job("job1")).await.is_ok());
        assert!(executor.submit_job(create_test_job("job2")).await.is_ok());
        
        let result = executor.submit_job(create_test_job("job3")).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("full"));
    }

    #[tokio::test]
    async fn test_executor_process_job() {
        let mut executor = DefaultJobExecutor::new(10);
        executor.submit_job(create_test_job("job1")).await.unwrap();
        
        let result = executor.process_next_job().await;
        assert!(result.is_ok());
        
        let job_result = result.unwrap();
        assert!(job_result.is_some());
        
        let job_result = job_result.unwrap();
        assert_eq!(job_result.job_id, "job1");
        assert_eq!(job_result.status, JobStatus::Completed);
    }

    #[tokio::test]
    async fn test_executor_cancel_job() {
        let mut executor = DefaultJobExecutor::new(10);
        executor.submit_job(create_test_job("job1")).await.unwrap();
        executor.submit_job(create_test_job("job2")).await.unwrap();
        
        let result = executor.cancel_job("job1").await;
        assert!(result.is_ok());
        
        let status = executor.get_queue_status();
        assert_eq!(status.pending_jobs, 1);
    }
}
