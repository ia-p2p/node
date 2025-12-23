//! Resource Management and Load Control for MVP Node
//!
//! This module provides:
//! - System resource monitoring (memory, CPU)
//! - Load management based on resource availability
//! - Fair job scheduling to prevent starvation
//! - Memory management strategies
//!
//! Implements Requirements 4.5, 6.2, 6.5

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// System resource snapshot
#[derive(Debug, Clone, Serialize)]
pub struct SystemResources {
    /// Available memory in bytes
    pub available_memory_bytes: u64,
    /// Total memory in bytes
    pub total_memory_bytes: u64,
    /// Memory usage percentage (0.0 - 100.0)
    pub memory_usage_percent: f64,
    /// CPU usage percentage (0.0 - 100.0)
    pub cpu_usage_percent: f64,
    /// Number of available CPU cores
    pub cpu_cores: usize,
    /// Timestamp when this snapshot was taken (not serialized)
    #[serde(skip)]
    pub timestamp: Instant,
}

impl Default for SystemResources {
    fn default() -> Self {
        Self {
            available_memory_bytes: 8 * 1024 * 1024 * 1024, // 8 GB default
            total_memory_bytes: 8 * 1024 * 1024 * 1024,
            memory_usage_percent: 0.0,
            cpu_usage_percent: 0.0,
            cpu_cores: num_cpus::get(),
            timestamp: Instant::now(),
        }
    }
}

impl SystemResources {
    /// Check if memory is under pressure (>80% used)
    pub fn is_memory_pressure(&self) -> bool {
        self.memory_usage_percent > 80.0
    }

    /// Check if CPU is under pressure (>90% used)
    pub fn is_cpu_pressure(&self) -> bool {
        self.cpu_usage_percent > 90.0
    }

    /// Check if system is under any resource pressure
    pub fn is_under_pressure(&self) -> bool {
        self.is_memory_pressure() || self.is_cpu_pressure()
    }
}

/// Load management thresholds and configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadConfig {
    /// Maximum memory usage percentage before rejecting new jobs
    pub max_memory_percent: f64,
    /// Maximum CPU usage percentage before throttling
    pub max_cpu_percent: f64,
    /// Minimum memory required for accepting new jobs (in bytes)
    pub min_available_memory_bytes: u64,
    /// Enable automatic load shedding
    pub enable_load_shedding: bool,
    /// Enable job aging for fair scheduling
    pub enable_job_aging: bool,
    /// Job aging interval in seconds
    pub aging_interval_secs: u64,
    /// Priority boost per aging interval
    pub aging_priority_boost: i32,
    /// Maximum priority from aging
    pub max_aging_priority: i32,
}

impl Default for LoadConfig {
    fn default() -> Self {
        Self {
            max_memory_percent: 85.0,
            max_cpu_percent: 95.0,
            min_available_memory_bytes: 512 * 1024 * 1024, // 512 MB
            enable_load_shedding: true,
            enable_job_aging: true,
            aging_interval_secs: 30,
            aging_priority_boost: 1,
            max_aging_priority: 10,
        }
    }
}

/// Load management decision
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadDecision {
    /// Accept the job normally
    Accept,
    /// Reject the job due to resource constraints
    Reject { reason: String },
    /// Throttle - accept but with delay
    Throttle { delay_ms: u64 },
    /// Defer - put in low priority queue
    Defer,
}

/// Job priority for fair scheduling
#[derive(Debug, Clone)]
pub struct JobPriority {
    /// Base priority (higher = more important)
    pub base_priority: i32,
    /// Priority boost from aging
    pub age_boost: i32,
    /// Time when job was queued
    pub queued_at: Instant,
    /// Last time aging was applied
    pub last_aging: Instant,
    /// Job ID for tracking
    pub job_id: String,
    /// Submitter ID for fairness tracking
    pub submitter_id: String,
}

impl JobPriority {
    pub fn new(job_id: String, submitter_id: String, base_priority: i32) -> Self {
        let now = Instant::now();
        Self {
            base_priority,
            age_boost: 0,
            queued_at: now,
            last_aging: now,
            job_id,
            submitter_id,
        }
    }

    /// Get effective priority (base + age boost)
    pub fn effective_priority(&self) -> i32 {
        self.base_priority + self.age_boost
    }

