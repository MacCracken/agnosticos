//! Hardware acceleration detection, quantization, and model sharding for LLM inference.
//!
//! This module re-exports types from the `ai-hwaccel` crate and adds
//! hoosh-specific inference configuration.

// Re-export everything from ai-hwaccel that the old API exposed.
// Some are only used by downstream consumers (benchmarks, agent-runtime),
// not by the binary itself.
#[allow(unused_imports)]
pub use ai_hwaccel::{
    estimate_training_memory, AcceleratorFamily, AcceleratorProfile, AcceleratorRegistry,
    AcceleratorRequirement, AcceleratorType, MemoryEstimate, ModelShard, QuantizationLevel,
    ShardingPlan, ShardingStrategy, TrainingMethod, TrainingTarget,
};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Backward-compatible extension trait
// ---------------------------------------------------------------------------

/// Extension methods that preserve the original hoosh `AcceleratorRegistry` API.
///
/// The upstream `ai-hwaccel` crate renamed several methods; this trait keeps
/// the old call-sites compiling without changes.
#[allow(dead_code)]
pub trait AcceleratorRegistryExt {
    /// Probe the system for available accelerators (wraps `AcceleratorRegistry::detect`).
    fn detect_available() -> AcceleratorRegistry;
    /// Returns `true` if any GPU or NPU is available (wraps `has_accelerator`).
    fn has_gpu(&self) -> bool;
    /// Total GPU/NPU memory in bytes, excluding CPU (wraps `total_accelerator_memory`).
    fn total_gpu_memory(&self) -> u64;
    /// Only the available accelerator profiles (wraps `available`).
    fn available_devices(&self) -> Vec<&AcceleratorProfile>;
}

impl AcceleratorRegistryExt for AcceleratorRegistry {
    fn detect_available() -> AcceleratorRegistry {
        AcceleratorRegistry::detect()
    }

    fn has_gpu(&self) -> bool {
        self.has_accelerator()
    }

    fn total_gpu_memory(&self) -> u64 {
        self.total_accelerator_memory()
    }

    fn available_devices(&self) -> Vec<&AcceleratorProfile> {
        self.available()
    }
}

// ---------------------------------------------------------------------------
// InferenceConfig (hoosh-specific)
// ---------------------------------------------------------------------------

/// Configuration for a single inference session.
#[allow(dead_code)]
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
// Tests — verify backward compatibility
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inference_config_default() {
        let cfg = InferenceConfig::default();
        assert_eq!(cfg.accelerator, AcceleratorType::Cpu);
        assert_eq!(cfg.quantization, QuantizationLevel::None);
        assert_eq!(cfg.sharding, ShardingStrategy::None);
        assert_eq!(cfg.max_batch_size, 1);
        assert_eq!(cfg.max_sequence_length, 2048);
    }

    #[test]
    fn accelerator_type_display() {
        assert_eq!(AcceleratorType::Cpu.to_string(), "CPU");
        assert_eq!(
            AcceleratorType::CudaGpu { device_id: 0 }.to_string(),
            "CUDA GPU (device 0)"
        );
    }

    #[test]
    fn accelerator_type_is_gpu() {
        assert!(AcceleratorType::CudaGpu { device_id: 0 }.is_gpu());
        assert!(AcceleratorType::RocmGpu { device_id: 0 }.is_gpu());
        assert!(AcceleratorType::MetalGpu.is_gpu());
        assert!(!AcceleratorType::Cpu.is_gpu());
        assert!(!AcceleratorType::IntelNpu.is_gpu());
    }

    #[test]
    fn quantization_bits_per_param() {
        assert_eq!(QuantizationLevel::None.bits_per_param(), 32);
        assert_eq!(QuantizationLevel::Float16.bits_per_param(), 16);
        assert_eq!(QuantizationLevel::Int8.bits_per_param(), 8);
        assert_eq!(QuantizationLevel::Int4.bits_per_param(), 4);
    }

    #[test]
    fn registry_detect_has_cpu() {
        let reg = AcceleratorRegistry::detect();
        assert!(reg
            .all_profiles()
            .iter()
            .any(|p| matches!(p.accelerator, AcceleratorType::Cpu)));
    }

    #[test]
    fn sharding_strategy_display() {
        assert_eq!(ShardingStrategy::None.to_string(), "None");
    }

    // New ai-hwaccel types now available through hoosh
    #[test]
    fn new_accelerator_types_available() {
        // TPU
        let tpu = AcceleratorType::Tpu {
            device_id: 0,
            chip_count: 4,
            version: ai_hwaccel::TpuVersion::V5p,
        };
        assert!(tpu.is_tpu());
        assert!(!tpu.is_gpu());

        // Gaudi
        let gaudi = AcceleratorType::Gaudi {
            device_id: 0,
            generation: ai_hwaccel::GaudiGeneration::Gaudi3,
        };
        assert!(gaudi.is_ai_asic());
    }

    #[test]
    fn suggest_quantization_works() {
        let reg = AcceleratorRegistry::new();
        let quant = reg.suggest_quantization(7_000_000_000);
        // CPU-only should suggest FP16
        assert_eq!(quant, QuantizationLevel::Float16);
    }

    #[test]
    fn plan_sharding_works() {
        let reg = AcceleratorRegistry::new();
        let plan = reg.plan_sharding(1_000_000_000, &QuantizationLevel::Int4);
        assert_eq!(plan.strategy, ShardingStrategy::None);
        assert_eq!(plan.shards.len(), 1);
    }

    // Backward-compat extension trait tests
    #[test]
    fn detect_available_compat() {
        let reg = AcceleratorRegistry::detect_available();
        assert!(reg
            .all_profiles()
            .iter()
            .any(|p| matches!(p.accelerator, AcceleratorType::Cpu)));
    }

    #[test]
    fn has_gpu_compat() {
        let reg = AcceleratorRegistry::new();
        // CPU-only registry should return false
        assert!(!reg.has_gpu());
    }

    #[test]
    fn total_gpu_memory_compat() {
        let reg = AcceleratorRegistry::new();
        // CPU-only registry should return 0
        assert_eq!(reg.total_gpu_memory(), 0);
    }

    #[test]
    fn available_devices_compat() {
        let reg = AcceleratorRegistry::new();
        let devs = reg.available_devices();
        assert!(!devs.is_empty());
    }
}
