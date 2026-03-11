//! Self-hosting validation — programmatic bootstrap readiness checks
//!
//! Validates that an AGNOS system has the tools, libraries, and recipes
//! needed to rebuild itself from source. Used by the agent runtime to
//! report self-hosting status via `/v1/health` and the dashboard.
//!
//! Named phases mirror the shell scripts:
//!   1. Toolchain — GCC, binutils, Rust, build tools
//!   2. Kernel — headers, build dir, module compilation support
//!   3. Userland — Cargo workspace, crate sources, dependencies
//!   4. Packages — ark-build.sh, recipe directory, dependency closure

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// Overall result of a self-hosting validation run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfHostReport {
    /// Timestamp of the validation run (UTC).
    pub timestamp: String,
    /// AGNOS version being validated.
    pub version: String,
    /// Individual phase reports.
    pub phases: Vec<PhaseReport>,
    /// Aggregate counts.
    pub total_passed: usize,
    pub total_failed: usize,
    pub total_skipped: usize,
    /// True if all phases passed.
    pub ready: bool,
}

/// Report for a single validation phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseReport {
    pub phase: ValidationPhase,
    pub checks: Vec<CheckResult>,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub duration_ms: u64,
}

/// The four validation phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValidationPhase {
    Toolchain,
    Kernel,
    Userland,
    Packages,
}

impl std::fmt::Display for ValidationPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Toolchain => write!(f, "toolchain"),
            Self::Kernel => write!(f, "kernel"),
            Self::Userland => write!(f, "userland"),
            Self::Packages => write!(f, "packages"),
        }
    }
}

/// Result of a single check within a phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
    pub detail: Option<String>,
}

/// Status of an individual check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Fail,
    Skip,
}

impl std::fmt::Display for CheckStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pass => write!(f, "PASS"),
            Self::Fail => write!(f, "FAIL"),
            Self::Skip => write!(f, "SKIP"),
        }
    }
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the self-hosting validator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfHostConfig {
    /// Root filesystem path (default: "/").
    pub root: PathBuf,
    /// Source tree path (default: "/usr/src/agnos").
    pub source_path: PathBuf,
    /// Recipe directories to check.
    pub recipe_dirs: Vec<PathBuf>,
    /// Minimum required disk space in GB.
    pub min_disk_gb: u64,
    /// Minimum required memory in MB.
    pub min_memory_mb: u64,
    /// Required toolchain binaries.
    pub required_tools: Vec<String>,
    /// Required Rust crates in the workspace.
    pub required_crates: Vec<String>,
}

