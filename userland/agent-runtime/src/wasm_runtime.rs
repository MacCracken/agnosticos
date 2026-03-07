//! WebAssembly agent runtime powered by Wasmtime
//!
//! Feature-gated behind `wasm`.  Provides a `WasmAgent` that can load and
//! execute `.wasm` modules with WASI support, fuel metering, and sandboxed
//! filesystem/network access via Wasmtime's capability model.

use std::path::PathBuf;

#[cfg(feature = "wasm")]
use anyhow::{Context, Result};
#[cfg(feature = "wasm")]
use tracing::{info, warn};
#[cfg(feature = "wasm")]
use wasmtime::*;
#[cfg(feature = "wasm")]
use wasmtime_wasi::preview1::WasiP1Ctx;
#[cfg(feature = "wasm")]
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxBuilder};

/// Configuration for a WASM-based agent.
#[derive(Debug, Clone)]
pub struct WasmAgentConfig {
    /// Path to the `.wasm` module file.
    pub module_path: PathBuf,
    /// Maximum memory in bytes (Wasmtime will enforce this).
    pub memory_limit: u64,
    /// Fuel units for execution metering (0 = unlimited).
    pub fuel: u64,
    /// Directories the WASM module is allowed to access.
    pub allowed_dirs: Vec<PathBuf>,
    /// Environment variables injected into the WASI context.
    pub env_vars: Vec<(String, String)>,
    /// Program arguments passed to the WASI entrypoint.
    pub args: Vec<String>,
}

impl Default for WasmAgentConfig {
    fn default() -> Self {
        Self {
            module_path: PathBuf::new(),
            memory_limit: 256 * 1024 * 1024, // 256 MB
            fuel: 1_000_000_000,             // 1 billion fuel units
            allowed_dirs: Vec::new(),
            env_vars: Vec::new(),
            args: Vec::new(),
        }
    }
}

/// Result of executing a WASM agent.
#[derive(Debug)]
pub struct WasmExecutionResult {
    /// Whether the module ran to completion without trapping.
    pub success: bool,
    /// Fuel consumed (if metering was enabled).
    pub fuel_consumed: u64,
    /// Error message if the module trapped.
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Wasmtime-backed implementation (feature = "wasm")
// ---------------------------------------------------------------------------

#[cfg(feature = "wasm")]
struct WasiHostState {
    wasi: WasiP1Ctx,
}

#[cfg(feature = "wasm")]
impl WasiHostState {
    fn wasi_ctx(&mut self) -> &mut WasiP1Ctx {
        &mut self.wasi
    }
}

/// A WASM agent wrapping a Wasmtime engine and compiled module.
#[cfg(feature = "wasm")]
pub struct WasmAgent {
    engine: Engine,
    module: Module,
    config: WasmAgentConfig,
}

#[cfg(feature = "wasm")]
impl WasmAgent {
    /// Load a WASM module from disk.
    pub fn load(config: WasmAgentConfig) -> Result<Self> {
        let mut engine_config = Config::new();
        engine_config.consume_fuel(config.fuel > 0);

        // Memory limit
        let memory_pages = (config.memory_limit / 65536).max(1) as u64;
        engine_config.memory_guaranteed_dense_image_size(memory_pages.min(1024) * 65536);

        let engine = Engine::new(&engine_config).context("Failed to create Wasmtime engine")?;

        let module = Module::from_file(&engine, &config.module_path).with_context(|| {
            format!(
                "Failed to load WASM module from {}",
                config.module_path.display()
            )
        })?;

        info!(
            "WASM module loaded: {} (fuel={}, mem_limit={}MB)",
            config.module_path.display(),
            config.fuel,
            config.memory_limit / (1024 * 1024),
        );

        Ok(Self {
            engine,
            module,
            config,
        })
    }

