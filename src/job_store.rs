//! Job Store for Tracking Jobs and Results
//!
//! Provides persistent (in-memory) storage for jobs, their status, and results.
//! Used by QueryHandler to respond to job-related queries.

use crate::protocol::queries::{JobStatusInfo, JobInfo, JobResultInfo, JobExecutionMetrics, JobFilter};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Status of a tracked job
#[derive(Debug, Clone, PartialEq)]
pub enum TrackedJobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for TrackedJobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrackedJobStatus::Pending => write!(f, "pending"),
            TrackedJobStatus::Running => write!(f, "running"),
            TrackedJobStatus::Completed => write!(f, "completed"),
            TrackedJobStatus::Failed => write!(f, "failed"),
            TrackedJobStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// A tracked job with all metadata
#[derive(Debug, Clone)]
pub struct TrackedJob {
    pub job_id: String,
    pub model: String,
    pub input: String,
    pub status: TrackedJobStatus,
    pub progress: Option<f64>,
    pub node_id: String,
    pub submitted_at: u64,
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub output: Option<String>,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub tokens_processed: Option<u64>,
}

impl TrackedJob {
    /// Create a new pending job
    pub fn new(job_id: String, model: String, input: String, node_id: String) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        
        Self {
            job_id,
            model,
            input,
            status: TrackedJobStatus::Pending,
            progress: None,
            node_id,
            submitted_at: now,
            started_at: None,
            completed_at: None,
            output: None,
            error: None,
            duration_ms: 0,
            tokens_processed: None,
        }
    }
    
    /// Convert to JobStatusInfo for query response
    pub fn to_status_info(&self) -> JobStatusInfo {
        JobStatusInfo {
            job_id: self.job_id.clone(),
            status: self.status.to_string(),
            progress: self.progress,
            node_id: Some(self.node_id.clone()),
            submitted_at: self.submitted_at,
            started_at: self.started_at,
            completed_at: self.completed_at,
            error: self.error.clone(),
        }
    }
    
    /// Convert to JobInfo for listing
    pub fn to_info(&self) -> JobInfo {
        JobInfo {
            job_id: self.job_id.clone(),
            status: self.status.to_string(),
            model: self.model.clone(),
            node_id: Some(self.node_id.clone()),
            submitted_at: self.submitted_at,
        }
    }
    
    /// Convert to JobResultInfo
    pub fn to_result_info(&self) -> JobResultInfo {
        JobResultInfo {
            job_id: self.job_id.clone(),
            status: self.status.to_string(),
            output: self.output.clone(),
            metrics: JobExecutionMetrics {
                duration_ms: self.duration_ms,
                tokens_processed: self.tokens_processed,
                input_tokens: None,
                output_tokens: None,
            },
            error: self.error.clone(),
        }
    }
}

/// Thread-safe job store
pub struct JobStore {
    /// Jobs indexed by job_id
    jobs: RwLock<HashMap<String, TrackedJob>>,
    
    /// Node ID for this store
    node_id: String,
    
    /// Maximum jobs to keep (LRU eviction)
    max_jobs: usize,
    
    /// Order of job IDs for LRU eviction
    job_order: RwLock<Vec<String>>,
}

impl JobStore {
    /// Create a new job store
    pub fn new(node_id: String, max_jobs: usize) -> Self {
        Self {
            jobs: RwLock::new(HashMap::new()),
            node_id,
            max_jobs,
            job_order: RwLock::new(Vec::new()),
        }
    }
    
    /// Submit a new job
    pub async fn submit_job(&self, job_id: String, model: String, input: String) -> TrackedJob {
        let job = TrackedJob::new(job_id.clone(), model, input, self.node_id.clone());
        
        let mut jobs = self.jobs.write().await;
        let mut order = self.job_order.write().await;
        
        // Evict oldest jobs if at capacity
        while jobs.len() >= self.max_jobs && !order.is_empty() {
            if let Some(old_id) = order.first().cloned() {
                jobs.remove(&old_id);
                order.remove(0);
                debug!("Evicted old job: {}", old_id);
            }
        }
        
        jobs.insert(job_id.clone(), job.clone());
        order.push(job_id.clone());
        
        info!("Job {} submitted and tracked", job_id);
        job
    }
    
