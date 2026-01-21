//! Decision cache for the LLM Orchestrator
//!
//! This module provides an LRU cache for caching decisions based on state hashes,
//! enabling fast responses for similar queries without re-running inference.

use crate::orchestration::error::{OrchestratorError, OrchestratorResult};
use crate::orchestration::types::{Decision, DecisionType, LLMState};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, trace, warn};

/// Entry in the decision cache
#[derive(Debug, Clone)]
struct CacheEntry {
    /// The cached decision
    decision: Decision,
    /// Decision type that was used
    decision_type: DecisionType,
    /// Context that was provided
    context: String,
    /// When the entry was created
    created_at: Instant,
    /// When the entry was last accessed
    last_accessed: Instant,
    /// Access count for statistics
    access_count: u64,
}

impl CacheEntry {
    fn new(decision: Decision, decision_type: DecisionType, context: String) -> Self {
        let now = Instant::now();
        Self {
            decision,
            decision_type,
            context,
            created_at: now,
            last_accessed: now,
            access_count: 1,
        }
    }
    
    fn touch(&mut self) {
        self.last_accessed = Instant::now();
        self.access_count += 1;
    }
    
    fn is_expired(&self, ttl: Duration) -> bool {
        self.created_at.elapsed() > ttl
    }
    
    fn age(&self) -> Duration {
        self.created_at.elapsed()
    }
}

/// LRU Decision Cache
pub struct DecisionCache {
    /// Cache entries keyed by state hash
    entries: Arc<RwLock<HashMap<u64, CacheEntry>>>,
    /// Maximum number of entries
    max_entries: usize,
    /// Time-to-live for entries
    ttl: Duration,
    /// Cache statistics
    stats: Arc<RwLock<CacheStats>>,
}

/// Cache statistics
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub expirations: u64,
    pub invalidations: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            return 0.0;
        }
        self.hits as f64 / total as f64
    }
}

