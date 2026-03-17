//! Sandbox Backends — gVisor and Firecracker isolation
//!
//! Provides pluggable sandbox backends beyond native Landlock/seccomp:
//!
//! - **gVisor (runsc)**: Userspace kernel that intercepts all syscalls.
//!   Per-task OCI containers with full syscall isolation. Does not require
//!   Docker — we build OCI bundles directly.
//!
//! - **Firecracker**: Lightweight microVMs with KVM. Each agent task runs
//!   in its own VM with a minimal kernel. Strongest isolation, ~125ms boot.
//!
//! These backends also solve the crewAI 1.11 Docker requirement —
//! CodeInterpreterTool can use gVisor/Firecracker instead of Docker.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// Common Types
// ---------------------------------------------------------------------------

/// Result of running a task in a sandbox backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendResult {
    /// Whether the task completed successfully.
    pub success: bool,
    /// Task output (stdout).
    pub stdout: String,
    /// Task errors (stderr).
    pub stderr: String,
    /// Exit code.
    pub exit_code: i32,
    /// Execution time in milliseconds.
    pub duration_ms: u64,
    /// Resource usage.
    pub resources: ResourceUsage,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub memory_peak_mb: u64,
    pub cpu_time_ms: u64,
    pub io_read_bytes: u64,
    pub io_write_bytes: u64,
}

