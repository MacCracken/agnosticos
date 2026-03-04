//! Resource management for agents
//!
//! Handles GPU allocation, memory management, and CPU scheduling.

use std::collections::HashMap;

use anyhow::{Context, Result};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use agnos_common::AgentId;

/// GPU device information
#[derive(Debug)]
pub struct GpuDevice {
    pub id: u32,
    pub name: String,
    pub total_memory: u64,
    pub available_memory: RwLock<u64>,
    pub compute_capability: Option<String>,
}

impl Clone for GpuDevice {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            name: self.name.clone(),
            total_memory: self.total_memory,
            available_memory: RwLock::new(*self.available_memory.blocking_read()),
            compute_capability: self.compute_capability.clone(),
        }
    }
}

/// Resource manager for system resources
pub struct ResourceManager {
    /// Available GPUs
    gpus: RwLock<Vec<GpuDevice>>,
    /// GPU allocations by agent
    gpu_allocations: RwLock<HashMap<AgentId, Vec<u32>>>,
    /// CPU core allocations
    cpu_allocations: RwLock<HashMap<AgentId, Vec<usize>>>,
    /// Total system memory
    total_memory: u64,
    /// Available memory
    available_memory: RwLock<u64>,
}

impl ResourceManager {
    /// Create a new resource manager
    pub async fn new() -> Result<Self> {
        info!("Initializing resource manager...");

        // Detect GPUs
        let gpus = Self::detect_gpus().await?;
        
        // Detect system memory
        let total_memory = Self::detect_system_memory().await?;
        
        info!("Detected {} GPU(s)", gpus.len());
        info!("Total system memory: {} MB", total_memory / (1024 * 1024));

        Ok(Self {
            gpus: RwLock::new(gpus),
            gpu_allocations: RwLock::new(HashMap::new()),
            cpu_allocations: RwLock::new(HashMap::new()),
            total_memory,
            available_memory: RwLock::new(total_memory),
        })
    }

    /// Allocate GPU resources to an agent
    pub async fn allocate_gpu(&self, agent_id: AgentId, memory_required: u64) -> Result<Vec<u32>> {
        debug!("Allocating GPU for agent {} ({} bytes)", agent_id, memory_required);

        let mut gpus = self.gpus.write().await;
        let mut allocations = self.gpu_allocations.write().await;

        // Find GPU with sufficient memory
        let mut allocated_gpus = Vec::new();
        
        for gpu in gpus.iter_mut() {
            let available = *gpu.available_memory.read().await;
            
            if available >= memory_required {
                let mut gpu_available = gpu.available_memory.write().await;
                *gpu_available -= memory_required;
                
                allocated_gpus.push(gpu.id);
                info!("Allocated GPU {} to agent {}", gpu.id, agent_id);
                
                if allocated_gpus.len() >= 1 {
                    // For now, only allocate one GPU per agent
                    break;
                }
            }
        }

        if allocated_gpus.is_empty() {
            return Err(anyhow::anyhow!("No GPU with sufficient memory available"));
        }

        allocations.insert(agent_id, allocated_gpus.clone());
        Ok(allocated_gpus)
    }

    /// Release GPU resources from an agent
    pub async fn release_gpu(&self, agent_id: AgentId) -> Result<()> {
        debug!("Releasing GPU allocation for agent {}", agent_id);

        let mut allocations = self.gpu_allocations.write().await;
        
        if let Some(gpu_ids) = allocations.remove(&agent_id) {
            let mut gpus = self.gpus.write().await;
            
            for gpu_id in gpu_ids {
                if let Some(gpu) = gpus.iter_mut().find(|g| g.id == gpu_id) {
                    let mut available = gpu.available_memory.write().await;
                    // Restore full GPU memory (simplified)
                    *available = gpu.total_memory;
                    info!("Released GPU {} from agent {}", gpu_id, agent_id);
                }
            }
        }

        Ok(())
    }

    /// Get GPU information
    pub async fn list_gpus(&self) -> Vec<GpuDevice> {
        self.gpus.read().await.clone()
    }

