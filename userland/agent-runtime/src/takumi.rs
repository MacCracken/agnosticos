//! Takumi — Package Build System for AGNOS
//!
//! Compiles packages from source using TOML recipe files, producing signed
//! `.ark` packages. Named after the Japanese word for "master craftsman" —
//! takumi crafts every package in AGNOS with precision.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Build recipe types (parsed from TOML)
// ---------------------------------------------------------------------------

/// A complete build recipe parsed from a `.toml` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildRecipe {
    pub package: PackageMetadata,
    pub source: SourceSpec,
    pub depends: DependencySpec,
    pub build: BuildSteps,
    #[serde(default)]
    pub security: SecurityFlags,
}

/// Package metadata — identity and classification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMetadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub license: String,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default = "default_release")]
    pub release: u32,
    pub arch: Option<String>,
}

fn default_release() -> u32 {
    1
}

/// Source tarball location and integrity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSpec {
    pub url: String,
    pub sha256: String,
    #[serde(default)]
    pub patches: Vec<String>,
}

/// Runtime and build-time dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencySpec {
    #[serde(default)]
    pub runtime: Vec<String>,
    #[serde(default)]
    pub build: Vec<String>,
}

/// Shell commands for each build phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildSteps {
    pub configure: Option<String>,
    pub make: Option<String>,
    pub check: Option<String>,
    pub install: Option<String>,
    pub pre_build: Option<String>,
    pub post_install: Option<String>,
}

/// Security hardening configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityFlags {
    #[serde(default)]
    pub hardening: Vec<HardeningFlag>,
    pub cflags: Option<String>,
    pub ldflags: Option<String>,
}

/// Individual compiler hardening flags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HardeningFlag {
    Pie,
    Relro,
    #[serde(rename = "fullrelro")]
    FullRelro,
    Fortify,
    #[serde(rename = "stackprotector")]
    StackProtector,
    Bindnow,
}

impl fmt::Display for HardeningFlag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pie => write!(f, "pie"),
            Self::Relro => write!(f, "relro"),
            Self::FullRelro => write!(f, "fullrelro"),
            Self::Fortify => write!(f, "fortify"),
            Self::StackProtector => write!(f, "stackprotector"),
            Self::Bindnow => write!(f, "bindnow"),
        }
    }
}

impl HardeningFlag {
    /// Parse a hardening flag from a string.
    pub fn from_str_loose(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "pie" => Ok(Self::Pie),
            "relro" => Ok(Self::Relro),
            "fullrelro" | "full_relro" | "full-relro" => Ok(Self::FullRelro),
            "fortify" => Ok(Self::Fortify),
            "stackprotector" | "stack_protector" | "stack-protector" => Ok(Self::StackProtector),
            "bindnow" | "bind_now" | "bind-now" => Ok(Self::Bindnow),
            _ => bail!("unknown hardening flag: {}", s),
        }
    }
}

// ---------------------------------------------------------------------------
// .ark package output types
// ---------------------------------------------------------------------------

/// A built `.ark` package ready for distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArkPackage {
    pub manifest: ArkManifest,
    pub signature: Option<Vec<u8>>,
    pub files: Vec<ArkFileEntry>,
    pub data_hash: String,
}

/// Manifest embedded in every `.ark` package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArkManifest {
    pub name: String,
    pub version: String,
    pub release: u32,
    pub description: String,
    pub arch: String,
    pub size_installed: u64,
    pub build_date: DateTime<Utc>,
    pub builder: String,
    pub source_url: String,
    pub source_hash: String,
    pub license: String,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub depends: Vec<String>,
}

/// A single file entry inside an `.ark` package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArkFileEntry {
    pub path: String,
    pub sha256: String,
    pub size: u64,
    pub file_type: ArkFileType,
}

/// Type of file stored in an `.ark` package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArkFileType {
    /// A regular file.
    Regular,
    /// A directory entry.
    Directory,
    /// A symbolic link pointing to the given target.
    Symlink(String),
    /// A configuration file (preserved on upgrade).
    Config,
}

impl fmt::Display for ArkFileType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Regular => write!(f, "regular"),
            Self::Directory => write!(f, "directory"),
            Self::Symlink(target) => write!(f, "symlink -> {}", target),
            Self::Config => write!(f, "config"),
        }
    }
}

// ---------------------------------------------------------------------------
// Build context and status
// ---------------------------------------------------------------------------

/// Runtime context for a single package build.
#[derive(Debug, Clone)]
pub struct BuildContext {
    pub recipe: BuildRecipe,
    pub source_dir: PathBuf,
    pub build_dir: PathBuf,
    pub package_dir: PathBuf,
    pub output_dir: PathBuf,
    pub arch: String,
}

/// Current status of a build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildStatus {
    Pending,
    Downloading,
    Extracting,
    Configuring,
    Building,
    Testing,
    Installing,
    Packaging,
    Signing,
    Complete,
    Failed(String),
}

impl fmt::Display for BuildStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Downloading => write!(f, "downloading"),
            Self::Extracting => write!(f, "extracting"),
            Self::Configuring => write!(f, "configuring"),
            Self::Building => write!(f, "building"),
            Self::Testing => write!(f, "testing"),
            Self::Installing => write!(f, "installing"),
            Self::Packaging => write!(f, "packaging"),
            Self::Signing => write!(f, "signing"),
            Self::Complete => write!(f, "complete"),
            Self::Failed(msg) => write!(f, "failed: {}", msg),
        }
    }
}

/// A log entry for a build run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildLogEntry {
    pub package: String,
    pub status: BuildStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_secs: Option<u64>,
}

// ---------------------------------------------------------------------------
// Takumi build system
// ---------------------------------------------------------------------------

/// The takumi build system — loads recipes, resolves build order, and produces
/// `.ark` packages.
pub struct TakumiBuildSystem {
    pub recipes_dir: PathBuf,
    pub build_root: PathBuf,
    pub output_dir: PathBuf,
    loaded_recipes: HashMap<String, BuildRecipe>,
    build_log: Vec<BuildLogEntry>,
}

impl TakumiBuildSystem {
    /// Create a new build system with the given directory layout.
    pub fn new(recipes_dir: PathBuf, build_root: PathBuf, output_dir: PathBuf) -> Self {
        Self {
            recipes_dir,
            build_root,
            output_dir,
            loaded_recipes: HashMap::new(),
            build_log: Vec::new(),
        }
    }

