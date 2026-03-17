//! Hardware acceleration detection, quantization, and model sharding for LLM inference.
//!
//! This module provides:
//! - Accelerator detection (CPU, CUDA, ROCm, Metal, Intel NPU, Apple NPU)
//! - Quantization level configuration (FP32 through Int4)
//! - Model sharding strategies (pipeline, tensor, data parallel)
//! - Automatic sharding plan generation based on available hardware

use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// AcceleratorType
// ---------------------------------------------------------------------------

/// Supported hardware accelerator backends.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AcceleratorType {
    /// Default CPU execution — always available.
    Cpu,
    /// NVIDIA CUDA GPU.
    CudaGpu { device_id: u32 },
    /// AMD ROCm GPU.
    RocmGpu { device_id: u32 },
    /// Apple Metal GPU.
    MetalGpu,
    /// Intel Neural Processing Unit.
    IntelNpu,
    /// Apple Neural Engine (ANE).
    AppleNpu,
}

impl fmt::Display for AcceleratorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cpu => write!(f, "CPU"),
            Self::CudaGpu { device_id } => write!(f, "CUDA GPU (device {})", device_id),
            Self::RocmGpu { device_id } => write!(f, "ROCm GPU (device {})", device_id),
            Self::MetalGpu => write!(f, "Metal GPU"),
            Self::IntelNpu => write!(f, "Intel NPU"),
            Self::AppleNpu => write!(f, "Apple NPU"),
        }
    }
}

impl AcceleratorType {
    /// Returns `true` for any GPU variant.
    pub fn is_gpu(&self) -> bool {
        matches!(
            self,
            Self::CudaGpu { .. } | Self::RocmGpu { .. } | Self::MetalGpu
        )
    }

    /// Returns `true` for any NPU variant.
    pub fn is_npu(&self) -> bool {
        matches!(self, Self::IntelNpu | Self::AppleNpu)
    }

    /// Relative throughput multiplier vs CPU (rough estimate).
    pub fn throughput_multiplier(&self) -> f64 {
        match self {
            Self::Cpu => 1.0,
            Self::CudaGpu { .. } => 20.0,
            Self::RocmGpu { .. } => 15.0,
            Self::MetalGpu => 12.0,
            Self::IntelNpu => 8.0,
            Self::AppleNpu => 10.0,
        }
    }
}

// ---------------------------------------------------------------------------
// QuantizationLevel
// ---------------------------------------------------------------------------

/// Model weight quantization levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QuantizationLevel {
    /// Full precision — FP32, 32 bits per parameter.
    None,
    /// Half precision — FP16, 16 bits per parameter.
    Float16,
    /// Brain floating point — BF16, 16 bits per parameter.
    BFloat16,
    /// 8-bit integer quantization.
    Int8,
    /// 4-bit integer quantization (GPTQ / AWQ style).
    Int4,
}

impl QuantizationLevel {
    /// Number of bits used per model parameter.
    pub fn bits_per_param(&self) -> u32 {
        match self {
            Self::None => 32,
            Self::Float16 => 16,
            Self::BFloat16 => 16,
            Self::Int8 => 8,
            Self::Int4 => 4,
        }
    }

    /// Memory reduction factor relative to FP32.
    ///
    /// E.g. `Float16` returns `2.0` (uses half the memory), `Int4` returns `8.0`.
    pub fn memory_reduction_factor(&self) -> f64 {
        32.0 / self.bits_per_param() as f64
    }
}

impl fmt::Display for QuantizationLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "FP32"),
            Self::Float16 => write!(f, "FP16"),
            Self::BFloat16 => write!(f, "BF16"),
            Self::Int8 => write!(f, "INT8"),
            Self::Int4 => write!(f, "INT4"),
        }
    }
}

// ---------------------------------------------------------------------------
// ModelShard
// ---------------------------------------------------------------------------

/// A slice of model layers assigned to a specific device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelShard {
    /// Unique shard identifier within a plan.
    pub shard_id: u32,
    /// Inclusive layer range `(start, end)`.
    pub layer_range: (u32, u32),
    /// Device this shard is placed on.
    pub device: AcceleratorType,
    /// Estimated memory consumption in bytes.
    pub memory_bytes: u64,
}

impl ModelShard {
    /// Returns the number of layers in this shard.
    pub fn num_layers(&self) -> u32 {
        if self.layer_range.1 >= self.layer_range.0 {
            self.layer_range.1 - self.layer_range.0 + 1
        } else {
            0
        }
    }

    /// Returns `true` if the layer range is valid (start <= end).
    pub fn is_valid(&self) -> bool {
        self.layer_range.0 <= self.layer_range.1
    }
}

// ---------------------------------------------------------------------------
// ShardingStrategy
// ---------------------------------------------------------------------------