    /// Get current GPU allocations
    pub async fn get_gpu_allocations(&self) -> HashMap<AgentId, Vec<u32>> {
        self.gpu_allocations.read().await.clone()
    }

    /// Allocate CPU cores to an agent
    pub async fn allocate_cpu(&self, agent_id: AgentId, cores: usize) -> Result<Vec<usize>> {
        debug!("Allocating {} CPU cores for agent {}", cores, agent_id);

        let num_cores = Self::detect_cpu_cores().await?;
        
        if cores > num_cores {
            return Err(anyhow::anyhow!(
                "Requested {} cores but only {} available",
                cores, num_cores
            ));
        }

        // Simple allocation: assign cores 0 to N-1
        let allocated_cores: Vec<usize> = (0..cores).collect();
        
        let mut allocations = self.cpu_allocations.write().await;
        allocations.insert(agent_id, allocated_cores.clone());
        
        info!("Allocated CPU cores {:?} to agent {}", allocated_cores, agent_id);
        
        Ok(allocated_cores)
    }

    /// Release CPU allocation
    pub async fn release_cpu(&self, agent_id: AgentId) -> Result<()> {
        debug!("Releasing CPU allocation for agent {}", agent_id);
        
        let mut allocations = self.cpu_allocations.write().await;
        
        if allocations.remove(&agent_id).is_some() {
            info!("Released CPU allocation for agent {}", agent_id);
        }
        
        Ok(())
    }

    /// Get total system memory
    pub fn total_memory(&self) -> u64 {
        self.total_memory
    }

    /// Get available system memory
    pub async fn available_memory(&self) -> u64 {
        *self.available_memory.read().await
    }

    /// Reserve memory for an agent
    pub async fn reserve_memory(&self, agent_id: AgentId, bytes: u64) -> Result<()> {
        let mut available = self.available_memory.write().await;
        
        if *available < bytes {
            return Err(anyhow::anyhow!(
                "Insufficient memory: requested {} bytes, {} available",
                bytes, *available
            ));
        }
        
        *available -= bytes;
        debug!("Reserved {} bytes for agent {} ({} remaining)", 
               bytes, agent_id, *available);
        
        Ok(())
    }

    /// Release reserved memory
    pub async fn release_memory(&self, bytes: u64) {
        let mut available = self.available_memory.write().await;
        *available = (*available + bytes).min(self.total_memory);
        debug!("Released {} bytes of memory ({} available)", bytes, *available);
    }

    /// Detect available GPUs
    async fn detect_gpus() -> Result<Vec<GpuDevice>> {
        let mut gpus = Vec::new();

        // Try to detect NVIDIA GPUs via nvidia-smi
        match Self::detect_nvidia_gpus().await {
            Ok(nvidia_gpus) => {
                gpus.extend(nvidia_gpus);
            }
            Err(e) => {
                debug!("Failed to detect NVIDIA GPUs: {}", e);
            }
        }

        // Try to detect other GPUs (AMD, Intel)
        // TODO: Implement detection for other GPU vendors

        if gpus.is_empty() {
            info!("No GPUs detected");
        }

        Ok(gpus)
    }

    /// Detect NVIDIA GPUs
    async fn detect_nvidia_gpus() -> Result<Vec<GpuDevice>> {
        use tokio::process::Command;

        let output = Command::new("nvidia-smi")
            .args(&["--query-gpu=index,name,memory.total", "--format=csv,noheader,nounits"])
            .output()
            .await
            .context("Failed to run nvidia-smi")?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("nvidia-smi failed"));
        }

        let stdout = String::from_utf8(output.stdout)?;
        let mut gpus = Vec::new();

        for line in stdout.lines() {
            let parts: Vec<&str> = line.split(", ").collect();
            if parts.len() >= 3 {
                let id = parts[0].parse::<u32>()?;
                let name = parts[1].to_string();
                let memory_mb = parts[2].parse::<u64>()?;
                let total_memory = memory_mb * 1024 * 1024; // Convert to bytes

                gpus.push(GpuDevice {
                    id,
                    name,
                    total_memory,
                    available_memory: RwLock::new(total_memory),
                    compute_capability: None,
                });
            }
        }

        Ok(gpus)
    }

    /// Detect total system memory
    async fn detect_system_memory() -> Result<u64> {
        // Read from /proc/meminfo on Linux
        #[cfg(target_os = "linux")]
        {
            let meminfo = tokio::fs::read_to_string("/proc/meminfo").await?;
            
            for line in meminfo.lines() {
                if line.starts_with("MemTotal:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let kb = parts[1].parse::<u64>()?;
                        return Ok(kb * 1024); // Convert from KB to bytes
                    }
                }
            }
        }

        // Fallback: return 8GB
        warn!("Could not detect system memory, using default 8GB");
        Ok(8 * 1024 * 1024 * 1024)
    }

    /// Detect number of CPU cores
    async fn detect_cpu_cores() -> Result<usize> {
        // Use sysconf on Unix systems
        #[cfg(unix)]
        {
            let cores = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) };
            if cores > 0 {
                return Ok(cores as usize);
            }
        }

        // Fallback
        Ok(std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(1))
    }
}