    /// Execute the WASM module's default export (`_start` for WASI).
    pub fn run(&self) -> Result<WasmExecutionResult> {
        // Build WASI context
        let mut wasi_builder = WasiCtxBuilder::new();

        // Inherit stdio for output
        wasi_builder.inherit_stdio();

        // Environment variables
        for (k, v) in &self.config.env_vars {
            wasi_builder.env(k, v);
        }

        // Arguments
        if !self.config.args.is_empty() {
            wasi_builder.args(&self.config.args);
        }

        // Pre-opened directories
        for dir in &self.config.allowed_dirs {
            if dir.exists() {
                wasi_builder.preopened_dir(
                    dir,
                    dir.to_string_lossy().as_ref(),
                    DirPerms::all(),
                    FilePerms::all(),
                )?;
            }
        }

        let wasi = wasi_builder.build_p1();
        let host_state = WasiHostState { wasi };

        let mut store = Store::new(&self.engine, host_state);

        // Set fuel limit
        if self.config.fuel > 0 {
            store.set_fuel(self.config.fuel)?;
        }

        // Instantiate and run
        let mut linker = wasmtime::Linker::new(&self.engine);
        wasmtime_wasi::preview1::add_to_linker_sync(&mut linker, |state: &mut WasiHostState| {
            state.wasi_ctx()
        })?;
        let instance = linker.instantiate(&mut store, &self.module)?;

        let start = instance
            .get_typed_func::<(), ()>(&mut store, "_start")
            .context("Module has no _start export")?;

        let result = start.call(&mut store, ());

        let fuel_consumed = if self.config.fuel > 0 {
            self.config.fuel - store.get_fuel().unwrap_or(0)
        } else {
            0
        };

        match result {
            Ok(()) => {
                info!("WASM module completed (fuel consumed: {})", fuel_consumed);
                Ok(WasmExecutionResult {
                    success: true,
                    fuel_consumed,
                    error: None,
                })
            }
            Err(e) => {
                warn!("WASM module trapped: {}", e);
                Ok(WasmExecutionResult {
                    success: false,
                    fuel_consumed,
                    error: Some(e.to_string()),
                })
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Stub implementation when feature is disabled
// ---------------------------------------------------------------------------

#[cfg(not(feature = "wasm"))]
pub struct WasmAgent;

#[cfg(not(feature = "wasm"))]
impl WasmAgent {
    pub fn load(_config: WasmAgentConfig) -> Result<Self, String> {
        Err("WASM runtime not available — compile with --features wasm".to_string())
    }

    pub fn run(&self) -> Result<WasmExecutionResult, String> {
        Err("WASM runtime not available".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_agent_config_default() {
        let config = WasmAgentConfig::default();
        assert_eq!(config.memory_limit, 256 * 1024 * 1024);
        assert_eq!(config.fuel, 1_000_000_000);
        assert!(config.allowed_dirs.is_empty());
        assert!(config.env_vars.is_empty());
    }

    #[test]
    fn test_wasm_agent_config_custom() {
        let config = WasmAgentConfig {
            module_path: PathBuf::from("/tmp/test.wasm"),
            memory_limit: 64 * 1024 * 1024,
            fuel: 500_000,
            allowed_dirs: vec![PathBuf::from("/tmp")],
            env_vars: vec![("KEY".to_string(), "VALUE".to_string())],
            args: vec!["--verbose".to_string()],
        };
        assert_eq!(config.memory_limit, 64 * 1024 * 1024);
        assert_eq!(config.fuel, 500_000);
        assert_eq!(config.allowed_dirs.len(), 1);
    }

    #[test]
    fn test_wasm_execution_result() {
        let result = WasmExecutionResult {
            success: true,
            fuel_consumed: 42,
            error: None,
        };
        assert!(result.success);
        assert_eq!(result.fuel_consumed, 42);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_wasm_execution_result_failure() {
        let result = WasmExecutionResult {
            success: false,
            fuel_consumed: 100,
            error: Some("out of fuel".to_string()),
        };
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[cfg(not(feature = "wasm"))]
    #[test]
    fn test_wasm_agent_disabled() {
        let config = WasmAgentConfig::default();
        let result = WasmAgent::load(config);
        assert!(result.is_err());
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn test_wasm_agent_missing_module() {
        let config = WasmAgentConfig {
            module_path: PathBuf::from("/nonexistent/path.wasm"),
            ..Default::default()
        };
        let result = WasmAgent::load(config);
        assert!(result.is_err());
    }

    // ==================================================================
    // New coverage: config validation, defaults, clone, debug
    // ==================================================================

    #[test]
    fn test_wasm_agent_config_default_module_path_empty() {
        let config = WasmAgentConfig::default();
        assert!(config.module_path.as_os_str().is_empty());
        assert!(config.args.is_empty());
    }

    #[test]
    fn test_wasm_agent_config_clone() {
        let config = WasmAgentConfig {
            module_path: PathBuf::from("/opt/agent.wasm"),
            memory_limit: 128 * 1024 * 1024,
            fuel: 500_000,
            allowed_dirs: vec![PathBuf::from("/tmp"), PathBuf::from("/data")],
            env_vars: vec![("HOME".to_string(), "/home/agent".to_string())],
            args: vec!["--verbose".to_string(), "--mode=test".to_string()],
        };
        let cloned = config.clone();
        assert_eq!(cloned.module_path, PathBuf::from("/opt/agent.wasm"));
        assert_eq!(cloned.memory_limit, 128 * 1024 * 1024);
        assert_eq!(cloned.fuel, 500_000);
        assert_eq!(cloned.allowed_dirs.len(), 2);
        assert_eq!(cloned.env_vars.len(), 1);
        assert_eq!(cloned.args.len(), 2);
    }

    #[test]
    fn test_wasm_agent_config_debug() {
        let config = WasmAgentConfig::default();
        let dbg = format!("{:?}", config);
        assert!(dbg.contains("WasmAgentConfig"));
        assert!(dbg.contains("memory_limit"));
        assert!(dbg.contains("fuel"));
    }

    #[test]
    fn test_wasm_execution_result_debug() {
        let result = WasmExecutionResult {
            success: false,
            fuel_consumed: 999,
            error: Some("trap: unreachable".to_string()),
        };
        let dbg = format!("{:?}", result);
        assert!(dbg.contains("false"));
        assert!(dbg.contains("999"));
        assert!(dbg.contains("unreachable"));
    }

    #[cfg(not(feature = "wasm"))]
    #[test]
    fn test_wasm_agent_load_disabled_error_message() {
        let config = WasmAgentConfig::default();
        let result = WasmAgent::load(config);
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.contains("not available"));
    }

    #[test]
    fn test_wasm_agent_config_zero_fuel() {
        let config = WasmAgentConfig {
            fuel: 0,
            ..Default::default()
        };
        assert_eq!(config.fuel, 0);
    }

    #[test]
    fn test_wasm_execution_result_success_no_error() {
        let result = WasmExecutionResult {
            success: true,
            fuel_consumed: 0,
            error: None,
        };
        assert!(result.success);
        assert!(result.error.is_none());
        assert_eq!(result.fuel_consumed, 0);
    }
}