/// Common configuration for sandbox backends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    /// Maximum memory in MB.
    pub max_memory_mb: u64,
    /// CPU quota as percentage (0-100).
    pub cpu_quota_pct: u8,
    /// Maximum execution time in seconds.
    pub timeout_secs: u64,
    /// Filesystem paths to mount read-only.
    pub readonly_mounts: Vec<String>,
    /// Filesystem paths to mount read-write.
    pub writable_mounts: Vec<String>,
    /// Network access: none, host, or specific ports.
    pub network: NetworkMode,
    /// Environment variables for the sandboxed process.
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMode {
    /// No network access.
    None,
    /// Access to host network (for localhost services).
    Host,
    /// Access only to specific ports on localhost.
    LocalPorts(Vec<u16>),
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            max_memory_mb: 512,
            cpu_quota_pct: 50,
            timeout_secs: 300,
            readonly_mounts: vec!["/usr".to_string(), "/lib".to_string()],
            writable_mounts: vec!["/tmp".to_string()],
            network: NetworkMode::None,
            env: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// gVisor Backend
// ---------------------------------------------------------------------------

/// gVisor (runsc) sandbox backend.
///
/// Builds OCI runtime bundles and executes them via `runsc`.
/// No Docker required — just the `runsc` binary.
#[derive(Debug)]
pub struct GVisorBackend {
    /// Path to the runsc binary.
    runsc_path: PathBuf,
    /// Root directory for OCI bundles.
    bundle_root: PathBuf,
    /// Active containers: container_id → agent_id.
    active: HashMap<String, String>,
}

impl GVisorBackend {
    pub fn new() -> Self {
        Self {
            runsc_path: Self::find_runsc(),
            bundle_root: PathBuf::from("/var/lib/agnos/gvisor/bundles"),
            active: HashMap::new(),
        }
    }

    /// Check if runsc is available.
    pub fn is_available(&self) -> bool {
        self.runsc_path.exists()
    }

    /// Find the runsc binary.
    fn find_runsc() -> PathBuf {
        for path in &[
            "/usr/bin/runsc",
            "/usr/local/bin/runsc",
            "/opt/gvisor/runsc",
        ] {
            let p = PathBuf::from(path);
            if p.exists() {
                return p;
            }
        }
        PathBuf::from("/usr/bin/runsc") // default
    }

    /// Generate an OCI runtime spec (config.json) for a task.
    pub fn generate_oci_spec(
        &self,
        command: &[String],
        config: &BackendConfig,
    ) -> serde_json::Value {
        let mut mounts = vec![
            serde_json::json!({
                "destination": "/proc",
                "type": "proc",
                "source": "proc"
            }),
            serde_json::json!({
                "destination": "/dev",
                "type": "tmpfs",
                "source": "tmpfs",
                "options": ["nosuid", "strictatime", "mode=755", "size=65536k"]
            }),
        ];

        for path in &config.readonly_mounts {
            mounts.push(serde_json::json!({
                "destination": path,
                "type": "bind",
                "source": path,
                "options": ["rbind", "ro"]
            }));
        }

        for path in &config.writable_mounts {
            mounts.push(serde_json::json!({
                "destination": path,
                "type": "bind",
                "source": path,
                "options": ["rbind", "rw"]
            }));
        }

        let env: Vec<String> = config
            .env
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();

        serde_json::json!({
            "ociVersion": "1.0.2",
            "process": {
                "terminal": false,
                "user": { "uid": 65534, "gid": 65534 },
                "args": command,
                "env": env,
                "cwd": "/",
                "capabilities": {
                    "bounding": [],
                    "effective": [],
                    "inheritable": [],
                    "permitted": [],
                    "ambient": []
                },
                "rlimits": [
                    {
                        "type": "RLIMIT_AS",
                        "hard": config.max_memory_mb * 1024 * 1024,
                        "soft": config.max_memory_mb * 1024 * 1024
                    }
                ]
            },
            "root": {
                "path": "rootfs",
                "readonly": true
            },
            "hostname": "agnos-sandbox",
            "mounts": mounts,
            "linux": {
                "namespaces": [
                    { "type": "pid" },
                    { "type": "mount" },
                    { "type": "ipc" },
                    { "type": "uts" },
                    { "type": "network" }
                ],
                "resources": {
                    "memory": {
                        "limit": config.max_memory_mb * 1024 * 1024
                    },
                    "cpu": {
                        "quota": (config.cpu_quota_pct as i64) * 1000,
                        "period": 100000
                    }
                }
            }
        })
    }

    /// Create an OCI bundle for a task.
    pub fn create_bundle(
        &self,
        container_id: &str,
        command: &[String],
        config: &BackendConfig,
    ) -> std::io::Result<PathBuf> {
        let bundle_dir = self.bundle_root.join(container_id);
        std::fs::create_dir_all(&bundle_dir)?;

        let spec = self.generate_oci_spec(command, config);
        let spec_path = bundle_dir.join("config.json");
        std::fs::write(&spec_path, serde_json::to_string_pretty(&spec).unwrap())?;

        // Create minimal rootfs
        let rootfs = bundle_dir.join("rootfs");
        std::fs::create_dir_all(rootfs.join("tmp"))?;
        std::fs::create_dir_all(rootfs.join("dev"))?;
        std::fs::create_dir_all(rootfs.join("proc"))?;

        info!(container_id = %container_id, "gVisor: OCI bundle created");
        Ok(bundle_dir)
    }

    /// Clean up a bundle after execution.
    pub fn cleanup_bundle(&mut self, container_id: &str) -> std::io::Result<()> {
        let bundle_dir = self.bundle_root.join(container_id);
        if bundle_dir.exists() {
            std::fs::remove_dir_all(&bundle_dir)?;
        }
        self.active.remove(container_id);
        debug!(container_id = %container_id, "gVisor: bundle cleaned up");
        Ok(())
    }

    /// Get the runsc command line for running a container.
    pub fn runsc_command(&self, container_id: &str, bundle_path: &Path) -> Vec<String> {
        vec![
            self.runsc_path.to_string_lossy().to_string(),
            "--platform=systrap".to_string(),
            "--network=none".to_string(),
            "run".to_string(),
            "--bundle".to_string(),
            bundle_path.to_string_lossy().to_string(),
            container_id.to_string(),
        ]
    }

    /// Number of active containers.
    pub fn active_count(&self) -> usize {
        self.active.len()
    }
}

impl Default for GVisorBackend {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Firecracker Backend
// ---------------------------------------------------------------------------

/// Firecracker microVM sandbox backend.
///
/// Each agent task runs in its own lightweight VM:
/// - ~125ms boot time
/// - Minimal Linux kernel (from AGNOS edge kernel config)
/// - KVM-based hardware virtualization
/// - Strong isolation: separate kernel, separate address space
#[derive(Debug)]
pub struct FirecrackerBackend {
    /// Path to the firecracker binary.
    firecracker_path: PathBuf,
    /// Path to the jailer binary (optional, for production use).
    jailer_path: Option<PathBuf>,
    /// Path to the microVM kernel image.
    kernel_path: PathBuf,
    /// Path to the base rootfs image.
    rootfs_path: PathBuf,
    /// Working directory for VM sockets and logs.
    work_dir: PathBuf,
    /// Active VMs: vm_id → agent_id.
    active: HashMap<String, String>,
}

impl FirecrackerBackend {
    pub fn new() -> Self {
        Self {
            firecracker_path: Self::find_binary("firecracker"),
            jailer_path: Self::find_optional_binary("jailer"),
            kernel_path: PathBuf::from("/var/lib/agnos/firecracker/vmlinux"),
            rootfs_path: PathBuf::from("/var/lib/agnos/firecracker/rootfs.ext4"),
            work_dir: PathBuf::from("/var/lib/agnos/firecracker/vms"),
            active: HashMap::new(),
        }
    }

    /// Check if Firecracker is available.
    pub fn is_available(&self) -> bool {
        self.firecracker_path.exists()
            && self.kernel_path.exists()
            && Path::new("/dev/kvm").exists()
    }

    fn find_binary(name: &str) -> PathBuf {
        for dir in &["/usr/bin", "/usr/local/bin", "/opt/firecracker"] {
            let p = PathBuf::from(dir).join(name);
            if p.exists() {
                return p;
            }
        }
        PathBuf::from(format!("/usr/bin/{}", name))
    }

    fn find_optional_binary(name: &str) -> Option<PathBuf> {
        for dir in &["/usr/bin", "/usr/local/bin", "/opt/firecracker"] {
            let p = PathBuf::from(dir).join(name);
            if p.exists() {
                return Some(p);
            }
        }
        None
    }

    /// Generate a Firecracker VM configuration.
    pub fn generate_vm_config(
        &self,
        vm_id: &str,
        config: &BackendConfig,
    ) -> serde_json::Value {
        let vcpu_count = ((config.cpu_quota_pct as u32) / 25).clamp(1, 4);
        let socket_path = self.work_dir.join(format!("{}.sock", vm_id));

        serde_json::json!({
            "boot-source": {
                "kernel_image_path": self.kernel_path.to_string_lossy(),
                "boot_args": "console=ttyS0 reboot=k panic=1 pci=off agnos.sandbox=1"
            },
            "drives": [
                {
                    "drive_id": "rootfs",
                    "path_on_host": self.rootfs_path.to_string_lossy(),
                    "is_root_device": true,
                    "is_read_only": true
                }
            ],
            "machine-config": {
                "vcpu_count": vcpu_count,
                "mem_size_mib": config.max_memory_mb,
            },
            "network-interfaces": match &config.network {
                NetworkMode::None => serde_json::json!([]),
                NetworkMode::Host | NetworkMode::LocalPorts(_) => serde_json::json!([
                    {
                        "iface_id": "eth0",
                        "guest_mac": format!("AA:FC:00:00:00:{:02x}", vm_id.len() % 256),
                        "host_dev_name": format!("fc-{}", &vm_id[..8.min(vm_id.len())])
                    }
                ]),
            },
            "socket_path": socket_path.to_string_lossy(),
        })
    }

    /// Prepare a VM work directory.
    pub fn prepare_vm(&mut self, vm_id: &str, agent_id: &str) -> std::io::Result<PathBuf> {
        let vm_dir = self.work_dir.join(vm_id);
        std::fs::create_dir_all(&vm_dir)?;

        self.active.insert(vm_id.to_string(), agent_id.to_string());
        info!(vm_id = %vm_id, agent_id = %agent_id, "Firecracker: VM prepared");
        Ok(vm_dir)
    }

    /// Clean up a VM after execution.
    pub fn cleanup_vm(&mut self, vm_id: &str) -> std::io::Result<()> {
        let vm_dir = self.work_dir.join(vm_id);
        if vm_dir.exists() {
            std::fs::remove_dir_all(&vm_dir)?;
        }
        // Clean up socket
        let sock = self.work_dir.join(format!("{}.sock", vm_id));
        if sock.exists() {
            std::fs::remove_file(&sock)?;
        }
        self.active.remove(vm_id);
        debug!(vm_id = %vm_id, "Firecracker: VM cleaned up");
        Ok(())
    }

    /// Get the firecracker command line.
    pub fn firecracker_command(&self, vm_id: &str) -> Vec<String> {
        let socket = self.work_dir.join(format!("{}.sock", vm_id));

        if let Some(ref jailer) = self.jailer_path {
            // Production: use jailer for additional isolation
            vec![
                jailer.to_string_lossy().to_string(),
                "--id".to_string(),
                vm_id.to_string(),
                "--exec-file".to_string(),
                self.firecracker_path.to_string_lossy().to_string(),
                "--uid".to_string(),
                "65534".to_string(),
                "--gid".to_string(),
                "65534".to_string(),
                "--".to_string(),
                "--api-sock".to_string(),
                socket.to_string_lossy().to_string(),
            ]
        } else {
            vec![
                self.firecracker_path.to_string_lossy().to_string(),
                "--api-sock".to_string(),
                socket.to_string_lossy().to_string(),
            ]
        }
    }

    /// Number of active VMs.
    pub fn active_count(&self) -> usize {
        self.active.len()
    }
}

impl Default for FirecrackerBackend {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- gVisor tests ---

    #[test]
    fn test_gvisor_oci_spec_generation() {
        let backend = GVisorBackend::new();
        let config = BackendConfig::default();
        let spec = backend.generate_oci_spec(&["echo".to_string(), "hello".to_string()], &config);

        assert_eq!(spec["ociVersion"], "1.0.2");
        assert_eq!(spec["process"]["args"][0], "echo");
        assert_eq!(spec["process"]["args"][1], "hello");
        assert_eq!(spec["root"]["readonly"], true);
        assert!(spec["linux"]["namespaces"].as_array().unwrap().len() >= 4);
    }

    #[test]
    fn test_gvisor_oci_spec_memory_limit() {
        let backend = GVisorBackend::new();
        let config = BackendConfig {
            max_memory_mb: 256,
            ..Default::default()
        };
        let spec = backend.generate_oci_spec(&["test".to_string()], &config);
        let mem_limit = spec["linux"]["resources"]["memory"]["limit"].as_u64().unwrap();
        assert_eq!(mem_limit, 256 * 1024 * 1024);
    }

    #[test]
    fn test_gvisor_oci_spec_custom_mounts() {
        let backend = GVisorBackend::new();
        let config = BackendConfig {
            readonly_mounts: vec!["/opt/data".to_string()],
            writable_mounts: vec!["/workspace".to_string()],
            ..Default::default()
        };
        let spec = backend.generate_oci_spec(&["test".to_string()], &config);
        let mounts = spec["mounts"].as_array().unwrap();
        assert!(mounts
            .iter()
            .any(|m| m["destination"] == "/opt/data" && m["options"].as_array().unwrap().contains(&serde_json::json!("ro"))));
        assert!(mounts
            .iter()
            .any(|m| m["destination"] == "/workspace" && m["options"].as_array().unwrap().contains(&serde_json::json!("rw"))));
    }

    #[test]
    fn test_gvisor_runsc_command() {
        let backend = GVisorBackend::new();
        let cmd = backend.runsc_command("test-container", Path::new("/tmp/bundle"));
        assert!(cmd.iter().any(|s| s.contains("runsc")));
        assert!(cmd.contains(&"run".to_string()));
        assert!(cmd.contains(&"test-container".to_string()));
    }

    #[test]
    fn test_gvisor_active_count() {
        let backend = GVisorBackend::new();
        assert_eq!(backend.active_count(), 0);
    }

    // --- Firecracker tests ---

    #[test]
    fn test_firecracker_vm_config() {
        let backend = FirecrackerBackend::new();
        let config = BackendConfig {
            max_memory_mb: 256,
            cpu_quota_pct: 50,
            network: NetworkMode::None,
            ..Default::default()
        };
        let vm_config = backend.generate_vm_config("test-vm", &config);

        assert_eq!(vm_config["machine-config"]["mem_size_mib"], 256);
        assert_eq!(vm_config["machine-config"]["vcpu_count"], 2); // 50% / 25 = 2
        assert!(vm_config["boot-source"]["boot_args"]
            .as_str()
            .unwrap()
            .contains("agnos.sandbox=1"));
    }

    #[test]
    fn test_firecracker_vm_config_network_none() {
        let backend = FirecrackerBackend::new();
        let config = BackendConfig {
            network: NetworkMode::None,
            ..Default::default()
        };
        let vm_config = backend.generate_vm_config("test-vm", &config);
        assert!(vm_config["network-interfaces"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_firecracker_vm_config_network_host() {
        let backend = FirecrackerBackend::new();
        let config = BackendConfig {
            network: NetworkMode::Host,
            ..Default::default()
        };
        let vm_config = backend.generate_vm_config("test-vm", &config);
        assert!(!vm_config["network-interfaces"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_firecracker_vcpu_clamp() {
        let backend = FirecrackerBackend::new();
        // 10% → 1 vcpu (min)
        let config_low = BackendConfig {
            cpu_quota_pct: 10,
            ..Default::default()
        };
        assert_eq!(
            backend.generate_vm_config("vm", &config_low)["machine-config"]["vcpu_count"],
            1
        );
        // 100% → 4 vcpu (max)
        let config_high = BackendConfig {
            cpu_quota_pct: 100,
            ..Default::default()
        };
        assert_eq!(
            backend.generate_vm_config("vm", &config_high)["machine-config"]["vcpu_count"],
            4
        );
    }

    #[test]
    fn test_firecracker_command() {
        let backend = FirecrackerBackend::new();
        let cmd = backend.firecracker_command("test-vm");
        assert!(cmd.iter().any(|s| s.contains("firecracker")));
        assert!(cmd.contains(&"--api-sock".to_string()));
    }

    #[test]
    fn test_firecracker_active_count() {
        let backend = FirecrackerBackend::new();
        assert_eq!(backend.active_count(), 0);
    }

    // --- gVisor bundle lifecycle tests (tempdir) ---

    #[test]
    fn test_gvisor_create_and_cleanup_bundle() {
        let tmpdir = std::env::temp_dir().join(format!("agnos-gvisor-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmpdir).unwrap();

        let mut backend = GVisorBackend {
            runsc_path: PathBuf::from("/usr/bin/runsc"),
            bundle_root: tmpdir.clone(),
            active: HashMap::new(),
        };

        let config = BackendConfig::default();
        let bundle = backend
            .create_bundle("test-ctr", &["echo".to_string()], &config)
            .unwrap();

        // Verify bundle structure
        assert!(bundle.join("config.json").exists());
        assert!(bundle.join("rootfs/tmp").exists());
        assert!(bundle.join("rootfs/dev").exists());
        assert!(bundle.join("rootfs/proc").exists());

        // Verify config.json is valid JSON
        let content = std::fs::read_to_string(bundle.join("config.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["ociVersion"], "1.0.2");

        // Cleanup
        backend.cleanup_bundle("test-ctr").unwrap();
        assert!(!bundle.exists());

        std::fs::remove_dir_all(&tmpdir).ok();
    }

    #[test]
    fn test_gvisor_cleanup_nonexistent_bundle() {
        let tmpdir = std::env::temp_dir().join(format!("agnos-gvisor-test2-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmpdir).unwrap();

        let mut backend = GVisorBackend {
            runsc_path: PathBuf::from("/usr/bin/runsc"),
            bundle_root: tmpdir.clone(),
            active: HashMap::new(),
        };

        // Should not error on non-existent bundle
        backend.cleanup_bundle("nonexistent").unwrap();

        std::fs::remove_dir_all(&tmpdir).ok();
    }

    #[test]
    fn test_gvisor_is_available_false() {
        let backend = GVisorBackend {
            runsc_path: PathBuf::from("/nonexistent/runsc"),
            bundle_root: PathBuf::from("/tmp"),
            active: HashMap::new(),
        };
        assert!(!backend.is_available());
    }

    #[test]
    fn test_gvisor_default() {
        let backend = GVisorBackend::default();
        assert_eq!(backend.active_count(), 0);
    }

    #[test]
    fn test_gvisor_oci_spec_env_vars() {
        let backend = GVisorBackend::new();
        let mut config = BackendConfig::default();
        config.env.insert("RUST_LOG".to_string(), "info".to_string());
        config.env.insert("HOME".to_string(), "/tmp".to_string());
        let spec = backend.generate_oci_spec(&["test".to_string()], &config);
        let env = spec["process"]["env"].as_array().unwrap();
        assert!(env.len() >= 2);
    }

    #[test]
    fn test_gvisor_oci_spec_cpu_quota() {
        let backend = GVisorBackend::new();
        let config = BackendConfig {
            cpu_quota_pct: 75,
            ..Default::default()
        };
        let spec = backend.generate_oci_spec(&["test".to_string()], &config);
        let quota = spec["linux"]["resources"]["cpu"]["quota"].as_i64().unwrap();
        assert_eq!(quota, 75000); // 75 * 1000
    }

    // --- Firecracker lifecycle tests (tempdir) ---

    #[test]
    fn test_firecracker_prepare_and_cleanup_vm() {
        let tmpdir = std::env::temp_dir().join(format!("agnos-fc-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmpdir).unwrap();

        let mut backend = FirecrackerBackend {
            firecracker_path: PathBuf::from("/usr/bin/firecracker"),
            jailer_path: None,
            kernel_path: PathBuf::from("/var/lib/agnos/firecracker/vmlinux"),
            rootfs_path: PathBuf::from("/var/lib/agnos/firecracker/rootfs.ext4"),
            work_dir: tmpdir.clone(),
            active: HashMap::new(),
        };

        let vm_dir = backend.prepare_vm("test-vm", "agent-1").unwrap();
        assert!(vm_dir.exists());
        assert_eq!(backend.active_count(), 1);

        backend.cleanup_vm("test-vm").unwrap();
        assert!(!vm_dir.exists());
        assert_eq!(backend.active_count(), 0);

        std::fs::remove_dir_all(&tmpdir).ok();
    }

    #[test]
    fn test_firecracker_cleanup_nonexistent_vm() {
        let tmpdir = std::env::temp_dir().join(format!("agnos-fc-test2-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmpdir).unwrap();

        let mut backend = FirecrackerBackend {
            firecracker_path: PathBuf::from("/usr/bin/firecracker"),
            jailer_path: None,
            kernel_path: PathBuf::from("/nonexistent"),
            rootfs_path: PathBuf::from("/nonexistent"),
            work_dir: tmpdir.clone(),
            active: HashMap::new(),
        };

        backend.cleanup_vm("nonexistent").unwrap();

        std::fs::remove_dir_all(&tmpdir).ok();
    }

    #[test]
    fn test_firecracker_is_available_false() {
        let backend = FirecrackerBackend {
            firecracker_path: PathBuf::from("/nonexistent/firecracker"),
            jailer_path: None,
            kernel_path: PathBuf::from("/nonexistent"),
            rootfs_path: PathBuf::from("/nonexistent"),
            work_dir: PathBuf::from("/tmp"),
            active: HashMap::new(),
        };
        assert!(!backend.is_available());
    }

    #[test]
    fn test_firecracker_default() {
        let backend = FirecrackerBackend::default();
        assert_eq!(backend.active_count(), 0);
    }

    #[test]
    fn test_firecracker_command_with_jailer() {
        let backend = FirecrackerBackend {
            firecracker_path: PathBuf::from("/usr/bin/firecracker"),
            jailer_path: Some(PathBuf::from("/usr/bin/jailer")),
            kernel_path: PathBuf::from("/var/lib/agnos/firecracker/vmlinux"),
            rootfs_path: PathBuf::from("/var/lib/agnos/firecracker/rootfs.ext4"),
            work_dir: PathBuf::from("/tmp"),
            active: HashMap::new(),
        };
        let cmd = backend.firecracker_command("test-vm");
        assert!(cmd.iter().any(|s| s.contains("jailer")));
        assert!(cmd.contains(&"--id".to_string()));
        assert!(cmd.contains(&"test-vm".to_string()));
        assert!(cmd.contains(&"--uid".to_string()));
    }

    #[test]
    fn test_firecracker_vm_config_local_ports() {
        let backend = FirecrackerBackend::new();
        let config = BackendConfig {
            network: NetworkMode::LocalPorts(vec![8080, 8090]),
            ..Default::default()
        };
        let vm_config = backend.generate_vm_config("test-vm", &config);
        let interfaces = vm_config["network-interfaces"].as_array().unwrap();
        assert_eq!(interfaces.len(), 1);
        assert_eq!(interfaces[0]["iface_id"], "eth0");
    }

    #[test]
    fn test_firecracker_vm_config_socket_path() {
        let backend = FirecrackerBackend::new();
        let config = BackendConfig::default();
        let vm_config = backend.generate_vm_config("my-vm", &config);
        let socket = vm_config["socket_path"].as_str().unwrap();
        assert!(socket.contains("my-vm.sock"));
    }

    #[test]
    fn test_firecracker_vm_config_drive_readonly() {
        let backend = FirecrackerBackend::new();
        let config = BackendConfig::default();
        let vm_config = backend.generate_vm_config("vm", &config);
        let drives = vm_config["drives"].as_array().unwrap();
        assert_eq!(drives[0]["is_read_only"], true);
        assert_eq!(drives[0]["is_root_device"], true);
    }

    // --- BackendConfig tests ---

    #[test]
    fn test_default_config() {
        let config = BackendConfig::default();
        assert_eq!(config.max_memory_mb, 512);
        assert_eq!(config.cpu_quota_pct, 50);
        assert_eq!(config.timeout_secs, 300);
        assert!(matches!(config.network, NetworkMode::None));
    }

    #[test]
    fn test_backend_config_serialization() {
        let config = BackendConfig {
            max_memory_mb: 1024,
            cpu_quota_pct: 75,
            timeout_secs: 600,
            readonly_mounts: vec!["/opt".to_string()],
            writable_mounts: vec!["/workspace".to_string()],
            network: NetworkMode::Host,
            env: HashMap::from([("KEY".to_string(), "VAL".to_string())]),
        };
        let json = serde_json::to_string(&config).unwrap();
        let roundtrip: BackendConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.max_memory_mb, 1024);
        assert_eq!(roundtrip.cpu_quota_pct, 75);
    }

    #[test]
    fn test_backend_result_serialization() {
        let result = BackendResult {
            success: true,
            stdout: "hello".to_string(),
            stderr: String::new(),
            exit_code: 0,
            duration_ms: 42,
            resources: ResourceUsage::default(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"success\":true"));
    }
}