impl Default for SelfHostConfig {
    fn default() -> Self {
        Self {
            root: PathBuf::from("/"),
            source_path: PathBuf::from("/usr/src/agnos"),
            recipe_dirs: vec![
                PathBuf::from("/usr/share/agnos/recipes"),
                PathBuf::from("/etc/agnos/recipes"),
            ],
            min_disk_gb: 10,
            min_memory_mb: 2048,
            required_tools: vec![
                "gcc".into(),
                "g++".into(),
                "make".into(),
                "ld".into(),
                "as".into(),
                "ar".into(),
                "rustc".into(),
                "cargo".into(),
                "cmake".into(),
                "pkg-config".into(),
                "autoconf".into(),
                "automake".into(),
                "bison".into(),
                "m4".into(),
                "perl".into(),
                "python3".into(),
            ],
            required_crates: vec![
                "agnos-common".into(),
                "agnos-sys".into(),
                "agent-runtime".into(),
                "llm-gateway".into(),
                "ai-shell".into(),
                "desktop-environment".into(),
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// Validator
// ---------------------------------------------------------------------------

/// Self-hosting readiness validator.
///
/// Checks that the system has everything needed to rebuild AGNOS from source.
pub struct SelfHostValidator {
    config: SelfHostConfig,
}

impl SelfHostValidator {
    /// Create a new validator with default configuration.
    pub fn new() -> Self {
        Self {
            config: SelfHostConfig::default(),
        }
    }

    /// Create a new validator with custom configuration.
    pub fn with_config(config: SelfHostConfig) -> Self {
        Self { config }
    }

    /// Run all validation phases and return a complete report.
    pub fn validate_all(&self) -> SelfHostReport {
        info!("Starting self-hosting validation");
        let start = std::time::Instant::now();

        let phases = vec![
            self.validate_toolchain(),
            self.validate_kernel(),
            self.validate_userland(),
            self.validate_packages(),
        ];

        let total_passed: usize = phases.iter().map(|p| p.passed).sum();
        let total_failed: usize = phases.iter().map(|p| p.failed).sum();
        let total_skipped: usize = phases.iter().map(|p| p.skipped).sum();
        let ready = total_failed == 0;

        let elapsed = start.elapsed();
        info!(
            "Self-hosting validation complete: {} passed, {} failed, {} skipped ({}ms)",
            total_passed,
            total_failed,
            total_skipped,
            elapsed.as_millis()
        );

        SelfHostReport {
            timestamp: chrono::Utc::now().to_rfc3339(),
            version: std::fs::read_to_string(self.config.root.join("etc/agnos/version"))
                .unwrap_or_else(|_| "unknown".into())
                .trim()
                .to_string(),
            phases,
            total_passed,
            total_failed,
            total_skipped,
            ready,
        }
    }

    /// Validate a single phase.
    pub fn validate_phase(&self, phase: ValidationPhase) -> PhaseReport {
        match phase {
            ValidationPhase::Toolchain => self.validate_toolchain(),
            ValidationPhase::Kernel => self.validate_kernel(),
            ValidationPhase::Userland => self.validate_userland(),
            ValidationPhase::Packages => self.validate_packages(),
        }
    }

    // --- Phase 1: Toolchain ---

    fn validate_toolchain(&self) -> PhaseReport {
        let start = std::time::Instant::now();
        let mut checks = Vec::new();

        info!("Validating toolchain...");

        // Check each required tool
        for tool in &self.config.required_tools {
            let found = which_bin(tool).is_some();
            checks.push(CheckResult {
                name: format!("{}_present", tool),
                status: if found { CheckStatus::Pass } else { CheckStatus::Fail },
                detail: which_bin(tool).map(|p| p.to_string_lossy().to_string()),
            });
        }

        // Check system headers
        let include_paths = [
            self.config.root.join("usr/include/stdio.h"),
            self.config.root.join("usr/include/stdlib.h"),
            self.config.root.join("usr/include/linux/version.h"),
        ];
        for path in &include_paths {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            checks.push(CheckResult {
                name: format!("header_{}", name),
                status: if path.exists() {
                    CheckStatus::Pass
                } else {
                    CheckStatus::Fail
                },
                detail: Some(path.to_string_lossy().to_string()),
            });
        }

        // Check pkg-config libraries needed for userland
        let required_libs = ["openssl", "zlib"];
        for lib in &required_libs {
            let found = pkg_config_exists(lib);
            checks.push(CheckResult {
                name: format!("lib_{}", lib),
                status: if found { CheckStatus::Pass } else { CheckStatus::Fail },
                detail: None,
            });
        }

        // Check disk space
        let disk_ok = check_disk_space(&self.config.root, self.config.min_disk_gb);
        checks.push(CheckResult {
            name: "disk_space".into(),
            status: if disk_ok { CheckStatus::Pass } else { CheckStatus::Fail },
            detail: Some(format!("minimum {}GB required", self.config.min_disk_gb)),
        });

        // Check memory
        let mem_ok = check_memory(self.config.min_memory_mb);
        checks.push(CheckResult {
            name: "memory".into(),
            status: if mem_ok { CheckStatus::Pass } else { CheckStatus::Fail },
            detail: Some(format!("minimum {}MB required", self.config.min_memory_mb)),
        });

        build_phase_report(ValidationPhase::Toolchain, checks, start)
    }

    // --- Phase 2: Kernel ---

    fn validate_kernel(&self) -> PhaseReport {
        let start = std::time::Instant::now();
        let mut checks = Vec::new();

        info!("Validating kernel build support...");

        // Check kernel headers directory
        let kernel_include = self.config.root.join("usr/include/linux");
        checks.push(CheckResult {
            name: "kernel_headers".into(),
            status: if kernel_include.is_dir() {
                CheckStatus::Pass
            } else {
                CheckStatus::Fail
            },
            detail: Some(kernel_include.to_string_lossy().to_string()),
        });

        // Find kernel build directory
        let kver = get_kernel_version();
        let build_dir_candidates = [
            self.config
                .root
                .join(format!("lib/modules/{}/build", kver)),
            self.config.root.join("usr/src/linux"),
        ];

        let mut build_dir_found = false;
        for candidate in &build_dir_candidates {
            if candidate.is_dir() {
                checks.push(CheckResult {
                    name: "kernel_build_dir".into(),
                    status: CheckStatus::Pass,
                    detail: Some(candidate.to_string_lossy().to_string()),
                });
                build_dir_found = true;

                // Check for Makefile
                let makefile = candidate.join("Makefile");
                checks.push(CheckResult {
                    name: "kernel_makefile".into(),
                    status: if makefile.exists() {
                        CheckStatus::Pass
                    } else {
                        CheckStatus::Fail
                    },
                    detail: None,
                });
                break;
            }
        }

        if !build_dir_found {
            checks.push(CheckResult {
                name: "kernel_build_dir".into(),
                status: CheckStatus::Fail,
                detail: Some("no kernel build directory found".into()),
            });
        }

        // Check required kernel build tools
        for tool in &["bc", "kmod", "depmod"] {
            let found = which_bin(tool).is_some();
            checks.push(CheckResult {
                name: format!("{}_present", tool),
                status: if found { CheckStatus::Pass } else { CheckStatus::Fail },
                detail: None,
            });
        }

        // Check AGNOS kernel module sources
        let modules_dir = self.config.source_path.join("kernel/modules");
        if modules_dir.is_dir() {
            let module_count = count_files_with_extension(&modules_dir, "c");
            checks.push(CheckResult {
                name: "agnos_kernel_modules".into(),
                status: CheckStatus::Pass,
                detail: Some(format!("{} source files", module_count)),
            });
        } else {
            checks.push(CheckResult {
                name: "agnos_kernel_modules".into(),
                status: CheckStatus::Skip,
                detail: Some(format!("not found at {}", modules_dir.display())),
            });
        }

        build_phase_report(ValidationPhase::Kernel, checks, start)
    }

    // --- Phase 3: Userland ---

    fn validate_userland(&self) -> PhaseReport {
        let start = std::time::Instant::now();
        let mut checks = Vec::new();

        info!("Validating userland build support...");

        // Check cargo and rustc
        checks.push(CheckResult {
            name: "rustc_available".into(),
            status: if which_bin("rustc").is_some() {
                CheckStatus::Pass
            } else {
                CheckStatus::Fail
            },
            detail: None,
        });

        checks.push(CheckResult {
            name: "cargo_available".into(),
            status: if which_bin("cargo").is_some() {
                CheckStatus::Pass
            } else {
                CheckStatus::Fail
            },
            detail: None,
        });

        // Check workspace Cargo.toml
        let workspace_toml = self.config.source_path.join("userland/Cargo.toml");
        checks.push(CheckResult {
            name: "workspace_cargo_toml".into(),
            status: if workspace_toml.exists() {
                CheckStatus::Pass
            } else {
                CheckStatus::Fail
            },
            detail: Some(workspace_toml.to_string_lossy().to_string()),
        });

        // Check each required crate
        for crate_name in &self.config.required_crates {
            let crate_dir = self
                .config
                .source_path
                .join("userland")
                .join(crate_name);
            let crate_toml = crate_dir.join("Cargo.toml");
            checks.push(CheckResult {
                name: format!("crate_{}", crate_name),
                status: if crate_toml.exists() {
                    CheckStatus::Pass
                } else {
                    CheckStatus::Fail
                },
                detail: Some(crate_dir.to_string_lossy().to_string()),
            });
        }

        // Check Cargo.lock exists (reproducible builds)
        let lock_file = self.config.source_path.join("userland/Cargo.lock");
        checks.push(CheckResult {
            name: "cargo_lock".into(),
            status: if lock_file.exists() {
                CheckStatus::Pass
            } else {
                CheckStatus::Fail
            },
            detail: None,
        });

        // Check system libraries needed for compilation
        let sys_libs = [
            ("libseccomp", "seccomp"),
            ("libcap", "capability"),
            ("openssl", "TLS"),
        ];
        for (lib, purpose) in &sys_libs {
            let found = pkg_config_exists(lib);
            checks.push(CheckResult {
                name: format!("syslib_{}", lib),
                status: if found { CheckStatus::Pass } else { CheckStatus::Fail },
                detail: Some(format!("needed for {}", purpose)),
            });
        }

        build_phase_report(ValidationPhase::Userland, checks, start)
    }

    // --- Phase 4: Packages ---

    fn validate_packages(&self) -> PhaseReport {
        let start = std::time::Instant::now();
        let mut checks = Vec::new();

        info!("Validating package build support...");

        // Check ark-build.sh
        let ark_build_candidates = [
            self.config.root.join("usr/lib/agnos/ark-build.sh"),
            self.config.root.join("usr/local/bin/ark-build.sh"),
            self.config.source_path.join("scripts/ark-build.sh"),
        ];

        let mut ark_build_found = false;
        for candidate in &ark_build_candidates {
            if candidate.exists() {
                checks.push(CheckResult {
                    name: "ark_build_script".into(),
                    status: CheckStatus::Pass,
                    detail: Some(candidate.to_string_lossy().to_string()),
                });
                ark_build_found = true;
                break;
            }
        }
        if !ark_build_found {
            checks.push(CheckResult {
                name: "ark_build_script".into(),
                status: CheckStatus::Fail,
                detail: Some("ark-build.sh not found".into()),
            });
        }

        // Find recipe directory
        let mut recipe_dir_found = None;
        let mut recipe_candidates = self.config.recipe_dirs.clone();
        recipe_candidates.push(self.config.source_path.join("recipes"));

        for candidate in &recipe_candidates {
            if candidate.is_dir() {
                recipe_dir_found = Some(candidate.clone());
                checks.push(CheckResult {
                    name: "recipe_directory".into(),
                    status: CheckStatus::Pass,
                    detail: Some(candidate.to_string_lossy().to_string()),
                });
                break;
            }
        }
        if recipe_dir_found.is_none() {
            checks.push(CheckResult {
                name: "recipe_directory".into(),
                status: CheckStatus::Fail,
                detail: Some("no recipe directory found".into()),
            });
        }

        // Count and validate recipes
        if let Some(ref recipe_dir) = recipe_dir_found {
            let recipes = discover_recipes(recipe_dir);
            checks.push(CheckResult {
                name: "recipe_count".into(),
                status: if recipes.len() >= 100 {
                    CheckStatus::Pass
                } else {
                    CheckStatus::Fail
                },
                detail: Some(format!("{} recipes found (need >= 100)", recipes.len())),
            });

            // Check dependency closure
            let (closure_ok, missing) = check_dependency_closure(&recipes);
            checks.push(CheckResult {
                name: "dependency_closure".into(),
                status: if closure_ok {
                    CheckStatus::Pass
                } else {
                    CheckStatus::Fail
                },
                detail: if missing.is_empty() {
                    None
                } else {
                    Some(format!("missing: {}", missing.join(", ")))
                },
            });

            // Check critical base packages exist
            let critical_packages = [
                "glibc",
                "gcc",
                "binutils",
                "linux-api-headers",
                "coreutils",
                "bash",
                "make",
                "openssl",
            ];
            for pkg in &critical_packages {
                let found = recipes.iter().any(|r| r.name == *pkg);
                checks.push(CheckResult {
                    name: format!("recipe_{}", pkg),
                    status: if found { CheckStatus::Pass } else { CheckStatus::Fail },
                    detail: None,
                });
            }
        }

        build_phase_report(ValidationPhase::Packages, checks, start)
    }
}

impl Default for SelfHostValidator {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Recipe discovery (lightweight TOML parsing)
// ---------------------------------------------------------------------------

/// Minimal recipe info for dependency checking.
#[derive(Debug, Clone)]
pub struct RecipeInfo {
    pub name: String,
    pub path: PathBuf,
    pub runtime_deps: Vec<String>,
    pub build_deps: Vec<String>,
}

/// Discover all recipes in a directory tree.
pub fn discover_recipes(dir: &Path) -> Vec<RecipeInfo> {
    let mut recipes = Vec::new();

    let walker = match std::fs::read_dir(dir) {
        Ok(w) => w,
        Err(e) => {
            warn!("Cannot read recipe directory {}: {}", dir.display(), e);
            return recipes;
        }
    };

    for entry in walker.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Recurse into subdirectories
            recipes.extend(discover_recipes(&path));
        } else if path.extension().is_some_and(|e| e == "toml") {
            if let Some(info) = parse_recipe_info(&path) {
                recipes.push(info);
            }
        }
    }

    recipes
}

/// Parse minimal recipe info from a TOML file (lightweight, no full TOML parser).
fn parse_recipe_info(path: &Path) -> Option<RecipeInfo> {
    let content = std::fs::read_to_string(path).ok()?;

    let name = extract_toml_string(&content, "name")?;
    let runtime_deps = extract_toml_array(&content, "runtime");
    let build_deps = extract_toml_array(&content, "build");

    Some(RecipeInfo {
        name,
        path: path.to_path_buf(),
        runtime_deps,
        build_deps,
    })
}

/// Extract a quoted string value from a TOML field.
fn extract_toml_string(content: &str, field: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(field) && trimmed.contains('=') {
            if let Some(start) = trimmed.find('"') {
                if let Some(end) = trimmed[start + 1..].find('"') {
                    return Some(trimmed[start + 1..start + 1 + end].to_string());
                }
            }
        }
    }
    None
}

/// Extract a TOML array of strings (e.g., `runtime = ["glibc", "openssl"]`).
fn extract_toml_array(content: &str, field: &str) -> Vec<String> {
    let mut result = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(field) && trimmed.contains('=') && trimmed.contains('[') {
            // Extract all quoted strings from the array
            let mut pos = 0;
            let chars: Vec<char> = trimmed.chars().collect();
            while pos < chars.len() {
                if chars[pos] == '"' {
                    let start = pos + 1;
                    pos = start;
                    while pos < chars.len() && chars[pos] != '"' {
                        pos += 1;
                    }
                    if pos < chars.len() {
                        let s: String = chars[start..pos].iter().collect();
                        result.push(s);
                    }
                }
                pos += 1;
            }
            break;
        }
    }
    result
}

/// Check that all recipe dependencies have corresponding recipes.
fn check_dependency_closure(recipes: &[RecipeInfo]) -> (bool, Vec<String>) {
    let known: HashSet<&str> = recipes.iter().map(|r| r.name.as_str()).collect();

    // Virtual packages that don't need explicit recipes
    let virtual_pkgs: HashSet<&str> = [
        "libstdc++",
        "m4",
        "libudev",
        "python3",
        "pip",
        "cython",
        "gfortran",
        "ninja",
        "libslirp",
    ]
    .into_iter()
    .collect();

    let mut missing = Vec::new();

    for recipe in recipes {
        for dep in recipe.runtime_deps.iter().chain(recipe.build_deps.iter()) {
            if !known.contains(dep.as_str()) && !virtual_pkgs.contains(dep.as_str())
                && !missing.contains(dep) {
                    missing.push(dep.clone());
            }
        }
    }

    missing.sort();
    (missing.is_empty(), missing)
}

// ---------------------------------------------------------------------------
// System checks
// ---------------------------------------------------------------------------

/// Find a binary in PATH.
fn which_bin(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let candidate = dir.join(name);
            if candidate.is_file() {
                Some(candidate)
            } else {
                None
            }
        })
    })
}