    /// Get wait time in seconds
    pub fn wait_time_secs(&self) -> u64 {
        self.queued_at.elapsed().as_secs()
    }

    /// Apply aging if enough time has passed
    pub fn apply_aging(&mut self, config: &LoadConfig) {
        if !config.enable_job_aging {
            return;
        }

        // Prevent division by zero - if interval is 0, always apply boost
        if config.aging_interval_secs == 0 {
            let new_boost = (self.age_boost + config.aging_priority_boost)
                .min(config.max_aging_priority);
            if new_boost > self.age_boost {
                self.age_boost = new_boost;
                self.last_aging = Instant::now();
            }
            return;
        }

        let elapsed = self.last_aging.elapsed();
        if elapsed.as_secs() >= config.aging_interval_secs {
            let intervals = (elapsed.as_secs() / config.aging_interval_secs) as i32;
            let new_boost = (self.age_boost + intervals * config.aging_priority_boost)
                .min(config.max_aging_priority);
            
            if new_boost > self.age_boost {
                debug!(
                    "Job {} aged: priority boost {} -> {}",
                    self.job_id, self.age_boost, new_boost
                );
                self.age_boost = new_boost;
                self.last_aging = Instant::now();
            }
        }
    }
}

/// Fair job scheduler with aging support
#[derive(Debug)]
pub struct FairScheduler {
    /// Jobs by submitter for fairness tracking
    jobs_by_submitter: HashMap<String, Vec<String>>,
    /// Job priorities
    priorities: HashMap<String, JobPriority>,
    /// Priority queue (sorted by effective priority)
    priority_order: VecDeque<String>,
    /// Maximum jobs per submitter
    max_jobs_per_submitter: usize,
    /// Load configuration
    config: LoadConfig,
}

impl FairScheduler {
    pub fn new(config: LoadConfig, max_jobs_per_submitter: usize) -> Self {
        Self {
            jobs_by_submitter: HashMap::new(),
            priorities: HashMap::new(),
            priority_order: VecDeque::new(),
            max_jobs_per_submitter,
            config,
        }
    }

    /// Add a job to the scheduler
    pub fn add_job(&mut self, job_id: String, submitter_id: String, base_priority: i32) -> Result<(), String> {
        // Check submitter limits for fairness
        let submitter_jobs = self.jobs_by_submitter.entry(submitter_id.clone()).or_default();
        if submitter_jobs.len() >= self.max_jobs_per_submitter {
            return Err(format!(
                "Submitter {} has reached maximum concurrent jobs ({})",
                submitter_id, self.max_jobs_per_submitter
            ));
        }

        // Create priority entry
        let priority = JobPriority::new(job_id.clone(), submitter_id.clone(), base_priority);
        self.priorities.insert(job_id.clone(), priority);
        submitter_jobs.push(job_id.clone());
        
        // Insert into priority order (will be sorted on next_job)
        self.priority_order.push_back(job_id);

        Ok(())
    }

    /// Get the next job to process (highest effective priority)
    pub fn next_job(&mut self) -> Option<String> {
        if self.priority_order.is_empty() {
            return None;
        }

        // Apply aging to all jobs
        for priority in self.priorities.values_mut() {
            priority.apply_aging(&self.config);
        }

        // Sort by effective priority (highest first)
        let mut jobs: Vec<_> = self.priority_order.drain(..).collect();
        jobs.sort_by(|a, b| {
            let pa = self.priorities.get(a).map(|p| p.effective_priority()).unwrap_or(0);
            let pb = self.priorities.get(b).map(|p| p.effective_priority()).unwrap_or(0);
            pb.cmp(&pa) // Descending order
        });

        // Take the highest priority job
        let job_id = jobs.remove(0);
        
        // Put remaining jobs back
        self.priority_order = jobs.into_iter().collect();

        Some(job_id)
    }

    /// Remove a completed job
    pub fn remove_job(&mut self, job_id: &str) {
        if let Some(priority) = self.priorities.remove(job_id) {
            // Remove from submitter tracking
            if let Some(submitter_jobs) = self.jobs_by_submitter.get_mut(&priority.submitter_id) {
                submitter_jobs.retain(|id| id != job_id);
                if submitter_jobs.is_empty() {
                    self.jobs_by_submitter.remove(&priority.submitter_id);
                }
            }
        }
        self.priority_order.retain(|id| id != job_id);
    }

