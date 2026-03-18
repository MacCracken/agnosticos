//! Edge — Health/stats telemetry: GPU metrics and fleet-wide model registry.

use super::fleet::EdgeFleetManager;
use super::types::EdgeNodeStatus;

impl EdgeFleetManager {
    /// G3.2: Return a deduplicated list of all model names currently loaded
    /// across online fleet nodes, for advertising to hoosh.
    pub fn fleet_loaded_models(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut models: Vec<String> = self
            .nodes
            .values()
            .filter(|n| n.status == EdgeNodeStatus::Online)
            .flat_map(|n| n.loaded_models.iter().cloned())
            .filter(|m| seen.insert(m.clone()))
            .collect();
        models.sort();
        models
    }

    /// G3.2: Return a map of node_id → loaded model names for all online
    /// nodes that have at least one loaded model. Useful for targeted routing.
    pub fn nodes_by_model(&self) -> std::collections::HashMap<String, Vec<String>> {
        self.nodes
            .values()
            .filter(|n| n.status == EdgeNodeStatus::Online && !n.loaded_models.is_empty())
            .map(|n| (n.id.clone(), n.loaded_models.clone()))
            .collect()
    }
}