/// Check if a pkg-config library exists.
fn pkg_config_exists(name: &str) -> bool {
    std::process::Command::new("pkg-config")
        .args(["--exists", name])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check available disk space at a path.
fn check_disk_space(path: &Path, min_gb: u64) -> bool {
    // Use statvfs via libc or fall back to parsing df output
    let output = std::process::Command::new("df")
        .args(["-BG", path.to_str().unwrap_or("/")])
        .output();

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            // Parse second line: filesystem size used avail use% mount
            if let Some(line) = stdout.lines().nth(1) {
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.len() >= 4 {
                    let avail = fields[3].trim_end_matches('G');
                    return avail.parse::<u64>().unwrap_or(0) >= min_gb;
                }
            }
            false
        }
        Err(_) => {
            debug!("df command failed — skipping disk space check");
            true // Assume OK if we can't check
        }
    }
}

/// Check available system memory.
fn check_memory(min_mb: u64) -> bool {
    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    for line in meminfo.lines() {
        if line.starts_with("MemTotal:") {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() >= 2 {
                let kb = fields[1].parse::<u64>().unwrap_or(0);
                return kb / 1024 >= min_mb;
            }
        }
    }
    true // Assume OK if we can't read /proc/meminfo
}

/// Get the running kernel version.
fn get_kernel_version() -> String {
    std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .unwrap_or_else(|_| "unknown".into())
        .trim()
        .to_string()
}