impl DecisionCache {
    /// Create a new decision cache
    pub fn new(max_entries: usize, ttl_seconds: u64) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_entries,
            ttl: Duration::from_secs(ttl_seconds),
            stats: Arc::new(RwLock::new(CacheStats::default())),
        }
    }
    
    /// Create a disabled cache (always misses)
    pub fn disabled() -> Self {
        Self::new(0, 0)
    }
    
    /// Check if cache is enabled
    pub fn is_enabled(&self) -> bool {
        self.max_entries > 0 && self.ttl > Duration::ZERO
    }
    
    /// Look up a decision in the cache
    /// Returns Some(Decision) if found and valid, None otherwise
    pub async fn get(
        &self,
        state: &LLMState,
        decision_type: DecisionType,
    ) -> Option<Decision> {
        if !self.is_enabled() {
            return None;
        }
        
        let hash = state.calculate_hash();
        let start = Instant::now();
        
        // Try to get entry
        let mut entries = self.entries.write().await;
        
        if let Some(entry) = entries.get_mut(&hash) {
            // Check if expired
            if entry.is_expired(self.ttl) {
                debug!(hash = hash, age_ms = ?entry.age().as_millis(), "Cache entry expired");
                entries.remove(&hash);
                
                let mut stats = self.stats.write().await;
                stats.expirations += 1;
                stats.misses += 1;
                return None;
            }
            
            // Check if decision type matches
            if entry.decision_type != decision_type {
                debug!(
                    hash = hash,
                    expected = %decision_type,
                    found = %entry.decision_type,
                    "Cache hit but decision type mismatch"
                );
                let mut stats = self.stats.write().await;
                stats.misses += 1;
                return None;
            }
            
            // Valid hit
            entry.touch();
            let decision = entry.decision.clone();
            let latency = start.elapsed();
            
            trace!(
                hash = hash,
                latency_us = latency.as_micros() as u64,
                access_count = entry.access_count,
                "Cache hit"
            );
            
            let mut stats = self.stats.write().await;
            stats.hits += 1;
            
            // Verify we meet the <10ms latency target
            if latency.as_millis() > 10 {
                warn!(latency_ms = latency.as_millis() as u64, "Cache hit latency exceeded 10ms target");
            }
            
            return Some(decision);
        }
        
        // Miss
        let mut stats = self.stats.write().await;
        stats.misses += 1;
        
        None
    }
    
    /// Store a decision in the cache
    pub async fn put(
        &self,
        state: &LLMState,
        decision_type: DecisionType,
        context: String,
        decision: Decision,
    ) {
        if !self.is_enabled() {
            return;
        }
        
        let hash = state.calculate_hash();
        let entry = CacheEntry::new(decision, decision_type, context);
        
        let mut entries = self.entries.write().await;
        
        // Evict if at capacity
        if entries.len() >= self.max_entries && !entries.contains_key(&hash) {
            self.evict_lru(&mut entries).await;
        }
        
        entries.insert(hash, entry);
        
        debug!(hash = hash, total_entries = entries.len(), "Cached decision");
    }
    
    /// Evict the least recently used entry
    async fn evict_lru(&self, entries: &mut HashMap<u64, CacheEntry>) {
        // Find LRU entry
        let lru_key = entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(key, _)| *key);
        
        if let Some(key) = lru_key {
            entries.remove(&key);
            debug!(hash = key, "Evicted LRU cache entry");
            
            let mut stats = self.stats.write().await;
            stats.evictions += 1;
        }
    }
    
    /// Invalidate entries related to a state change
    /// This removes entries whose state hash differs significantly from the new state
    pub async fn invalidate_on_state_change(&self, new_state: &LLMState) {
        if !self.is_enabled() {
            return;
        }
        
        let new_hash = new_state.calculate_hash();
        let mut entries = self.entries.write().await;
        
        // Remove entries that are too different from new state
        // For now, we'll just remove expired entries and those with very different hashes
        let keys_to_remove: Vec<u64> = entries
            .iter()
            .filter(|(hash, entry)| {
                // Remove expired entries
                if entry.is_expired(self.ttl) {
                    return true;
                }
                // Remove entries with very different hashes (simple XOR distance)
                let distance = *hash ^ new_hash;
                distance.count_ones() > 32 // More than half the bits differ
            })
            .map(|(key, _)| *key)
            .collect();
        
        let removed_count = keys_to_remove.len();
        for key in keys_to_remove {
            entries.remove(&key);
        }
        
        if removed_count > 0 {
            info!(
                removed = removed_count,
                remaining = entries.len(),
                "Invalidated cache entries on state change"
            );
            
            let mut stats = self.stats.write().await;
            stats.invalidations += removed_count as u64;
        }
    }
    
    /// Clear all cache entries
    pub async fn clear(&self) {
        let mut entries = self.entries.write().await;
        let count = entries.len();
        entries.clear();
        
        info!(cleared = count, "Cache cleared");
    }
    
    /// Get cache statistics
    pub async fn get_stats(&self) -> CacheStats {
        self.stats.read().await.clone()
    }
    
    /// Get current cache size
    pub async fn len(&self) -> usize {
        self.entries.read().await.len()
    }
    
    /// Check if cache is empty
    pub async fn is_empty(&self) -> bool {
        self.entries.read().await.is_empty()
    }
    
    /// Remove expired entries
    pub async fn cleanup_expired(&self) {
        if !self.is_enabled() {
            return;
        }
        
        let mut entries = self.entries.write().await;
        
        let expired: Vec<u64> = entries
            .iter()
            .filter(|(_, entry)| entry.is_expired(self.ttl))
            .map(|(key, _)| *key)
            .collect();
        
        let count = expired.len();
        for key in expired {
            entries.remove(&key);
        }
        
        if count > 0 {
            debug!(expired = count, "Cleaned up expired cache entries");
            
            let mut stats = self.stats.write().await;
            stats.expirations += count as u64;
        }
    }
    
    /// Get detailed cache info for debugging
    pub async fn get_debug_info(&self) -> CacheDebugInfo {
        let entries = self.entries.read().await;
        let stats = self.stats.read().await;
        
        let ages: Vec<Duration> = entries.values().map(|e| e.age()).collect();
        let avg_age = if ages.is_empty() {
            Duration::ZERO
        } else {
            ages.iter().sum::<Duration>() / ages.len() as u32
        };
        
        let access_counts: Vec<u64> = entries.values().map(|e| e.access_count).collect();
        let avg_accesses = if access_counts.is_empty() {
            0.0
        } else {
            access_counts.iter().sum::<u64>() as f64 / access_counts.len() as f64
        };
        
        CacheDebugInfo {
            entry_count: entries.len(),
            max_entries: self.max_entries,
            ttl_secs: self.ttl.as_secs(),
            avg_age_secs: avg_age.as_secs(),
            avg_access_count: avg_accesses,
            hit_rate: stats.hit_rate(),
            stats: stats.clone(),
        }
    }
}

/// Detailed cache information for debugging
#[derive(Debug, Clone)]
pub struct CacheDebugInfo {
    pub entry_count: usize,
    pub max_entries: usize,
    pub ttl_secs: u64,
    pub avg_age_secs: u64,
    pub avg_access_count: f64,
    pub hit_rate: f64,
    pub stats: CacheStats,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::types::*;