/// Strategy for distributing a model across devices.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShardingStrategy {
    /// No sharding — run on a single device.
    None,
    /// Split layers across devices in a pipeline.
    PipelineParallel { num_stages: u32 },
    /// Split individual tensors across devices.
    TensorParallel { num_devices: u32 },
    /// Replicate the full model for higher throughput.
    DataParallel { num_replicas: u32 },
}

impl ShardingStrategy {
    /// Minimum number of devices required.
    pub fn min_devices(&self) -> u32 {
        match self {
            Self::None => 1,
            Self::PipelineParallel { num_stages } => *num_stages,
            Self::TensorParallel { num_devices } => *num_devices,
            Self::DataParallel { num_replicas } => *num_replicas,
        }
    }
}

impl fmt::Display for ShardingStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::PipelineParallel { num_stages } => {
                write!(f, "Pipeline Parallel ({} stages)", num_stages)
            }
            Self::TensorParallel { num_devices } => {
                write!(f, "Tensor Parallel ({} devices)", num_devices)
            }
            Self::DataParallel { num_replicas } => {
                write!(f, "Data Parallel ({} replicas)", num_replicas)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// AcceleratorProfile
// ---------------------------------------------------------------------------

/// Describes a detected hardware accelerator and its capabilities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AcceleratorProfile {
    /// The accelerator type.
    pub accelerator: AcceleratorType,
    /// Whether this device is currently available for use.
    pub available: bool,
    /// Total device memory in bytes.
    pub memory_bytes: u64,
    /// Compute capability string (e.g. `"8.6"` for CUDA Ampere).
    pub compute_capability: Option<String>,
    /// Driver version string.
    pub driver_version: Option<String>,
}

impl AcceleratorProfile {
    /// Returns `true` if this profile supports the given quantization level.
    ///
    /// - CPU and GPU devices support all quantization levels.
    /// - NPU devices only support `Int8` and `Int4`.
    pub fn supports_quantization(&self, level: &QuantizationLevel) -> bool {
        if self.accelerator.is_npu() {
            matches!(level, QuantizationLevel::Int8 | QuantizationLevel::Int4)
        } else {
            // CPU and GPU support all levels.
            true
        }
    }
}

// ---------------------------------------------------------------------------
// ShardingPlan
// ---------------------------------------------------------------------------

/// A concrete plan for distributing model shards across devices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShardingPlan {
    /// Ordered list of model shards.
    pub shards: Vec<ModelShard>,
    /// The parallelism strategy used.
    pub strategy: ShardingStrategy,
    /// Total memory required across all shards.
    pub total_memory_bytes: u64,
    /// Estimated inference throughput (tokens/second), if calculable.
    pub estimated_tokens_per_sec: Option<f64>,
}

impl ShardingPlan {
    /// Returns `true` if the plan fits within the available memory of `registry`.
    pub fn fits_in_memory(&self, registry: &AcceleratorRegistry) -> bool {
        self.total_memory_bytes <= registry.total_memory()
    }
}

// ---------------------------------------------------------------------------
// InferenceConfig
// ---------------------------------------------------------------------------

/// Configuration for a single inference session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InferenceConfig {
    /// Target accelerator.
    pub accelerator: AcceleratorType,
    /// Weight quantization level.
    pub quantization: QuantizationLevel,
    /// Sharding strategy.
    pub sharding: ShardingStrategy,
    /// Maximum batch size for concurrent requests.
    pub max_batch_size: u32,
    /// Maximum sequence length in tokens.
    pub max_sequence_length: u32,
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            accelerator: AcceleratorType::Cpu,
            quantization: QuantizationLevel::None,
            sharding: ShardingStrategy::None,
            max_batch_size: 1,
            max_sequence_length: 2048,
        }
    }
}

// ---------------------------------------------------------------------------
// AcceleratorRegistry
// ---------------------------------------------------------------------------

/// Registry of detected hardware accelerators with planning helpers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceleratorRegistry {
    profiles: Vec<AcceleratorProfile>,
}

impl AcceleratorRegistry {
    /// Creates a new registry with a default CPU profile.
    pub fn new() -> Self {
        Self {
            profiles: vec![AcceleratorProfile {
                accelerator: AcceleratorType::Cpu,
                available: true,
                // Default 16 GiB assumption for CPU addressable memory.
                memory_bytes: 16 * 1024 * 1024 * 1024,
                compute_capability: None,
                driver_version: None,
            }],
        }
    }

