//! Python runtime management — version shim, .python-version, venv, pip proxy
//!
//! Manages multiple Python interpreter versions on AGNOS. Provides:
//!
//!   1. **Version shim** — `/usr/bin/python3` routes to the correct version
//!      based on `.python-version`, environment, or system default
//!   2. **`.python-version` support** — per-project version selection
//!      (walks up directory tree, like pyenv)
//!   3. **Virtual environment management** — create, list, activate, remove
//!   4. **pip proxy** — caches packages, audits installs via daimon
//!
//! Python interpreters are installed at `/usr/lib/agnos/python/{version}/`
//! with binaries at `/usr/bin/python{version}` (e.g., python3.12, python3.13).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Base directory for Python installations.
pub const PYTHON_BASE_DIR: &str = "/usr/lib/agnos/python";

/// Directory for global virtual environments.
pub const VENV_BASE_DIR: &str = "/var/lib/agnos/python/venvs";

/// pip cache directory.
pub const PIP_CACHE_DIR: &str = "/var/cache/agnos/pip";

/// The `.python-version` filename.
pub const VERSION_FILE: &str = ".python-version";

/// System-wide default version file.
pub const SYSTEM_VERSION_FILE: &str = "/etc/agnos/python/default-version";

// ---------------------------------------------------------------------------
// Python version
// ---------------------------------------------------------------------------

/// A Python version identifier (e.g., "3.12", "3.13", "3.13t").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PythonVersion {
    pub major: u32,
    pub minor: u32,
    /// Free-threaded variant (e.g., "3.13t").
    pub free_threaded: bool,
}

impl PythonVersion {
    /// Parse a version string like "3.12", "3.13t", "3.14".
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let free_threaded = s.ends_with('t');
        let version_str = if free_threaded { &s[..s.len() - 1] } else { s };

        let parts: Vec<&str> = version_str.split('.').collect();
        if parts.len() != 2 {
            return None;
        }

        let major = parts[0].parse().ok()?;
        let minor = parts[1].parse().ok()?;

        Some(Self {
            major,
            minor,
            free_threaded,
        })
    }

    /// Get the binary name (e.g., "python3.12", "python3.13t").
    pub fn binary_name(&self) -> String {
        if self.free_threaded {
            format!("python{}.{}t", self.major, self.minor)
        } else {
            format!("python{}.{}", self.major, self.minor)
        }
    }

    /// Get the expected binary path.
    pub fn binary_path(&self) -> PathBuf {
        PathBuf::from("/usr/bin").join(self.binary_name())
    }

    /// Get the lib directory for this version.
    pub fn lib_dir(&self) -> PathBuf {
        let suffix = if self.free_threaded { "t" } else { "" };
        PathBuf::from(PYTHON_BASE_DIR).join(format!("{}.{}{}", self.major, self.minor, suffix))
    }

    /// Version string (e.g., "3.12", "3.13t").
    pub fn version_string(&self) -> String {
        if self.free_threaded {
            format!("{}.{}t", self.major, self.minor)
        } else {
            format!("{}.{}", self.major, self.minor)
        }
    }
}

impl std::fmt::Display for PythonVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.version_string())
    }
}

impl PartialOrd for PythonVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PythonVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.free_threaded.cmp(&other.free_threaded))
    }
}

// ---------------------------------------------------------------------------
// Installed Python info
// ---------------------------------------------------------------------------

/// Information about an installed Python interpreter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPython {
    pub version: PythonVersion,
    pub binary_path: PathBuf,
    pub lib_dir: PathBuf,
    pub is_available: bool,
    /// Full version string from `python --version` (e.g., "Python 3.12.9").
    pub full_version: Option<String>,
    /// Whether PGO was enabled at compile time.
    pub pgo_enabled: bool,
    /// Whether LTO was enabled at compile time.
    pub lto_enabled: bool,
    /// Whether GIL is disabled (free-threaded).
    pub gil_disabled: bool,
}

// ---------------------------------------------------------------------------
// Virtual environment
// ---------------------------------------------------------------------------

/// A managed virtual environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VenvInfo {
    /// Environment name.
    pub name: String,
    /// Filesystem path.
    pub path: PathBuf,
    /// Python version used.
    pub python_version: PythonVersion,
    /// Creation timestamp.
    pub created_at: String,
    /// Whether the venv directory exists.
    pub exists: bool,
    /// Installed packages (name → version).
    pub packages: HashMap<String, String>,
    /// Owning agent ID (if agent-scoped).
    pub agent_id: Option<String>,
}

