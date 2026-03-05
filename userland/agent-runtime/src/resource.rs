//! Resource management for agents
//!
//! Handles GPU allocation, memory management, and CPU scheduling.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use agnos_common::AgentId;

/// GPU device information
#[derive(Debug)]
pub struct GpuDevice {
    pub id: u32,
    pub name: String,
    pub total_memory: u64,
    pub available_memory: AtomicU64,
    pub compute_capability: Option<String>,
}

impl Clone for GpuDevice {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            name: self.name.clone(),
            total_memory: self.total_memory,
            available_memory: AtomicU64::new(self.available_memory.load(Ordering::Relaxed)),
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
            let available = gpu.available_memory.load(Ordering::Relaxed);

            if available >= memory_required {
                gpu.available_memory.fetch_sub(memory_required, Ordering::Relaxed);
                
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
                    // Restore full GPU memory (simplified)
                    gpu.available_memory.store(gpu.total_memory, Ordering::Relaxed);
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

        // Try to detect AMD GPUs
        match Self::detect_amd_gpus().await {
            Ok(amd_gpus) => {
                let offset = gpus.len() as u32;
                for mut gpu in amd_gpus {
                    gpu.id += offset;
                    gpus.push(gpu);
                }
            }
            Err(e) => {
                debug!("Failed to detect AMD GPUs: {}", e);
            }
        }

        // Try to detect Intel GPUs
        match Self::detect_intel_gpus().await {
            Ok(intel_gpus) => {
                let offset = gpus.len() as u32;
                for mut gpu in intel_gpus {
                    gpu.id += offset;
                    gpus.push(gpu);
                }
            }
            Err(e) => {
                debug!("Failed to detect Intel GPUs: {}", e);
            }
        }

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
                    available_memory: AtomicU64::new(total_memory),
                    compute_capability: None,
                });
            }
        }

        Ok(gpus)
    }

    /// Detect AMD GPUs via /sys/class/drm and rocm-smi
    async fn detect_amd_gpus() -> Result<Vec<GpuDevice>> {
        use tokio::process::Command;

        // Try rocm-smi first for detailed info
        let output = Command::new("rocm-smi")
            .args(&["--showid", "--showmeminfo", "vram", "--csv"])
            .output()
            .await;

        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8(output.stdout)?;
                let mut gpus = Vec::new();
                // Parse CSV: skip header, each row has device info
                for (idx, line) in stdout.lines().skip(1).enumerate() {
                    let parts: Vec<&str> = line.split(',').collect();
                    let name = parts.get(0).unwrap_or(&"AMD GPU").trim().to_string();
                    let total_mem = parts
                        .get(1)
                        .and_then(|s| s.trim().parse::<u64>().ok())
                        .unwrap_or(0);

                    gpus.push(GpuDevice {
                        id: idx as u32,
                        name,
                        total_memory: total_mem,
                        available_memory: AtomicU64::new(total_mem),
                        compute_capability: None,
                    });
                }
                if !gpus.is_empty() {
                    return Ok(gpus);
                }
            }
        }

        // Fallback: scan /sys/class/drm for AMD render nodes
        let mut gpus = Vec::new();
        let drm_path = std::path::Path::new("/sys/class/drm");
        if drm_path.exists() {
            if let Ok(entries) = std::fs::read_dir(drm_path) {
                for (idx, entry) in entries.flatten().enumerate() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if !name.starts_with("card") || name.contains('-') {
                        continue;
                    }
                    let vendor_path = entry.path().join("device/vendor");
                    if let Ok(vendor) = std::fs::read_to_string(&vendor_path) {
                        let vendor = vendor.trim();
                        // AMD vendor ID
                        if vendor == "0x1002" {
                            let device_name = std::fs::read_to_string(
                                entry.path().join("device/label"),
                            )
                            .unwrap_or_else(|_| format!("AMD GPU {}", idx));
                            let mem_path = entry.path().join("device/mem_info_vram_total");
                            let total_memory = std::fs::read_to_string(&mem_path)
                                .ok()
                                .and_then(|s| s.trim().parse::<u64>().ok())
                                .unwrap_or(0);

                            gpus.push(GpuDevice {
                                id: idx as u32,
                                name: device_name.trim().to_string(),
                                total_memory,
                                available_memory: AtomicU64::new(total_memory),
                                compute_capability: None,
                            });
                        }
                    }
                }
            }
        }

        if gpus.is_empty() {
            anyhow::bail!("No AMD GPUs detected");
        }
        Ok(gpus)
    }

    /// Detect Intel GPUs via /sys/class/drm
    async fn detect_intel_gpus() -> Result<Vec<GpuDevice>> {
        let mut gpus = Vec::new();
        let drm_path = std::path::Path::new("/sys/class/drm");
        if !drm_path.exists() {
            anyhow::bail!("No /sys/class/drm directory");
        }

        if let Ok(entries) = std::fs::read_dir(drm_path) {
            for (idx, entry) in entries.flatten().enumerate() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with("card") || name.contains('-') {
                    continue;
                }
                let vendor_path = entry.path().join("device/vendor");
                if let Ok(vendor) = std::fs::read_to_string(&vendor_path) {
                    let vendor = vendor.trim();
                    // Intel vendor ID
                    if vendor == "0x8086" {
                        let device_name =
                            std::fs::read_to_string(entry.path().join("device/label"))
                                .unwrap_or_else(|_| format!("Intel GPU {}", idx));
                        // Intel GPUs expose local memory via i915 sysfs when available
                        let mem_path = entry.path().join("device/lmem_total_bytes");
                        let total_memory = std::fs::read_to_string(&mem_path)
                            .ok()
                            .and_then(|s| s.trim().parse::<u64>().ok())
                            .unwrap_or(0);

                        gpus.push(GpuDevice {
                            id: idx as u32,
                            name: device_name.trim().to_string(),
                            total_memory,
                            available_memory: AtomicU64::new(total_memory),
                            compute_capability: None,
                        });
                    }
                }
            }
        }

        if gpus.is_empty() {
            anyhow::bail!("No Intel GPUs detected");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_device_clone() {
        let gpu = GpuDevice {
            id: 0,
            name: "RTX 4090".to_string(),
            total_memory: 24 * 1024 * 1024 * 1024,
            available_memory: AtomicU64::new(24 * 1024 * 1024 * 1024),
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
            available_memory: AtomicU64::new(24 * 1024 * 1024 * 1024),
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
            available_memory: AtomicU64::new(8 * 1024 * 1024 * 1024),
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
            available_memory: AtomicU64::new(available),
            compute_capability: Some("8.0".to_string()),
        };

        let cloned = gpu.clone();
        assert_eq!(cloned.id, 2);
        assert_eq!(cloned.name, "A100");
        assert_eq!(cloned.total_memory, total);
        assert_eq!(cloned.available_memory.load(Ordering::Relaxed), available);
        assert_eq!(cloned.compute_capability, Some("8.0".to_string()));
    }

    #[test]
    fn test_gpu_device_no_compute_capability() {
        let gpu = GpuDevice {
            id: 0,
            name: "Intel Integrated".to_string(),
            total_memory: 2 * 1024 * 1024 * 1024,
            available_memory: AtomicU64::new(2 * 1024 * 1024 * 1024),
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

    // ==================================================================
    // Additional coverage: GPU allocation/release with injected GPUs,
    // memory reserve/release edge cases, CPU allocation boundaries,
    // detect_cpu_cores, GpuDevice edge cases, concurrent operations
    // ==================================================================

    #[tokio::test]
    async fn test_resource_manager_gpu_allocation_with_injected_gpu() {
        let rm = ResourceManager::new().await.unwrap();
        let total_mem = 8 * 1024 * 1024 * 1024u64;

        // Inject a test GPU
        {
            let mut gpus = rm.gpus.write().await;
            gpus.push(GpuDevice {
                id: 99,
                name: "Test GPU".to_string(),
                total_memory: total_mem,
                available_memory: AtomicU64::new(total_mem),
                compute_capability: Some("9.0".to_string()),
            });
        }

        let agent_id = AgentId::new();
        let allocated = rm.allocate_gpu(agent_id, 4 * 1024 * 1024 * 1024).await.unwrap();
        assert!(allocated.contains(&99));

        // Check allocations
        let allocs = rm.get_gpu_allocations().await;
        assert!(allocs.contains_key(&agent_id));
    }

    #[tokio::test]
    async fn test_resource_manager_gpu_allocation_insufficient_memory() {
        let rm = ResourceManager::new().await.unwrap();

        // Clear any detected GPUs and inject only a small one
        {
            let mut gpus = rm.gpus.write().await;
            gpus.clear();
            gpus.push(GpuDevice {
                id: 50,
                name: "Small GPU".to_string(),
                total_memory: 1024, // Very small
                available_memory: AtomicU64::new(1024),
                compute_capability: None,
            });
        }

        let agent_id = AgentId::new();
        // Request more memory than available
        let result = rm.allocate_gpu(agent_id, 1024 * 1024).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No GPU"));
    }

    #[tokio::test]
    async fn test_resource_manager_gpu_release_restores_memory() {
        let rm = ResourceManager::new().await.unwrap();
        let total_mem = 16 * 1024 * 1024 * 1024u64;

        {
            let mut gpus = rm.gpus.write().await;
            gpus.push(GpuDevice {
                id: 10,
                name: "Release Test GPU".to_string(),
                total_memory: total_mem,
                available_memory: AtomicU64::new(total_mem),
                compute_capability: None,
            });
        }

        let agent_id = AgentId::new();
        rm.allocate_gpu(agent_id, 4 * 1024 * 1024 * 1024).await.unwrap();

        // After allocation, available should be reduced
        {
            let gpus = rm.gpus.read().await;
            let gpu = gpus.iter().find(|g| g.id == 10).unwrap();
            assert!(gpu.available_memory.load(Ordering::Relaxed) < total_mem);
        }

        // Release
        rm.release_gpu(agent_id).await.unwrap();

        // After release, available should be restored to total
        {
            let gpus = rm.gpus.read().await;
            let gpu = gpus.iter().find(|g| g.id == 10).unwrap();
            assert_eq!(gpu.available_memory.load(Ordering::Relaxed), total_mem);
        }
    }

    #[tokio::test]
    async fn test_resource_manager_memory_reserve_exact_total() {
        let rm = ResourceManager::new().await.unwrap();
        let total = rm.total_memory();
        let agent_id = AgentId::new();

        // Reserve exactly total
        let result = rm.reserve_memory(agent_id, total).await;
        assert!(result.is_ok());
        assert_eq!(rm.available_memory().await, 0);
    }

    #[tokio::test]
    async fn test_resource_manager_memory_reserve_then_release_then_reserve() {
        let rm = ResourceManager::new().await.unwrap();
        let total = rm.total_memory();
        let amount = 1024 * 1024u64;
        let agent_id = AgentId::new();

        rm.reserve_memory(agent_id, amount).await.unwrap();
        rm.release_memory(amount).await;
        assert_eq!(rm.available_memory().await, total);

        // Should be able to reserve again
        rm.reserve_memory(agent_id, amount).await.unwrap();
        assert_eq!(rm.available_memory().await, total - amount);
    }

    #[tokio::test]
    async fn test_resource_manager_cpu_allocate_zero() {
        let rm = ResourceManager::new().await.unwrap();
        let agent_id = AgentId::new();

        let cores = rm.allocate_cpu(agent_id, 0).await.unwrap();
        assert!(cores.is_empty());
    }

    #[tokio::test]
    async fn test_resource_manager_cpu_allocate_exactly_available() {
        let rm = ResourceManager::new().await.unwrap();
        let agent_id = AgentId::new();

        // Get actual core count
        let num_cores = ResourceManager::detect_cpu_cores().await.unwrap();
        let cores = rm.allocate_cpu(agent_id, num_cores).await.unwrap();
        assert_eq!(cores.len(), num_cores);
    }

    #[test]
    fn test_gpu_device_zero_memory() {
        let gpu = GpuDevice {
            id: 0,
            name: "Zero Mem GPU".to_string(),
            total_memory: 0,
            available_memory: AtomicU64::new(0),
            compute_capability: None,
        };
        let cloned = gpu.clone();
        assert_eq!(cloned.total_memory, 0);
        assert_eq!(cloned.available_memory.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_gpu_device_clone_independent_atomic() {
        let gpu = GpuDevice {
            id: 0,
            name: "Atomic Test".to_string(),
            total_memory: 1000,
            available_memory: AtomicU64::new(1000),
            compute_capability: None,
        };

        let cloned = gpu.clone();
        // Modify original's available_memory
        gpu.available_memory.store(500, Ordering::Relaxed);

        // Cloned should still have old value (independent AtomicU64)
        assert_eq!(cloned.available_memory.load(Ordering::Relaxed), 1000);
        assert_eq!(gpu.available_memory.load(Ordering::Relaxed), 500);
    }

    #[tokio::test]
    async fn test_resource_manager_total_memory_positive() {
        let rm = ResourceManager::new().await.unwrap();
        assert!(rm.total_memory() > 0);
    }

    #[tokio::test]
    async fn test_resource_manager_multiple_gpu_allocations() {
        let rm = ResourceManager::new().await.unwrap();

        // Inject two GPUs
        {
            let mut gpus = rm.gpus.write().await;
            for i in 0..2 {
                gpus.push(GpuDevice {
                    id: 200 + i,
                    name: format!("Multi GPU {}", i),
                    total_memory: 4 * 1024 * 1024 * 1024,
                    available_memory: AtomicU64::new(4 * 1024 * 1024 * 1024),
                    compute_capability: None,
                });
            }
        }

        let a1 = AgentId::new();
        let a2 = AgentId::new();

        let _g1 = rm.allocate_gpu(a1, 2 * 1024 * 1024 * 1024).await.unwrap();
        let _g2 = rm.allocate_gpu(a2, 2 * 1024 * 1024 * 1024).await.unwrap();

        let allocs = rm.get_gpu_allocations().await;
        assert_eq!(allocs.len(), 2);
        assert!(allocs.contains_key(&a1));
        assert!(allocs.contains_key(&a2));

        // Release both
        rm.release_gpu(a1).await.unwrap();
        rm.release_gpu(a2).await.unwrap();
        assert!(rm.get_gpu_allocations().await.is_empty());
    }

    #[tokio::test]
    async fn test_detect_cpu_cores_returns_positive() {
        let cores = ResourceManager::detect_cpu_cores().await.unwrap();
        assert!(cores >= 1);
    }

    #[tokio::test]
    async fn test_resource_manager_release_memory_zero() {
        let rm = ResourceManager::new().await.unwrap();
        let total = rm.total_memory();

        rm.release_memory(0).await;
        assert_eq!(rm.available_memory().await, total);
    }
}