use std::sync::Arc;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_device_clone() {
        let gpu = GpuDevice {
            id: 0,
            name: "RTX 4090".to_string(),
            total_memory: 24 * 1024 * 1024 * 1024,
            available_memory: RwLock::new(24 * 1024 * 1024 * 1024),
            compute_capability: Some("8.9".to_string()),
        };
        
        let cloned = gpu.clone();
        assert_eq!(cloned.name, "RTX 4090");
        assert_eq!(cloned.total_memory, gpu.total_memory);
    }

    #[test]
    fn test_gpu_device_id() {
        let gpu = GpuDevice {
            id: 1,
            name: "RTX 3090".to_string(),
            total_memory: 24 * 1024 * 1024 * 1024,
            available_memory: RwLock::new(24 * 1024 * 1024 * 1024),
            compute_capability: None,
        };

        assert_eq!(gpu.id, 1);
        assert!(gpu.compute_capability.is_none());
    }

    #[test]
    fn test_gpu_device_debug() {
        let gpu = GpuDevice {
            id: 0,
            name: "Test GPU".to_string(),
            total_memory: 8 * 1024 * 1024 * 1024,
            available_memory: RwLock::new(8 * 1024 * 1024 * 1024),
            compute_capability: Some("7.5".to_string()),
        };
        let debug_str = format!("{:?}", gpu);
        assert!(debug_str.contains("Test GPU"));
        assert!(debug_str.contains("7.5"));
    }

    #[test]
    fn test_gpu_device_clone_preserves_available_memory() {
        let total = 16 * 1024 * 1024 * 1024u64;
        let available = 10 * 1024 * 1024 * 1024u64;
        let gpu = GpuDevice {
            id: 2,
            name: "A100".to_string(),
            total_memory: total,
            available_memory: RwLock::new(available),
            compute_capability: Some("8.0".to_string()),
        };

        let cloned = gpu.clone();
        assert_eq!(cloned.id, 2);
        assert_eq!(cloned.name, "A100");
        assert_eq!(cloned.total_memory, total);
        assert_eq!(*cloned.available_memory.blocking_read(), available);
        assert_eq!(cloned.compute_capability, Some("8.0".to_string()));
    }

    #[test]
    fn test_gpu_device_no_compute_capability() {
        let gpu = GpuDevice {
            id: 0,
            name: "Intel Integrated".to_string(),
            total_memory: 2 * 1024 * 1024 * 1024,
            available_memory: RwLock::new(2 * 1024 * 1024 * 1024),
            compute_capability: None,
        };
        assert!(gpu.compute_capability.is_none());
    }

    #[tokio::test]
    async fn test_resource_manager_new() {
        let rm = ResourceManager::new().await.unwrap();
        // total_memory should be detected (> 0 on any real system)
        assert!(rm.total_memory() > 0);
        // available should equal total at creation
        assert_eq!(rm.available_memory().await, rm.total_memory());
    }

    #[tokio::test]
    async fn test_resource_manager_list_gpus() {
        let rm = ResourceManager::new().await.unwrap();
        // May be empty if no GPU, but should not panic
        let gpus = rm.list_gpus().await;
        let _ = gpus;
    }

    #[tokio::test]
    async fn test_resource_manager_gpu_allocations_empty() {
        let rm = ResourceManager::new().await.unwrap();
        let allocs = rm.get_gpu_allocations().await;
        assert!(allocs.is_empty());
    }

    #[tokio::test]
    async fn test_resource_manager_reserve_memory() {
        let rm = ResourceManager::new().await.unwrap();
        let total = rm.total_memory();
        let agent_id = AgentId::new();

        // Reserve 1 MB
        let reserve_amount = 1024 * 1024u64;
        rm.reserve_memory(agent_id, reserve_amount).await.unwrap();
        assert_eq!(rm.available_memory().await, total - reserve_amount);
    }

    #[tokio::test]
    async fn test_resource_manager_reserve_memory_insufficient() {
        let rm = ResourceManager::new().await.unwrap();
        let total = rm.total_memory();
        let agent_id = AgentId::new();

        // Try to reserve more than total
        let result = rm.reserve_memory(agent_id, total + 1).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Insufficient memory"));
    }

    #[tokio::test]
    async fn test_resource_manager_release_memory() {
        let rm = ResourceManager::new().await.unwrap();
        let total = rm.total_memory();
        let agent_id = AgentId::new();

        let reserve_amount = 1024 * 1024u64;
        rm.reserve_memory(agent_id, reserve_amount).await.unwrap();
        assert_eq!(rm.available_memory().await, total - reserve_amount);

        rm.release_memory(reserve_amount).await;
        assert_eq!(rm.available_memory().await, total);
    }

    #[tokio::test]
    async fn test_resource_manager_release_memory_capped_at_total() {
        let rm = ResourceManager::new().await.unwrap();
        let total = rm.total_memory();

        // Release more than was reserved — should be capped at total
        rm.release_memory(total + 1_000_000).await;
        assert_eq!(rm.available_memory().await, total);
    }

    #[tokio::test]
    async fn test_resource_manager_reserve_multiple() {
        let rm = ResourceManager::new().await.unwrap();
        let total = rm.total_memory();
        let amount = 1024 * 1024u64;

        let a1 = AgentId::new();
        let a2 = AgentId::new();
        rm.reserve_memory(a1, amount).await.unwrap();
        rm.reserve_memory(a2, amount).await.unwrap();
        assert_eq!(rm.available_memory().await, total - 2 * amount);
    }

    #[tokio::test]
    async fn test_resource_manager_allocate_cpu() {
        let rm = ResourceManager::new().await.unwrap();
        let agent_id = AgentId::new();

        let cores = rm.allocate_cpu(agent_id, 1).await.unwrap();
        assert_eq!(cores, vec![0]);
    }

    #[tokio::test]
    async fn test_resource_manager_allocate_cpu_multiple() {
        let rm = ResourceManager::new().await.unwrap();
        let agent_id = AgentId::new();

        let cores = rm.allocate_cpu(agent_id, 2).await.unwrap();
        assert_eq!(cores, vec![0, 1]);
    }

    #[tokio::test]
    async fn test_resource_manager_allocate_cpu_too_many() {
        let rm = ResourceManager::new().await.unwrap();
        let agent_id = AgentId::new();

        // Request more cores than available
        let result = rm.allocate_cpu(agent_id, 99999).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resource_manager_release_cpu() {
        let rm = ResourceManager::new().await.unwrap();
        let agent_id = AgentId::new();

        rm.allocate_cpu(agent_id, 1).await.unwrap();
        let result = rm.release_cpu(agent_id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_resource_manager_release_cpu_not_allocated() {
        let rm = ResourceManager::new().await.unwrap();
        let agent_id = AgentId::new();

        // Release without allocating — should be ok
        let result = rm.release_cpu(agent_id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_resource_manager_release_gpu_not_allocated() {
        let rm = ResourceManager::new().await.unwrap();
        let agent_id = AgentId::new();

        // Release without allocating — should be ok
        let result = rm.release_gpu(agent_id).await;
        assert!(result.is_ok());
    }
}