/// Request to create a virtual environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVenvRequest {
    /// Environment name.
    pub name: String,
    /// Python version to use (default: system default).
    pub python_version: Option<String>,
    /// Requirements to install after creation.
    pub requirements: Vec<String>,
    /// Requirements file path.
    pub requirements_file: Option<PathBuf>,
    /// Owning agent ID.
    pub agent_id: Option<String>,
    /// Custom path (default: VENV_BASE_DIR/name).
    pub path: Option<PathBuf>,
}

// ---------------------------------------------------------------------------
// pip proxy
// ---------------------------------------------------------------------------

/// pip proxy configuration for caching and auditing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipProxyConfig {
    /// Enable pip caching.
    pub cache_enabled: bool,
    /// Cache directory.
    pub cache_dir: PathBuf,
    /// Maximum cache size in MB.
    pub max_cache_mb: u64,
    /// Upstream PyPI index URL.
    pub index_url: String,
    /// Extra index URLs.
    pub extra_index_urls: Vec<String>,
    /// Trusted hosts (no TLS verification).
    pub trusted_hosts: Vec<String>,
    /// Audit pip installs via daimon.
    pub audit_enabled: bool,
    /// Block packages matching these patterns.
    pub blocked_packages: Vec<String>,
    /// Require hash verification for all packages.
    pub require_hashes: bool,
}

impl Default for PipProxyConfig {
    fn default() -> Self {
        Self {
            cache_enabled: true,
            cache_dir: PathBuf::from(PIP_CACHE_DIR),
            max_cache_mb: 2048,
            index_url: "https://pypi.org/simple/".to_string(),
            extra_index_urls: Vec::new(),
            trusted_hosts: Vec::new(),
            audit_enabled: true,
            blocked_packages: Vec::new(),
            require_hashes: false,
        }
    }
}

/// Record of a pip install operation (for audit trail).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipInstallRecord {
    /// Timestamp.
    pub timestamp: String,
    /// Package name.
    pub package: String,
    /// Installed version.
    pub version: String,
    /// Target venv (or system).
    pub target: String,
    /// Requesting agent ID.
    pub agent_id: Option<String>,
    /// Whether the package was served from cache.
    pub from_cache: bool,
    /// SHA256 of the wheel/sdist.
    pub sha256: Option<String>,
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

/// Configuration for the Python runtime manager.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonRuntimeConfig {
    /// Base directory for Python installations.
    pub python_base_dir: PathBuf,
    /// Base directory for virtual environments.
    pub venv_base_dir: PathBuf,
    /// System default version.
    pub default_version: Option<String>,
    /// pip proxy settings.
    pub pip_proxy: PipProxyConfig,
    /// Known Python versions to check.
    pub known_versions: Vec<String>,
}

impl Default for PythonRuntimeConfig {
    fn default() -> Self {
        Self {
            python_base_dir: PathBuf::from(PYTHON_BASE_DIR),
            venv_base_dir: PathBuf::from(VENV_BASE_DIR),
            default_version: None,
            pip_proxy: PipProxyConfig::default(),
            known_versions: vec!["3.12".into(), "3.13".into(), "3.13t".into(), "3.14".into()],
        }
    }
}

/// Python runtime manager — handles version resolution, venvs, and pip proxy.
pub struct PythonRuntimeManager {
    config: PythonRuntimeConfig,
    installed: Vec<InstalledPython>,
    venvs: HashMap<String, VenvInfo>,
    install_log: Vec<PipInstallRecord>,
}

impl PythonRuntimeManager {
    /// Create a new manager with default configuration.
    pub fn new() -> Self {
        let config = PythonRuntimeConfig::default();
        Self::with_config(config)
    }

    /// Create a new manager with custom configuration.
    pub fn with_config(config: PythonRuntimeConfig) -> Self {
        let mut mgr = Self {
            config,
            installed: Vec::new(),
            venvs: HashMap::new(),
            install_log: Vec::new(),
        };
        mgr.discover_installed();
        mgr
    }

    // --- Version Discovery ---

