//! Context Group Manager
//!
//! Manages dynamic grouping of nodes by context/domain.

use super::config::ContextGroupConfig;
use super::types::{ContextGroup, GroupAction, NodeId};
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Manages context groups
pub struct ContextGroupManager {
    /// Active groups
    groups: Arc<RwLock<HashMap<String, ContextGroup>>>,
    
    /// Configuration
    config: ContextGroupConfig,
    
    /// Job patterns (context -> count)
    job_patterns: Arc<RwLock<HashMap<String, u64>>>,
    
    /// Last activity time per group
    last_activity: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
    
    /// Known nodes that can be assigned to groups
    available_nodes: Arc<RwLock<Vec<NodeId>>>,
}

/// Trait for context group management
#[async_trait::async_trait]
pub trait ContextGroupManagerTrait: Send + Sync {
    /// Form a new group
    async fn form_group(&self, context: &str, nodes: Vec<NodeId>) -> Result<ContextGroup>;
    
    /// Dissolve a group
    async fn dissolve_group(&self, group_id: &str) -> Result<()>;
    
    /// Add member to group
    async fn add_member(&self, group_id: &str, node_id: NodeId) -> Result<()>;
    
    /// Remove member from group
    async fn remove_member(&self, group_id: &str, node_id: &NodeId) -> Result<()>;
    
    /// Elect group leader
    async fn elect_leader(&self, group_id: &str) -> Result<Option<NodeId>>;
    
    /// Get group by context
    async fn get_group(&self, context: &str) -> Option<ContextGroup>;
    
    /// Get all groups
    async fn get_all_groups(&self) -> Vec<ContextGroup>;
    
    /// Record job for pattern detection
    async fn record_job(&self, context: &str);
    
    /// Evaluate if groups should be formed/dissolved
    async fn evaluate_groups(&self) -> Vec<GroupAction>;
}