    /// Get job priority info
    pub fn get_priority(&self, job_id: &str) -> Option<&JobPriority> {
        self.priorities.get(job_id)
    }

    /// Get queue statistics
    pub fn get_stats(&self) -> FairSchedulerStats {
        let mut total_wait_time = 0u64;
        let mut max_wait_time = 0u64;
        let mut total_priority = 0i32;

        for priority in self.priorities.values() {
            let wait = priority.wait_time_secs();
            total_wait_time += wait;
            max_wait_time = max_wait_time.max(wait);
            total_priority += priority.effective_priority();
        }

        let count = self.priorities.len();
        FairSchedulerStats {
            pending_jobs: count,
            unique_submitters: self.jobs_by_submitter.len(),
            avg_wait_time_secs: if count > 0 { total_wait_time / count as u64 } else { 0 },
            max_wait_time_secs: max_wait_time,
            avg_priority: if count > 0 { total_priority / count as i32 } else { 0 },
        }
    }

    /// Check for starvation (jobs waiting too long)
    pub fn check_starvation(&self, threshold_secs: u64) -> Vec<String> {
        self.priorities
            .iter()
            .filter(|(_, p)| p.wait_time_secs() > threshold_secs)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Get number of pending jobs
    pub fn len(&self) -> usize {
        self.priority_order.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.priority_order.is_empty()
    }
}

/// Statistics from fair scheduler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FairSchedulerStats {
    pub pending_jobs: usize,
    pub unique_submitters: usize,
    pub avg_wait_time_secs: u64,
    pub max_wait_time_secs: u64,
    pub avg_priority: i32,
}

/// Resource monitor for tracking system resources
pub struct ResourceMonitor {
    /// Current resource snapshot
    current: Arc<RwLock<SystemResources>>,
    /// Load configuration
    config: LoadConfig,
    /// Resource history for trend analysis
    history: Arc<RwLock<VecDeque<SystemResources>>>,
    /// Maximum history entries
    max_history: usize,
}

impl ResourceMonitor {
    pub fn new(config: LoadConfig) -> Self {
        Self {
            current: Arc::new(RwLock::new(SystemResources::default())),
            config,
            history: Arc::new(RwLock::new(VecDeque::with_capacity(100))),
            max_history: 100,
        }
    }

    /// Update resource snapshot
    pub async fn update(&self) {
        let resources = self.collect_resources();
        
        let mut current = self.current.write().await;
        *current = resources.clone();

        let mut history = self.history.write().await;
        history.push_back(resources);
        if history.len() > self.max_history {
            history.pop_front();
        }
    }

    /// Collect current system resources
    fn collect_resources(&self) -> SystemResources {
        // Get memory info using sys-info or similar
        let (total_mem, avail_mem) = self.get_memory_info();
        let mem_usage = if total_mem > 0 {
            ((total_mem - avail_mem) as f64 / total_mem as f64) * 100.0
        } else {
            0.0
        };

        SystemResources {
            available_memory_bytes: avail_mem,
            total_memory_bytes: total_mem,
            memory_usage_percent: mem_usage,
            cpu_usage_percent: 0.0, // Would need more complex tracking
            cpu_cores: num_cpus::get(),
            timestamp: Instant::now(),
        }
    }

    /// Get memory information
    fn get_memory_info(&self) -> (u64, u64) {
        // Try to read from /proc/meminfo on Linux
        #[cfg(target_os = "linux")]
        {
            if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
                let mut total = 0u64;
                let mut available = 0u64;
                
                for line in content.lines() {
                    if line.starts_with("MemTotal:") {
                        total = parse_meminfo_value(line);
                    } else if line.starts_with("MemAvailable:") {
                        available = parse_meminfo_value(line);
                    }
                }
                
                if total > 0 {
                    return (total * 1024, available * 1024); // Convert from KB to bytes
                }
            }
        }