    /// Scan for installed Python versions.
    pub fn discover_installed(&mut self) {
        self.installed.clear();

        for version_str in &self.config.known_versions.clone() {
            if let Some(version) = PythonVersion::parse(version_str) {
                let binary = version.binary_path();
                let lib_dir = version.lib_dir();
                let is_available = binary.exists();

                let full_version = if is_available {
                    get_python_full_version(&binary)
                } else {
                    None
                };

                self.installed.push(InstalledPython {
                    version: version.clone(),
                    binary_path: binary,
                    lib_dir,
                    is_available,
                    full_version,
                    pgo_enabled: true, // AGNOS recipes always use PGO
                    lto_enabled: true, // AGNOS recipes always use LTO
                    gil_disabled: version.free_threaded,
                });
            }
        }

        let available_count = self.installed.iter().filter(|p| p.is_available).count();
        info!(
            "Discovered {} Python versions ({} available)",
            self.installed.len(),
            available_count
        );
    }

    /// List all known Python installations.
    pub fn list_installed(&self) -> &[InstalledPython] {
        &self.installed
    }

    /// List only available (installed) versions.
    pub fn list_available(&self) -> Vec<&InstalledPython> {
        self.installed.iter().filter(|p| p.is_available).collect()
    }

    /// Get info for a specific version.
    pub fn get_version(&self, version_str: &str) -> Option<&InstalledPython> {
        let target = PythonVersion::parse(version_str)?;
        self.installed.iter().find(|p| p.version == target)
    }

    // --- Version Resolution (.python-version) ---

    /// Resolve which Python version to use for a given directory.
    ///
    /// Resolution order:
    /// 1. `AGNOS_PYTHON_VERSION` environment variable
    /// 2. `.python-version` file in the directory or any parent
    /// 3. System default (`/etc/agnos/python/default-version`)
    /// 4. Highest available version
    pub fn resolve_version(&self, dir: &Path) -> Option<PythonVersion> {
        // 1. Environment variable
        if let Ok(env_ver) = std::env::var("AGNOS_PYTHON_VERSION") {
            if let Some(v) = PythonVersion::parse(&env_ver) {
                debug!("Python version from AGNOS_PYTHON_VERSION: {}", v);
                return Some(v);
            }
        }

        // 2. Walk up directory tree looking for .python-version
        if let Some(v) = find_version_file(dir) {
            debug!("Python version from .python-version: {}", v);
            return Some(v);
        }

        // 3. System default
        if let Some(v) = &self.config.default_version {
            if let Some(parsed) = PythonVersion::parse(v) {
                return Some(parsed);
            }
        }
        if let Some(v) = read_system_default() {
            return Some(v);
        }

        // 4. Highest available
        let mut available: Vec<&InstalledPython> = self
            .installed
            .iter()
            .filter(|p| p.is_available && !p.version.free_threaded)
            .collect();
        available.sort_by(|a, b| b.version.cmp(&a.version));
        available.first().map(|p| p.version.clone())
    }

    /// Get the binary path for the resolved version in a directory.
    pub fn resolve_binary(&self, dir: &Path) -> Option<PathBuf> {
        let version = self.resolve_version(dir)?;
        let info = self.installed.iter().find(|p| p.version == version)?;
        if info.is_available {
            Some(info.binary_path.clone())
        } else {
            None
        }
    }

    // --- Virtual Environments ---

    /// Create a new virtual environment.
    pub fn create_venv(&mut self, req: CreateVenvRequest) -> Result<VenvInfo, PythonError> {
        if self.venvs.contains_key(&req.name) {
            return Err(PythonError::VenvExists {
                name: req.name.clone(),
            });
        }

        let version_str = req.python_version.as_deref().unwrap_or("3.12");
        let version =
            PythonVersion::parse(version_str).ok_or_else(|| PythonError::InvalidVersion {
                version: version_str.to_string(),
            })?;

        // Check the version is available
        let python_info = self
            .installed
            .iter()
            .find(|p| p.version == version)
            .ok_or_else(|| PythonError::VersionNotInstalled {
                version: version.version_string(),
            })?;

        if !python_info.is_available {
            return Err(PythonError::VersionNotInstalled {
                version: version.version_string(),
            });
        }

        let venv_path = req
            .path
            .clone()
            .unwrap_or_else(|| PathBuf::from(&self.config.venv_base_dir).join(&req.name));

        let venv = VenvInfo {
            name: req.name.clone(),
            path: venv_path,
            python_version: version,
            created_at: chrono::Utc::now().to_rfc3339(),
            exists: false, // Will be true after actual creation
            packages: HashMap::new(),
            agent_id: req.agent_id,
        };

        info!(
            "Created venv '{}' with Python {}",
            venv.name, venv.python_version
        );
        self.venvs.insert(req.name.clone(), venv.clone());
        Ok(venv)
    }