    /// Mark job as running
    pub async fn start_job(&self, job_id: &str) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = TrackedJobStatus::Running;
            job.started_at = Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0)
            );
            job.progress = Some(0.0);
            debug!("Job {} started", job_id);
        }
    }
    
    /// Update job progress
    pub async fn update_progress(&self, job_id: &str, progress: f64) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.progress = Some(progress.clamp(0.0, 100.0));
        }
    }
    
    /// Complete job with result
    pub async fn complete_job(&self, job_id: &str, output: String, duration_ms: u64, tokens: Option<u64>) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = TrackedJobStatus::Completed;
            job.completed_at = Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0)
            );
            job.progress = Some(100.0);
            job.output = Some(output);
            job.duration_ms = duration_ms;
            job.tokens_processed = tokens;
            info!("Job {} completed in {}ms", job_id, duration_ms);
        }
    }
    
    /// Fail job with error
    pub async fn fail_job(&self, job_id: &str, error: String) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = TrackedJobStatus::Failed;
            job.completed_at = Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0)
            );
            job.error = Some(error.clone());
            info!("Job {} failed: {}", job_id, error);
        }
    }
    
    /// Cancel job
    pub async fn cancel_job(&self, job_id: &str) -> bool {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            if job.status == TrackedJobStatus::Pending || job.status == TrackedJobStatus::Running {
                job.status = TrackedJobStatus::Cancelled;
                job.completed_at = Some(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0)
                );
                info!("Job {} cancelled", job_id);
                return true;
            }
        }
        false
    }
    
    /// Get job by ID
    pub async fn get_job(&self, job_id: &str) -> Option<TrackedJob> {
        let jobs = self.jobs.read().await;
        jobs.get(job_id).cloned()
    }
    
    /// Get job status
    pub async fn get_job_status(&self, job_id: &str) -> Option<JobStatusInfo> {
        let jobs = self.jobs.read().await;
        jobs.get(job_id).map(|j| j.to_status_info())
    }
    
    /// Get job result
    pub async fn get_job_result(&self, job_id: &str) -> Option<JobResultInfo> {
        let jobs = self.jobs.read().await;
        jobs.get(job_id).map(|j| j.to_result_info())
    }
    
    /// List jobs with optional filter
    pub async fn list_jobs(&self, filter: Option<JobFilter>) -> Vec<JobInfo> {
        let jobs = self.jobs.read().await;
        
        let mut result: Vec<JobInfo> = jobs.values()
            .filter(|job| {
                if let Some(ref f) = filter {
                    // Filter by status
                    if let Some(ref status) = f.status {
                        if job.status.to_string() != *status {
                            return false;
                        }
                    }
                    // Filter by model
                    if let Some(ref model) = f.model {
                        if !job.model.contains(model) {
                            return false;
                        }
                    }
                }
                true
            })
            .map(|j| j.to_info())
            .collect();
        
        // Sort by submitted_at descending (newest first)
        result.sort_by(|a, b| b.submitted_at.cmp(&a.submitted_at));
        
        // Apply limit
        if let Some(ref f) = filter {
            if let Some(limit) = f.limit {
                result.truncate(limit);
            }
        }
        
        result
    }
    
    /// Get total job count
    pub async fn job_count(&self) -> usize {
        let jobs = self.jobs.read().await;
        jobs.len()
    }
    
    /// Get queue position for a job
    pub async fn get_queue_position(&self, job_id: &str) -> Option<usize> {
        let jobs = self.jobs.read().await;
        
        // Count pending jobs before this one
        let job = jobs.get(job_id)?;
        if job.status != TrackedJobStatus::Pending {
            return None;
        }
        
        let mut position = 0;
        for (id, j) in jobs.iter() {
            if j.status == TrackedJobStatus::Pending && j.submitted_at < job.submitted_at {
                position += 1;
            }
        }
        
        Some(position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_submit_job() {
        let store = JobStore::new("test-node".to_string(), 100);
        
        let job = store.submit_job(
            "job-1".to_string(),
            "tinyllama".to_string(),
            "Hello".to_string(),
        ).await;
        
        assert_eq!(job.job_id, "job-1");
        assert_eq!(job.status, TrackedJobStatus::Pending);
        assert_eq!(job.node_id, "test-node");
    }
    
    #[tokio::test]
    async fn test_job_lifecycle() {
        let store = JobStore::new("test-node".to_string(), 100);
        
        store.submit_job("job-1".to_string(), "tinyllama".to_string(), "Hello".to_string()).await;
        
        // Start
        store.start_job("job-1").await;
        let status = store.get_job_status("job-1").await.unwrap();
        assert_eq!(status.status, "running");
        
        // Update progress
        store.update_progress("job-1", 50.0).await;
        let job = store.get_job("job-1").await.unwrap();
        assert_eq!(job.progress, Some(50.0));
        
        // Complete
        store.complete_job("job-1", "Output".to_string(), 100, Some(50)).await;
        let result = store.get_job_result("job-1").await.unwrap();
        assert_eq!(result.status, "completed");
        assert_eq!(result.output, Some("Output".to_string()));
        assert_eq!(result.metrics.duration_ms, 100);
    }
    
    #[tokio::test]
    async fn test_cancel_job() {
        let store = JobStore::new("test-node".to_string(), 100);
        
        store.submit_job("job-1".to_string(), "tinyllama".to_string(), "Hello".to_string()).await;
        
        let cancelled = store.cancel_job("job-1").await;
        assert!(cancelled);
        
        let status = store.get_job_status("job-1").await.unwrap();
        assert_eq!(status.status, "cancelled");
    }
    
    #[tokio::test]
    async fn test_list_jobs_with_filter() {
        let store = JobStore::new("test-node".to_string(), 100);
        
        store.submit_job("job-1".to_string(), "tinyllama".to_string(), "Input1".to_string()).await;
        store.submit_job("job-2".to_string(), "mistral".to_string(), "Input2".to_string()).await;
        store.complete_job("job-1", "Output1".to_string(), 100, None).await;
        
        // Filter by status
        let pending = store.list_jobs(Some(JobFilter {
            status: Some("pending".to_string()),
            model: None,
            limit: None,
            offset: None,
        })).await;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].job_id, "job-2");
        
        // Filter by model
        let mistral_jobs = store.list_jobs(Some(JobFilter {
            status: None,
            model: Some("mistral".to_string()),
            limit: None,
            offset: None,
        })).await;
        assert_eq!(mistral_jobs.len(), 1);
        assert_eq!(mistral_jobs[0].model, "mistral");
    }
    
    #[tokio::test]
    async fn test_lru_eviction() {
        let store = JobStore::new("test-node".to_string(), 3);
        
        store.submit_job("job-1".to_string(), "m1".to_string(), "i1".to_string()).await;
        store.submit_job("job-2".to_string(), "m2".to_string(), "i2".to_string()).await;
        store.submit_job("job-3".to_string(), "m3".to_string(), "i3".to_string()).await;
        
        assert_eq!(store.job_count().await, 3);
        
        // Adding 4th should evict 1st
        store.submit_job("job-4".to_string(), "m4".to_string(), "i4".to_string()).await;
        
        assert_eq!(store.job_count().await, 3);
        assert!(store.get_job("job-1").await.is_none());
        assert!(store.get_job("job-4").await.is_some());
    }
}