        // Default fallback
        (8 * 1024 * 1024 * 1024, 4 * 1024 * 1024 * 1024)
    }

    /// Get current resources
    pub async fn get_current(&self) -> SystemResources {
        self.current.read().await.clone()
    }

    /// Make load decision based on current resources
    pub async fn make_load_decision(&self, estimated_memory_bytes: u64) -> LoadDecision {
        let resources = self.get_current().await;

        // Check if we have enough available memory
        if resources.available_memory_bytes < self.config.min_available_memory_bytes {
            return LoadDecision::Reject {
                reason: format!(
                    "Insufficient memory: {} MB available, {} MB required",
                    resources.available_memory_bytes / (1024 * 1024),
                    self.config.min_available_memory_bytes / (1024 * 1024)
                ),
            };
        }

        // Check if adding this job would exceed memory limits
        if resources.available_memory_bytes < estimated_memory_bytes {
            return LoadDecision::Reject {
                reason: format!(
                    "Job requires {} MB but only {} MB available",
                    estimated_memory_bytes / (1024 * 1024),
                    resources.available_memory_bytes / (1024 * 1024)
                ),
            };
        }

        // Check memory pressure
        if resources.memory_usage_percent > self.config.max_memory_percent {
            if self.config.enable_load_shedding {
                return LoadDecision::Reject {
                    reason: format!(
                        "Memory usage {}% exceeds limit {}%",
                        resources.memory_usage_percent as u32,
                        self.config.max_memory_percent as u32
                    ),
                };
            } else {
                return LoadDecision::Defer;
            }
        }

        // Check CPU pressure
        if resources.cpu_usage_percent > self.config.max_cpu_percent {
            return LoadDecision::Throttle {
                delay_ms: 1000, // 1 second delay
            };
        }

        // Light memory pressure - throttle
        if resources.is_memory_pressure() {
            return LoadDecision::Throttle {
                delay_ms: 500,
            };
        }

        LoadDecision::Accept
    }

    /// Get load config
    pub fn get_config(&self) -> &LoadConfig {
        &self.config
    }

    /// Get resource history
    pub async fn get_history(&self) -> Vec<SystemResources> {
        self.history.read().await.iter().cloned().collect()
    }
}

/// Parse a value from /proc/meminfo line
#[cfg(target_os = "linux")]
fn parse_meminfo_value(line: &str) -> u64 {
    line.split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Memory management strategies
#[derive(Debug, Clone)]
pub struct MemoryManager {
    /// Configuration
    config: MemoryConfig,
    /// Current memory state
    state: Arc<RwLock<MemoryState>>,
}

/// Memory management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Target memory usage percentage
    pub target_usage_percent: f64,
    /// High watermark for triggering cleanup
    pub high_watermark_percent: f64,
    /// Low watermark for stopping cleanup
    pub low_watermark_percent: f64,
    /// Enable aggressive cleanup under pressure
    pub enable_aggressive_cleanup: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            target_usage_percent: 70.0,
            high_watermark_percent: 80.0,
            low_watermark_percent: 60.0,
            enable_aggressive_cleanup: true,
        }
    }
}

/// Current memory management state
#[derive(Debug, Clone)]
pub struct MemoryState {
    /// Whether cleanup is in progress
    pub cleanup_in_progress: bool,
    /// Last cleanup time
    pub last_cleanup: Option<Instant>,
    /// Number of cleanups performed
    pub cleanup_count: u64,
    /// Bytes freed in last cleanup
    pub last_cleanup_freed_bytes: u64,
}

impl Default for MemoryState {
    fn default() -> Self {
        Self {
            cleanup_in_progress: false,
            last_cleanup: None,
            cleanup_count: 0,
            last_cleanup_freed_bytes: 0,
        }
    }
}