    /// Probes the system for available accelerators and returns a populated registry.
    ///
    /// Detection is best-effort: missing tools or sysfs entries simply mean
    /// the corresponding accelerator is not registered.
    pub fn detect_available() -> Self {
        let mut profiles = vec![AcceleratorProfile {
            accelerator: AcceleratorType::Cpu,
            available: true,
            memory_bytes: detect_cpu_memory(),
            compute_capability: None,
            driver_version: None,
        }];

        // NVIDIA CUDA
        if which_exists("nvidia-smi") {
            profiles.push(AcceleratorProfile {
                accelerator: AcceleratorType::CudaGpu { device_id: 0 },
                available: true,
                // Default 8 GiB; real implementation would parse nvidia-smi output.
                memory_bytes: 8 * 1024 * 1024 * 1024,
                compute_capability: Some("8.0".to_string()),
                driver_version: Some("535.0".to_string()),
            });
        }

        // AMD ROCm
        if which_exists("rocm-smi") {
            profiles.push(AcceleratorProfile {
                accelerator: AcceleratorType::RocmGpu { device_id: 0 },
                available: true,
                memory_bytes: 8 * 1024 * 1024 * 1024,
                compute_capability: None,
                driver_version: None,
            });
        }

        // Intel NPU
        if Path::new("/sys/class/misc/intel_npu").exists() {
            profiles.push(AcceleratorProfile {
                accelerator: AcceleratorType::IntelNpu,
                available: true,
                memory_bytes: 2 * 1024 * 1024 * 1024,
                compute_capability: None,
                driver_version: None,
            });
        }

        // Apple devices (Metal GPU + ANE) — check /proc/device-tree/compatible
        if let Ok(compat) = std::fs::read_to_string("/proc/device-tree/compatible") {
            if compat.contains("apple") {
                profiles.push(AcceleratorProfile {
                    accelerator: AcceleratorType::MetalGpu,
                    available: true,
                    memory_bytes: 16 * 1024 * 1024 * 1024,
                    compute_capability: None,
                    driver_version: None,
                });
                profiles.push(AcceleratorProfile {
                    accelerator: AcceleratorType::AppleNpu,
                    available: true,
                    memory_bytes: 4 * 1024 * 1024 * 1024,
                    compute_capability: None,
                    driver_version: None,
                });
            }
        }

        Self { profiles }
    }

    /// Returns all registered profiles (including unavailable ones).
    pub fn all_profiles(&self) -> &[AcceleratorProfile] {
        &self.profiles
    }

    /// Returns only the available accelerator profiles.
    pub fn available_devices(&self) -> Vec<&AcceleratorProfile> {
        self.profiles.iter().filter(|p| p.available).collect()
    }

    /// Returns the highest-capability available device.
    ///
    /// Priority: CUDA GPU > ROCm GPU > Metal GPU > Apple NPU > Intel NPU > CPU.
    pub fn best_available(&self) -> Option<&AcceleratorProfile> {
        self.profiles.iter().filter(|p| p.available).max_by(|a, b| {
            let rank = |p: &AcceleratorProfile| -> u32 {
                match &p.accelerator {
                    AcceleratorType::CudaGpu { .. } => 60,
                    AcceleratorType::RocmGpu { .. } => 50,
                    AcceleratorType::MetalGpu => 40,
                    AcceleratorType::AppleNpu => 30,
                    AcceleratorType::IntelNpu => 20,
                    AcceleratorType::Cpu => 10,
                }
            };
            rank(a).cmp(&rank(b))
        })
    }

    /// Total memory across all **available** devices.
    pub fn total_memory(&self) -> u64 {
        self.profiles
            .iter()
            .filter(|p| p.available)
            .map(|p| p.memory_bytes)
            .sum()
    }

    /// Total GPU/NPU memory (excludes CPU).
    pub fn total_gpu_memory(&self) -> u64 {
        self.profiles
            .iter()
            .filter(|p| p.available && (p.accelerator.is_gpu() || p.accelerator.is_npu()))
            .map(|p| p.memory_bytes)
            .sum()
    }

    /// Suggest a quantization level based on available GPU VRAM and model size.
    ///
    /// - FP16 if best GPU has >= model memory at FP16
    /// - Int8 if best GPU has >= model memory at Int8
    /// - Int4 if best GPU has >= model memory at Int4
    /// - FP16 fallback (CPU) if no GPU can fit the model
    pub fn suggest_quantization(&self, model_params: u64) -> QuantizationLevel {
        let best_gpu = self
            .profiles
            .iter()
            .filter(|p| p.available && p.accelerator.is_gpu())
            .map(|p| p.memory_bytes)
            .max();

        let gpu_mem = match best_gpu {
            Some(m) => m,
            None => return QuantizationLevel::Float16, // No GPU — use FP16 on CPU
        };

        // Try from highest quality to most compressed
        for quant in &[
            QuantizationLevel::Float16,
            QuantizationLevel::Int8,
            QuantizationLevel::Int4,
        ] {
            if Self::estimate_memory(model_params, quant) <= gpu_mem {
                return quant.clone();
            }
        }

        // Model is too large even at Int4 — fall back to FP16 (will use CPU)
        QuantizationLevel::Float16
    }