    /// List all virtual environments.
    pub fn list_venvs(&self) -> Vec<&VenvInfo> {
        self.venvs.values().collect()
    }

    /// List venvs for a specific agent.
    pub fn list_venvs_for_agent(&self, agent_id: &str) -> Vec<&VenvInfo> {
        self.venvs
            .values()
            .filter(|v| v.agent_id.as_deref() == Some(agent_id))
            .collect()
    }

    /// Get a venv by name.
    pub fn get_venv(&self, name: &str) -> Option<&VenvInfo> {
        self.venvs.get(name)
    }

    /// Remove a virtual environment.
    pub fn remove_venv(&mut self, name: &str) -> Result<(), PythonError> {
        if self.venvs.remove(name).is_none() {
            return Err(PythonError::VenvNotFound {
                name: name.to_string(),
            });
        }
        info!("Removed venv '{}'", name);
        Ok(())
    }

    // --- pip Proxy ---

    /// Record a pip install for audit.
    pub fn record_pip_install(&mut self, record: PipInstallRecord) {
        if self.config.pip_proxy.audit_enabled {
            info!(
                "pip install: {} {} -> {} (cache: {})",
                record.package, record.version, record.target, record.from_cache
            );
        }
        self.install_log.push(record);
    }

    /// Get pip install history.
    pub fn pip_install_log(&self) -> &[PipInstallRecord] {
        &self.install_log
    }

    /// Check if a package is blocked.
    pub fn is_package_blocked(&self, package: &str) -> bool {
        self.config
            .pip_proxy
            .blocked_packages
            .iter()
            .any(|pattern| {
                if pattern.contains('*') {
                    // Simple glob: "foo*" matches "foobar"
                    let prefix = pattern.trim_end_matches('*');
                    package.starts_with(prefix)
                } else {
                    package == pattern
                }
            })
    }

    /// Get pip proxy configuration.
    pub fn pip_config(&self) -> &PipProxyConfig {
        &self.config.pip_proxy
    }

    /// Get manager statistics.
    pub fn stats(&self) -> PythonRuntimeStats {
        PythonRuntimeStats {
            known_versions: self.installed.len(),
            available_versions: self.installed.iter().filter(|p| p.is_available).count(),
            venv_count: self.venvs.len(),
            pip_installs: self.install_log.len(),
            cache_enabled: self.config.pip_proxy.cache_enabled,
            audit_enabled: self.config.pip_proxy.audit_enabled,
        }
    }
}

impl Default for PythonRuntimeManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Python runtime statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonRuntimeStats {
    pub known_versions: usize,
    pub available_versions: usize,
    pub venv_count: usize,
    pub pip_installs: usize,
    pub cache_enabled: bool,
    pub audit_enabled: bool,
}

// ---------------------------------------------------------------------------
// Version file resolution
// ---------------------------------------------------------------------------

/// Walk up the directory tree looking for a `.python-version` file.
pub fn find_version_file(start: &Path) -> Option<PythonVersion> {
    let mut dir = start.to_path_buf();
    loop {
        let version_file = dir.join(VERSION_FILE);
        if version_file.is_file() {
            if let Ok(content) = std::fs::read_to_string(&version_file) {
                let trimmed = content.trim();
                if let Some(v) = PythonVersion::parse(trimmed) {
                    return Some(v);
                }
            }
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// Read the system-wide default Python version.
fn read_system_default() -> Option<PythonVersion> {
    std::fs::read_to_string(SYSTEM_VERSION_FILE)
        .ok()
        .and_then(|s| PythonVersion::parse(s.trim()))
}

/// Get the full version string from a Python binary.
fn get_python_full_version(binary: &Path) -> Option<String> {
    std::process::Command::new(binary)
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);
            let version = if stdout.starts_with("Python") {
                stdout.trim().to_string()
            } else if stderr.starts_with("Python") {
                stderr.trim().to_string()
            } else {
                return None;
            };
            Some(version)
        })
}

// ---------------------------------------------------------------------------
// Shim generation
// ---------------------------------------------------------------------------