    fn create_test_state(id: &str, queue_size: usize) -> LLMState {
        LLMState::new(
            NodeState {
                id: id.to_string(),
                capacity: CapacityLevel::Medium,
                latency_avg: 100,
                uptime_seconds: 3600,
                error_count: 0,
            },
            ContextState {
                id: "ctx".to_string(),
                health: HealthLevel::Healthy,
                queue_size,
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
    async fn test_cache_put_and_get() {
        let cache = DecisionCache::new(100, 300);
        let state = create_test_state("node-1", 10);
        let decision = create_test_decision();
        
        // Put
        cache.put(
            &state,
            DecisionType::Infrastructure,
            "test context".to_string(),
            decision.clone(),
        ).await;
        
        assert_eq!(cache.len().await, 1);
        
        // Get
        let result = cache.get(&state, DecisionType::Infrastructure).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().decision, decision.decision);
    }

    #[tokio::test]
    async fn test_cache_miss_on_different_type() {
        let cache = DecisionCache::new(100, 300);
        let state = create_test_state("node-1", 10);
        let decision = create_test_decision();
        
        cache.put(
            &state,
            DecisionType::Infrastructure,
            "test".to_string(),
            decision,
        ).await;
        
        // Should miss for different decision type
        let result = cache.get(&state, DecisionType::Mediation).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_cache_expiration() {
        // Very short TTL
        let cache = DecisionCache::new(100, 1);
        let state = create_test_state("node-1", 10);
        let decision = create_test_decision();
        
        cache.put(
            &state,
            DecisionType::Infrastructure,
            "test".to_string(),
            decision,
        ).await;
        
        // Wait for expiration
        tokio::time::sleep(Duration::from_secs(2)).await;
        
        let result = cache.get(&state, DecisionType::Infrastructure).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_lru_eviction() {
        let cache = DecisionCache::new(2, 300);
        
        let state1 = create_test_state("node-1", 10);
        let state2 = create_test_state("node-2", 20);
        let state3 = create_test_state("node-3", 30);
        let decision = create_test_decision();
        
        // Add two entries
        cache.put(&state1, DecisionType::Infrastructure, "1".to_string(), decision.clone()).await;
        cache.put(&state2, DecisionType::Infrastructure, "2".to_string(), decision.clone()).await;
        
        // Access state1 to make it more recently used
        let _ = cache.get(&state1, DecisionType::Infrastructure).await;
        
        // Add third entry, should evict state2 (LRU)
        cache.put(&state3, DecisionType::Infrastructure, "3".to_string(), decision).await;
        
        assert_eq!(cache.len().await, 2);
        
        // state1 should still be there
        assert!(cache.get(&state1, DecisionType::Infrastructure).await.is_some());
        
        // state2 should be evicted
        let stats = cache.get_stats().await;
        assert!(stats.evictions > 0);
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let cache = DecisionCache::new(100, 300);
        let state = create_test_state("node-1", 10);
        let decision = create_test_decision();
        
        // Miss
        let _ = cache.get(&state, DecisionType::Infrastructure).await;
        
        // Put
        cache.put(&state, DecisionType::Infrastructure, "test".to_string(), decision).await;
        
        // Hit
        let _ = cache.get(&state, DecisionType::Infrastructure).await;
        let _ = cache.get(&state, DecisionType::Infrastructure).await;
        
        let stats = cache.get_stats().await;
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate() - 0.666).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_disabled_cache() {
        let cache = DecisionCache::disabled();
        let state = create_test_state("node-1", 10);
        let decision = create_test_decision();
        
        cache.put(&state, DecisionType::Infrastructure, "test".to_string(), decision).await;
        
        assert!(cache.is_empty().await);
        assert!(cache.get(&state, DecisionType::Infrastructure).await.is_none());
    }

    #[tokio::test]
    async fn test_clear() {
        let cache = DecisionCache::new(100, 300);
        let state = create_test_state("node-1", 10);
        let decision = create_test_decision();
        
        cache.put(&state, DecisionType::Infrastructure, "test".to_string(), decision).await;
        assert!(!cache.is_empty().await);
        
        cache.clear().await;
        assert!(cache.is_empty().await);
    }

    #[tokio::test]
    async fn test_cache_hit_latency() {
        let cache = DecisionCache::new(100, 300);
        let state = create_test_state("node-1", 10);
        let decision = create_test_decision();
        
        cache.put(&state, DecisionType::Infrastructure, "test".to_string(), decision).await;
        
        // Measure hit latency
        let start = Instant::now();
        let result = cache.get(&state, DecisionType::Infrastructure).await;
        let latency = start.elapsed();
        
        assert!(result.is_some());
        // Should be well under 10ms (typically < 1ms)
        assert!(latency.as_millis() < 10, "Cache hit latency {} ms exceeded 10ms", latency.as_millis());
    }
}

