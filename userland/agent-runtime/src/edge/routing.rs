//! Edge — Task routing: capability-based selection and constraint filtering.

use super::fleet::EdgeFleetManager;
use super::types::{EdgeFleetError, EdgeNode, EdgeNodeStatus};

impl EdgeFleetManager {
    /// Find the best node for a task based on required capabilities.
    /// Returns nodes sorted by suitability (least loaded, matching caps).
    ///
    /// G3.1: When `min_gpu_memory_mb` is provided, only nodes whose
    /// `capabilities.gpu_memory_mb` meets or exceeds the threshold are
    /// considered.  When `required_compute_capability` is provided (e.g.
    /// `"8.6"`), only nodes advertising that exact CUDA compute capability
    /// are considered.
    pub fn route_task(
        &self,
        required_tags: &[String],
        require_gpu: bool,
        preferred_location: Option<&str>,
        min_gpu_memory_mb: Option<u64>,
        required_compute_capability: Option<&str>,
    ) -> Vec<&EdgeNode> {
        let mut candidates: Vec<&EdgeNode> = self
            .nodes
            .values()
            .filter(|n| {
                // Must be online.
                if n.status != EdgeNodeStatus::Online {
                    return false;
                }
                // Must have GPU if required.
                if require_gpu && !n.capabilities.has_gpu {
                    return false;
                }
                // G3.1: Filter by minimum GPU VRAM when specified.
                if let Some(min_vram) = min_gpu_memory_mb {
                    match n.capabilities.gpu_memory_mb {
                        Some(vram) if vram >= min_vram => {}
                        _ => return false,
                    }
                }
                // G3.1: Filter by CUDA compute capability when specified.
                if let Some(required_cc) = required_compute_capability {
                    match n.capabilities.gpu_compute_capability.as_deref() {
                        Some(cc) if cc == required_cc => {}
                        _ => return false,
                    }
                }
                // Must have all required tags.
                for tag in required_tags {
                    if !n.capabilities.tags.contains(tag) {
                        return false;
                    }
                }
                true
            })
            .collect();

        // Sort by: preferred location first, then least loaded, then best network.
        candidates.sort_by(|a, b| {
            // Location preference.
            let a_loc = preferred_location
                .is_some_and(|loc| a.capabilities.location.as_deref() == Some(loc));
            let b_loc = preferred_location
                .is_some_and(|loc| b.capabilities.location.as_deref() == Some(loc));
            if a_loc != b_loc {
                return b_loc.cmp(&a_loc); // preferred location first
            }
            // Least active tasks.
            let task_cmp = a.active_tasks.cmp(&b.active_tasks);
            if task_cmp != std::cmp::Ordering::Equal {
                return task_cmp;
            }
            // Best network quality.
            b.capabilities
                .network_quality
                .partial_cmp(&a.capabilities.network_quality)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        candidates
    }

    /// Check if a node can accept a task based on bandwidth and resource
    /// constraints. Returns `Ok(())` if the node can accept, or an error
    /// describing why it cannot.
    pub fn check_task_acceptance(
        &self,
        node_id: &str,
        min_bandwidth: f64,
        min_memory_mb: u64,
    ) -> Result<(), EdgeFleetError> {
        let node = self
            .nodes
            .get(node_id)
            .ok_or_else(|| EdgeFleetError::NodeNotFound(node_id.to_string()))?;

        if node.status != EdgeNodeStatus::Online {
            return Err(EdgeFleetError::NodeNotOnline(node_id.to_string()));
        }

        if node.capabilities.network_quality < min_bandwidth {
            return Err(EdgeFleetError::InsufficientBandwidth {
                node_id: node_id.to_string(),
                required: min_bandwidth,
                available: node.capabilities.network_quality,
            });
        }

        if node.capabilities.memory_mb < min_memory_mb {
            return Err(EdgeFleetError::InsufficientResources {
                node_id: node_id.to_string(),
                reason: format!(
                    "need {} MB memory, node has {} MB",
                    min_memory_mb, node.capabilities.memory_mb
                ),
            });
        }

        Ok(())
    }

    /// Route a task with bandwidth and resource constraints.
    /// Returns nodes that meet all requirements, sorted by suitability.
    pub fn route_task_with_constraints(
        &self,
        required_tags: &[String],
        require_gpu: bool,
        preferred_location: Option<&str>,
        min_bandwidth: f64,
        min_memory_mb: u64,
    ) -> Vec<&EdgeNode> {
        self.route_task(required_tags, require_gpu, preferred_location, None, None)
            .into_iter()
            .filter(|n| {
                n.capabilities.network_quality >= min_bandwidth
                    && n.capabilities.memory_mb >= min_memory_mb
            })
            .collect()
    }
}