/// Count files with a specific extension in a directory.
fn count_files_with_extension(dir: &Path, ext: &str) -> usize {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| {
                    e.path()
                        .extension()
                        .is_some_and(|e| e == ext)
                })
                .count()
        })
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_phase_report(
    phase: ValidationPhase,
    checks: Vec<CheckResult>,
    start: std::time::Instant,
) -> PhaseReport {
    let passed = checks.iter().filter(|c| c.status == CheckStatus::Pass).count();
    let failed = checks.iter().filter(|c| c.status == CheckStatus::Fail).count();
    let skipped = checks.iter().filter(|c| c.status == CheckStatus::Skip).count();
    let duration_ms = start.elapsed().as_millis() as u64;

    PhaseReport {
        phase,
        checks,
        passed,
        failed,
        skipped,
        duration_ms,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SelfHostConfig::default();
        assert_eq!(config.root, PathBuf::from("/"));
        assert_eq!(config.source_path, PathBuf::from("/usr/src/agnos"));
        assert_eq!(config.min_disk_gb, 10);
        assert_eq!(config.min_memory_mb, 2048);
        assert!(!config.required_tools.is_empty());
        assert!(config.required_tools.contains(&"gcc".to_string()));
        assert!(config.required_tools.contains(&"rustc".to_string()));
        assert!(config.required_tools.contains(&"cargo".to_string()));
    }

    #[test]
    fn test_custom_config() {
        let config = SelfHostConfig {
            root: PathBuf::from("/mnt/agnos"),
            source_path: PathBuf::from("/home/user/agnos"),
            recipe_dirs: vec![PathBuf::from("/custom/recipes")],
            min_disk_gb: 20,
            min_memory_mb: 4096,
            required_tools: vec!["gcc".into()],
            required_crates: vec!["agent-runtime".into()],
        };
        assert_eq!(config.root, PathBuf::from("/mnt/agnos"));
        assert_eq!(config.min_disk_gb, 20);
    }

    #[test]
    fn test_validation_phase_display() {
        assert_eq!(ValidationPhase::Toolchain.to_string(), "toolchain");
        assert_eq!(ValidationPhase::Kernel.to_string(), "kernel");
        assert_eq!(ValidationPhase::Userland.to_string(), "userland");
        assert_eq!(ValidationPhase::Packages.to_string(), "packages");
    }

    #[test]
    fn test_check_status_display() {
        assert_eq!(CheckStatus::Pass.to_string(), "PASS");
        assert_eq!(CheckStatus::Fail.to_string(), "FAIL");
        assert_eq!(CheckStatus::Skip.to_string(), "SKIP");
    }

    #[test]
    fn test_check_result_creation() {
        let check = CheckResult {
            name: "test_check".into(),
            status: CheckStatus::Pass,
            detail: Some("all good".into()),
        };
        assert_eq!(check.name, "test_check");
        assert_eq!(check.status, CheckStatus::Pass);
        assert_eq!(check.detail, Some("all good".to_string()));
    }

    #[test]
    fn test_check_result_without_detail() {
        let check = CheckResult {
            name: "simple_check".into(),
            status: CheckStatus::Fail,
            detail: None,
        };
        assert!(check.detail.is_none());
    }

    #[test]
    fn test_phase_report_counts() {
        let checks = vec![
            CheckResult {
                name: "a".into(),
                status: CheckStatus::Pass,
                detail: None,
            },
            CheckResult {
                name: "b".into(),
                status: CheckStatus::Pass,
                detail: None,
            },
            CheckResult {
                name: "c".into(),
                status: CheckStatus::Fail,
                detail: None,
            },
            CheckResult {
                name: "d".into(),
                status: CheckStatus::Skip,
                detail: None,
            },
        ];
        let report =
            build_phase_report(ValidationPhase::Toolchain, checks, std::time::Instant::now());
        assert_eq!(report.passed, 2);
        assert_eq!(report.failed, 1);
        assert_eq!(report.skipped, 1);
        assert_eq!(report.phase, ValidationPhase::Toolchain);
    }

    #[test]
    fn test_self_host_report_ready() {
        let report = SelfHostReport {
            timestamp: "2026-03-10T00:00:00Z".into(),
            version: "2026.3.10".into(),
            phases: vec![PhaseReport {
                phase: ValidationPhase::Toolchain,
                checks: vec![CheckResult {
                    name: "gcc".into(),
                    status: CheckStatus::Pass,
                    detail: None,
                }],
                passed: 1,
                failed: 0,
                skipped: 0,
                duration_ms: 10,
            }],
            total_passed: 1,
            total_failed: 0,
            total_skipped: 0,
            ready: true,
        };
        assert!(report.ready);
    }

    #[test]
    fn test_self_host_report_not_ready() {
        let report = SelfHostReport {
            timestamp: "2026-03-10T00:00:00Z".into(),
            version: "2026.3.10".into(),
            phases: vec![],
            total_passed: 5,
            total_failed: 2,
            total_skipped: 1,
            ready: false,
        };
        assert!(!report.ready);
    }

    #[test]
    fn test_extract_toml_string() {
        let content = r#"
[package]
name = "glibc"
version = "2.42"
"#;
        assert_eq!(
            extract_toml_string(content, "name"),
            Some("glibc".to_string())
        );
        assert_eq!(
            extract_toml_string(content, "version"),
            Some("2.42".to_string())
        );
        assert_eq!(extract_toml_string(content, "missing"), None);
    }

    #[test]
    fn test_extract_toml_array() {
        let content = r#"
[depends]
runtime = ["glibc", "openssl", "zlib"]
build = ["gcc", "make"]
optional = []
"#;
        let runtime = extract_toml_array(content, "runtime");
        assert_eq!(runtime, vec!["glibc", "openssl", "zlib"]);

        let build = extract_toml_array(content, "build");
        assert_eq!(build, vec!["gcc", "make"]);

        let optional = extract_toml_array(content, "optional");
        assert!(optional.is_empty());
    }

    #[test]
    fn test_extract_toml_array_empty() {
        let content = "build = []\n";
        let result = extract_toml_array(content, "build");
        assert!(result.is_empty());
    }

    #[test]
    fn test_extract_toml_string_not_found() {
        let content = "name = \"test\"\n";
        assert_eq!(extract_toml_string(content, "version"), None);
    }

    #[test]
    fn test_parse_recipe_info() {
        let dir = std::env::temp_dir().join("agnos_selfhost_test");
        let _ = std::fs::create_dir_all(&dir);
        let recipe_path = dir.join("test.toml");

        let content = r#"[package]
name = "test-pkg"
version = "1.0"
description = "A test package"
license = "MIT"

[source]
url = "https://example.com/test.tar.gz"
sha256 = "abc123"

[depends]
runtime = ["glibc", "openssl"]
build = ["gcc", "make"]

[build]
make = "make"
install = "make install"
"#;
        std::fs::write(&recipe_path, content).unwrap();

        let info = parse_recipe_info(&recipe_path).unwrap();
        assert_eq!(info.name, "test-pkg");
        assert_eq!(info.runtime_deps, vec!["glibc", "openssl"]);
        assert_eq!(info.build_deps, vec!["gcc", "make"]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_dependency_closure_pass() {
        let recipes = vec![
            RecipeInfo {
                name: "gcc".into(),
                path: PathBuf::from("/r/gcc.toml"),
                runtime_deps: vec!["glibc".into()],
                build_deps: vec![],
            },
            RecipeInfo {
                name: "glibc".into(),
                path: PathBuf::from("/r/glibc.toml"),
                runtime_deps: vec![],
                build_deps: vec!["gcc".into()],
            },
        ];
        let (ok, missing) = check_dependency_closure(&recipes);
        assert!(ok);
        assert!(missing.is_empty());
    }

    #[test]
    fn test_dependency_closure_fail() {
        let recipes = vec![RecipeInfo {
            name: "openssl".into(),
            path: PathBuf::from("/r/openssl.toml"),
            runtime_deps: vec!["glibc".into(), "zlib".into()],
            build_deps: vec!["gcc".into()],
        }];
        let (ok, missing) = check_dependency_closure(&recipes);
        assert!(!ok);
        assert!(missing.contains(&"glibc".to_string()));
        assert!(missing.contains(&"zlib".to_string()));
        assert!(missing.contains(&"gcc".to_string()));
    }

    #[test]
    fn test_dependency_closure_virtual_packages() {
        let recipes = vec![RecipeInfo {
            name: "python-pkg".into(),
            path: PathBuf::from("/r/python-pkg.toml"),
            runtime_deps: vec!["python3".into()],
            build_deps: vec!["ninja".into(), "cython".into()],
        }];
        // python3, ninja, cython are virtual — should pass
        let (ok, missing) = check_dependency_closure(&recipes);
        assert!(ok, "virtual deps should be allowed: {:?}", missing);
    }

    #[test]
    fn test_discover_recipes() {
        let dir = std::env::temp_dir().join("agnos_discover_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("base")).unwrap();

        std::fs::write(
            dir.join("base/pkg1.toml"),
            "name = \"pkg1\"\n[depends]\nruntime = []\nbuild = []\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("base/pkg2.toml"),
            "name = \"pkg2\"\n[depends]\nruntime = [\"pkg1\"]\nbuild = []\n",
        )
        .unwrap();

        let recipes = discover_recipes(&dir);
        assert_eq!(recipes.len(), 2);

        let names: HashSet<&str> = recipes.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains("pkg1"));
        assert!(names.contains("pkg2"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_which_bin_finds_sh() {
        // /bin/sh should exist on any Unix system
        let result = which_bin("sh");
        assert!(result.is_some(), "sh should be findable in PATH");
    }

    #[test]
    fn test_which_bin_missing() {
        let result = which_bin("definitely_not_a_real_binary_12345");
        assert!(result.is_none());
    }

    #[test]
    fn test_get_kernel_version() {
        let ver = get_kernel_version();
        // Should return something, even if "unknown"
        assert!(!ver.is_empty());
    }

    #[test]
    fn test_check_memory() {
        // 1 MB minimum should pass on any system
        assert!(check_memory(1));
        // 999999999 MB should fail
        assert!(!check_memory(999_999_999));
    }

    #[test]
    fn test_validator_new() {
        let v = SelfHostValidator::new();
        assert_eq!(v.config.root, PathBuf::from("/"));
    }

    #[test]
    fn test_validator_with_config() {
        let config = SelfHostConfig {
            root: PathBuf::from("/test"),
            ..SelfHostConfig::default()
        };
        let v = SelfHostValidator::with_config(config);
        assert_eq!(v.config.root, PathBuf::from("/test"));
    }

    #[test]
    fn test_validator_default_trait() {
        let v = SelfHostValidator::default();
        assert_eq!(v.config.root, PathBuf::from("/"));
    }

    #[test]
    fn test_validate_phase_dispatch() {
        let v = SelfHostValidator::new();
        // Just verify it doesn't panic for each phase
        let _ = v.validate_phase(ValidationPhase::Toolchain);
        let _ = v.validate_phase(ValidationPhase::Kernel);
        let _ = v.validate_phase(ValidationPhase::Userland);
        let _ = v.validate_phase(ValidationPhase::Packages);
    }

    #[test]
    fn test_validate_all() {
        let v = SelfHostValidator::new();
        let report = v.validate_all();
        assert_eq!(report.phases.len(), 4);
        assert_eq!(
            report.total_passed + report.total_failed + report.total_skipped,
            report
                .phases
                .iter()
                .map(|p| p.checks.len())
                .sum::<usize>()
        );
    }

    #[test]
    fn test_selfhost_report_serialization() {
        let report = SelfHostReport {
            timestamp: "2026-03-10T00:00:00Z".into(),
            version: "2026.3.10".into(),
            phases: vec![PhaseReport {
                phase: ValidationPhase::Toolchain,
                checks: vec![
                    CheckResult {
                        name: "gcc".into(),
                        status: CheckStatus::Pass,
                        detail: Some("/usr/bin/gcc".into()),
                    },
                    CheckResult {
                        name: "rustc".into(),
                        status: CheckStatus::Fail,
                        detail: None,
                    },
                ],
                passed: 1,
                failed: 1,
                skipped: 0,
                duration_ms: 42,
            }],
            total_passed: 1,
            total_failed: 1,
            total_skipped: 0,
            ready: false,
        };

        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"toolchain\""));
        assert!(json.contains("\"pass\""));
        assert!(json.contains("\"fail\""));

        let deserialized: SelfHostReport = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.version, "2026.3.10");
        assert!(!deserialized.ready);
    }

    #[test]
    fn test_phase_report_all_pass() {
        let checks = vec![
            CheckResult {
                name: "a".into(),
                status: CheckStatus::Pass,
                detail: None,
            },
            CheckResult {
                name: "b".into(),
                status: CheckStatus::Pass,
                detail: None,
            },
        ];
        let report =
            build_phase_report(ValidationPhase::Packages, checks, std::time::Instant::now());
        assert_eq!(report.passed, 2);
        assert_eq!(report.failed, 0);
        assert_eq!(report.skipped, 0);
    }

    #[test]
    fn test_phase_report_all_fail() {
        let checks = vec![
            CheckResult {
                name: "a".into(),
                status: CheckStatus::Fail,
                detail: Some("missing".into()),
            },
            CheckResult {
                name: "b".into(),
                status: CheckStatus::Fail,
                detail: None,
            },
        ];
        let report =
            build_phase_report(ValidationPhase::Kernel, checks, std::time::Instant::now());
        assert_eq!(report.passed, 0);
        assert_eq!(report.failed, 2);
    }

    #[test]
    fn test_count_files_with_extension() {
        let dir = std::env::temp_dir().join("agnos_count_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(dir.join("a.c"), "").unwrap();
        std::fs::write(dir.join("b.c"), "").unwrap();
        std::fs::write(dir.join("c.h"), "").unwrap();
        std::fs::write(dir.join("d.rs"), "").unwrap();

        assert_eq!(count_files_with_extension(&dir, "c"), 2);
        assert_eq!(count_files_with_extension(&dir, "h"), 1);
        assert_eq!(count_files_with_extension(&dir, "rs"), 1);
        assert_eq!(count_files_with_extension(&dir, "py"), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_count_files_nonexistent_dir() {
        let dir = PathBuf::from("/nonexistent/dir/12345");
        assert_eq!(count_files_with_extension(&dir, "c"), 0);
    }

    #[test]
    fn test_validation_phase_equality() {
        assert_eq!(ValidationPhase::Toolchain, ValidationPhase::Toolchain);
        assert_ne!(ValidationPhase::Toolchain, ValidationPhase::Kernel);
    }

    #[test]
    fn test_check_status_equality() {
        assert_eq!(CheckStatus::Pass, CheckStatus::Pass);
        assert_ne!(CheckStatus::Pass, CheckStatus::Fail);
        assert_ne!(CheckStatus::Fail, CheckStatus::Skip);
    }

    #[test]
    fn test_recipe_info_clone() {
        let info = RecipeInfo {
            name: "test".into(),
            path: PathBuf::from("/test.toml"),
            runtime_deps: vec!["a".into()],
            build_deps: vec!["b".into()],
        };
        let cloned = info.clone();
        assert_eq!(cloned.name, "test");
        assert_eq!(cloned.runtime_deps, vec!["a"]);
    }

    #[test]
    fn test_selfhost_config_clone() {
        let config = SelfHostConfig::default();
        let cloned = config.clone();
        assert_eq!(cloned.min_disk_gb, config.min_disk_gb);
        assert_eq!(cloned.required_tools.len(), config.required_tools.len());
    }

    #[test]
    fn test_required_tools_comprehensive() {
        let config = SelfHostConfig::default();
        // Must include both C and Rust toolchains
        assert!(config.required_tools.contains(&"gcc".to_string()));
        assert!(config.required_tools.contains(&"rustc".to_string()));
        assert!(config.required_tools.contains(&"cargo".to_string()));
        assert!(config.required_tools.contains(&"make".to_string()));
        assert!(config.required_tools.contains(&"cmake".to_string()));
    }

    #[test]
    fn test_required_crates_comprehensive() {
        let config = SelfHostConfig::default();
        assert!(config.required_crates.contains(&"agnos-common".to_string()));
        assert!(config.required_crates.contains(&"agent-runtime".to_string()));
        assert!(config.required_crates.contains(&"llm-gateway".to_string()));
        assert!(config.required_crates.contains(&"ai-shell".to_string()));
        assert!(config
            .required_crates
            .contains(&"desktop-environment".to_string()));
    }
}