    /// Load a single recipe from a TOML file.
    pub fn load_recipe(path: &Path) -> Result<BuildRecipe> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read recipe: {}", path.display()))?;
        let recipe: BuildRecipe = toml::from_str(&content)
            .with_context(|| format!("failed to parse recipe: {}", path.display()))?;
        info!(name = %recipe.package.name, version = %recipe.package.version, "loaded recipe");
        Ok(recipe)
    }

    /// Load all `.toml` recipe files from the recipes directory, recursing
    /// into subdirectories (e.g. `recipes/browser/`, `recipes/python/`).
    /// Returns the number of recipes loaded.
    pub fn load_all_recipes(&mut self) -> Result<usize> {
        let mut count = 0;
        self.load_recipes_from_dir(&self.recipes_dir.clone(), &mut count)?;
        info!(count, "loaded recipes from directory");
        Ok(count)
    }

    /// Recursively scan a directory for `.toml` recipe files.
    fn load_recipes_from_dir(&mut self, dir: &Path, count: &mut usize) -> Result<()> {
        let entries = std::fs::read_dir(dir)
            .with_context(|| format!("cannot read recipes dir: {}", dir.display()))?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                self.load_recipes_from_dir(&path, count)?;
            } else if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                match Self::load_recipe(&path) {
                    Ok(recipe) => {
                        debug!(name = %recipe.package.name, "loaded recipe from dir");
                        if self.loaded_recipes.contains_key(&recipe.package.name) {
                            warn!(
                                name = %recipe.package.name,
                                path = %path.display(),
                                "duplicate recipe name; overwriting previously loaded recipe"
                            );
                        }
                        self.loaded_recipes
                            .insert(recipe.package.name.clone(), recipe);
                        *count += 1;
                    }
                    Err(e) => {
                        warn!(path = %path.display(), error = %e, "skipping invalid recipe");
                    }
                }
            }
        }

        Ok(())
    }

    /// Validate a recipe and return a list of warnings. Returns `Err` for
    /// fatal issues (e.g. empty name).
    pub fn validate_recipe(recipe: &BuildRecipe) -> Result<Vec<String>> {
        let mut warnings = Vec::new();

        if recipe.package.name.is_empty() {
            bail!("recipe has empty package name");
        }
        // Validate package name for path traversal and shell injection safety
        if recipe.package.name.contains('/')
            || recipe.package.name.contains('\0')
            || recipe.package.name.contains("..")
            || recipe.package.name.contains(' ')
            || recipe.package.name.contains('\\')
        {
            bail!(
                "package name '{}' contains unsafe characters (/, .., \\0, space, or backslash)",
                recipe.package.name
            );
        }
        if !recipe
            .package
            .name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            bail!(
                "package name '{}' contains invalid characters; only alphanumeric, hyphens, underscores, and dots are allowed",
                recipe.package.name
            );
        }
        if recipe.package.version.is_empty() {
            bail!("recipe has empty package version");
        }
        if recipe.source.url.is_empty() {
            bail!("recipe has empty source URL");
        }
        if recipe.source.sha256.is_empty() {
            bail!("recipe has empty source sha256");
        }
        // Validate source URL scheme — only https:// and http:// are allowed
        if !recipe.source.url.starts_with("https://") && !recipe.source.url.starts_with("http://") {
            bail!(
                "source URL '{}' uses an unsupported scheme; only https:// and http:// are allowed",
                recipe.source.url
            );
        }
        // Validate sha256 format: exactly 64 lowercase hex characters
        if recipe.source.sha256.len() != 64
            || !recipe
                .source
                .sha256
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            warnings.push(format!(
                "source sha256 '{}' is not a valid lowercase 64-character hex digest",
                recipe.source.sha256
            ));
        }
        if recipe.package.description.is_empty() {
            warnings.push("package description is empty".to_string());
        }
        if recipe.package.license.is_empty() {
            warnings.push("package license is empty".to_string());
        }
        if recipe.build.configure.is_none()
            && recipe.build.make.is_none()
            && recipe.build.install.is_none()
        {
            warnings.push("recipe has no build steps defined".to_string());
        }
        if recipe.security.hardening.is_empty() {
            warnings.push("no hardening flags specified".to_string());
        }
        if recipe.package.release == 0 {
            warnings.push("release number is 0, should start at 1".to_string());
        }

        // Check for version format (simple semver-ish check)
        let parts: Vec<&str> = recipe.package.version.split('.').collect();
        if parts.len() < 2 {
            warnings.push(format!(
                "version '{}' may not be a valid version string",
                recipe.package.version
            ));
        }

        Ok(warnings)
    }

    /// Resolve build order using topological sort on build dependencies.
    /// Returns packages in the order they should be built. Detects cycles.
    pub fn resolve_build_order(&self, packages: &[String]) -> Result<Vec<String>> {
        // Build adjacency list from loaded recipes
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        for name in packages {
            if let Some(recipe) = self.loaded_recipes.get(name) {
                let build_deps: Vec<String> = recipe
                    .depends
                    .build
                    .iter()
                    .filter(|d| packages.contains(d))
                    .cloned()
                    .collect();
                adj.insert(name.clone(), build_deps);
            } else {
                // Package not loaded — assume no build deps within the set
                adj.insert(name.clone(), Vec::new());
            }
        }

        // Kahn's algorithm for topological sort
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        for name in packages {
            in_degree.entry(name.clone()).or_insert(0);
        }
        for deps in adj.values() {
            for dep in deps {
                *in_degree.entry(dep.clone()).or_insert(0) += 1;
            }
        }

        let mut queue: Vec<String> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(name, _)| name.clone())
            .collect();
        queue.sort(); // deterministic ordering

        let mut result = Vec::new();
        while let Some(node) = queue.pop() {
            result.push(node.clone());
            if let Some(deps) = adj.get(&node) {
                for dep in deps {
                    if let Some(deg) = in_degree.get_mut(dep) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push(dep.clone());
                            queue.sort();
                        }
                    }
                }
            }
        }

        if result.len() != packages.len() {
            let missing: Vec<String> = packages
                .iter()
                .filter(|p| !result.contains(p))
                .cloned()
                .collect();
            bail!(
                "dependency cycle detected involving: {}",
                missing.join(", ")
            );
        }

        // Our adjacency list maps each package to its *build dependencies*,
        // but in_degree counts incoming edges from those dependency edges —
        // so a node with 0 in-degree has no packages depending on it (i.e. it
        // is a leaf/consumer). Kahn's algorithm therefore emits leaves first
        // and sources (depended-upon packages) last. We reverse to get the
        // correct build order: sources first, dependents last.
        result.reverse();

        Ok(result)
    }

    /// Convert security flags to GCC CFLAGS string.
    pub fn generate_cflags(flags: &SecurityFlags) -> String {
        let mut parts = Vec::new();

        for flag in &flags.hardening {
            match flag {
                HardeningFlag::Pie => parts.push("-fPIE".to_string()),
                HardeningFlag::Fortify => parts.push("-D_FORTIFY_SOURCE=2".to_string()),
                HardeningFlag::StackProtector => {
                    parts.push("-fstack-protector-strong".to_string());
                }
                // Relro, FullRelro, Bindnow are linker flags, not CFLAGS
                _ => {}
            }
        }

        if let Some(extra) = &flags.cflags {
            parts.push(extra.clone());
        }

        parts.join(" ")
    }

    /// Convert security flags to GCC LDFLAGS string.
    ///
    /// Avoids redundant flags: if `FullRelro` is present, `Relro` (already
    /// implied by `-Wl,-z,relro,-z,now`) and `Bindnow` (already implied by
    /// the `-z,now` portion) are skipped.
    pub fn generate_ldflags(flags: &SecurityFlags) -> String {
        let mut parts = Vec::new();
        let has_full_relro = flags.hardening.contains(&HardeningFlag::FullRelro);

        for flag in &flags.hardening {
            match flag {
                HardeningFlag::Pie => parts.push("-pie".to_string()),
                HardeningFlag::Relro if has_full_relro => {
                    // FullRelro already emits -Wl,-z,relro,-z,now; skip redundant relro
                }
                HardeningFlag::Relro => parts.push("-Wl,-z,relro".to_string()),
                HardeningFlag::FullRelro => parts.push("-Wl,-z,relro,-z,now".to_string()),
                HardeningFlag::Bindnow if has_full_relro => {
                    // FullRelro already emits -z,now; skip redundant bindnow
                }
                HardeningFlag::Bindnow => parts.push("-Wl,-z,now".to_string()),
                // Fortify and StackProtector are compiler flags, not linker flags
                _ => {}
            }
        }

        if let Some(extra) = &flags.ldflags {
            parts.push(extra.clone());
        }

        parts.join(" ")
    }

    /// Create an `.ark` manifest by scanning the fake-root install directory.
    pub fn create_ark_manifest(recipe: &BuildRecipe, package_dir: &Path) -> Result<ArkManifest> {
        let size = Self::compute_dir_size(package_dir)?;
        let arch = recipe
            .package
            .arch
            .clone()
            .unwrap_or_else(|| std::env::consts::ARCH.to_string());

        Ok(ArkManifest {
            name: recipe.package.name.clone(),
            version: recipe.package.version.clone(),
            release: recipe.package.release,
            description: recipe.package.description.clone(),
            arch,
            size_installed: size,
            build_date: Utc::now(),
            builder: "takumi/0.1.0".to_string(),
            source_url: recipe.source.url.clone(),
            source_hash: recipe.source.sha256.clone(),
            license: recipe.package.license.clone(),
            groups: recipe.package.groups.clone(),
            depends: recipe.depends.runtime.clone(),
        })
    }

    /// Walk the fake-root directory and build a file list with SHA-256 hashes.
    pub fn create_file_list(package_dir: &Path) -> Result<Vec<ArkFileEntry>> {
        let mut entries = Vec::new();
        Self::walk_dir(package_dir, package_dir, &mut entries)?;
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(entries)
    }

    /// Look up a loaded recipe by name.
    pub fn get_recipe(&self, name: &str) -> Option<&BuildRecipe> {
        self.loaded_recipes.get(name)
    }

    /// Number of loaded recipes.
    pub fn recipe_count(&self) -> usize {
        self.loaded_recipes.len()
    }

    /// Access the build log.
    pub fn build_log(&self) -> &[BuildLogEntry] {
        &self.build_log
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Recursively walk a directory, recording all files relative to `root`.
    fn walk_dir(root: &Path, dir: &Path, entries: &mut Vec<ArkFileEntry>) -> Result<()> {
        let read_dir = std::fs::read_dir(dir)
            .with_context(|| format!("cannot read directory: {}", dir.display()))?;

        for entry in read_dir {
            let entry = entry?;
            let path = entry.path();
            let rel = path
                .strip_prefix(root)
                .with_context(|| "failed to strip prefix")?;
            let rel_str = format!("/{}", rel.display());
            let metadata = std::fs::symlink_metadata(&path)?;

            if metadata.is_symlink() {
                let target = std::fs::read_link(&path)?;
                entries.push(ArkFileEntry {
                    path: rel_str,
                    sha256: String::new(),
                    size: 0,
                    file_type: ArkFileType::Symlink(target.to_string_lossy().to_string()),
                });
            } else if metadata.is_dir() {
                entries.push(ArkFileEntry {
                    path: rel_str,
                    sha256: String::new(),
                    size: 0,
                    file_type: ArkFileType::Directory,
                });
                Self::walk_dir(root, &path, entries)?;
            } else {
                let content = std::fs::read(&path)?;
                let hash = hex_sha256(&content);
                let size = metadata.len();

                // Treat files under /etc as config files
                let file_type = if rel_str.starts_with("/etc/") {
                    ArkFileType::Config
                } else {
                    ArkFileType::Regular
                };

                entries.push(ArkFileEntry {
                    path: rel_str,
                    sha256: hash,
                    size,
                    file_type,
                });
            }
        }

        Ok(())
    }

    /// Compute total size of all regular files under a directory.
    fn compute_dir_size(dir: &Path) -> Result<u64> {
        let mut total = 0u64;
        if dir.is_dir() {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                let meta = std::fs::symlink_metadata(&path)?;
                if meta.is_dir() {
                    total += Self::compute_dir_size(&path)?;
                } else if meta.is_file() {
                    total += meta.len();
                }
            }
        }
        Ok(total)
    }
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