impl ContextGroupManager {
    /// Create a new ContextGroupManager
    pub fn new(config: ContextGroupConfig) -> Self {
        Self {
            groups: Arc::new(RwLock::new(HashMap::new())),
            config,
            job_patterns: Arc::new(RwLock::new(HashMap::new())),
            last_activity: Arc::new(RwLock::new(HashMap::new())),
            available_nodes: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Register available nodes for group assignment
    pub async fn register_available_nodes(&self, nodes: Vec<NodeId>) {
        let mut available = self.available_nodes.write().await;
        *available = nodes;
    }
    
    /// Add a single available node
    pub async fn add_available_node(&self, node_id: NodeId) {
        let mut available = self.available_nodes.write().await;
        if !available.contains(&node_id) {
            available.push(node_id);
        }
    }
    
    /// Remove a node from available list
    pub async fn remove_available_node(&self, node_id: &NodeId) {
        let mut available = self.available_nodes.write().await;
        available.retain(|n| n != node_id);
    }
    
    /// Get nodes suitable for a context based on their group membership
    async fn get_nodes_for_context(&self, _context: &str, count: usize) -> Vec<NodeId> {
        let available = self.available_nodes.read().await;
        let groups = self.groups.read().await;
        
        // Filter out nodes that are already in max groups
        let nodes_group_count: HashMap<&NodeId, usize> = groups
            .values()
            .flat_map(|g| g.members.iter())
            .fold(HashMap::new(), |mut acc, n| {
                *acc.entry(n).or_insert(0) += 1;
                acc
            });
        
        available
            .iter()
            .filter(|n| {
                nodes_group_count
                    .get(n)
                    .map(|c| *c < self.config.max_groups_per_node)
                    .unwrap_or(true)
            })
            .take(count)
            .cloned()
            .collect()
    }
}

#[async_trait::async_trait]
impl ContextGroupManagerTrait for ContextGroupManager {
    async fn form_group(&self, context: &str, nodes: Vec<NodeId>) -> Result<ContextGroup> {
        if nodes.len() < self.config.min_nodes_per_group {
            anyhow::bail!(
                "Need at least {} nodes to form group, got {}",
                self.config.min_nodes_per_group,
                nodes.len()
            );
        }
        
        let group_id = format!("group-{}-{}", context, uuid::Uuid::new_v4());
        
        let group = ContextGroup {
            id: group_id.clone(),
            context: context.to_string(),
            leader: nodes.first().cloned(), // First node is initial leader
            members: nodes,
            specializations: vec![context.to_string()],
            formed_at: Utc::now(),
            jobs_processed: 0,
            avg_latency_ms: 0.0,
        };
        
        let mut groups = self.groups.write().await;
        groups.insert(context.to_string(), group.clone());
        
        Ok(group)
    }
    
    async fn dissolve_group(&self, group_id: &str) -> Result<()> {
        let mut groups = self.groups.write().await;
        
        // Find and remove by ID
        let context_to_remove = groups
            .iter()
            .find(|(_, g)| g.id == group_id)
            .map(|(c, _)| c.clone());
        
        if let Some(context) = context_to_remove {
            groups.remove(&context);
            Ok(())
        } else {
            anyhow::bail!("Group not found: {}", group_id)
        }
    }
    
    async fn add_member(&self, group_id: &str, node_id: NodeId) -> Result<()> {
        let mut groups = self.groups.write().await;
        
        for group in groups.values_mut() {
            if group.id == group_id {
                if !group.members.contains(&node_id) {
                    group.members.push(node_id);
                }
                return Ok(());
            }
        }
        
        anyhow::bail!("Group not found: {}", group_id)
    }
    
    async fn remove_member(&self, group_id: &str, node_id: &NodeId) -> Result<()> {
        let mut groups = self.groups.write().await;
        
        for group in groups.values_mut() {
            if group.id == group_id {
                group.members.retain(|n| n != node_id);
                
                // If leader was removed, elect new one
                if group.leader.as_ref() == Some(node_id) {
                    group.leader = group.members.first().cloned();
                }
                
                return Ok(());
            }
        }
        
        anyhow::bail!("Group not found: {}", group_id)
    }
    
    async fn elect_leader(&self, group_id: &str) -> Result<Option<NodeId>> {
        let mut groups = self.groups.write().await;
        
        for group in groups.values_mut() {
            if group.id == group_id {
                // Select leader based on member order (can be sorted by affinity externally)
                // For now, rotate to the next member after current leader for fairness
                if let Some(current_leader) = &group.leader {
                    let current_idx = group.members.iter().position(|m| m == current_leader);
                    if let Some(idx) = current_idx {
                        let next_idx = (idx + 1) % group.members.len();
                        group.leader = group.members.get(next_idx).cloned();
                    } else {
                        group.leader = group.members.first().cloned();
                    }
                } else {
                    group.leader = group.members.first().cloned();
                }
                return Ok(group.leader.clone());
            }
        }
        
        anyhow::bail!("Group not found: {}", group_id)
    }
    
    async fn get_group(&self, context: &str) -> Option<ContextGroup> {
        let groups = self.groups.read().await;
        groups.get(context).cloned()
    }
    
    async fn get_all_groups(&self) -> Vec<ContextGroup> {
        let groups = self.groups.read().await;
        groups.values().cloned().collect()
    }
    
    async fn record_job(&self, context: &str) {
        let mut patterns = self.job_patterns.write().await;
        *patterns.entry(context.to_string()).or_insert(0) += 1;
        
        // Update last activity time
        {
            let mut activity = self.last_activity.write().await;
            activity.insert(context.to_string(), Utc::now());
        }
        
        // Also update group stats if exists
        let mut groups = self.groups.write().await;
        if let Some(group) = groups.get_mut(context) {
            group.jobs_processed += 1;
        }
    }
    
    async fn evaluate_groups(&self) -> Vec<GroupAction> {
        let patterns = self.job_patterns.read().await;
        let groups = self.groups.read().await;
        let activity = self.last_activity.read().await;
        let now = Utc::now();
        
        let mut actions = Vec::new();
        
        // Check for contexts that should form groups
        for (context, count) in patterns.iter() {
            if *count >= self.config.min_jobs_for_formation && !groups.contains_key(context) {
                // Get available nodes for this context
                let nodes = self.get_nodes_for_context(context, self.config.min_nodes_per_group + 2).await;
                
                if nodes.len() >= self.config.min_nodes_per_group {
                    actions.push(GroupAction::FormGroup {
                        context: context.clone(),
                        nodes,
                    });
                }
            }
        }
        
        // Check for groups that should be dissolved (idle)
        let idle_threshold = chrono::Duration::seconds(self.config.dissolution_idle_threshold_secs as i64);
        
        for (context, group) in groups.iter() {
            if let Some(last) = activity.get(context) {
                let idle_duration = now.signed_duration_since(*last);
                if idle_duration > idle_threshold {
                    actions.push(GroupAction::DissolveGroup {
                        group_id: group.id.clone(),
                        reason: format!("Idle for {} seconds", idle_duration.num_seconds()),
                    });
                }
            }
        }
        
        // Check for leader rotation within groups
        for group in groups.values() {
            if self.config.leader_rotation_enabled {
                if group.jobs_processed > 0 && 
                   group.jobs_processed % self.config.leader_rotation_interval_jobs as u64 == 0 {
                    actions.push(GroupAction::RotateLeader {
                        group_id: group.id.clone(),
                    });
                }
            }
        }
        
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_form_group() {
        let manager = ContextGroupManager::new(ContextGroupConfig::default());
        
        let nodes = vec!["node-1".to_string(), "node-2".to_string()];
        let group = manager.form_group("translation", nodes).await.unwrap();
        
        assert_eq!(group.context, "translation");
        assert_eq!(group.members.len(), 2);
        assert!(group.leader.is_some());
    }
    
    #[tokio::test]
    async fn test_form_group_requires_min_nodes() {
        let config = ContextGroupConfig {
            min_nodes_per_group: 3,
            ..Default::default()
        };
        let manager = ContextGroupManager::new(config);
        
        let nodes = vec!["node-1".to_string(), "node-2".to_string()];
        let result = manager.form_group("translation", nodes).await;
        
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_add_remove_member() {
        let manager = ContextGroupManager::new(ContextGroupConfig::default());
        
        let nodes = vec!["node-1".to_string(), "node-2".to_string()];
        let group = manager.form_group("translation", nodes).await.unwrap();
        
        manager.add_member(&group.id, "node-3".to_string()).await.unwrap();
        
        let updated = manager.get_group("translation").await.unwrap();
        assert_eq!(updated.members.len(), 3);
        
        manager.remove_member(&group.id, &"node-1".to_string()).await.unwrap();
        
        let updated = manager.get_group("translation").await.unwrap();
        assert_eq!(updated.members.len(), 2);
    }
    
    #[tokio::test]
    async fn test_record_job() {
        let manager = ContextGroupManager::new(ContextGroupConfig::default());
        
        let nodes = vec!["node-1".to_string(), "node-2".to_string()];
        manager.form_group("translation", nodes).await.unwrap();
        
        manager.record_job("translation").await;
        manager.record_job("translation").await;
        
        let group = manager.get_group("translation").await.unwrap();
        assert_eq!(group.jobs_processed, 2);
    }
}