impl MemoryManager {
    pub fn new(config: MemoryConfig) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(MemoryState::default())),
        }
    }

    /// Check if memory cleanup is needed
    pub async fn needs_cleanup(&self, current_usage_percent: f64) -> bool {
        let state = self.state.read().await;
        if state.cleanup_in_progress {
            return false;
        }
        current_usage_percent > self.config.high_watermark_percent
    }

    /// Signal that cleanup should start
    pub async fn start_cleanup(&self) {
        let mut state = self.state.write().await;
        state.cleanup_in_progress = true;
        info!("Starting memory cleanup");
    }

    /// Signal that cleanup is complete
    pub async fn complete_cleanup(&self, freed_bytes: u64) {
        let mut state = self.state.write().await;
        state.cleanup_in_progress = false;
        state.last_cleanup = Some(Instant::now());
        state.cleanup_count += 1;
        state.last_cleanup_freed_bytes = freed_bytes;
        info!("Memory cleanup complete, freed {} bytes", freed_bytes);
    }

    /// Check if we're below the low watermark and can stop aggressive cleanup
    pub fn below_low_watermark(&self, current_usage_percent: f64) -> bool {
        current_usage_percent < self.config.low_watermark_percent
    }

    /// Get memory state
    pub async fn get_state(&self) -> MemoryState {
        self.state.read().await.clone()
    }

    /// Get config
    pub fn get_config(&self) -> &MemoryConfig {
        &self.config
    }
}

/// Combined load manager that coordinates all resource management
pub struct LoadManager {
    /// Resource monitor
    resource_monitor: ResourceMonitor,
    /// Fair scheduler
    fair_scheduler: Arc<RwLock<FairScheduler>>,
    /// Memory manager
    memory_manager: MemoryManager,
    /// Load statistics
    stats: Arc<RwLock<LoadStats>>,
}

/// Load management statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoadStats {
    pub jobs_accepted: u64,
    pub jobs_rejected: u64,
    pub jobs_throttled: u64,
    pub jobs_deferred: u64,
    pub total_throttle_delay_ms: u64,
    pub memory_cleanups: u64,
    pub starvation_preventions: u64,
}

impl LoadManager {
    pub fn new(load_config: LoadConfig, memory_config: MemoryConfig, max_jobs_per_submitter: usize) -> Self {
        Self {
            resource_monitor: ResourceMonitor::new(load_config.clone()),
            fair_scheduler: Arc::new(RwLock::new(FairScheduler::new(load_config, max_jobs_per_submitter))),
            memory_manager: MemoryManager::new(memory_config),
            stats: Arc::new(RwLock::new(LoadStats::default())),
        }
    }

    /// Submit a job for scheduling
    pub async fn submit_job(
        &self,
        job_id: String,
        submitter_id: String,
        base_priority: i32,
        estimated_memory_bytes: u64,
    ) -> Result<LoadDecision, String> {
        // First, check resource availability
        let decision = self.resource_monitor.make_load_decision(estimated_memory_bytes).await;

        let mut stats = self.stats.write().await;
        
        match &decision {
            LoadDecision::Accept => {
                // Add to fair scheduler
                let mut scheduler = self.fair_scheduler.write().await;
                scheduler.add_job(job_id, submitter_id, base_priority)?;
                stats.jobs_accepted += 1;
            }
            LoadDecision::Reject { .. } => {
                stats.jobs_rejected += 1;
                return Ok(decision);
            }
            LoadDecision::Throttle { delay_ms } => {
                stats.jobs_throttled += 1;
                stats.total_throttle_delay_ms += delay_ms;
                // Still add to scheduler after delay
                let mut scheduler = self.fair_scheduler.write().await;
                scheduler.add_job(job_id, submitter_id, base_priority)?;
            }
            LoadDecision::Defer => {
                stats.jobs_deferred += 1;
                // Add with lower priority
                let mut scheduler = self.fair_scheduler.write().await;
                scheduler.add_job(job_id, submitter_id, base_priority - 5)?;
            }
        }

        Ok(decision)
    }

    /// Get next job to process
    pub async fn next_job(&self) -> Option<String> {
        let mut scheduler = self.fair_scheduler.write().await;
        scheduler.next_job()
    }

    /// Mark job as completed
    pub async fn complete_job(&self, job_id: &str) {
        let mut scheduler = self.fair_scheduler.write().await;
        scheduler.remove_job(job_id);
    }

    /// Update resource monitoring
    pub async fn update_resources(&self) {
        self.resource_monitor.update().await;
    }

    /// Get current system resources
    pub async fn get_resources(&self) -> SystemResources {
        self.resource_monitor.get_current().await
    }

    /// Get scheduler statistics
    pub async fn get_scheduler_stats(&self) -> FairSchedulerStats {
        self.fair_scheduler.read().await.get_stats()
    }