/// Compute hex-encoded SHA-256 of bytes.
fn hex_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    result.iter().map(|b| format!("{:02x}", b)).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// A minimal valid recipe TOML.
    const MINIMAL_RECIPE: &str = r#"
[package]
name = "hello"
version = "1.0.0"
description = "Hello world"
license = "MIT"

[source]
url = "https://example.com/hello-1.0.0.tar.gz"
sha256 = "abc123def456"

[depends]
runtime = []
build = ["gcc"]

[build]
configure = "./configure --prefix=/usr"
make = "make"
install = "make DESTDIR=$PKG install"
"#;

    /// A full recipe with all optional fields.
    const FULL_RECIPE: &str = r#"
[package]
name = "openssl"
version = "3.5.2"
description = "TLS/SSL cryptographic library"
license = "Apache-2.0"
groups = ["base", "crypto"]
release = 3
arch = "x86_64"

[source]
url = "https://www.openssl.org/source/openssl-3.5.2.tar.gz"
sha256 = "e1f5c1c2b3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0"
patches = ["fix-musl.patch", "agnos-paths.patch"]

[depends]
runtime = ["glibc", "zlib"]
build = ["perl", "make"]

[build]
pre_build = "sed -i 's/foo/bar/' Makefile.in"
configure = "./config --prefix=/usr --openssldir=/etc/ssl shared"
make = "make -j$(nproc)"
check = "make test"
install = "make DESTDIR=$PKG install"
post_install = "rm -rf $PKG/usr/share/doc"