    /// Returns true if any GPU or NPU is available.
    pub fn has_gpu(&self) -> bool {
        self.profiles
            .iter()
            .any(|p| p.available && (p.accelerator.is_gpu() || p.accelerator.is_npu()))
    }

    /// Estimates the memory required for a model with `model_params` parameters
    /// at the given quantization level.
    ///
    /// Formula: `model_params * (bits_per_param / 8)` plus a 20% overhead for
    /// activations, KV cache, and runtime buffers.
    pub fn estimate_memory(model_params: u64, quant: &QuantizationLevel) -> u64 {
        let bytes_per_param = quant.bits_per_param() as u64;
        let raw = model_params * bytes_per_param / 8;
        // Add 20% overhead for activations / KV cache.
        raw + raw / 5
    }

    /// Generates a sharding plan for a model given its parameter count and
    /// quantization level.
    ///
    /// Strategy selection:
    /// - If the model fits on the best single device, use `None` (no sharding).
    /// - If multiple GPUs exist, use `PipelineParallel`.
    /// - Otherwise fall back to CPU with `None`.
    pub fn plan_sharding(&self, model_params: u64, quant: &QuantizationLevel) -> ShardingPlan {
        let needed = Self::estimate_memory(model_params, quant);
        let best = match self.best_available() {
            Some(b) => b,
            None => {
                return ShardingPlan {
                    shards: vec![],
                    strategy: ShardingStrategy::DataParallel { num_replicas: 0 },
                    total_memory_bytes: 0,
                    estimated_tokens_per_sec: None,
                };
            }
        };

        // Case 1: fits on a single best device.
        if needed <= best.memory_bytes {
            let tps = estimate_tokens_per_sec(&best.accelerator, model_params, quant);
            return ShardingPlan {
                shards: vec![ModelShard {
                    shard_id: 0,
                    layer_range: (0, 0),
                    device: best.accelerator.clone(),
                    memory_bytes: needed,
                }],
                strategy: ShardingStrategy::None,
                total_memory_bytes: needed,
                estimated_tokens_per_sec: Some(tps),
            };
        }

        // Case 2: try pipeline parallel across available GPU/NPU devices.
        let gpu_devices: Vec<&AcceleratorProfile> = self
            .profiles
            .iter()
            .filter(|p| p.available && (p.accelerator.is_gpu() || p.accelerator.is_npu()))
            .collect();

        let gpu_memory: u64 = gpu_devices.iter().map(|p| p.memory_bytes).sum();

        if !gpu_devices.is_empty() && gpu_memory >= needed {
            let num_stages = gpu_devices.len() as u32;
            let per_shard = needed / num_stages as u64;
            // Estimate layers from model params (rough: 1 layer per ~250M params)
            let estimated_layers = (model_params / 250_000_000).max(1) as u32;
            let layers_per_shard = (estimated_layers / num_stages).max(1);
            let shards: Vec<ModelShard> = gpu_devices
                .iter()
                .enumerate()
                .map(|(i, dev)| {
                    let start = i as u32 * layers_per_shard;
                    let end = start + layers_per_shard - 1;
                    ModelShard {
                        shard_id: i as u32,
                        layer_range: (start, end),
                        device: dev.accelerator.clone(),
                        memory_bytes: per_shard,
                    }
                })
                .collect();

            let slowest_multiplier = gpu_devices
                .iter()
                .map(|d| d.accelerator.throughput_multiplier())
                .fold(f64::INFINITY, f64::min);
            let tps = slowest_multiplier * 10.0 / (quant.bits_per_param() as f64 / 4.0);

            return ShardingPlan {
                shards,
                strategy: ShardingStrategy::PipelineParallel { num_stages },
                total_memory_bytes: needed,
                estimated_tokens_per_sec: Some(tps),
            };
        }

        // Case 3: fall back to CPU.
        let tps = self
            .profiles
            .iter()
            .find(|p| matches!(p.accelerator, AcceleratorType::Cpu))
            .map(|cpu| estimate_tokens_per_sec(&cpu.accelerator, model_params, quant))
            .unwrap_or(0.0);

        ShardingPlan {
            shards: vec![ModelShard {
                shard_id: 0,
                layer_range: (0, 0),
                device: AcceleratorType::Cpu,
                memory_bytes: needed,
            }],
            strategy: ShardingStrategy::None,
            total_memory_bytes: needed,
            estimated_tokens_per_sec: Some(tps),
        }
    }

    /// Adds a profile to the registry (useful for testing or manual config).
    pub fn add_profile(&mut self, profile: AcceleratorProfile) {
        self.profiles.push(profile);
    }
}