    /// Get load statistics
    pub async fn get_load_stats(&self) -> LoadStats {
        self.stats.read().await.clone()
    }

    /// Check for starvation and prevent it
    pub async fn prevent_starvation(&self, threshold_secs: u64) -> Vec<String> {
        let scheduler = self.fair_scheduler.read().await;
        let starving = scheduler.check_starvation(threshold_secs);
        
        if !starving.is_empty() {
            warn!("Detected {} potentially starving jobs", starving.len());
            let mut stats = self.stats.write().await;
            stats.starvation_preventions += starving.len() as u64;
        }
        
        starving
    }

    /// Trigger memory management if needed
    pub async fn manage_memory(&self) -> bool {
        let resources = self.resource_monitor.get_current().await;
        
        if self.memory_manager.needs_cleanup(resources.memory_usage_percent).await {
            self.memory_manager.start_cleanup().await;
            
            // In a real implementation, this would trigger model unloading
            // or other memory cleanup strategies
            
            let mut stats = self.stats.write().await;
            stats.memory_cleanups += 1;
            
            return true;
        }
        
        false
    }

    /// Get pending job count
    pub async fn pending_jobs(&self) -> usize {
        self.fair_scheduler.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_resources_default() {
        let resources = SystemResources::default();
        assert!(!resources.is_memory_pressure());
        assert!(!resources.is_cpu_pressure());
        assert!(!resources.is_under_pressure());
    }

    #[test]
    fn test_system_resources_pressure_detection() {
        let resources = SystemResources {
            memory_usage_percent: 85.0,
            ..Default::default()
        };
        assert!(resources.is_memory_pressure());

        let resources = SystemResources {
            cpu_usage_percent: 95.0,
            ..Default::default()
        };
        assert!(resources.is_cpu_pressure());
    }

    #[test]
    fn test_load_config_default() {
        let config = LoadConfig::default();
        assert!(config.max_memory_percent > 0.0);
        assert!(config.enable_job_aging);
    }

    #[test]
    fn test_job_priority_creation() {
        let priority = JobPriority::new("job1".to_string(), "user1".to_string(), 5);
        assert_eq!(priority.base_priority, 5);
        assert_eq!(priority.age_boost, 0);
        assert_eq!(priority.effective_priority(), 5);
    }

    #[test]
    fn test_fair_scheduler_add_job() {
        let config = LoadConfig::default();
        let mut scheduler = FairScheduler::new(config, 10);
        
        assert!(scheduler.add_job("job1".to_string(), "user1".to_string(), 5).is_ok());
        assert_eq!(scheduler.len(), 1);
    }

    #[test]
    fn test_fair_scheduler_max_jobs_per_submitter() {
        let config = LoadConfig::default();
        let mut scheduler = FairScheduler::new(config, 2);
        
        assert!(scheduler.add_job("job1".to_string(), "user1".to_string(), 5).is_ok());
        assert!(scheduler.add_job("job2".to_string(), "user1".to_string(), 5).is_ok());
        assert!(scheduler.add_job("job3".to_string(), "user1".to_string(), 5).is_err());
    }

    #[test]
    fn test_fair_scheduler_next_job() {
        let config = LoadConfig::default();
        let mut scheduler = FairScheduler::new(config, 10);
        
        scheduler.add_job("low".to_string(), "user1".to_string(), 1).unwrap();
        scheduler.add_job("high".to_string(), "user2".to_string(), 10).unwrap();
        
        // High priority should come first
        assert_eq!(scheduler.next_job(), Some("high".to_string()));
        assert_eq!(scheduler.next_job(), Some("low".to_string()));
        assert_eq!(scheduler.next_job(), None);
    }

    #[test]
    fn test_load_decision_equality() {
        assert_eq!(LoadDecision::Accept, LoadDecision::Accept);
        assert_ne!(LoadDecision::Accept, LoadDecision::Defer);
    }

    #[tokio::test]
    async fn test_memory_manager_needs_cleanup() {
        let config = MemoryConfig {
            high_watermark_percent: 80.0,
            ..Default::default()
        };
        let manager = MemoryManager::new(config);
        
        assert!(!manager.needs_cleanup(70.0).await);
        assert!(manager.needs_cleanup(85.0).await);
    }
}