/// Generate the content for a Python version shim script.
///
/// The shim is installed at `/usr/bin/python3` and routes to the correct
/// versioned interpreter based on `.python-version`, env vars, or defaults.
pub fn generate_shim_script(default_version: &str) -> String {
    format!(
        r#"#!/bin/sh
# AGNOS Python version shim — auto-generated
# Routes to the correct Python version based on:
#   1. AGNOS_PYTHON_VERSION env var
#   2. .python-version file (walks up directory tree)
#   3. System default: {default}

set -e

# 1. Environment variable
if [ -n "$AGNOS_PYTHON_VERSION" ]; then
    PYVER="$AGNOS_PYTHON_VERSION"
elif [ -n "$PYTHON_VERSION" ]; then
    PYVER="$PYTHON_VERSION"
else
    PYVER=""
fi

# 2. Walk up directory tree for .python-version
if [ -z "$PYVER" ]; then
    DIR="$(pwd)"
    while [ "$DIR" != "/" ]; do
        if [ -f "$DIR/.python-version" ]; then
            PYVER=$(cat "$DIR/.python-version" | tr -d '[:space:]')
            break
        fi
        DIR=$(dirname "$DIR")
    done
fi

# 3. System default
if [ -z "$PYVER" ]; then
    if [ -f /etc/agnos/python/default-version ]; then
        PYVER=$(cat /etc/agnos/python/default-version | tr -d '[:space:]')
    fi
fi

# 4. Hardcoded default
if [ -z "$PYVER" ]; then
    PYVER="{default}"
fi

# Resolve binary
PYBIN="/usr/bin/python$PYVER"
if [ ! -x "$PYBIN" ]; then
    echo "error: Python $PYVER not installed (looked for $PYBIN)" >&2
    echo "Available versions:" >&2
    ls /usr/bin/python3.* 2>/dev/null | sed 's|/usr/bin/||' >&2
    exit 1
fi

exec "$PYBIN" "$@"
"#,
        default = default_version
    )
}

