//! Strategy Implementations
//!
//! Implements the various decision strategies: heuristic, LLM, and hybrid.

use super::types::HeuristicAlgorithm;

/// Heuristic strategy implementations
pub mod heuristic {
    use super::*;
    
    /// Select node using greedy affinity (highest score wins)
    pub fn greedy_affinity(affinities: &[(String, f64)]) -> Option<String> {
        affinities
            .iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(id, _)| id.clone())
    }
    
    /// Select node using round-robin
    pub fn round_robin(nodes: &[String], counter: usize) -> Option<String> {
        if nodes.is_empty() {
            return None;
        }
        Some(nodes[counter % nodes.len()].clone())
    }
    
    /// Select least loaded node
    pub fn least_loaded(loads: &[(String, f64)]) -> Option<String> {
        loads
            .iter()
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(id, _)| id.clone())
    }
    
    /// Select random node
    pub fn random(nodes: &[String]) -> Option<String> {
        if nodes.is_empty() {
            return None;
        }
        let idx = rand::random::<usize>() % nodes.len();
        Some(nodes[idx].clone())
    }
    
    /// Execute a heuristic algorithm
    pub fn execute(
        algorithm: &HeuristicAlgorithm,
        nodes: &[String],
        affinities: &[(String, f64)],
        loads: &[(String, f64)],
        counter: usize,
    ) -> Option<String> {
        match algorithm {
            HeuristicAlgorithm::GreedyAffinity => greedy_affinity(affinities),
            HeuristicAlgorithm::RoundRobin => round_robin(nodes, counter),
            HeuristicAlgorithm::LeastLoaded => least_loaded(loads),
            HeuristicAlgorithm::Random => random(nodes),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::heuristic::*;
    
    #[test]
    fn test_greedy_affinity() {
        let affinities = vec![
            ("node-1".to_string(), 0.5),
            ("node-2".to_string(), 0.9),
            ("node-3".to_string(), 0.7),
        ];
        
        let result = greedy_affinity(&affinities);
        assert_eq!(result, Some("node-2".to_string()));
    }
    
    #[test]
    fn test_round_robin() {
        let nodes = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        
        assert_eq!(round_robin(&nodes, 0), Some("a".to_string()));
        assert_eq!(round_robin(&nodes, 1), Some("b".to_string()));
        assert_eq!(round_robin(&nodes, 2), Some("c".to_string()));
        assert_eq!(round_robin(&nodes, 3), Some("a".to_string())); // Wraps
    }
    
    #[test]
    fn test_least_loaded() {
        let loads = vec![
            ("node-1".to_string(), 0.8),
            ("node-2".to_string(), 0.2),
            ("node-3".to_string(), 0.5),
        ];
        
        let result = least_loaded(&loads);
        assert_eq!(result, Some("node-2".to_string()));
    }
    
    #[test]
    fn test_random() {
        let nodes = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        
        // Just check it returns something valid
        let result = random(&nodes);
        assert!(result.is_some());
        assert!(nodes.contains(&result.unwrap()));
    }
    
    #[test]
    fn test_empty_nodes() {
        let empty: Vec<String> = vec![];
        
        assert_eq!(greedy_affinity(&[]), None);
        assert_eq!(round_robin(&empty, 0), None);
        assert_eq!(least_loaded(&[]), None);
        assert_eq!(random(&empty), None);
    }
}

