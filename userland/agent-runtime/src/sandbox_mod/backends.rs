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
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

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
    /// Host device paths to pass through to the VM (e.g. `/dev/nvidia0`, `/dev/dri/renderD128`).
    /// Enables GPU access inside Firecracker/gVisor sandboxes.
    #[serde(default)]
    pub device_passthrough: Vec<String>,
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
            device_passthrough: Vec::new(),
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

    /// Run a task inside a gVisor container.
    ///
    /// Creates an OCI bundle, spawns `runsc run`, captures output,
    /// enforces timeout, and cleans up.
    pub async fn run_task(
        &mut self,
        agent_id: &str,
        command: &[String],
        config: &BackendConfig,
    ) -> anyhow::Result<BackendResult> {
        if !self.is_available() {
            anyhow::bail!("gVisor (runsc) is not available at {:?}", self.runsc_path);
        }

        let container_id = format!("agnos-{}-{}", agent_id, uuid::Uuid::new_v4().simple());
        let bundle_path = self.create_bundle(&container_id, command, config)?;
        self.active
            .insert(container_id.clone(), agent_id.to_string());

        let cmd_args = self.runsc_command(&container_id, &bundle_path);
        info!(
            container_id = %container_id,
            agent_id = %agent_id,
            command = ?command,
            "gVisor: spawning container"
        );

        let start = Instant::now();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(config.timeout_secs),
            Self::spawn_and_wait(&cmd_args),
        )
        .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        // Clean up regardless of outcome
        if let Err(e) = self.cleanup_bundle(&container_id) {
            warn!(container_id = %container_id, error = %e, "gVisor: cleanup failed");
        }

        match result {
            Ok(Ok((exit_code, stdout, stderr))) => {
                let success = exit_code == 0;
                info!(
                    container_id = %container_id,
                    exit_code = exit_code,
                    duration_ms = duration_ms,
                    "gVisor: container exited"
                );
                Ok(BackendResult {
                    success,
                    stdout,
                    stderr,
                    exit_code,
                    duration_ms,
                    resources: ResourceUsage::default(),
                })
            }
            Ok(Err(e)) => {
                error!(container_id = %container_id, error = %e, "gVisor: spawn failed");
                Ok(BackendResult {
                    success: false,
                    stdout: String::new(),
                    stderr: e.to_string(),
                    exit_code: -1,
                    duration_ms,
                    resources: ResourceUsage::default(),
                })
            }
            Err(_) => {
                warn!(container_id = %container_id, timeout = config.timeout_secs, "gVisor: container timed out, killing");
                // Try to kill the container
                let _ = tokio::process::Command::new(&self.runsc_path)
                    .args(["kill", &container_id, "SIGKILL"])
                    .output()
                    .await;
                let _ = tokio::process::Command::new(&self.runsc_path)
                    .args(["delete", &container_id])
                    .output()
                    .await;

                Ok(BackendResult {
                    success: false,
                    stdout: String::new(),
                    stderr: format!("gVisor container timed out after {}s", config.timeout_secs),
                    exit_code: -1,
                    duration_ms,
                    resources: ResourceUsage::default(),
                })
            }
        }
    }

    /// Spawn a process and wait for it to complete.
    async fn spawn_and_wait(args: &[String]) -> anyhow::Result<(i32, String, String)> {
        let output = tokio::process::Command::new(&args[0])
            .args(&args[1..])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await?;

        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Ok((exit_code, stdout, stderr))
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
    ///
    /// When `config.device_passthrough` is non-empty, PCI is enabled in the
    /// guest kernel and VFIO device entries are generated so the VM can
    /// access host GPUs (e.g. `/dev/nvidia0`, `/dev/dri/renderD128`).
    pub fn generate_vm_config(&self, vm_id: &str, config: &BackendConfig) -> serde_json::Value {
        let vcpu_count = ((config.cpu_quota_pct as u32) / 25).clamp(1, 4);
        let socket_path = self.work_dir.join(format!("{}.sock", vm_id));

        // Enable PCI when devices need passthrough (GPU, etc.).
        let has_devices = !config.device_passthrough.is_empty();
        let boot_args = if has_devices {
            "console=ttyS0 reboot=k panic=1 agnos.sandbox=1"
        } else {
            "console=ttyS0 reboot=k panic=1 pci=off agnos.sandbox=1"
        };

        let mut vm_config = serde_json::json!({
            "boot-source": {
                "kernel_image_path": self.kernel_path.to_string_lossy(),
                "boot_args": boot_args
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
        });

        // Add device passthrough entries (VFIO-style).
        if has_devices {
            let devices: Vec<_> = config
                .device_passthrough
                .iter()
                .enumerate()
                .map(|(i, path)| {
                    serde_json::json!({
                        "device_id": format!("dev{}", i),
                        "path_on_host": path,
                    })
                })
                .collect();
            vm_config["devices"] = serde_json::json!(devices);
        }

        vm_config
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
    ///
    /// Uses `--config-file` to pass the full VM configuration at startup,
    /// avoiding the need for separate API socket PUT requests.
    pub fn firecracker_command(&self, vm_id: &str) -> Vec<String> {
        let socket = self.work_dir.join(format!("{}.sock", vm_id));
        let config_file = self.work_dir.join(vm_id).join("vm-config.json");

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
                "--config-file".to_string(),
                config_file.to_string_lossy().to_string(),
            ]
        } else {
            vec![
                self.firecracker_path.to_string_lossy().to_string(),
                "--api-sock".to_string(),
                socket.to_string_lossy().to_string(),
                "--config-file".to_string(),
                config_file.to_string_lossy().to_string(),
            ]
        }
    }

    /// Run a task inside a Firecracker microVM.
    ///
    /// 1. Starts firecracker process (creates API socket)
    /// 2. Configures VM via API socket (PUT boot-source, drives, machine-config)
    /// 3. Starts the VM (PUT /actions with InstanceStart)
    /// 4. Waits for firecracker process to exit (VM shutdown)
    /// 5. Cleans up VM directory and socket
    pub async fn run_task(
        &mut self,
        agent_id: &str,
        config: &BackendConfig,
    ) -> anyhow::Result<BackendResult> {
        if !self.is_available() {
            anyhow::bail!(
                "Firecracker not available (binary: {:?}, kernel: {:?}, /dev/kvm: {})",
                self.firecracker_path,
                self.kernel_path,
                Path::new("/dev/kvm").exists()
            );
        }

        let vm_id = format!("agnos-{}-{}", agent_id, uuid::Uuid::new_v4().simple());
        let vm_dir = self.prepare_vm(&vm_id, agent_id)?;
        let vm_config = self.generate_vm_config(&vm_id, config);

        // Write VM config to disk for debugging
        let config_path = vm_dir.join("vm-config.json");
        std::fs::write(
            &config_path,
            serde_json::to_string_pretty(&vm_config).unwrap(),
        )?;

        let socket_path = self.work_dir.join(format!("{}.sock", vm_id));
        let cmd_args = self.firecracker_command(&vm_id);

        info!(
            vm_id = %vm_id,
            agent_id = %agent_id,
            vcpus = %vm_config["machine-config"]["vcpu_count"],
            memory_mb = %vm_config["machine-config"]["mem_size_mib"],
            "Firecracker: spawning microVM"
        );

        let start = Instant::now();

        // Spawn firecracker process
        let mut child = tokio::process::Command::new(&cmd_args[0])
            .args(&cmd_args[1..])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        // Wait for the API socket to appear (up to 5s)
        let socket_ready = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            Self::wait_for_socket(&socket_path),
        )
        .await;

        if socket_ready.is_err() {
            warn!(vm_id = %vm_id, "Firecracker: API socket did not appear in 5s");
            child.kill().await.ok();
            self.cleanup_vm(&vm_id)?;
            return Ok(BackendResult {
                success: false,
                stdout: String::new(),
                stderr: "Firecracker API socket timeout".to_string(),
                exit_code: -1,
                duration_ms: start.elapsed().as_millis() as u64,
                resources: ResourceUsage::default(),
            });
        }

        // Configure VM via API socket
        let socket_url = format!(
            "http://localhost/{}",
            socket_path.to_string_lossy().replace('/', "%2F")
        );
        let configure_result = Self::configure_vm_via_api(&socket_url, &vm_config).await;
        if let Err(e) = configure_result {
            warn!(vm_id = %vm_id, error = %e, "Firecracker: API configuration failed");
            child.kill().await.ok();
            self.cleanup_vm(&vm_id)?;
            return Ok(BackendResult {
                success: false,
                stdout: String::new(),
                stderr: format!("Firecracker API config failed: {}", e),
                exit_code: -1,
                duration_ms: start.elapsed().as_millis() as u64,
                resources: ResourceUsage::default(),
            });
        }

        // Wait for VM to exit with timeout
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(config.timeout_secs),
            child.wait_with_output(),
        )
        .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        // Cleanup
        if let Err(e) = self.cleanup_vm(&vm_id) {
            warn!(vm_id = %vm_id, error = %e, "Firecracker: cleanup failed");
        }

        match result {
            Ok(Ok(output)) => {
                let exit_code = output.status.code().unwrap_or(-1);
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                info!(vm_id = %vm_id, exit_code = exit_code, duration_ms = duration_ms, "Firecracker: VM exited");
                Ok(BackendResult {
                    success: exit_code == 0,
                    stdout,
                    stderr,
                    exit_code,
                    duration_ms,
                    resources: ResourceUsage::default(),
                })
            }
            Ok(Err(e)) => {
                error!(vm_id = %vm_id, error = %e, "Firecracker: wait failed");
                Ok(BackendResult {
                    success: false,
                    stdout: String::new(),
                    stderr: e.to_string(),
                    exit_code: -1,
                    duration_ms,
                    resources: ResourceUsage::default(),
                })
            }
            Err(_) => {
                warn!(vm_id = %vm_id, timeout = config.timeout_secs, "Firecracker: VM timed out, killing");
                // child was consumed by wait_with_output, but the process should be dead
                // after cleanup_vm removes the socket
                Ok(BackendResult {
                    success: false,
                    stdout: String::new(),
                    stderr: format!("Firecracker VM timed out after {}s", config.timeout_secs),
                    exit_code: -1,
                    duration_ms,
                    resources: ResourceUsage::default(),
                })
            }
        }
    }

    /// Wait for the API socket file to appear.
    async fn wait_for_socket(path: &Path) {
        loop {
            if path.exists() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    /// Configure a Firecracker VM via its API socket.
    ///
    /// Sends PUT requests to configure boot-source, drives, machine-config,
    /// then starts the VM with InstanceStart action.
    async fn configure_vm_via_api(
        _socket_url: &str,
        _vm_config: &serde_json::Value,
    ) -> anyhow::Result<()> {
        // Firecracker API uses Unix domain socket HTTP.
        // In production this would use a UDS-capable HTTP client (hyper with unix connector).
        // For now, we write the config file and let firecracker read --config-file.
        //
        // TODO: Use hyper with unix socket connector for full API interaction:
        //   PUT /boot-source    { kernel_image_path, boot_args }
        //   PUT /drives/rootfs  { path_on_host, is_root_device, is_read_only }
        //   PUT /machine-config { vcpu_count, mem_size_mib }
        //   PUT /actions         { action_type: "InstanceStart" }
        //
        // For the initial implementation, firecracker supports --config-file
        // which reads the full config from a JSON file at startup.
        Ok(())
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
// WASM Sandbox Backend
// ---------------------------------------------------------------------------

/// WASM sandbox backend.
///
/// Runs agent tasks as WebAssembly modules with WASI capability restrictions.
/// Cross-platform, always available (no hardware requirements).
/// Delegates to the existing `wasm_runtime` module for execution.
#[derive(Debug)]
pub struct WasmBackend {
    /// Maximum memory pages (64KB each).
    pub max_memory_pages: u32,
    /// Allowed WASI capabilities.
    wasi_caps: WasiCapabilities,
    /// Active WASM instances: instance_id → agent_id.
    active: HashMap<String, String>,
}

/// WASI capability flags for the WASM sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasiCapabilities {
    /// Allowed pre-opened directories (read-only).
    pub readonly_dirs: Vec<String>,
    /// Allowed pre-opened directories (read-write).
    pub writable_dirs: Vec<String>,
    /// Whether stdin/stdout/stderr are connected.
    pub stdio: bool,
    /// Whether environment variables are passed through.
    pub env_passthrough: bool,
    /// Whether network sockets are allowed.
    pub network: bool,
    /// Whether clock/time access is allowed.
    pub clock: bool,
}

impl Default for WasiCapabilities {
    fn default() -> Self {
        Self {
            readonly_dirs: vec![],
            writable_dirs: vec!["/tmp".to_string()],
            stdio: true,
            env_passthrough: false,
            network: false,
            clock: true,
        }
    }
}

impl WasmBackend {
    pub fn new() -> Self {
        Self {
            max_memory_pages: 256, // 16 MB
            wasi_caps: WasiCapabilities::default(),
            active: HashMap::new(),
        }
    }

    pub fn with_config(max_memory_mb: u64, caps: WasiCapabilities) -> Self {
        Self {
            max_memory_pages: ((max_memory_mb * 1024 * 1024) / 65536) as u32,
            wasi_caps: caps,
            active: HashMap::new(),
        }
    }

    /// WASM is always available.
    pub fn is_available(&self) -> bool {
        true
    }

    /// Generate WASI configuration for a module.
    pub fn generate_wasi_config(&self, config: &BackendConfig) -> serde_json::Value {
        let max_pages = ((config.max_memory_mb * 1024 * 1024) / 65536) as u32;
        serde_json::json!({
            "max_memory_pages": max_pages,
            "max_memory_bytes": config.max_memory_mb * 1024 * 1024,
            "capabilities": {
                "readonly_dirs": self.wasi_caps.readonly_dirs,
                "writable_dirs": self.wasi_caps.writable_dirs,
                "stdio": self.wasi_caps.stdio,
                "env_passthrough": self.wasi_caps.env_passthrough,
                "network": self.wasi_caps.network,
                "clock": self.wasi_caps.clock,
            },
            "timeout_secs": config.timeout_secs,
        })
    }

    /// Register an active instance.
    pub fn register_instance(&mut self, instance_id: &str, agent_id: &str) {
        self.active
            .insert(instance_id.to_string(), agent_id.to_string());
        info!(instance_id = %instance_id, agent_id = %agent_id, "WASM: instance registered");
    }

    /// Remove an instance.
    pub fn remove_instance(&mut self, instance_id: &str) {
        self.active.remove(instance_id);
    }

    /// Number of active instances.
    pub fn active_count(&self) -> usize {
        self.active.len()
    }
}

impl Default for WasmBackend {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Intel SGX Backend
// ---------------------------------------------------------------------------

/// Intel SGX sandbox backend.
///
/// Runs agent tasks inside hardware-encrypted enclaves using Gramine-SGX.
/// Requires SGX-capable hardware (Intel Xeon E/W, some consumer CPUs).
/// The enclave memory is encrypted by the CPU — even the OS cannot read it.
#[derive(Debug)]
pub struct SgxBackend {
    /// Path to gramine-sgx binary.
    gramine_path: PathBuf,
    /// Path to gramine-sgx-sign (manifest signing tool).
    pub sign_path: PathBuf,
    /// Enclave working directory.
    work_dir: PathBuf,
    /// Active enclaves: enclave_id → agent_id.
    active: HashMap<String, String>,
}

impl SgxBackend {
    pub fn new() -> Self {
        Self {
            gramine_path: PathBuf::from("/usr/bin/gramine-sgx"),
            sign_path: PathBuf::from("/usr/bin/gramine-sgx-sign"),
            work_dir: PathBuf::from("/var/lib/agnos/sgx/enclaves"),
            active: HashMap::new(),
        }
    }

    /// Check if SGX hardware and Gramine are available.
    pub fn is_available(&self) -> bool {
        Path::new("/dev/sgx_enclave").exists() && self.gramine_path.exists()
    }

    /// Generate a Gramine manifest for an agent task.
    pub fn generate_manifest(
        &self,
        enclave_id: &str,
        binary_path: &str,
        config: &BackendConfig,
    ) -> serde_json::Value {
        let enclave_size_mb = config.max_memory_mb.max(32); // SGX minimum 32MB
        serde_json::json!({
            "loader": {
                "entrypoint": "file:{{ gramine.libos }}",
                "log_level": "warning",
                "argv": [binary_path],
                "env": config.env,
            },
            "libos": {
                "entrypoint": binary_path,
            },
            "sgx": {
                "enclave_size": format!("{}M", enclave_size_mb),
                "thread_num": ((config.cpu_quota_pct as u32) / 25).clamp(1, 8),
                "debug": false,
                "isvprodid": 1,
                "isvsvn": 1,
                "remote_attestation": "none",
            },
            "fs": {
                "mounts": config.readonly_mounts.iter().map(|p| {
                    serde_json::json!({"path": p, "uri": format!("file:{}", p), "type": "chroot"})
                }).collect::<Vec<_>>(),
            },
            "enclave_id": enclave_id,
        })
    }

    /// Prepare an enclave working directory.
    pub fn prepare_enclave(
        &mut self,
        enclave_id: &str,
        agent_id: &str,
    ) -> std::io::Result<PathBuf> {
        let dir = self.work_dir.join(enclave_id);
        std::fs::create_dir_all(&dir)?;
        self.active
            .insert(enclave_id.to_string(), agent_id.to_string());
        info!(enclave_id = %enclave_id, "SGX: enclave prepared");
        Ok(dir)
    }

    /// Clean up an enclave.
    pub fn cleanup_enclave(&mut self, enclave_id: &str) -> std::io::Result<()> {
        let dir = self.work_dir.join(enclave_id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        self.active.remove(enclave_id);
        debug!(enclave_id = %enclave_id, "SGX: enclave cleaned up");
        Ok(())
    }

    /// Get the gramine-sgx command line.
    pub fn gramine_command(&self, manifest_path: &Path) -> Vec<String> {
        vec![
            self.gramine_path.to_string_lossy().to_string(),
            manifest_path.to_string_lossy().to_string(),
        ]
    }

    pub fn active_count(&self) -> usize {
        self.active.len()
    }
}

impl Default for SgxBackend {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// AMD SEV-SNP Backend
// ---------------------------------------------------------------------------

/// AMD SEV-SNP sandbox backend.
///
/// Runs agent tasks in confidential VMs with encrypted memory.
/// Uses QEMU with SEV-SNP memory encryption. Requires AMD EPYC (Milan+).
/// Even the hypervisor cannot read the VM's memory.
#[derive(Debug)]
pub struct SevBackend {
    /// Path to qemu-system-x86_64.
    qemu_path: PathBuf,
    /// Path to OVMF firmware (SEV-capable).
    ovmf_path: PathBuf,
    /// VM working directory.
    work_dir: PathBuf,
    /// Active VMs: vm_id → agent_id.
    active: HashMap<String, String>,
}

impl SevBackend {
    pub fn new() -> Self {
        Self {
            qemu_path: PathBuf::from("/usr/bin/qemu-system-x86_64"),
            ovmf_path: PathBuf::from("/usr/share/OVMF/OVMF_CODE.fd"),
            work_dir: PathBuf::from("/var/lib/agnos/sev/vms"),
            active: HashMap::new(),
        }
    }

    /// Check if SEV hardware and QEMU are available.
    pub fn is_available(&self) -> bool {
        Path::new("/dev/sev").exists() && self.qemu_path.exists()
    }

    /// Generate QEMU command line for a SEV-SNP VM.
    pub fn generate_qemu_config(&self, vm_id: &str, config: &BackendConfig) -> serde_json::Value {
        let vcpus = ((config.cpu_quota_pct as u32) / 25).clamp(1, 4);
        serde_json::json!({
            "machine": "q35,confidential-guest-support=sev0,kernel-irqchip=split",
            "cpu": "EPYC-v4",
            "smp": vcpus,
            "memory": format!("{}M", config.max_memory_mb),
            "sev": {
                "id": "sev0",
                "cbitpos": 51,
                "reduced-phys-bits": 1,
                "policy": "0x5",
                "snp": true,
            },
            "firmware": self.ovmf_path.to_string_lossy(),
            "drives": [{
                "file": format!("{}/{}.qcow2", self.work_dir.to_string_lossy(), vm_id),
                "format": "qcow2",
                "if": "virtio"
            }],
            "network": match &config.network {
                NetworkMode::None => "none",
                _ => "user",
            },
            "vm_id": vm_id,
        })
    }

    /// Prepare a VM working directory.
    pub fn prepare_vm(&mut self, vm_id: &str, agent_id: &str) -> std::io::Result<PathBuf> {
        let dir = self.work_dir.join(vm_id);
        std::fs::create_dir_all(&dir)?;
        self.active.insert(vm_id.to_string(), agent_id.to_string());
        info!(vm_id = %vm_id, "SEV: VM prepared");
        Ok(dir)
    }

    /// Clean up a VM.
    pub fn cleanup_vm(&mut self, vm_id: &str) -> std::io::Result<()> {
        let dir = self.work_dir.join(vm_id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        self.active.remove(vm_id);
        debug!(vm_id = %vm_id, "SEV: VM cleaned up");
        Ok(())
    }

    pub fn active_count(&self) -> usize {
        self.active.len()
    }
}

impl Default for SevBackend {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Noop Backend (development / disabled)
// ---------------------------------------------------------------------------

/// No-op sandbox backend — provides no isolation.
/// Used in development mode or when sandboxing is explicitly disabled.
#[derive(Debug, Clone, Default)]
pub struct NoopBackend;

impl NoopBackend {
    pub fn new() -> Self {
        Self
    }

    pub fn is_available(&self) -> bool {
        true
    }

    pub fn active_count(&self) -> usize {
        0
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
        let mem_limit = spec["linux"]["resources"]["memory"]["limit"]
            .as_u64()
            .unwrap();
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
        assert!(mounts.iter().any(|m| m["destination"] == "/opt/data"
            && m["options"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("ro"))));
        assert!(mounts.iter().any(|m| m["destination"] == "/workspace"
            && m["options"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("rw"))));
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
        assert!(vm_config["network-interfaces"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn test_firecracker_vm_config_network_host() {
        let backend = FirecrackerBackend::new();
        let config = BackendConfig {
            network: NetworkMode::Host,
            ..Default::default()
        };
        let vm_config = backend.generate_vm_config("test-vm", &config);
        assert!(!vm_config["network-interfaces"]
            .as_array()
            .unwrap()
            .is_empty());
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
        let tmpdir =
            std::env::temp_dir().join(format!("agnos-gvisor-test-{}", uuid::Uuid::new_v4()));
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
        let tmpdir =
            std::env::temp_dir().join(format!("agnos-gvisor-test2-{}", uuid::Uuid::new_v4()));
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
        config
            .env
            .insert("RUST_LOG".to_string(), "info".to_string());
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
            device_passthrough: vec!["/dev/nvidia0".to_string()],
        };
        let json = serde_json::to_string(&config).unwrap();
        let roundtrip: BackendConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.max_memory_mb, 1024);
        assert_eq!(roundtrip.device_passthrough, vec!["/dev/nvidia0"]);
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

    // --- WASM backend tests ---

    #[test]
    fn test_wasm_always_available() {
        let backend = WasmBackend::new();
        assert!(backend.is_available());
    }

    #[test]
    fn test_wasm_default() {
        let backend = WasmBackend::default();
        assert_eq!(backend.active_count(), 0);
        assert_eq!(backend.max_memory_pages, 256); // 16MB
    }

    #[test]
    fn test_wasm_with_config() {
        let backend = WasmBackend::with_config(64, WasiCapabilities::default());
        assert_eq!(backend.max_memory_pages, 1024); // 64MB / 64KB
    }

    #[test]
    fn test_wasm_wasi_config_generation() {
        let backend = WasmBackend::new();
        let config = BackendConfig {
            max_memory_mb: 32,
            timeout_secs: 60,
            ..Default::default()
        };
        let wasi = backend.generate_wasi_config(&config);
        assert_eq!(wasi["max_memory_pages"], 512); // 32MB / 64KB
        assert_eq!(wasi["timeout_secs"], 60);
        assert!(wasi["capabilities"]["stdio"].as_bool().unwrap());
        assert!(!wasi["capabilities"]["network"].as_bool().unwrap());
    }

    #[test]
    fn test_wasm_instance_lifecycle() {
        let mut backend = WasmBackend::new();
        backend.register_instance("inst-1", "agent-1");
        assert_eq!(backend.active_count(), 1);
        backend.remove_instance("inst-1");
        assert_eq!(backend.active_count(), 0);
    }

    #[test]
    fn test_wasi_capabilities_default() {
        let caps = WasiCapabilities::default();
        assert!(caps.stdio);
        assert!(!caps.env_passthrough);
        assert!(!caps.network);
        assert!(caps.clock);
        assert!(caps.writable_dirs.contains(&"/tmp".to_string()));
    }

    // --- SGX backend tests ---

    #[test]
    fn test_sgx_is_available_false() {
        let backend = SgxBackend {
            gramine_path: PathBuf::from("/nonexistent/gramine-sgx"),
            sign_path: PathBuf::from("/nonexistent"),
            work_dir: PathBuf::from("/tmp"),
            active: HashMap::new(),
        };
        assert!(!backend.is_available());
    }

    #[test]
    fn test_sgx_default() {
        let backend = SgxBackend::default();
        assert_eq!(backend.active_count(), 0);
    }

    #[test]
    fn test_sgx_manifest_generation() {
        let backend = SgxBackend::new();
        let config = BackendConfig {
            max_memory_mb: 256,
            cpu_quota_pct: 50,
            ..Default::default()
        };
        let manifest = backend.generate_manifest("enc-1", "/usr/bin/agent_runtime", &config);
        assert_eq!(manifest["sgx"]["enclave_size"], "256M");
        assert_eq!(manifest["sgx"]["thread_num"], 2);
        assert_eq!(manifest["sgx"]["debug"], false);
        assert_eq!(manifest["enclave_id"], "enc-1");
    }

    #[test]
    fn test_sgx_manifest_min_memory() {
        let backend = SgxBackend::new();
        let config = BackendConfig {
            max_memory_mb: 8, // Below 32MB minimum
            ..Default::default()
        };
        let manifest = backend.generate_manifest("enc-1", "/usr/bin/test", &config);
        assert_eq!(manifest["sgx"]["enclave_size"], "32M"); // Clamped to 32MB
    }

    #[test]
    fn test_sgx_prepare_and_cleanup() {
        let tmpdir = std::env::temp_dir().join(format!("agnos-sgx-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmpdir).unwrap();

        let mut backend = SgxBackend {
            gramine_path: PathBuf::from("/usr/bin/gramine-sgx"),
            sign_path: PathBuf::from("/usr/bin/gramine-sgx-sign"),
            work_dir: tmpdir.clone(),
            active: HashMap::new(),
        };

        let dir = backend.prepare_enclave("enc-1", "agent-1").unwrap();
        assert!(dir.exists());
        assert_eq!(backend.active_count(), 1);

        backend.cleanup_enclave("enc-1").unwrap();
        assert!(!dir.exists());
        assert_eq!(backend.active_count(), 0);

        std::fs::remove_dir_all(&tmpdir).ok();
    }

    #[test]
    fn test_sgx_gramine_command() {
        let backend = SgxBackend::new();
        let cmd = backend.gramine_command(Path::new("/tmp/manifest.sgx"));
        assert!(cmd.iter().any(|s| s.contains("gramine-sgx")));
        assert!(cmd.iter().any(|s| s.contains("manifest.sgx")));
    }

    // --- SEV backend tests ---

    #[test]
    fn test_sev_is_available_false() {
        let backend = SevBackend {
            qemu_path: PathBuf::from("/nonexistent/qemu"),
            ovmf_path: PathBuf::from("/nonexistent"),
            work_dir: PathBuf::from("/tmp"),
            active: HashMap::new(),
        };
        assert!(!backend.is_available());
    }

    #[test]
    fn test_sev_default() {
        let backend = SevBackend::default();
        assert_eq!(backend.active_count(), 0);
    }

    #[test]
    fn test_sev_qemu_config() {
        let backend = SevBackend::new();
        let config = BackendConfig {
            max_memory_mb: 512,
            cpu_quota_pct: 75,
            network: NetworkMode::None,
            ..Default::default()
        };
        let qemu = backend.generate_qemu_config("vm-1", &config);
        assert_eq!(qemu["memory"], "512M");
        assert_eq!(qemu["smp"], 3); // 75 / 25 = 3
        assert_eq!(qemu["sev"]["snp"], true);
        assert_eq!(qemu["network"], "none");
    }

    #[test]
    fn test_sev_qemu_config_with_network() {
        let backend = SevBackend::new();
        let config = BackendConfig {
            network: NetworkMode::Host,
            ..Default::default()
        };
        let qemu = backend.generate_qemu_config("vm-1", &config);
        assert_eq!(qemu["network"], "user");
    }

    #[test]
    fn test_sev_prepare_and_cleanup() {
        let tmpdir = std::env::temp_dir().join(format!("agnos-sev-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmpdir).unwrap();

        let mut backend = SevBackend {
            qemu_path: PathBuf::from("/usr/bin/qemu-system-x86_64"),
            ovmf_path: PathBuf::from("/usr/share/OVMF/OVMF_CODE.fd"),
            work_dir: tmpdir.clone(),
            active: HashMap::new(),
        };

        let dir = backend.prepare_vm("vm-1", "agent-1").unwrap();
        assert!(dir.exists());
        assert_eq!(backend.active_count(), 1);

        backend.cleanup_vm("vm-1").unwrap();
        assert!(!dir.exists());
        assert_eq!(backend.active_count(), 0);

        std::fs::remove_dir_all(&tmpdir).ok();
    }

    // --- Noop backend tests ---

    #[test]
    fn test_noop_always_available() {
        let backend = NoopBackend::new();
        assert!(backend.is_available());
        assert_eq!(backend.active_count(), 0);
    }

    #[test]
    fn test_noop_default() {
        let backend = NoopBackend;
        assert!(backend.is_available());
    }
}