[security]
hardening = ["pie", "relro", "fortify", "stackprotector"]
cflags = "-O2"
ldflags = "-Wl,--as-needed"
"#;

    // -- Recipe parsing tests -----------------------------------------------

    #[test]
    fn parse_minimal_recipe() {
        let recipe: BuildRecipe = toml::from_str(MINIMAL_RECIPE).unwrap();
        assert_eq!(recipe.package.name, "hello");
        assert_eq!(recipe.package.version, "1.0.0");
        assert_eq!(recipe.package.release, 1); // default
        assert!(recipe.package.arch.is_none());
        assert!(recipe.package.groups.is_empty());
        assert_eq!(recipe.source.sha256, "abc123def456");
        assert!(recipe.source.patches.is_empty());
        assert!(recipe.security.hardening.is_empty());
    }

    #[test]
    fn parse_full_recipe() {
        let recipe: BuildRecipe = toml::from_str(FULL_RECIPE).unwrap();
        assert_eq!(recipe.package.name, "openssl");
        assert_eq!(recipe.package.release, 3);
        assert_eq!(recipe.package.arch.as_deref(), Some("x86_64"));
        assert_eq!(recipe.package.groups, vec!["base", "crypto"]);
        assert_eq!(recipe.source.patches.len(), 2);
        assert_eq!(recipe.depends.runtime, vec!["glibc", "zlib"]);
        assert_eq!(recipe.depends.build, vec!["perl", "make"]);
        assert_eq!(
            recipe.build.pre_build.as_deref(),
            Some("sed -i 's/foo/bar/' Makefile.in")
        );
        assert_eq!(recipe.build.check.as_deref(), Some("make test"));
        assert_eq!(
            recipe.build.post_install.as_deref(),
            Some("rm -rf $PKG/usr/share/doc")
        );
        assert_eq!(recipe.security.hardening.len(), 4);
        assert_eq!(recipe.security.cflags.as_deref(), Some("-O2"));
    }

    #[test]
    fn parse_recipe_missing_required_field() {
        let bad = r#"
[package]
name = "bad"
version = "1.0"
description = "test"

[source]
url = "https://example.com"
sha256 = "abc"

[depends]

[build]
"#;
        // Missing license field
        let result = toml::from_str::<BuildRecipe>(bad);
        assert!(result.is_err());
    }

    #[test]
    fn parse_recipe_minimal_fields() {
        let minimal = r#"
[package]
name = "tiny"
version = "0.1"
description = ""
license = ""

[source]
url = "https://example.com/tiny.tar.gz"
sha256 = "deadbeef"

[depends]

[build]
"#;
        let recipe: BuildRecipe = toml::from_str(minimal).unwrap();
        assert_eq!(recipe.package.name, "tiny");
        assert!(recipe.build.configure.is_none());
        assert!(recipe.build.make.is_none());
        assert!(recipe.build.install.is_none());
    }

    // -- Validation tests ---------------------------------------------------

    #[test]
    fn validate_valid_recipe() {
        let recipe: BuildRecipe = toml::from_str(FULL_RECIPE).unwrap();
        let warnings = TakumiBuildSystem::validate_recipe(&recipe).unwrap();
        // Full recipe should produce no warnings (has all fields)
        assert!(warnings.is_empty(), "unexpected warnings: {:?}", warnings);
    }

    #[test]
    fn validate_empty_name_is_error() {
        let mut recipe: BuildRecipe = toml::from_str(MINIMAL_RECIPE).unwrap();
        recipe.package.name = String::new();
        let result = TakumiBuildSystem::validate_recipe(&recipe);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("empty package name"));
    }

    #[test]
    fn validate_empty_version_is_error() {
        let mut recipe: BuildRecipe = toml::from_str(MINIMAL_RECIPE).unwrap();
        recipe.package.version = String::new();
        let result = TakumiBuildSystem::validate_recipe(&recipe);
        assert!(result.is_err());
    }

    #[test]
    fn validate_empty_source_url_is_error() {
        let mut recipe: BuildRecipe = toml::from_str(MINIMAL_RECIPE).unwrap();
        recipe.source.url = String::new();
        let result = TakumiBuildSystem::validate_recipe(&recipe);
        assert!(result.is_err());
    }

    #[test]
    fn validate_warnings_for_missing_optional() {
        let minimal = r#"
[package]
name = "warn"
version = "1.0"
description = ""
license = ""

[source]
url = "https://example.com/warn.tar.gz"
sha256 = "abc123"

[depends]

[build]
"#;
        let recipe: BuildRecipe = toml::from_str(minimal).unwrap();
        let warnings = TakumiBuildSystem::validate_recipe(&recipe).unwrap();
        assert!(warnings.iter().any(|w| w.contains("description is empty")));
        assert!(warnings.iter().any(|w| w.contains("license is empty")));
        assert!(warnings.iter().any(|w| w.contains("no build steps")));
        assert!(warnings.iter().any(|w| w.contains("no hardening")));
    }

    #[test]
    fn validate_single_component_version_warning() {
        let mut recipe: BuildRecipe = toml::from_str(MINIMAL_RECIPE).unwrap();
        recipe.package.version = "7".to_string();
        let warnings = TakumiBuildSystem::validate_recipe(&recipe).unwrap();
        assert!(warnings.iter().any(|w| w.contains("valid version")));
    }

    // -- HardeningFlag tests ------------------------------------------------

    #[test]
    fn hardening_flag_from_str_all_variants() {
        assert_eq!(
            HardeningFlag::from_str_loose("pie").unwrap(),
            HardeningFlag::Pie
        );
        assert_eq!(
            HardeningFlag::from_str_loose("relro").unwrap(),
            HardeningFlag::Relro
        );
        assert_eq!(
            HardeningFlag::from_str_loose("fullrelro").unwrap(),
            HardeningFlag::FullRelro
        );
        assert_eq!(
            HardeningFlag::from_str_loose("full_relro").unwrap(),
            HardeningFlag::FullRelro
        );
        assert_eq!(
            HardeningFlag::from_str_loose("full-relro").unwrap(),
            HardeningFlag::FullRelro
        );
        assert_eq!(
            HardeningFlag::from_str_loose("fortify").unwrap(),
            HardeningFlag::Fortify
        );
        assert_eq!(
            HardeningFlag::from_str_loose("stackprotector").unwrap(),
            HardeningFlag::StackProtector
        );
        assert_eq!(
            HardeningFlag::from_str_loose("stack-protector").unwrap(),
            HardeningFlag::StackProtector
        );
        assert_eq!(
            HardeningFlag::from_str_loose("bindnow").unwrap(),
            HardeningFlag::Bindnow
        );
        assert_eq!(
            HardeningFlag::from_str_loose("bind_now").unwrap(),
            HardeningFlag::Bindnow
        );
    }

    #[test]
    fn hardening_flag_from_str_unknown() {
        assert!(HardeningFlag::from_str_loose("garbage").is_err());
    }

    #[test]
    fn hardening_flag_display() {
        assert_eq!(HardeningFlag::Pie.to_string(), "pie");
        assert_eq!(HardeningFlag::Relro.to_string(), "relro");
        assert_eq!(HardeningFlag::FullRelro.to_string(), "fullrelro");
        assert_eq!(HardeningFlag::Fortify.to_string(), "fortify");
        assert_eq!(HardeningFlag::StackProtector.to_string(), "stackprotector");
        assert_eq!(HardeningFlag::Bindnow.to_string(), "bindnow");
    }

    // -- CFLAGS / LDFLAGS generation ----------------------------------------

    #[test]
    fn generate_cflags_multiple_flags() {
        let flags = SecurityFlags {
            hardening: vec![
                HardeningFlag::Pie,
                HardeningFlag::Fortify,
                HardeningFlag::StackProtector,
            ],
            cflags: None,
            ldflags: None,
        };
        let cflags = TakumiBuildSystem::generate_cflags(&flags);
        assert!(cflags.contains("-fPIE"));
        assert!(cflags.contains("-D_FORTIFY_SOURCE=2"));
        assert!(cflags.contains("-fstack-protector-strong"));
    }

    #[test]
    fn generate_cflags_with_custom_appended() {
        let flags = SecurityFlags {
            hardening: vec![HardeningFlag::Pie],
            cflags: Some("-O2 -march=native".to_string()),
            ldflags: None,
        };
        let cflags = TakumiBuildSystem::generate_cflags(&flags);
        assert!(cflags.contains("-fPIE"));
        assert!(cflags.contains("-O2 -march=native"));
    }

    #[test]
    fn generate_ldflags_relro() {
        let flags = SecurityFlags {
            hardening: vec![HardeningFlag::Relro],
            cflags: None,
            ldflags: None,
        };
        let ldflags = TakumiBuildSystem::generate_ldflags(&flags);
        assert_eq!(ldflags, "-Wl,-z,relro");
    }

    #[test]
    fn generate_ldflags_fullrelro_bindnow_dedup() {
        // When FullRelro is present, Bindnow is redundant and should be skipped
        let flags = SecurityFlags {
            hardening: vec![HardeningFlag::FullRelro, HardeningFlag::Bindnow],
            cflags: None,
            ldflags: None,
        };
        let ldflags = TakumiBuildSystem::generate_ldflags(&flags);
        assert_eq!(ldflags, "-Wl,-z,relro,-z,now");
    }

    #[test]
    fn generate_ldflags_fullrelro_skips_relro() {
        // When FullRelro is present, plain Relro should be skipped
        let flags = SecurityFlags {
            hardening: vec![HardeningFlag::FullRelro, HardeningFlag::Relro],
            cflags: None,
            ldflags: None,
        };
        let ldflags = TakumiBuildSystem::generate_ldflags(&flags);
        assert_eq!(ldflags, "-Wl,-z,relro,-z,now");
    }

    #[test]
    fn generate_ldflags_bindnow_alone() {
        // Bindnow without FullRelro should still emit -Wl,-z,now
        let flags = SecurityFlags {
            hardening: vec![HardeningFlag::Bindnow],
            cflags: None,
            ldflags: None,
        };
        let ldflags = TakumiBuildSystem::generate_ldflags(&flags);
        assert_eq!(ldflags, "-Wl,-z,now");
    }

    #[test]
    fn generate_cflags_empty_flags() {
        let flags = SecurityFlags::default();
        let cflags = TakumiBuildSystem::generate_cflags(&flags);
        assert_eq!(cflags, "");
    }

    #[test]
    fn generate_ldflags_empty_flags() {
        let flags = SecurityFlags::default();
        let ldflags = TakumiBuildSystem::generate_ldflags(&flags);
        assert_eq!(ldflags, "");
    }

    // -- BuildStatus Display ------------------------------------------------

    #[test]
    fn build_status_display() {
        assert_eq!(BuildStatus::Pending.to_string(), "pending");
        assert_eq!(BuildStatus::Downloading.to_string(), "downloading");
        assert_eq!(BuildStatus::Extracting.to_string(), "extracting");
        assert_eq!(BuildStatus::Configuring.to_string(), "configuring");
        assert_eq!(BuildStatus::Building.to_string(), "building");
        assert_eq!(BuildStatus::Testing.to_string(), "testing");
        assert_eq!(BuildStatus::Installing.to_string(), "installing");
        assert_eq!(BuildStatus::Packaging.to_string(), "packaging");
        assert_eq!(BuildStatus::Signing.to_string(), "signing");
        assert_eq!(BuildStatus::Complete.to_string(), "complete");
        assert_eq!(
            BuildStatus::Failed("oom".to_string()).to_string(),
            "failed: oom"
        );
    }

    // -- ArkManifest --------------------------------------------------------

    #[test]
    fn ark_manifest_serialization_roundtrip() {
        let manifest = ArkManifest {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            release: 1,
            description: "A test package".to_string(),
            arch: "x86_64".to_string(),
            size_installed: 1024,
            build_date: Utc::now(),
            builder: "takumi/0.1.0".to_string(),
            source_url: "https://example.com/test.tar.gz".to_string(),
            source_hash: "abc123".to_string(),
            license: "MIT".to_string(),
            groups: vec!["base".to_string()],
            depends: vec!["glibc".to_string()],
        };

        let json = serde_json::to_string(&manifest).unwrap();
        let deserialized: ArkManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "test");
        assert_eq!(deserialized.version, "1.0.0");
        assert_eq!(deserialized.release, 1);
        assert_eq!(deserialized.builder, "takumi/0.1.0");
        assert_eq!(deserialized.depends, vec!["glibc"]);
    }

    // -- ArkFileEntry / ArkFileType -----------------------------------------

    #[test]
    fn ark_file_entry_types() {
        let regular = ArkFileEntry {
            path: "/usr/bin/hello".to_string(),
            sha256: "abc".to_string(),
            size: 4096,
            file_type: ArkFileType::Regular,
        };
        assert_eq!(regular.file_type, ArkFileType::Regular);

        let dir = ArkFileEntry {
            path: "/usr/bin".to_string(),
            sha256: String::new(),
            size: 0,
            file_type: ArkFileType::Directory,
        };
        assert_eq!(dir.file_type, ArkFileType::Directory);

        let config = ArkFileEntry {
            path: "/etc/hello.conf".to_string(),
            sha256: "def".to_string(),
            size: 128,
            file_type: ArkFileType::Config,
        };
        assert_eq!(config.file_type, ArkFileType::Config);
    }

    #[test]
    fn ark_file_type_symlink() {
        let sym = ArkFileType::Symlink("/usr/lib/libssl.so.3".to_string());
        assert_eq!(sym.to_string(), "symlink -> /usr/lib/libssl.so.3");
        if let ArkFileType::Symlink(target) = &sym {
            assert_eq!(target, "/usr/lib/libssl.so.3");
        } else {
            panic!("expected Symlink variant");
        }
    }

    // -- Build order (topological sort) -------------------------------------

    fn build_system_with_recipes(recipes: Vec<(&str, Vec<&str>)>) -> TakumiBuildSystem {
        let mut sys = TakumiBuildSystem::new(
            PathBuf::from("/tmp/recipes"),
            PathBuf::from("/tmp/build"),
            PathBuf::from("/tmp/output"),
        );
        for (name, build_deps) in recipes {
            let recipe: BuildRecipe = toml::from_str(&format!(
                r#"
[package]
name = "{name}"
version = "1.0"
description = "test"
license = "MIT"

[source]
url = "https://example.com/{name}.tar.gz"
sha256 = "abc"

[depends]
build = {deps}

[build]
make = "make"
"#,
                name = name,
                deps = format!(
                    "[{}]",
                    build_deps
                        .iter()
                        .map(|d| format!("\"{}\"", d))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ))
            .unwrap();
            sys.loaded_recipes.insert(name.to_string(), recipe);
        }
        sys
    }

    #[test]
    fn resolve_build_order_simple_chain() {
        // c depends on b, b depends on a => build order: a, b, c
        let sys =
            build_system_with_recipes(vec![("a", vec![]), ("b", vec!["a"]), ("c", vec!["b"])]);
        let order = sys
            .resolve_build_order(&["a".into(), "b".into(), "c".into()])
            .unwrap();
        let pos_a = order.iter().position(|x| x == "a").unwrap();
        let pos_b = order.iter().position(|x| x == "b").unwrap();
        let pos_c = order.iter().position(|x| x == "c").unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn resolve_build_order_diamond() {
        // d depends on b and c; b and c both depend on a
        let sys = build_system_with_recipes(vec![
            ("a", vec![]),
            ("b", vec!["a"]),
            ("c", vec!["a"]),
            ("d", vec!["b", "c"]),
        ]);
        let order = sys
            .resolve_build_order(&["a".into(), "b".into(), "c".into(), "d".into()])
            .unwrap();
        let pos_a = order.iter().position(|x| x == "a").unwrap();
        let pos_b = order.iter().position(|x| x == "b").unwrap();
        let pos_c = order.iter().position(|x| x == "c").unwrap();
        let pos_d = order.iter().position(|x| x == "d").unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_a < pos_c);
        assert!(pos_b < pos_d);
        assert!(pos_c < pos_d);
    }

    #[test]
    fn resolve_build_order_cycle_detection() {
        let sys = build_system_with_recipes(vec![("a", vec!["b"]), ("b", vec!["a"])]);
        let result = sys.resolve_build_order(&["a".into(), "b".into()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cycle"));
    }

    #[test]
    fn resolve_build_order_independent_packages() {
        let sys = build_system_with_recipes(vec![("x", vec![]), ("y", vec![]), ("z", vec![])]);
        let order = sys
            .resolve_build_order(&["x".into(), "y".into(), "z".into()])
            .unwrap();
        assert_eq!(order.len(), 3);
        // All should be present
        assert!(order.contains(&"x".to_string()));
        assert!(order.contains(&"y".to_string()));
        assert!(order.contains(&"z".to_string()));
    }

    // -- File list with real temp directory ----------------------------------

    #[test]
    fn create_file_list_with_temp_dir() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Create some files
        fs::create_dir_all(root.join("usr/bin")).unwrap();
        fs::write(root.join("usr/bin/hello"), b"#!/bin/sh\necho hello").unwrap();
        fs::write(root.join("usr/bin/world"), b"#!/bin/sh\necho world").unwrap();

        let entries = TakumiBuildSystem::create_file_list(root).unwrap();
        assert!(entries.len() >= 3); // usr dir, bin dir, 2 files

        let hello = entries.iter().find(|e| e.path == "/usr/bin/hello").unwrap();
        assert_eq!(hello.file_type, ArkFileType::Regular);
        assert!(hello.size > 0);
        assert!(!hello.sha256.is_empty());

        // Verify hash is correct
        let expected = hex_sha256(b"#!/bin/sh\necho hello");
        assert_eq!(hello.sha256, expected);
    }

    #[test]
    fn create_file_list_nested_dirs() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        fs::create_dir_all(root.join("a/b/c")).unwrap();
        fs::write(root.join("a/b/c/deep.txt"), b"deep").unwrap();

        let entries = TakumiBuildSystem::create_file_list(root).unwrap();
        let dirs: Vec<_> = entries
            .iter()
            .filter(|e| e.file_type == ArkFileType::Directory)
            .collect();
        assert!(dirs.len() >= 3); // a, b, c

        let deep = entries
            .iter()
            .find(|e| e.path == "/a/b/c/deep.txt")
            .unwrap();
        assert_eq!(deep.file_type, ArkFileType::Regular);
    }

    #[test]
    fn create_file_list_config_files() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        fs::create_dir_all(root.join("etc")).unwrap();
        fs::write(root.join("etc/app.conf"), b"key=value").unwrap();

        let entries = TakumiBuildSystem::create_file_list(root).unwrap();
        let conf = entries.iter().find(|e| e.path == "/etc/app.conf").unwrap();
        assert_eq!(conf.file_type, ArkFileType::Config);
    }

    #[cfg(unix)]
    #[test]
    fn create_file_list_with_symlinks() {
        use std::os::unix::fs::symlink;

        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        fs::create_dir_all(root.join("usr/lib")).unwrap();
        fs::write(root.join("usr/lib/libfoo.so.1"), b"ELF...").unwrap();
        symlink("libfoo.so.1", root.join("usr/lib/libfoo.so")).unwrap();

        let entries = TakumiBuildSystem::create_file_list(root).unwrap();
        let link = entries
            .iter()
            .find(|e| e.path == "/usr/lib/libfoo.so")
            .unwrap();
        match &link.file_type {
            ArkFileType::Symlink(target) => assert_eq!(target, "libfoo.so.1"),
            other => panic!("expected Symlink, got {:?}", other),
        }
    }

    // -- BuildLogEntry ------------------------------------------------------

    #[test]
    fn build_log_entry_creation() {
        let entry = BuildLogEntry {
            package: "openssl".to_string(),
            status: BuildStatus::Complete,
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            duration_secs: Some(120),
        };
        assert_eq!(entry.package, "openssl");
        assert_eq!(entry.status, BuildStatus::Complete);
        assert!(entry.duration_secs.is_some());
    }

    // -- TakumiBuildSystem basics -------------------------------------------

    #[test]
    fn build_system_new_and_recipe_count() {
        let sys = TakumiBuildSystem::new(
            PathBuf::from("/tmp/recipes"),
            PathBuf::from("/tmp/build"),
            PathBuf::from("/tmp/output"),
        );
        assert_eq!(sys.recipe_count(), 0);
        assert!(sys.build_log().is_empty());
        assert!(sys.get_recipe("nonexistent").is_none());
    }

    #[test]
    fn load_recipe_from_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("hello.toml");
        fs::write(&path, MINIMAL_RECIPE).unwrap();

        let recipe = TakumiBuildSystem::load_recipe(&path).unwrap();
        assert_eq!(recipe.package.name, "hello");
        assert_eq!(recipe.package.version, "1.0.0");
    }

    #[test]
    fn load_all_recipes_from_directory() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        fs::write(dir.join("hello.toml"), MINIMAL_RECIPE).unwrap();
        fs::write(dir.join("openssl.toml"), FULL_RECIPE).unwrap();
        // Non-toml file should be ignored
        fs::write(dir.join("README.md"), "# Recipes").unwrap();

        let mut sys = TakumiBuildSystem::new(
            dir.to_path_buf(),
            PathBuf::from("/tmp/build"),
            PathBuf::from("/tmp/output"),
        );
        let count = sys.load_all_recipes().unwrap();
        assert_eq!(count, 2);
        assert_eq!(sys.recipe_count(), 2);
        assert!(sys.get_recipe("hello").is_some());
        assert!(sys.get_recipe("openssl").is_some());
    }

    #[test]
    fn package_metadata_default_release() {
        let recipe: BuildRecipe = toml::from_str(MINIMAL_RECIPE).unwrap();
        assert_eq!(recipe.package.release, 1);
    }

    #[test]
    fn security_flags_empty_produces_no_flags() {
        let flags = SecurityFlags::default();
        assert!(flags.hardening.is_empty());
        assert!(flags.cflags.is_none());
        assert!(flags.ldflags.is_none());
        assert_eq!(TakumiBuildSystem::generate_cflags(&flags), "");
        assert_eq!(TakumiBuildSystem::generate_ldflags(&flags), "");
    }

    #[test]
    fn build_context_creation() {
        let recipe: BuildRecipe = toml::from_str(MINIMAL_RECIPE).unwrap();
        let ctx = BuildContext {
            recipe: recipe.clone(),
            source_dir: PathBuf::from("/tmp/src/hello-1.0.0"),
            build_dir: PathBuf::from("/tmp/build/hello"),
            package_dir: PathBuf::from("/tmp/pkg/hello"),
            output_dir: PathBuf::from("/tmp/out"),
            arch: "x86_64".to_string(),
        };
        assert_eq!(ctx.arch, "x86_64");
        assert_eq!(ctx.recipe.package.name, "hello");
    }

    #[test]
    fn ark_package_struct_creation() {
        let pkg = ArkPackage {
            manifest: ArkManifest {
                name: "test".to_string(),
                version: "1.0".to_string(),
                release: 1,
                description: "test".to_string(),
                arch: "x86_64".to_string(),
                size_installed: 0,
                build_date: Utc::now(),
                builder: "takumi/0.1.0".to_string(),
                source_url: "https://example.com".to_string(),
                source_hash: "abc".to_string(),
                license: "MIT".to_string(),
                groups: vec![],
                depends: vec![],
            },
            signature: None,
            files: vec![],
            data_hash: "deadbeef".to_string(),
        };
        assert!(pkg.signature.is_none());
        assert!(pkg.files.is_empty());
        assert_eq!(pkg.data_hash, "deadbeef");
    }

    #[test]
    fn source_spec_patch_list() {
        let recipe: BuildRecipe = toml::from_str(FULL_RECIPE).unwrap();
        assert_eq!(
            recipe.source.patches,
            vec!["fix-musl.patch", "agnos-paths.patch"]
        );
    }

    #[test]
    fn create_ark_manifest_from_recipe() {
        let tmp = TempDir::new().unwrap();
        let pkg_dir = tmp.path();
        fs::create_dir_all(pkg_dir.join("usr/bin")).unwrap();
        fs::write(pkg_dir.join("usr/bin/hello"), b"binary content here").unwrap();

        let recipe: BuildRecipe = toml::from_str(MINIMAL_RECIPE).unwrap();
        let manifest = TakumiBuildSystem::create_ark_manifest(&recipe, pkg_dir).unwrap();

        assert_eq!(manifest.name, "hello");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.release, 1);
        assert_eq!(manifest.builder, "takumi/0.1.0");
        assert!(manifest.size_installed > 0);
        assert_eq!(manifest.source_hash, "abc123def456");
    }

    #[test]
    fn hex_sha256_known_value() {
        // SHA-256 of empty string
        let hash = hex_sha256(b"");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn ark_file_type_display() {
        assert_eq!(ArkFileType::Regular.to_string(), "regular");
        assert_eq!(ArkFileType::Directory.to_string(), "directory");
        assert_eq!(ArkFileType::Config.to_string(), "config");
        assert_eq!(
            ArkFileType::Symlink("/lib/libc.so".to_string()).to_string(),
            "symlink -> /lib/libc.so"
        );
    }

    // -- Audit fix tests ----------------------------------------------------

    #[test]
    fn validate_package_name_with_slash_rejected() {
        let mut recipe: BuildRecipe = toml::from_str(MINIMAL_RECIPE).unwrap();
        recipe.package.name = "../etc/passwd".to_string();
        let result = TakumiBuildSystem::validate_recipe(&recipe);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unsafe characters"));
    }

    #[test]
    fn validate_package_name_with_dotdot_rejected() {
        let mut recipe: BuildRecipe = toml::from_str(MINIMAL_RECIPE).unwrap();
        recipe.package.name = "foo..bar".to_string();
        let result = TakumiBuildSystem::validate_recipe(&recipe);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unsafe characters"));
    }

    #[test]
    fn validate_package_name_with_special_chars_rejected() {
        let mut recipe: BuildRecipe = toml::from_str(MINIMAL_RECIPE).unwrap();
        recipe.package.name = "hello world".to_string();
        let result = TakumiBuildSystem::validate_recipe(&recipe);
        assert!(result.is_err());
    }

    #[test]
    fn validate_package_name_valid_chars_accepted() {
        let mut recipe: BuildRecipe = toml::from_str(MINIMAL_RECIPE).unwrap();
        recipe.package.name = "lib-foo_bar.2".to_string();
        let result = TakumiBuildSystem::validate_recipe(&recipe);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_valid_url_schemes_accepted() {
        let mut recipe: BuildRecipe = toml::from_str(MINIMAL_RECIPE).unwrap();
        recipe.source.url = "https://example.com/foo.tar.gz".to_string();
        assert!(TakumiBuildSystem::validate_recipe(&recipe).is_ok());

        recipe.source.url = "http://example.com/foo.tar.gz".to_string();
        assert!(TakumiBuildSystem::validate_recipe(&recipe).is_ok());
    }

    #[test]
    fn validate_file_url_rejected() {
        let mut recipe: BuildRecipe = toml::from_str(MINIMAL_RECIPE).unwrap();
        recipe.source.url = "file:///etc/passwd".to_string();
        let result = TakumiBuildSystem::validate_recipe(&recipe);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unsupported scheme"));
    }

    #[test]
    fn validate_ftp_url_rejected() {
        let mut recipe: BuildRecipe = toml::from_str(MINIMAL_RECIPE).unwrap();
        recipe.source.url = "ftp://mirror.example.com/foo.tar.gz".to_string();
        let result = TakumiBuildSystem::validate_recipe(&recipe);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unsupported scheme"));
    }

    #[test]
    fn validate_sha256_valid_format_no_warning() {
        let mut recipe: BuildRecipe = toml::from_str(FULL_RECIPE).unwrap();
        recipe.source.sha256 =
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string();
        let warnings = TakumiBuildSystem::validate_recipe(&recipe).unwrap();
        assert!(
            !warnings.iter().any(|w| w.contains("sha256")),
            "unexpected sha256 warning: {:?}",
            warnings
        );
    }

    #[test]
    fn validate_sha256_invalid_format_warns() {
        let mut recipe: BuildRecipe = toml::from_str(MINIMAL_RECIPE).unwrap();
        // too short
        recipe.source.sha256 = "abc123".to_string();
        let warnings = TakumiBuildSystem::validate_recipe(&recipe).unwrap();
        assert!(warnings.iter().any(|w| w.contains("sha256")));
    }

    #[test]
    fn validate_sha256_uppercase_warns() {
        let mut recipe: BuildRecipe = toml::from_str(MINIMAL_RECIPE).unwrap();
        recipe.source.sha256 =
            "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855".to_string();
        let warnings = TakumiBuildSystem::validate_recipe(&recipe).unwrap();
        assert!(warnings.iter().any(|w| w.contains("sha256")));
    }

    #[test]
    fn load_all_recipes_duplicate_name_warning() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Write two recipe files with the same package name "hello"
        fs::write(dir.join("hello1.toml"), MINIMAL_RECIPE).unwrap();
        fs::write(dir.join("hello2.toml"), MINIMAL_RECIPE).unwrap();

        let mut sys = TakumiBuildSystem::new(
            dir.to_path_buf(),
            PathBuf::from("/tmp/build"),
            PathBuf::from("/tmp/output"),
        );
        let count = sys.load_all_recipes().unwrap();
        // Both are counted (even though one overwrites the other)
        assert_eq!(count, 2);
        // Only one entry remains in the map
        assert_eq!(sys.recipe_count(), 1);
        assert!(sys.get_recipe("hello").is_some());
    }

    #[test]
    fn load_all_recipes_recurses_into_subdirectories() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Top-level recipe
        fs::write(dir.join("hello.toml"), MINIMAL_RECIPE).unwrap();

        // Subdirectory with recipe (like recipes/python/)
        let sub = dir.join("python");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("cpython.toml"), FULL_RECIPE).unwrap();

        // Nested non-toml file should be ignored
        fs::write(sub.join("README.md"), "# Python recipes").unwrap();

        let mut sys = TakumiBuildSystem::new(
            dir.to_path_buf(),
            PathBuf::from("/tmp/build"),
            PathBuf::from("/tmp/output"),
        );
        let count = sys.load_all_recipes().unwrap();
        assert_eq!(count, 2);
        assert!(sys.get_recipe("hello").is_some());
        assert!(sys.get_recipe("openssl").is_some()); // FULL_RECIPE has name "openssl"
    }
}