impl Default for AcceleratorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Rough tokens/sec estimate based on device and model size.
fn estimate_tokens_per_sec(
    accel: &AcceleratorType,
    model_params: u64,
    quant: &QuantizationLevel,
) -> f64 {
    // Base: ~1 tok/s per billion params on CPU at FP32.
    let base = 1_000_000_000.0 / model_params as f64;
    let quant_speedup = quant.memory_reduction_factor();
    base * accel.throughput_multiplier() * quant_speedup
}

/// Returns available system memory (or 16 GiB fallback).
fn detect_cpu_memory() -> u64 {
    if let Ok(info) = std::fs::read_to_string("/proc/meminfo") {
        for line in info.lines() {
            if line.starts_with("MemTotal:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(kb_str) = parts.get(1) {
                    if let Ok(kb) = kb_str.parse::<u64>() {
                        return kb * 1024;
                    }
                }
            }
        }
    }
    // Fallback: 16 GiB.
    16 * 1024 * 1024 * 1024
}

/// Checks if an executable is on `$PATH`.
fn which_exists(name: &str) -> bool {
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            if Path::new(dir).join(name).exists() {
                return true;
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- AcceleratorType Display --

    #[test]
    fn test_accelerator_type_display_cpu() {
        assert_eq!(AcceleratorType::Cpu.to_string(), "CPU");
    }

    #[test]
    fn test_accelerator_type_display_cuda() {
        let a = AcceleratorType::CudaGpu { device_id: 3 };
        assert_eq!(a.to_string(), "CUDA GPU (device 3)");
    }

    #[test]
    fn test_accelerator_type_display_rocm() {
        let a = AcceleratorType::RocmGpu { device_id: 0 };
        assert_eq!(a.to_string(), "ROCm GPU (device 0)");
    }

    #[test]
    fn test_accelerator_type_display_metal() {
        assert_eq!(AcceleratorType::MetalGpu.to_string(), "Metal GPU");
    }

    #[test]
    fn test_accelerator_type_display_intel_npu() {
        assert_eq!(AcceleratorType::IntelNpu.to_string(), "Intel NPU");
    }

    #[test]
    fn test_accelerator_type_display_apple_npu() {
        assert_eq!(AcceleratorType::AppleNpu.to_string(), "Apple NPU");
    }

    #[test]
    fn test_accelerator_type_is_gpu() {
        assert!(!AcceleratorType::Cpu.is_gpu());
        assert!(AcceleratorType::CudaGpu { device_id: 0 }.is_gpu());
        assert!(AcceleratorType::RocmGpu { device_id: 0 }.is_gpu());
        assert!(AcceleratorType::MetalGpu.is_gpu());
        assert!(!AcceleratorType::IntelNpu.is_gpu());
        assert!(!AcceleratorType::AppleNpu.is_gpu());
    }

    #[test]
    fn test_accelerator_type_is_npu() {
        assert!(!AcceleratorType::Cpu.is_npu());
        assert!(!AcceleratorType::CudaGpu { device_id: 0 }.is_npu());
        assert!(AcceleratorType::IntelNpu.is_npu());
        assert!(AcceleratorType::AppleNpu.is_npu());
    }

    // -- QuantizationLevel --

    #[test]
    fn test_quantization_bits_per_param() {
        assert_eq!(QuantizationLevel::None.bits_per_param(), 32);
        assert_eq!(QuantizationLevel::Float16.bits_per_param(), 16);
        assert_eq!(QuantizationLevel::BFloat16.bits_per_param(), 16);
        assert_eq!(QuantizationLevel::Int8.bits_per_param(), 8);
        assert_eq!(QuantizationLevel::Int4.bits_per_param(), 4);
    }

    #[test]
    fn test_quantization_memory_reduction_factor() {
        assert!((QuantizationLevel::None.memory_reduction_factor() - 1.0).abs() < f64::EPSILON);
        assert!((QuantizationLevel::Float16.memory_reduction_factor() - 2.0).abs() < f64::EPSILON);
        assert!((QuantizationLevel::BFloat16.memory_reduction_factor() - 2.0).abs() < f64::EPSILON);
        assert!((QuantizationLevel::Int8.memory_reduction_factor() - 4.0).abs() < f64::EPSILON);
        assert!((QuantizationLevel::Int4.memory_reduction_factor() - 8.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_quantization_display() {
        assert_eq!(QuantizationLevel::None.to_string(), "FP32");
        assert_eq!(QuantizationLevel::Float16.to_string(), "FP16");
        assert_eq!(QuantizationLevel::BFloat16.to_string(), "BF16");
        assert_eq!(QuantizationLevel::Int8.to_string(), "INT8");
        assert_eq!(QuantizationLevel::Int4.to_string(), "INT4");
    }

    // -- ShardingStrategy --

    #[test]
    fn test_sharding_strategy_min_devices_none() {
        assert_eq!(ShardingStrategy::None.min_devices(), 1);
    }

    #[test]
    fn test_sharding_strategy_min_devices_pipeline() {
        let s = ShardingStrategy::PipelineParallel { num_stages: 4 };
        assert_eq!(s.min_devices(), 4);
    }

    #[test]
    fn test_sharding_strategy_min_devices_tensor() {
        let s = ShardingStrategy::TensorParallel { num_devices: 8 };
        assert_eq!(s.min_devices(), 8);
    }

    #[test]
    fn test_sharding_strategy_min_devices_data() {
        let s = ShardingStrategy::DataParallel { num_replicas: 3 };
        assert_eq!(s.min_devices(), 3);
    }

    // -- AcceleratorProfile supports_quantization --

    #[test]
    fn test_cpu_supports_all_quantization() {
        let cpu = AcceleratorProfile {
            accelerator: AcceleratorType::Cpu,
            available: true,
            memory_bytes: 1024,
            compute_capability: None,
            driver_version: None,
        };
        assert!(cpu.supports_quantization(&QuantizationLevel::None));
        assert!(cpu.supports_quantization(&QuantizationLevel::Float16));
        assert!(cpu.supports_quantization(&QuantizationLevel::BFloat16));
        assert!(cpu.supports_quantization(&QuantizationLevel::Int8));
        assert!(cpu.supports_quantization(&QuantizationLevel::Int4));
    }

    #[test]
    fn test_gpu_supports_all_quantization() {
        let gpu = AcceleratorProfile {
            accelerator: AcceleratorType::CudaGpu { device_id: 0 },
            available: true,
            memory_bytes: 8 * 1024 * 1024 * 1024,
            compute_capability: Some("8.6".into()),
            driver_version: Some("535.0".into()),
        };
        assert!(gpu.supports_quantization(&QuantizationLevel::None));
        assert!(gpu.supports_quantization(&QuantizationLevel::Float16));
        assert!(gpu.supports_quantization(&QuantizationLevel::Int4));
    }

    #[test]
    fn test_npu_only_supports_int_quantization() {
        let npu = AcceleratorProfile {
            accelerator: AcceleratorType::IntelNpu,
            available: true,
            memory_bytes: 2 * 1024 * 1024 * 1024,
            compute_capability: None,
            driver_version: None,
        };
        assert!(!npu.supports_quantization(&QuantizationLevel::None));
        assert!(!npu.supports_quantization(&QuantizationLevel::Float16));
        assert!(!npu.supports_quantization(&QuantizationLevel::BFloat16));
        assert!(npu.supports_quantization(&QuantizationLevel::Int8));
        assert!(npu.supports_quantization(&QuantizationLevel::Int4));
    }

    #[test]
    fn test_apple_npu_only_supports_int_quantization() {
        let npu = AcceleratorProfile {
            accelerator: AcceleratorType::AppleNpu,
            available: true,
            memory_bytes: 4 * 1024 * 1024 * 1024,
            compute_capability: None,
            driver_version: None,
        };
        assert!(!npu.supports_quantization(&QuantizationLevel::None));
        assert!(npu.supports_quantization(&QuantizationLevel::Int8));
        assert!(npu.supports_quantization(&QuantizationLevel::Int4));
    }

    // -- AcceleratorRegistry --

    #[test]
    fn test_registry_new_has_cpu() {
        let reg = AcceleratorRegistry::new();
        assert_eq!(reg.profiles.len(), 1);
        assert_eq!(reg.profiles[0].accelerator, AcceleratorType::Cpu);
        assert!(reg.profiles[0].available);
    }

    #[test]
    fn test_registry_detect_available_has_cpu() {
        let reg = AcceleratorRegistry::detect_available();
        let cpu = reg
            .profiles
            .iter()
            .find(|p| matches!(p.accelerator, AcceleratorType::Cpu));
        assert!(cpu.is_some());
        assert!(cpu.unwrap().available);
    }

    #[test]
    fn test_registry_best_available_cpu_only() {
        let reg = AcceleratorRegistry::new();
        let best = reg.best_available().unwrap();
        assert_eq!(best.accelerator, AcceleratorType::Cpu);
    }

    #[test]
    fn test_registry_best_available_prefers_cuda() {
        let mut reg = AcceleratorRegistry::new();
        reg.add_profile(AcceleratorProfile {
            accelerator: AcceleratorType::CudaGpu { device_id: 0 },
            available: true,
            memory_bytes: 8 * 1024 * 1024 * 1024,
            compute_capability: Some("8.6".into()),
            driver_version: None,
        });
        let best = reg.best_available().unwrap();
        assert!(matches!(best.accelerator, AcceleratorType::CudaGpu { .. }));
    }

    #[test]
    fn test_registry_best_available_skips_unavailable() {
        let mut reg = AcceleratorRegistry::new();
        reg.add_profile(AcceleratorProfile {
            accelerator: AcceleratorType::CudaGpu { device_id: 0 },
            available: false, // not available
            memory_bytes: 24 * 1024 * 1024 * 1024,
            compute_capability: None,
            driver_version: None,
        });
        let best = reg.best_available().unwrap();
        assert_eq!(best.accelerator, AcceleratorType::Cpu);
    }

    #[test]
    fn test_registry_available_devices() {
        let mut reg = AcceleratorRegistry::new();
        reg.add_profile(AcceleratorProfile {
            accelerator: AcceleratorType::RocmGpu { device_id: 0 },
            available: true,
            memory_bytes: 8 * 1024 * 1024 * 1024,
            compute_capability: None,
            driver_version: None,
        });
        reg.add_profile(AcceleratorProfile {
            accelerator: AcceleratorType::IntelNpu,
            available: false,
            memory_bytes: 2 * 1024 * 1024 * 1024,
            compute_capability: None,
            driver_version: None,
        });
        let avail = reg.available_devices();
        assert_eq!(avail.len(), 2); // CPU + ROCm
    }

    #[test]
    fn test_registry_total_memory() {
        let mut reg = AcceleratorRegistry::new(); // 16 GiB CPU
        reg.add_profile(AcceleratorProfile {
            accelerator: AcceleratorType::CudaGpu { device_id: 0 },
            available: true,
            memory_bytes: 8 * 1024 * 1024 * 1024,
            compute_capability: None,
            driver_version: None,
        });
        // 16 GiB + 8 GiB = 24 GiB
        assert_eq!(reg.total_memory(), 24 * 1024 * 1024 * 1024);
    }

    // -- estimate_memory --

    #[test]
    fn test_estimate_memory_fp32() {
        // 1 billion params at FP32 = 4 bytes each = 4 GB + 20% = 4.8 GB
        let est = AcceleratorRegistry::estimate_memory(1_000_000_000, &QuantizationLevel::None);
        assert_eq!(est, 4_800_000_000);
    }

    #[test]
    fn test_estimate_memory_int4() {
        // 1 billion params at INT4 = 0.5 bytes each = 500 MB + 20% = 600 MB
        let est = AcceleratorRegistry::estimate_memory(1_000_000_000, &QuantizationLevel::Int4);
        assert_eq!(est, 600_000_000);
    }

    #[test]
    fn test_estimate_memory_fp16() {
        // 7 billion params at FP16 = 2 bytes each = 14 GB + 20% = 16.8 GB
        let est = AcceleratorRegistry::estimate_memory(7_000_000_000, &QuantizationLevel::Float16);
        assert_eq!(est, 16_800_000_000);
    }

    // -- ShardingPlan fits_in_memory --

    #[test]
    fn test_sharding_plan_fits_in_memory() {
        let reg = AcceleratorRegistry::new(); // 16 GiB
        let plan = ShardingPlan {
            shards: vec![],
            strategy: ShardingStrategy::None,
            total_memory_bytes: 8 * 1024 * 1024 * 1024,
            estimated_tokens_per_sec: None,
        };
        assert!(plan.fits_in_memory(&reg));
    }

    #[test]
    fn test_sharding_plan_does_not_fit() {
        let reg = AcceleratorRegistry::new(); // 16 GiB
        let plan = ShardingPlan {
            shards: vec![],
            strategy: ShardingStrategy::None,
            total_memory_bytes: 64 * 1024 * 1024 * 1024,
            estimated_tokens_per_sec: None,
        };
        assert!(!plan.fits_in_memory(&reg));
    }

    // -- InferenceConfig Default --

    #[test]
    fn test_inference_config_default() {
        let cfg = InferenceConfig::default();
        assert_eq!(cfg.accelerator, AcceleratorType::Cpu);
        assert_eq!(cfg.quantization, QuantizationLevel::None);
        assert_eq!(cfg.sharding, ShardingStrategy::None);
        assert_eq!(cfg.max_batch_size, 1);
        assert_eq!(cfg.max_sequence_length, 2048);
    }

    // -- plan_sharding --

    #[test]
    fn test_plan_sharding_small_model_single_device() {
        let reg = AcceleratorRegistry::new();
        // 1B params at INT4 = 600 MB — fits on CPU.
        let plan = reg.plan_sharding(1_000_000_000, &QuantizationLevel::Int4);
        assert_eq!(plan.strategy, ShardingStrategy::None);
        assert_eq!(plan.shards.len(), 1);
        assert!(plan.estimated_tokens_per_sec.is_some());
    }

    #[test]
    fn test_plan_sharding_prefers_gpu() {
        let mut reg = AcceleratorRegistry::new();
        reg.add_profile(AcceleratorProfile {
            accelerator: AcceleratorType::CudaGpu { device_id: 0 },
            available: true,
            memory_bytes: 24 * 1024 * 1024 * 1024,
            compute_capability: Some("8.6".into()),
            driver_version: None,
        });
        // 7B at FP16 = 16.8 GB — fits on 24 GiB GPU.
        let plan = reg.plan_sharding(7_000_000_000, &QuantizationLevel::Float16);
        assert_eq!(plan.strategy, ShardingStrategy::None);
        assert!(matches!(
            plan.shards[0].device,
            AcceleratorType::CudaGpu { .. }
        ));
    }

    #[test]
    fn test_plan_sharding_pipeline_when_too_large_for_single() {
        let mut reg = AcceleratorRegistry::new();
        // Two 8 GiB GPUs.
        reg.add_profile(AcceleratorProfile {
            accelerator: AcceleratorType::CudaGpu { device_id: 0 },
            available: true,
            memory_bytes: 8 * 1024 * 1024 * 1024,
            compute_capability: None,
            driver_version: None,
        });
        reg.add_profile(AcceleratorProfile {
            accelerator: AcceleratorType::CudaGpu { device_id: 1 },
            available: true,
            memory_bytes: 8 * 1024 * 1024 * 1024,
            compute_capability: None,
            driver_version: None,
        });
        // 7B at FP16 = 16.8 GB — doesn't fit on one 8 GiB GPU, but fits on 2.
        let plan = reg.plan_sharding(7_000_000_000, &QuantizationLevel::Float16);
        assert!(matches!(
            plan.strategy,
            ShardingStrategy::PipelineParallel { .. }
        ));
        assert_eq!(plan.shards.len(), 2);
    }

    #[test]
    fn test_plan_sharding_falls_back_to_cpu() {
        let reg = AcceleratorRegistry::new();
        // 70B at FP32 = 336 GB — won't fit, falls back to CPU shard.
        let plan = reg.plan_sharding(70_000_000_000, &QuantizationLevel::None);
        assert_eq!(plan.strategy, ShardingStrategy::None);
        assert_eq!(plan.shards[0].device, AcceleratorType::Cpu);
    }

    // -- ModelShard --

    #[test]
    fn test_model_shard_num_layers() {
        let shard = ModelShard {
            shard_id: 0,
            layer_range: (0, 31),
            device: AcceleratorType::Cpu,
            memory_bytes: 1024,
        };
        assert_eq!(shard.num_layers(), 32);
    }

    #[test]
    fn test_model_shard_single_layer() {
        let shard = ModelShard {
            shard_id: 0,
            layer_range: (5, 5),
            device: AcceleratorType::Cpu,
            memory_bytes: 1024,
        };
        assert_eq!(shard.num_layers(), 1);
    }

    #[test]
    fn test_model_shard_is_valid() {
        let valid = ModelShard {
            shard_id: 0,
            layer_range: (0, 10),
            device: AcceleratorType::Cpu,
            memory_bytes: 0,
        };
        assert!(valid.is_valid());

        let invalid = ModelShard {
            shard_id: 0,
            layer_range: (10, 5),
            device: AcceleratorType::Cpu,
            memory_bytes: 0,
        };
        assert!(!invalid.is_valid());
    }

    #[test]
    fn test_model_shard_invalid_range_zero_layers() {
        let shard = ModelShard {
            shard_id: 0,
            layer_range: (10, 5),
            device: AcceleratorType::Cpu,
            memory_bytes: 0,
        };
        assert_eq!(shard.num_layers(), 0);
    }

    // -- Misc --

    #[test]
    fn test_throughput_multiplier_ordering() {
        assert!(
            AcceleratorType::Cpu.throughput_multiplier()
                < AcceleratorType::IntelNpu.throughput_multiplier()
        );
        assert!(
            AcceleratorType::IntelNpu.throughput_multiplier()
                < AcceleratorType::CudaGpu { device_id: 0 }.throughput_multiplier()
        );
    }

    #[test]
    fn test_registry_default_impl() {
        let reg = AcceleratorRegistry::default();
        assert_eq!(reg.profiles.len(), 1);
    }

    #[test]
    fn test_sharding_strategy_display() {
        assert_eq!(ShardingStrategy::None.to_string(), "None");
        assert_eq!(
            ShardingStrategy::PipelineParallel { num_stages: 2 }.to_string(),
            "Pipeline Parallel (2 stages)"
        );
        assert_eq!(
            ShardingStrategy::TensorParallel { num_devices: 4 }.to_string(),
            "Tensor Parallel (4 devices)"
        );
        assert_eq!(
            ShardingStrategy::DataParallel { num_replicas: 3 }.to_string(),
            "Data Parallel (3 replicas)"
        );
    }
}