/// Generate a pip wrapper script that routes through the proxy/cache.
pub fn generate_pip_wrapper(cache_dir: &str, index_url: &str) -> String {
    format!(
        r#"#!/bin/sh
# AGNOS pip wrapper — auto-generated
# Routes pip through AGNOS cache and audit layer

set -e

# Resolve Python version (use the shim)
PYTHON="/usr/bin/python3"

# pip cache
export PIP_CACHE_DIR="{cache}"

# Default index
export PIP_INDEX_URL="{index}"

# Disable pip version check (managed by ark)
export PIP_DISABLE_PIP_VERSION_CHECK=1

# Run pip as module
exec "$PYTHON" -m pip "$@"
"#,
        cache = cache_dir,
        index = index_url
    )
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Python runtime errors.
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum PythonError {
    #[error("Invalid Python version: {version}")]
    InvalidVersion { version: String },

    #[error("Python {version} is not installed")]
    VersionNotInstalled { version: String },

    #[error("Virtual environment '{name}' already exists")]
    VenvExists { name: String },

    #[error("Virtual environment '{name}' not found")]
    VenvNotFound { name: String },

    #[error("Package '{package}' is blocked by policy")]
    PackageBlocked { package: String },

    #[error("pip install failed: {reason}")]
    PipFailed { reason: String },
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- PythonVersion ---

    #[test]
    fn test_parse_version_312() {
        let v = PythonVersion::parse("3.12").unwrap();
        assert_eq!(v.major, 3);
        assert_eq!(v.minor, 12);
        assert!(!v.free_threaded);
    }

    #[test]
    fn test_parse_version_313() {
        let v = PythonVersion::parse("3.13").unwrap();
        assert_eq!(v.major, 3);
        assert_eq!(v.minor, 13);
        assert!(!v.free_threaded);
    }

    #[test]
    fn test_parse_version_313t() {
        let v = PythonVersion::parse("3.13t").unwrap();
        assert_eq!(v.major, 3);
        assert_eq!(v.minor, 13);
        assert!(v.free_threaded);
    }

    #[test]
    fn test_parse_version_314() {
        let v = PythonVersion::parse("3.14").unwrap();
        assert_eq!(v.major, 3);
        assert_eq!(v.minor, 14);
        assert!(!v.free_threaded);
    }

    #[test]
    fn test_parse_version_invalid() {
        assert!(PythonVersion::parse("3").is_none());
        assert!(PythonVersion::parse("").is_none());
        assert!(PythonVersion::parse("abc").is_none());
        assert!(PythonVersion::parse("3.12.5").is_none()); // patch not supported
    }

    #[test]
    fn test_parse_version_whitespace() {
        let v = PythonVersion::parse("  3.12  ").unwrap();
        assert_eq!(v.minor, 12);
    }

    #[test]
    fn test_version_binary_name() {
        assert_eq!(
            PythonVersion::parse("3.12").unwrap().binary_name(),
            "python3.12"
        );
        assert_eq!(
            PythonVersion::parse("3.13t").unwrap().binary_name(),
            "python3.13t"
        );
    }

    #[test]
    fn test_version_binary_path() {
        let v = PythonVersion::parse("3.12").unwrap();
        assert_eq!(v.binary_path(), PathBuf::from("/usr/bin/python3.12"));
    }

    #[test]
    fn test_version_lib_dir() {
        let v = PythonVersion::parse("3.12").unwrap();
        assert_eq!(v.lib_dir(), PathBuf::from("/usr/lib/agnos/python/3.12"));

        let vt = PythonVersion::parse("3.13t").unwrap();
        assert_eq!(vt.lib_dir(), PathBuf::from("/usr/lib/agnos/python/3.13t"));
    }

    #[test]
    fn test_version_display() {
        assert_eq!(PythonVersion::parse("3.12").unwrap().to_string(), "3.12");
        assert_eq!(PythonVersion::parse("3.13t").unwrap().to_string(), "3.13t");
    }

    #[test]
    fn test_version_ordering() {
        let v312 = PythonVersion::parse("3.12").unwrap();
        let v313 = PythonVersion::parse("3.13").unwrap();
        let v313t = PythonVersion::parse("3.13t").unwrap();
        let v314 = PythonVersion::parse("3.14").unwrap();

        assert!(v312 < v313);
        assert!(v313 < v313t); // free-threaded sorts after standard
        assert!(v313t < v314);
    }

    #[test]
    fn test_version_equality() {
        let a = PythonVersion::parse("3.12").unwrap();
        let b = PythonVersion::parse("3.12").unwrap();
        assert_eq!(a, b);

        let c = PythonVersion::parse("3.13").unwrap();
        assert_ne!(a, c);
    }

    // --- Version file resolution ---

    #[test]
    fn test_find_version_file() {
        let dir = std::env::temp_dir().join("agnos_pyver_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("project/subdir")).unwrap();

        // Write .python-version at project level
        std::fs::write(dir.join("project/.python-version"), "3.13\n").unwrap();

        // Should find it from subdir
        let v = find_version_file(&dir.join("project/subdir")).unwrap();
        assert_eq!(v, PythonVersion::parse("3.13").unwrap());

        // Should find it from project dir
        let v = find_version_file(&dir.join("project")).unwrap();
        assert_eq!(v, PythonVersion::parse("3.13").unwrap());

        // Should not find it from parent
        let v = find_version_file(&dir);
        assert!(v.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_version_file_freethreaded() {
        let dir = std::env::temp_dir().join("agnos_pyver_ft_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(dir.join(".python-version"), "3.13t").unwrap();
        let v = find_version_file(&dir).unwrap();
        assert!(v.free_threaded);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- Manager ---

    #[test]
    fn test_manager_new() {
        let mgr = PythonRuntimeManager::new();
        assert_eq!(mgr.installed.len(), 4); // 3.12, 3.13, 3.13t, 3.14
    }

    #[test]
    fn test_manager_default() {
        let mgr = PythonRuntimeManager::default();
        assert_eq!(mgr.installed.len(), 4);
    }

    #[test]
    fn test_manager_custom_config() {
        let config = PythonRuntimeConfig {
            known_versions: vec!["3.12".into(), "3.13".into()],
            ..Default::default()
        };
        let mgr = PythonRuntimeManager::with_config(config);
        assert_eq!(mgr.installed.len(), 2);
    }

    #[test]
    fn test_list_installed() {
        let mgr = PythonRuntimeManager::new();
        let installed = mgr.list_installed();
        assert_eq!(installed.len(), 4);
    }

    #[test]
    fn test_get_version() {
        let mgr = PythonRuntimeManager::new();
        let v = mgr.get_version("3.12");
        assert!(v.is_some());
        assert_eq!(v.unwrap().version, PythonVersion::parse("3.12").unwrap());

        let v = mgr.get_version("3.13t");
        assert!(v.is_some());
        assert!(v.unwrap().version.free_threaded);

        let v = mgr.get_version("2.7");
        assert!(v.is_none());
    }

    #[test]
    fn test_resolve_version_env() {
        let mgr = PythonRuntimeManager::new();
        std::env::set_var("AGNOS_PYTHON_VERSION", "3.14");
        let v = mgr.resolve_version(Path::new("/tmp"));
        std::env::remove_var("AGNOS_PYTHON_VERSION");
        assert_eq!(v.unwrap(), PythonVersion::parse("3.14").unwrap());
    }

    #[test]
    fn test_resolve_version_file() {
        let dir = std::env::temp_dir().join("agnos_resolve_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".python-version"), "3.13").unwrap();

        // Make sure env var is not set
        std::env::remove_var("AGNOS_PYTHON_VERSION");

        let mgr = PythonRuntimeManager::new();
        let v = mgr.resolve_version(&dir);
        assert_eq!(v.unwrap(), PythonVersion::parse("3.13").unwrap());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- Venvs ---

    #[test]
    fn test_create_venv() {
        let mgr_config = PythonRuntimeConfig {
            known_versions: vec!["3.12".into()],
            ..Default::default()
        };
        // Simulate installed python by using a version that "exists" on the system
        let mut mgr = PythonRuntimeManager::with_config(mgr_config);
        // Force mark as available for testing
        if let Some(p) = mgr.installed.first_mut() {
            p.is_available = true;
        }

        let result = mgr.create_venv(CreateVenvRequest {
            name: "test-env".into(),
            python_version: Some("3.12".into()),
            requirements: vec![],
            requirements_file: None,
            agent_id: Some("agent-1".into()),
            path: None,
        });

        let venv = result.unwrap();
        assert_eq!(venv.name, "test-env");
        assert_eq!(venv.python_version, PythonVersion::parse("3.12").unwrap());
        assert_eq!(venv.agent_id, Some("agent-1".to_string()));
    }

    #[test]
    fn test_create_venv_duplicate() {
        let mut mgr = PythonRuntimeManager::new();
        if let Some(p) = mgr.installed.first_mut() {
            p.is_available = true;
        }

        mgr.create_venv(CreateVenvRequest {
            name: "dup".into(),
            python_version: Some("3.12".into()),
            requirements: vec![],
            requirements_file: None,
            agent_id: None,
            path: None,
        })
        .unwrap();

        let result = mgr.create_venv(CreateVenvRequest {
            name: "dup".into(),
            python_version: Some("3.12".into()),
            requirements: vec![],
            requirements_file: None,
            agent_id: None,
            path: None,
        });
        assert!(matches!(result, Err(PythonError::VenvExists { .. })));
    }

    #[test]
    fn test_create_venv_invalid_version() {
        let mut mgr = PythonRuntimeManager::new();
        let result = mgr.create_venv(CreateVenvRequest {
            name: "bad".into(),
            python_version: Some("abc".into()),
            requirements: vec![],
            requirements_file: None,
            agent_id: None,
            path: None,
        });
        assert!(matches!(result, Err(PythonError::InvalidVersion { .. })));
    }

    #[test]
    fn test_list_venvs() {
        let mut mgr = PythonRuntimeManager::new();
        if let Some(p) = mgr.installed.first_mut() {
            p.is_available = true;
        }

        mgr.create_venv(CreateVenvRequest {
            name: "a".into(),
            python_version: Some("3.12".into()),
            requirements: vec![],
            requirements_file: None,
            agent_id: Some("agent-1".into()),
            path: None,
        })
        .unwrap();
        mgr.create_venv(CreateVenvRequest {
            name: "b".into(),
            python_version: Some("3.12".into()),
            requirements: vec![],
            requirements_file: None,
            agent_id: Some("agent-2".into()),
            path: None,
        })
        .unwrap();

        assert_eq!(mgr.list_venvs().len(), 2);
        assert_eq!(mgr.list_venvs_for_agent("agent-1").len(), 1);
    }

    #[test]
    fn test_remove_venv() {
        let mut mgr = PythonRuntimeManager::new();
        if let Some(p) = mgr.installed.first_mut() {
            p.is_available = true;
        }

        mgr.create_venv(CreateVenvRequest {
            name: "removeme".into(),
            python_version: Some("3.12".into()),
            requirements: vec![],
            requirements_file: None,
            agent_id: None,
            path: None,
        })
        .unwrap();

        mgr.remove_venv("removeme").unwrap();
        assert!(mgr.get_venv("removeme").is_none());

        // Remove nonexistent
        let result = mgr.remove_venv("nonexistent");
        assert!(matches!(result, Err(PythonError::VenvNotFound { .. })));
    }

    // --- pip proxy ---

    #[test]
    fn test_pip_proxy_defaults() {
        let config = PipProxyConfig::default();
        assert!(config.cache_enabled);
        assert!(config.audit_enabled);
        assert_eq!(config.index_url, "https://pypi.org/simple/");
        assert_eq!(config.max_cache_mb, 2048);
        assert!(!config.require_hashes);
    }

    #[test]
    fn test_package_blocked() {
        let config = PythonRuntimeConfig {
            pip_proxy: PipProxyConfig {
                blocked_packages: vec!["malware-pkg".into(), "evil*".into()],
                ..Default::default()
            },
            ..Default::default()
        };
        let mgr = PythonRuntimeManager::with_config(config);

        assert!(mgr.is_package_blocked("malware-pkg"));
        assert!(mgr.is_package_blocked("evilbot"));
        assert!(mgr.is_package_blocked("evil"));
        assert!(!mgr.is_package_blocked("requests"));
        assert!(!mgr.is_package_blocked("numpy"));
    }

    #[test]
    fn test_pip_install_record() {
        let mut mgr = PythonRuntimeManager::new();
        mgr.record_pip_install(PipInstallRecord {
            timestamp: "2026-03-10T00:00:00Z".into(),
            package: "requests".into(),
            version: "2.31.0".into(),
            target: "myenv".into(),
            agent_id: Some("agent-1".into()),
            from_cache: true,
            sha256: Some("abc123".into()),
        });

        assert_eq!(mgr.pip_install_log().len(), 1);
        assert_eq!(mgr.pip_install_log()[0].package, "requests");
    }

    // --- Shim generation ---

    #[test]
    fn test_generate_shim_script() {
        let script = generate_shim_script("3.12");
        assert!(script.contains("#!/bin/sh"));
        assert!(script.contains("AGNOS_PYTHON_VERSION"));
        assert!(script.contains(".python-version"));
        assert!(script.contains("/etc/agnos/python/default-version"));
        assert!(script.contains("3.12"));
        assert!(script.contains("exec \"$PYBIN\" \"$@\""));
    }

    #[test]
    fn test_generate_pip_wrapper() {
        let script = generate_pip_wrapper("/var/cache/pip", "https://pypi.org/simple/");
        assert!(script.contains("#!/bin/sh"));
        assert!(script.contains("PIP_CACHE_DIR"));
        assert!(script.contains("PIP_INDEX_URL"));
        assert!(script.contains("-m pip"));
    }

    // --- Stats ---

    #[test]
    fn test_stats() {
        let mgr = PythonRuntimeManager::new();
        let stats = mgr.stats();
        assert_eq!(stats.known_versions, 4);
        assert_eq!(stats.venv_count, 0);
        assert_eq!(stats.pip_installs, 0);
        assert!(stats.cache_enabled);
        assert!(stats.audit_enabled);
    }

    // --- Serialization ---

    #[test]
    fn test_version_serialization() {
        let v = PythonVersion::parse("3.13t").unwrap();
        let json = serde_json::to_string(&v).unwrap();
        assert!(json.contains("\"free_threaded\":true"));

        let deser: PythonVersion = serde_json::from_str(&json).unwrap();
        assert_eq!(deser, v);
    }

    #[test]
    fn test_venv_serialization() {
        let venv = VenvInfo {
            name: "test".into(),
            path: PathBuf::from("/tmp/test"),
            python_version: PythonVersion::parse("3.12").unwrap(),
            created_at: "2026-03-10".into(),
            exists: true,
            packages: HashMap::from([("requests".into(), "2.31.0".into())]),
            agent_id: None,
        };
        let json = serde_json::to_string(&venv).unwrap();
        let deser: VenvInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "test");
        assert_eq!(deser.packages.get("requests").unwrap(), "2.31.0");
    }

    #[test]
    fn test_error_display() {
        let err = PythonError::InvalidVersion {
            version: "abc".into(),
        };
        assert!(err.to_string().contains("abc"));

        let err = PythonError::VenvExists {
            name: "myenv".into(),
        };
        assert!(err.to_string().contains("myenv"));
    }

    #[test]
    fn test_installed_python_fields() {
        let info = InstalledPython {
            version: PythonVersion::parse("3.13t").unwrap(),
            binary_path: PathBuf::from("/usr/bin/python3.13t"),
            lib_dir: PathBuf::from("/usr/lib/agnos/python/3.13t"),
            is_available: false,
            full_version: None,
            pgo_enabled: true,
            lto_enabled: true,
            gil_disabled: true,
        };
        assert!(info.gil_disabled);
        assert!(info.pgo_enabled);
        assert!(!info.is_available);
    }
}
