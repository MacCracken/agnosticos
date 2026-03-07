//! Ark — Unified Package Manager CLI for AGNOS
//!
//! `ark` is the user-facing CLI interface for AGNOS package management. It
//! translates user commands into operations, using the `nous` resolver to
//! figure out where packages come from (system apt, marketplace agents, or
//! Flutter app bundles), then produces execution plans that callers (HTTP API,
//! CLI binary) can run with appropriate permissions via `agnos-sudo`.
//!
//! Ark does **not** directly execute `apt-get` or `dpkg`. It generates
//! [`InstallPlan`] instructions — a deliberate security design choice.

use std::fmt;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::nous::{
    AvailableUpdate, InstalledPackage, NousResolver, PackageSource, ResolutionStrategy,
    ResolvedPackage,
};

// ---------------------------------------------------------------------------
// Ark CLI command
// ---------------------------------------------------------------------------

/// An ark command parsed from CLI args.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArkCommand {
    /// Install one or more packages.
    Install { packages: Vec<String>, force: bool },
    /// Remove/uninstall packages.
    Remove { packages: Vec<String>, purge: bool },
    /// Search across all sources.
    Search {
        query: String,
        source: Option<PackageSource>,
    },
    /// List installed packages.
    List { source: Option<PackageSource> },
    /// Show detailed info about a package.
    Info { package: String },
    /// Check for updates.
    Update,
    /// Upgrade packages with available updates.
    Upgrade { packages: Option<Vec<String>> },
    /// Show ark version and status.
    Status,
}

// ---------------------------------------------------------------------------
// Ark result and output types
// ---------------------------------------------------------------------------

/// Result of an ark operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArkResult {
    pub success: bool,
    pub message: String,
    pub packages_affected: Vec<String>,
    pub source: PackageSource,
}

/// Formatted output for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArkOutput {
    pub lines: Vec<ArkOutputLine>,
}

/// A single line of formatted ark output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArkOutputLine {
    Header(String),
    Package {
        name: String,
        version: String,
        source: PackageSource,
        description: String,
    },
    Info {
        key: String,
        value: String,
    },
    Separator,
    Success(String),
    Error(String),
    Warning(String),
}

impl ArkOutput {
    /// Create an empty output.
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    /// Format the output as a human-readable string.
    pub fn to_display_string(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            match line {
                ArkOutputLine::Header(s) => {
                    out.push_str(&format!("=== {} ===\n", s));
                }
                ArkOutputLine::Package {
                    name,
                    version,
                    source,
                    description,
                } => {
                    out.push_str(&format!(
                        "  {} ({}) [{}] -- {}\n",
                        name, version, source, description
                    ));
                }
                ArkOutputLine::Info { key, value } => {
                    out.push_str(&format!("  {}: {}\n", key, value));
                }
                ArkOutputLine::Separator => {
                    out.push_str("---\n");
                }
                ArkOutputLine::Success(s) => {
                    out.push_str(&format!("OK: {}\n", s));
                }
                ArkOutputLine::Error(s) => {
                    out.push_str(&format!("ERROR: {}\n", s));
                }
                ArkOutputLine::Warning(s) => {
                    out.push_str(&format!("WARN: {}\n", s));
                }
            }
        }
        out
    }
}

impl Default for ArkOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ArkOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_display_string())
    }
}

// ---------------------------------------------------------------------------
// Install plan
// ---------------------------------------------------------------------------

/// A plan for what ark wants to execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallPlan {
    pub steps: Vec<InstallStep>,
    pub requires_root: bool,
    pub estimated_size_bytes: u64,
}

impl InstallPlan {
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            requires_root: false,
            estimated_size_bytes: 0,
        }
    }
}

impl Default for InstallPlan {
    fn default() -> Self {
        Self::new()
    }
}

/// A single step in an install plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstallStep {
    SystemInstall {
        package: String,
        version: Option<String>,
    },
    SystemRemove {
        package: String,
        purge: bool,
    },
    MarketplaceInstall {
        package: String,
        version: Option<String>,
    },
    MarketplaceRemove {
        package: String,
    },
    FlutterInstall {
        package: String,
        version: Option<String>,
    },
    FlutterRemove {
        package: String,
    },
    /// Run `apt-get update` to refresh system package lists.
    SystemUpdate,
}

// ---------------------------------------------------------------------------
// Ark configuration
// ---------------------------------------------------------------------------

/// Configuration for ark behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArkConfig {
    /// Default resolution strategy when source is ambiguous.
    pub default_strategy: ResolutionStrategy,
    /// Require confirmation for apt installs (default: true).
    pub confirm_system_installs: bool,
    /// Require confirmation for removals (default: true).
    pub confirm_removals: bool,
    /// Check for updates on search (default: false).
    pub auto_update_check: bool,
    /// ANSI colors in output (default: true).
    pub color_output: bool,
    /// Marketplace package storage directory.
    pub marketplace_dir: PathBuf,
    /// Cache directory for ark metadata.
    pub cache_dir: PathBuf,
}

impl Default for ArkConfig {
    fn default() -> Self {
        Self {
            default_strategy: ResolutionStrategy::SystemFirst,
            confirm_system_installs: true,
            confirm_removals: true,
            auto_update_check: false,
            color_output: true,
            marketplace_dir: PathBuf::from("/var/lib/agnos/marketplace"),
            cache_dir: PathBuf::from("/var/cache/agnos/ark"),
        }
    }
}

// ---------------------------------------------------------------------------
// ArkPackageManager — the main engine
// ---------------------------------------------------------------------------

/// The main ark package management engine.
pub struct ArkPackageManager {
    config: ArkConfig,
    resolver: NousResolver,
}

/// Ark version string.
pub const ARK_VERSION: &str = "0.1.0";

impl ArkPackageManager {
    /// Create a new package manager with the given configuration.
    pub fn new(config: ArkConfig) -> Result<Self> {
        let resolver = NousResolver::new(&config.marketplace_dir, &config.cache_dir)
            .with_strategy(config.default_strategy.clone());
        Ok(Self { config, resolver })
    }

    /// Main dispatch — execute an `ArkCommand` and return its result.
    pub fn execute(&self, command: &ArkCommand) -> Result<ArkResult> {
        match command {
            ArkCommand::Install { packages, force } => self.install(packages, *force),
            ArkCommand::Remove { packages, purge } => self.remove(packages, *purge),
            ArkCommand::Search { query, source } => {
                let output = self.search(query, source.as_ref())?;
                Ok(ArkResult {
                    success: true,
                    message: output.to_display_string(),
                    packages_affected: Vec::new(),
                    source: PackageSource::Unknown,
                })
            }
            ArkCommand::List { source } => {
                let output = self.list(source.as_ref())?;
                Ok(ArkResult {
                    success: true,
                    message: output.to_display_string(),
                    packages_affected: Vec::new(),
                    source: PackageSource::Unknown,
                })
            }
            ArkCommand::Info { package } => {
                let output = self.info(package)?;
                Ok(ArkResult {
                    success: true,
                    message: output.to_display_string(),
                    packages_affected: vec![package.clone()],
                    source: PackageSource::Unknown,
                })
            }
            ArkCommand::Update => {
                let output = self.update()?;
                Ok(ArkResult {
                    success: true,
                    message: output.to_display_string(),
                    packages_affected: Vec::new(),
                    source: PackageSource::Unknown,
                })
            }
            ArkCommand::Upgrade { packages } => self.upgrade(packages.as_deref()),
            ArkCommand::Status => {
                let output = self.status();
                Ok(ArkResult {
                    success: true,
                    message: output.to_display_string(),
                    packages_affected: Vec::new(),
                    source: PackageSource::Unknown,
                })
            }
        }
    }

    /// Install packages — resolve each, generate plan, return unified result.
    pub fn install(&self, packages: &[String], force: bool) -> Result<ArkResult> {
        if packages.is_empty() {
            bail!("No packages specified for installation");
        }

        let plan = self.plan_install(packages)?;
        let affected: Vec<String> = packages.to_vec();
        let primary_source = plan
            .steps
            .first()
            .map(|s| match s {
                InstallStep::SystemInstall { .. } => PackageSource::System,
                InstallStep::MarketplaceInstall { .. } => PackageSource::Marketplace,
                InstallStep::FlutterInstall { .. } => PackageSource::FlutterApp,
                _ => PackageSource::Unknown,
            })
            .unwrap_or(PackageSource::Unknown);

        let step_summary: Vec<String> = plan
            .steps
            .iter()
            .map(|s| match s {
                InstallStep::SystemInstall { package, .. } => {
                    format!("apt-get install -y {}", package)
                }
                InstallStep::MarketplaceInstall { package, .. } => {
                    format!("marketplace install {}", package)
                }
                InstallStep::FlutterInstall { package, .. } => {
                    format!("agpkg install {}", package)
                }
                _ => String::new(),
            })
            .filter(|s| !s.is_empty())
            .collect();

        info!(
            packages = ?affected,
            force,
            requires_root = plan.requires_root,
            "Install plan generated"
        );

        Ok(ArkResult {
            success: true,
            message: format!(
                "Install plan ({} steps): {}",
                step_summary.len(),
                step_summary.join("; ")
            ),
            packages_affected: affected,
            source: primary_source,
        })
    }

    /// Remove packages — detect source and generate removal commands.
    pub fn remove(&self, packages: &[String], purge: bool) -> Result<ArkResult> {
        if packages.is_empty() {
            bail!("No packages specified for removal");
        }

        let plan = self.plan_remove(packages, purge)?;
        let affected: Vec<String> = packages.to_vec();
        let primary_source = plan
            .steps
            .first()
            .map(|s| match s {
                InstallStep::SystemRemove { .. } => PackageSource::System,
                InstallStep::MarketplaceRemove { .. } => PackageSource::Marketplace,
                InstallStep::FlutterRemove { .. } => PackageSource::FlutterApp,
                _ => PackageSource::Unknown,
            })
            .unwrap_or(PackageSource::Unknown);

        info!(
            packages = ?affected,
            purge,
            requires_root = plan.requires_root,
            "Remove plan generated"
        );

        Ok(ArkResult {
            success: true,
            message: format!("Remove plan ({} steps)", plan.steps.len()),
            packages_affected: affected,
            source: primary_source,
        })
    }

    /// Search across all sources, optionally filtering by source.
    pub fn search(&self, query: &str, source: Option<&PackageSource>) -> Result<ArkOutput> {
        let search_result = self.resolver.search(query)?;

        let mut output = ArkOutput::new();
        output
            .lines
            .push(ArkOutputLine::Header(format!("Search: {}", query)));

        let filtered: Vec<&ResolvedPackage> = if let Some(src) = source {
            search_result
                .results
                .iter()
                .filter(|r| r.source == *src)
                .collect()
        } else {
            search_result.results.iter().collect()
        };

        if filtered.is_empty() {
            output
                .lines
                .push(ArkOutputLine::Warning("No packages found".to_string()));
        } else {
            for result in &filtered {
                output.lines.push(ArkOutputLine::Package {
                    name: result.name.clone(),
                    version: result.version.clone(),
                    source: result.source.clone(),
                    description: result.description.clone(),
                });
            }
        }

        output.lines.push(ArkOutputLine::Separator);
        output.lines.push(ArkOutputLine::Info {
            key: "Total".to_string(),
            value: format!("{} result(s)", filtered.len()),
        });

        Ok(output)
    }

    /// List installed packages, optionally filtered by source.
    pub fn list(&self, source: Option<&PackageSource>) -> Result<ArkOutput> {
        let packages = self.resolver.list_installed()?;

        let filtered: Vec<&InstalledPackage> = if let Some(src) = source {
            packages.iter().filter(|p| p.source == *src).collect()
        } else {
            packages.iter().collect()
        };

        let mut output = ArkOutput::new();
        let header = if let Some(src) = source {
            format!("Installed packages [{}]", src)
        } else {
            "Installed packages".to_string()
        };
        output.lines.push(ArkOutputLine::Header(header));

        if filtered.is_empty() {
            output
                .lines
                .push(ArkOutputLine::Warning("No packages installed".to_string()));
        } else {
            for pkg in &filtered {
                let size_info = pkg
                    .size_bytes
                    .map(|s| format!(" ({} bytes)", s))
                    .unwrap_or_default();
                output.lines.push(ArkOutputLine::Package {
                    name: pkg.name.clone(),
                    version: pkg.version.clone(),
                    source: pkg.source.clone(),
                    description: size_info,
                });
            }
        }

        output.lines.push(ArkOutputLine::Separator);
        output.lines.push(ArkOutputLine::Info {
            key: "Total".to_string(),
            value: format!("{} package(s)", filtered.len()),
        });

        Ok(output)
    }

    /// Show detailed info about a package.
    pub fn info(&self, package: &str) -> Result<ArkOutput> {
        let resolved = self
            .resolver
            .resolve(package)
            .with_context(|| format!("Failed to resolve package: {}", package))?;

        let mut output = ArkOutput::new();
        output
            .lines
            .push(ArkOutputLine::Header(format!("Package: {}", package)));

        match resolved {
            Some(pkg) => {
                output.lines.push(ArkOutputLine::Info {
                    key: "Name".to_string(),
                    value: pkg.name.clone(),
                });
                output.lines.push(ArkOutputLine::Info {
                    key: "Version".to_string(),
                    value: pkg.version.clone(),
                });
                output.lines.push(ArkOutputLine::Info {
                    key: "Source".to_string(),
                    value: pkg.source.to_string(),
                });
                output.lines.push(ArkOutputLine::Info {
                    key: "Description".to_string(),
                    value: pkg.description.clone(),
                });
                if let Some(size) = pkg.size_bytes {
                    output.lines.push(ArkOutputLine::Info {
                        key: "Size".to_string(),
                        value: format!("{} bytes", size),
                    });
                }
                if !pkg.dependencies.is_empty() {
                    output.lines.push(ArkOutputLine::Info {
                        key: "Dependencies".to_string(),
                        value: pkg.dependencies.join(", "),
                    });
                }
                output.lines.push(ArkOutputLine::Info {
                    key: "Trusted".to_string(),
                    value: format!("{}", pkg.trusted),
                });
            }
            None => {
                output.lines.push(ArkOutputLine::Warning(format!(
                    "Package '{}' not found in any source",
                    package
                )));
            }
        }

        Ok(output)
    }

    /// Check all sources for updates.
    pub fn update(&self) -> Result<ArkOutput> {
        let updates = self.resolver.check_updates()?;

        let mut output = ArkOutput::new();
        output
            .lines
            .push(ArkOutputLine::Header("Update check".to_string()));

        if updates.is_empty() {
            output.lines.push(ArkOutputLine::Success(
                "All packages up to date".to_string(),
            ));
        } else {
            for update in &updates {
                output.lines.push(ArkOutputLine::Package {
                    name: update.name.clone(),
                    version: format!(
                        "{} -> {}",
                        update.installed_version, update.available_version
                    ),
                    source: update.source.clone(),
                    description: update
                        .changelog
                        .clone()
                        .unwrap_or_else(|| "Update available".to_string()),
                });
            }
        }

        output.lines.push(ArkOutputLine::Separator);
        output.lines.push(ArkOutputLine::Info {
            key: "Updates available".to_string(),
            value: format!("{}", updates.len()),
        });

        Ok(output)
    }

    /// Generate upgrade commands for packages with available updates.
    pub fn upgrade(&self, packages: Option<&[String]>) -> Result<ArkResult> {
        let updates = self.resolver.check_updates()?;

        let filtered: Vec<&AvailableUpdate> = if let Some(names) = packages {
            updates.iter().filter(|u| names.contains(&u.name)).collect()
        } else {
            updates.iter().collect()
        };

        let affected: Vec<String> = filtered.iter().map(|u| u.name.clone()).collect();

        let mut plan = InstallPlan::new();
        for update in &filtered {
            match &update.source {
                PackageSource::System => {
                    plan.steps.push(InstallStep::SystemInstall {
                        package: update.name.clone(),
                        version: Some(update.available_version.clone()),
                    });
                    plan.requires_root = true;
                }
                PackageSource::Marketplace => {
                    plan.steps.push(InstallStep::MarketplaceInstall {
                        package: update.name.clone(),
                        version: Some(update.available_version.clone()),
                    });
                }
                PackageSource::FlutterApp => {
                    plan.steps.push(InstallStep::FlutterInstall {
                        package: update.name.clone(),
                        version: Some(update.available_version.clone()),
                    });
                }
                PackageSource::Unknown => {
                    warn!(package = %update.name, "Cannot upgrade package with unknown source");
                }
            }
        }

        Ok(ArkResult {
            success: true,
            message: format!("Upgrade plan: {} package(s) to upgrade", filtered.len()),
            packages_affected: affected,
            source: PackageSource::Unknown,
        })
    }

    /// Show ark version, available sources, and package counts.
    pub fn status(&self) -> ArkOutput {
        let mut output = ArkOutput::new();
        output
            .lines
            .push(ArkOutputLine::Header("ark status".to_string()));
        output.lines.push(ArkOutputLine::Info {
            key: "Version".to_string(),
            value: ARK_VERSION.to_string(),
        });
        output.lines.push(ArkOutputLine::Info {
            key: "Strategy".to_string(),
            value: format!("{:?}", self.config.default_strategy),
        });
        output.lines.push(ArkOutputLine::Info {
            key: "Marketplace dir".to_string(),
            value: self.config.marketplace_dir.display().to_string(),
        });
        output.lines.push(ArkOutputLine::Info {
            key: "Cache dir".to_string(),
            value: self.config.cache_dir.display().to_string(),
        });
        output.lines.push(ArkOutputLine::Separator);

        // Source availability
        output.lines.push(ArkOutputLine::Info {
            key: "Sources".to_string(),
            value: "system (apt), marketplace, flutter".to_string(),
        });

        // Package counts from installed list
        let installed = self.resolver.list_installed().unwrap_or_default();
        let marketplace_count = installed
            .iter()
            .filter(|p| p.source == PackageSource::Marketplace)
            .count();
        let system_count = installed
            .iter()
            .filter(|p| p.source == PackageSource::System)
            .count();
        let flutter_count = installed
            .iter()
            .filter(|p| p.source == PackageSource::FlutterApp)
            .count();

        output.lines.push(ArkOutputLine::Info {
            key: "System packages".to_string(),
            value: format!("{}", system_count),
        });
        output.lines.push(ArkOutputLine::Info {
            key: "Marketplace packages".to_string(),
            value: format!("{}", marketplace_count),
        });
        output.lines.push(ArkOutputLine::Info {
            key: "Flutter apps".to_string(),
            value: format!("{}", flutter_count),
        });

        output
            .lines
            .push(ArkOutputLine::Success("ark is operational".to_string()));

        output
    }

    /// Create an install plan without executing anything.
    pub fn plan_install(&self, packages: &[String]) -> Result<InstallPlan> {
        let mut plan = InstallPlan::new();

        for pkg_name in packages {
            let resolved = self
                .resolver
                .resolve(pkg_name)
                .with_context(|| format!("Failed to resolve package: {}", pkg_name))?;

            match resolved {
                Some(pkg) => {
                    if let Some(size) = pkg.size_bytes {
                        plan.estimated_size_bytes += size;
                    }
                    match &pkg.source {
                        PackageSource::System => {
                            plan.steps.push(InstallStep::SystemInstall {
                                package: pkg.name,
                                version: if pkg.version == "latest" || pkg.version.is_empty() {
                                    None
                                } else {
                                    Some(pkg.version)
                                },
                            });
                            plan.requires_root = true;
                        }
                        PackageSource::Marketplace => {
                            plan.steps.push(InstallStep::MarketplaceInstall {
                                package: pkg.name,
                                version: if pkg.version == "latest" || pkg.version.is_empty() {
                                    None
                                } else {
                                    Some(pkg.version)
                                },
                            });
                        }
                        PackageSource::FlutterApp => {
                            plan.steps.push(InstallStep::FlutterInstall {
                                package: pkg.name,
                                version: if pkg.version == "latest" || pkg.version.is_empty() {
                                    None
                                } else {
                                    Some(pkg.version)
                                },
                            });
                        }
                        PackageSource::Unknown => {
                            warn!(package = %pkg_name, "Could not determine source");
                            bail!("Cannot determine source for package: {}", pkg_name);
                        }
                    }
                }
                None => {
                    bail!("Package not found: {}", pkg_name);
                }
            }
        }

        debug!(steps = plan.steps.len(), "Install plan created");
        Ok(plan)
    }

    /// Create a removal plan without executing anything.
    pub fn plan_remove(&self, packages: &[String], purge: bool) -> Result<InstallPlan> {
        let mut plan = InstallPlan::new();

        for pkg_name in packages {
            // For removal, check if the package is installed in marketplace first,
            // then fall back to system
            if self.resolver.is_marketplace_package(pkg_name) {
                plan.steps.push(InstallStep::MarketplaceRemove {
                    package: pkg_name.clone(),
                });
            } else if self.resolver.is_system_package(pkg_name) {
                plan.steps.push(InstallStep::SystemRemove {
                    package: pkg_name.clone(),
                    purge,
                });
                plan.requires_root = true;
            } else {
                // Try resolution as fallback
                let resolved = self.resolver.resolve(pkg_name)?;
                match resolved {
                    Some(pkg) => match &pkg.source {
                        PackageSource::System => {
                            plan.steps.push(InstallStep::SystemRemove {
                                package: pkg.name,
                                purge,
                            });
                            plan.requires_root = true;
                        }
                        PackageSource::Marketplace => {
                            plan.steps
                                .push(InstallStep::MarketplaceRemove { package: pkg.name });
                        }
                        PackageSource::FlutterApp => {
                            plan.steps
                                .push(InstallStep::FlutterRemove { package: pkg.name });
                        }
                        PackageSource::Unknown => {
                            bail!("Cannot determine source for package: {}", pkg_name);
                        }
                    },
                    None => {
                        bail!("Package not found for removal: {}", pkg_name);
                    }
                }
            }
        }

        debug!(steps = plan.steps.len(), purge, "Remove plan created");
        Ok(plan)
    }

    /// Format an install plan as human-readable output.
    pub fn format_plan(plan: &InstallPlan) -> ArkOutput {
        let mut output = ArkOutput::new();
        output
            .lines
            .push(ArkOutputLine::Header("Execution plan".to_string()));

        if plan.steps.is_empty() {
            output
                .lines
                .push(ArkOutputLine::Warning("No steps in plan".to_string()));
            return output;
        }

        for (i, step) in plan.steps.iter().enumerate() {
            let desc = match step {
                InstallStep::SystemInstall { package, version } => {
                    let ver = version.as_deref().unwrap_or("latest");
                    format!("{}. apt-get install {} ({})", i + 1, package, ver)
                }
                InstallStep::SystemRemove { package, purge } => {
                    let cmd = if *purge { "purge" } else { "remove" };
                    format!("{}. apt-get {} {}", i + 1, cmd, package)
                }
                InstallStep::MarketplaceInstall { package, version } => {
                    let ver = version.as_deref().unwrap_or("latest");
                    format!("{}. marketplace install {} ({})", i + 1, package, ver)
                }
                InstallStep::MarketplaceRemove { package } => {
                    format!("{}. marketplace remove {}", i + 1, package)
                }
                InstallStep::FlutterInstall { package, version } => {
                    let ver = version.as_deref().unwrap_or("latest");
                    format!("{}. agpkg install {} ({})", i + 1, package, ver)
                }
                InstallStep::FlutterRemove { package } => {
                    format!("{}. agpkg remove {}", i + 1, package)
                }
                InstallStep::SystemUpdate => {
                    format!("{}. apt-get update", i + 1)
                }
            };
            output.lines.push(ArkOutputLine::Info {
                key: "Step".to_string(),
                value: desc,
            });
        }

        output.lines.push(ArkOutputLine::Separator);
        output.lines.push(ArkOutputLine::Info {
            key: "Requires root".to_string(),
            value: format!("{}", plan.requires_root),
        });
        output.lines.push(ArkOutputLine::Info {
            key: "Estimated size".to_string(),
            value: format!("{} bytes", plan.estimated_size_bytes),
        });

        output
    }
}

// ---------------------------------------------------------------------------
// CLI argument parsing
// ---------------------------------------------------------------------------

/// Parse CLI-style args into an `ArkCommand`.
///
/// # Examples
///
/// ```text
/// ["install", "nginx"]           -> Install { packages: ["nginx"], force: false }
/// ["install", "--force", "curl"] -> Install { packages: ["curl"], force: true }
/// ["remove", "--purge", "nginx"] -> Remove { packages: ["nginx"], purge: true }
/// ["search", "web server"]       -> Search { query: "web server", source: None }
/// ["status"]                     -> Status
/// ```
pub fn parse_args(args: &[&str]) -> Result<ArkCommand> {
    if args.is_empty() {
        bail!("No command specified. Usage: ark <command> [options] [packages...]");
    }

    let command = args[0];
    let rest = &args[1..];

    match command {
        "install" => {
            let mut force = false;
            let mut packages = Vec::new();
            for &arg in rest {
                if arg == "--force" || arg == "-f" {
                    force = true;
                } else if !arg.starts_with('-') {
                    packages.push(arg.to_string());
                } else {
                    bail!("Unknown flag for install: {}", arg);
                }
            }
            if packages.is_empty() {
                bail!("install requires at least one package name");
            }
            Ok(ArkCommand::Install { packages, force })
        }

        "remove" | "uninstall" => {
            let mut purge = false;
            let mut packages = Vec::new();
            for &arg in rest {
                if arg == "--purge" {
                    purge = true;
                } else if !arg.starts_with('-') {
                    packages.push(arg.to_string());
                } else {
                    bail!("Unknown flag for remove: {}", arg);
                }
            }
            if packages.is_empty() {
                bail!("remove requires at least one package name");
            }
            Ok(ArkCommand::Remove { packages, purge })
        }

        "search" => {
            let mut source: Option<PackageSource> = None;
            let mut query_parts = Vec::new();
            let mut i = 0;
            while i < rest.len() {
                if rest[i] == "--source" || rest[i] == "-s" {
                    i += 1;
                    if i >= rest.len() {
                        bail!("--source requires a value (system, marketplace, flutter)");
                    }
                    source = Some(parse_source_arg(rest[i])?);
                } else if !rest[i].starts_with('-') {
                    query_parts.push(rest[i]);
                } else {
                    bail!("Unknown flag for search: {}", rest[i]);
                }
                i += 1;
            }
            if query_parts.is_empty() {
                bail!("search requires a query string");
            }
            Ok(ArkCommand::Search {
                query: query_parts.join(" "),
                source,
            })
        }

        "list" | "ls" => {
            let mut source: Option<PackageSource> = None;
            for &arg in rest {
                match arg {
                    "--marketplace" | "--market" => source = Some(PackageSource::Marketplace),
                    "--system" | "--apt" => source = Some(PackageSource::System),
                    "--flutter" => source = Some(PackageSource::FlutterApp),
                    _ if arg.starts_with('-') => bail!("Unknown flag for list: {}", arg),
                    _ => bail!("Unexpected argument for list: {}", arg),
                }
            }
            Ok(ArkCommand::List { source })
        }

        "info" | "show" => {
            if rest.is_empty() {
                bail!("info requires a package name");
            }
            if rest.len() > 1 {
                bail!("info accepts only one package name");
            }
            Ok(ArkCommand::Info {
                package: rest[0].to_string(),
            })
        }

        "update" => Ok(ArkCommand::Update),

        "upgrade" => {
            let packages: Vec<String> = rest
                .iter()
                .filter(|a| !a.starts_with('-'))
                .map(|a| a.to_string())
                .collect();
            Ok(ArkCommand::Upgrade {
                packages: if packages.is_empty() {
                    None
                } else {
                    Some(packages)
                },
            })
        }

        "status" => Ok(ArkCommand::Status),

        _ => bail!(
            "Unknown command: {}. Available: install, remove, search, list, info, update, upgrade, status",
            command
        ),
    }
}

/// Parse a source argument string into a `PackageSource`.
fn parse_source_arg(s: &str) -> Result<PackageSource> {
    match s.to_lowercase().as_str() {
        "system" | "apt" => Ok(PackageSource::System),
        "marketplace" | "market" => Ok(PackageSource::Marketplace),
        "flutter" | "flutter-app" | "flutterapp" => Ok(PackageSource::FlutterApp),
        _ => bail!(
            "Unknown package source: '{}'. Use: system, marketplace, flutter",
            s
        ),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // -- parse_args tests --

    #[test]
    fn test_parse_install_single() {
        let cmd = parse_args(&["install", "nginx"]).unwrap();
        assert_eq!(
            cmd,
            ArkCommand::Install {
                packages: vec!["nginx".to_string()],
                force: false,
            }
        );
    }

    #[test]
    fn test_parse_install_multiple() {
        let cmd = parse_args(&["install", "nginx", "curl"]).unwrap();
        assert_eq!(
            cmd,
            ArkCommand::Install {
                packages: vec!["nginx".to_string(), "curl".to_string()],
                force: false,
            }
        );
    }

    #[test]
    fn test_parse_install_force() {
        let cmd = parse_args(&["install", "--force", "nginx", "curl"]).unwrap();
        assert_eq!(
            cmd,
            ArkCommand::Install {
                packages: vec!["nginx".to_string(), "curl".to_string()],
                force: true,
            }
        );
    }

    #[test]
    fn test_parse_remove_basic() {
        let cmd = parse_args(&["remove", "nginx"]).unwrap();
        assert_eq!(
            cmd,
            ArkCommand::Remove {
                packages: vec!["nginx".to_string()],
                purge: false,
            }
        );
    }

    #[test]
    fn test_parse_remove_purge() {
        let cmd = parse_args(&["remove", "--purge", "nginx"]).unwrap();
        assert_eq!(
            cmd,
            ArkCommand::Remove {
                packages: vec!["nginx".to_string()],
                purge: true,
            }
        );
    }

    #[test]
    fn test_parse_search_query() {
        let cmd = parse_args(&["search", "web", "server"]).unwrap();
        assert_eq!(
            cmd,
            ArkCommand::Search {
                query: "web server".to_string(),
                source: None,
            }
        );
    }

    #[test]
    fn test_parse_search_with_source() {
        let cmd = parse_args(&["search", "--source", "system", "web", "server"]).unwrap();
        assert_eq!(
            cmd,
            ArkCommand::Search {
                query: "web server".to_string(),
                source: Some(PackageSource::System),
            }
        );
    }

    #[test]
    fn test_parse_list_all() {
        let cmd = parse_args(&["list"]).unwrap();
        assert_eq!(cmd, ArkCommand::List { source: None });
    }

    #[test]
    fn test_parse_list_marketplace() {
        let cmd = parse_args(&["list", "--marketplace"]).unwrap();
        assert_eq!(
            cmd,
            ArkCommand::List {
                source: Some(PackageSource::Marketplace),
            }
        );
    }

    #[test]
    fn test_parse_info() {
        let cmd = parse_args(&["info", "nginx"]).unwrap();
        assert_eq!(
            cmd,
            ArkCommand::Info {
                package: "nginx".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_update() {
        let cmd = parse_args(&["update"]).unwrap();
        assert_eq!(cmd, ArkCommand::Update);
    }

    #[test]
    fn test_parse_upgrade_all() {
        let cmd = parse_args(&["upgrade"]).unwrap();
        assert_eq!(cmd, ArkCommand::Upgrade { packages: None });
    }

    #[test]
    fn test_parse_upgrade_specific() {
        let cmd = parse_args(&["upgrade", "nginx"]).unwrap();
        assert_eq!(
            cmd,
            ArkCommand::Upgrade {
                packages: Some(vec!["nginx".to_string()]),
            }
        );
    }

    #[test]
    fn test_parse_status() {
        let cmd = parse_args(&["status"]).unwrap();
        assert_eq!(cmd, ArkCommand::Status);
    }

    #[test]
    fn test_parse_empty_args() {
        assert!(parse_args(&[]).is_err());
    }

    #[test]
    fn test_parse_unknown_command() {
        assert!(parse_args(&["frobnicate"]).is_err());
    }

    // -- Config and construction tests --

    #[test]
    fn test_ark_config_defaults() {
        let config = ArkConfig::default();
        assert_eq!(config.default_strategy, ResolutionStrategy::SystemFirst);
        assert!(config.confirm_system_installs);
        assert!(config.confirm_removals);
        assert!(!config.auto_update_check);
        assert!(config.color_output);
        assert_eq!(
            config.marketplace_dir,
            PathBuf::from("/var/lib/agnos/marketplace")
        );
        assert_eq!(config.cache_dir, PathBuf::from("/var/cache/agnos/ark"));
    }

    #[test]
    fn test_ark_package_manager_new() {
        let tmp = TempDir::new().unwrap();
        let config = ArkConfig {
            marketplace_dir: tmp.path().to_path_buf(),
            cache_dir: tmp.path().join("cache"),
            ..ArkConfig::default()
        };
        let mgr = ArkPackageManager::new(config).unwrap();
        assert_eq!(mgr.config.default_strategy, ResolutionStrategy::SystemFirst);
    }

    // -- Plan and execution tests --

    #[test]
    fn test_plan_install_marketplace() {
        let tmp = TempDir::new().unwrap();
        let marketplace_dir = tmp.path().to_path_buf();

        // Write an index.json that the LocalRegistry (used by nous) will load.
        // This simulates a marketplace package being installed.
        let index_json = serde_json::json!({
            "test-agent": {
                "manifest": {
                    "name": "test-agent",
                    "description": "A test marketplace agent",
                    "version": "1.0.0",
                    "publisher": {
                        "name": "Test",
                        "key_id": "abc12345",
                        "homepage": ""
                    },
                    "category": "Utility",
                    "runtime": "native",
                    "screenshots": [],
                    "changelog": "",
                    "min_agnos_version": "",
                    "dependencies": {},
                    "tags": []
                },
                "installed_at": "2026-03-06T00:00:00Z",
                "install_dir": "/tmp/test-agent",
                "package_hash": "fakehash",
                "auto_update": false,
                "installed_size": 1024
            }
        });
        std::fs::write(
            marketplace_dir.join("index.json"),
            serde_json::to_string_pretty(&index_json).unwrap(),
        )
        .unwrap();

        let config = ArkConfig {
            marketplace_dir: marketplace_dir.clone(),
            cache_dir: tmp.path().join("cache"),
            ..ArkConfig::default()
        };
        let mgr = ArkPackageManager::new(config).unwrap();

        let plan = mgr.plan_install(&["test-agent".to_string()]);
        assert!(plan.is_ok(), "plan_install failed: {:?}", plan.err());
        let plan = plan.unwrap();
        assert!(!plan.steps.is_empty());
        // Should resolve as marketplace
        assert_eq!(
            plan.steps[0],
            InstallStep::MarketplaceInstall {
                package: "test-agent".to_string(),
                version: Some("1.0.0".to_string()),
            }
        );
        assert!(!plan.requires_root);
    }

    #[test]
    fn test_plan_remove_generates_steps() {
        let tmp = TempDir::new().unwrap();
        let marketplace_dir = tmp.path().to_path_buf();

        // Populate marketplace index so nous can find the package for removal
        let index_json = serde_json::json!({
            "my-agent": {
                "manifest": {
                    "name": "my-agent",
                    "description": "Agent to remove",
                    "version": "1.0.0",
                    "publisher": {
                        "name": "Test",
                        "key_id": "abc12345",
                        "homepage": ""
                    },
                    "category": "Utility",
                    "runtime": "native",
                    "screenshots": [],
                    "changelog": "",
                    "min_agnos_version": "",
                    "dependencies": {},
                    "tags": []
                },
                "installed_at": "2026-03-06T00:00:00Z",
                "install_dir": "/tmp/my-agent",
                "package_hash": "fakehash",
                "auto_update": false,
                "installed_size": 512
            }
        });
        std::fs::write(
            marketplace_dir.join("index.json"),
            serde_json::to_string_pretty(&index_json).unwrap(),
        )
        .unwrap();

        let config = ArkConfig {
            marketplace_dir: marketplace_dir.clone(),
            cache_dir: tmp.path().join("cache"),
            ..ArkConfig::default()
        };
        let mgr = ArkPackageManager::new(config).unwrap();
        let plan = mgr.plan_remove(&["my-agent".to_string()], false);
        assert!(plan.is_ok(), "plan_remove failed: {:?}", plan.err());
        let plan = plan.unwrap();
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(
            plan.steps[0],
            InstallStep::MarketplaceRemove {
                package: "my-agent".to_string(),
            }
        );
        assert!(!plan.requires_root);
    }

    #[test]
    fn test_format_plan_output() {
        let plan = InstallPlan {
            steps: vec![
                InstallStep::SystemInstall {
                    package: "nginx".to_string(),
                    version: None,
                },
                InstallStep::MarketplaceInstall {
                    package: "my-agent".to_string(),
                    version: Some("1.0.0".to_string()),
                },
            ],
            requires_root: true,
            estimated_size_bytes: 1024,
        };

        let output = ArkPackageManager::format_plan(&plan);
        let text = output.to_display_string();
        assert!(text.contains("Execution plan"));
        assert!(text.contains("apt-get install nginx"));
        assert!(text.contains("marketplace install my-agent"));
        assert!(text.contains("Requires root"));
        assert!(text.contains("true"));
    }

    #[test]
    fn test_ark_output_formatting() {
        let mut output = ArkOutput::new();
        output
            .lines
            .push(ArkOutputLine::Header("Test Header".to_string()));
        output.lines.push(ArkOutputLine::Package {
            name: "nginx".to_string(),
            version: "1.24.0".to_string(),
            source: PackageSource::System,
            description: "HTTP server".to_string(),
        });
        output.lines.push(ArkOutputLine::Separator);
        output
            .lines
            .push(ArkOutputLine::Success("Done".to_string()));
        output
            .lines
            .push(ArkOutputLine::Error("Something failed".to_string()));
        output
            .lines
            .push(ArkOutputLine::Warning("Caution".to_string()));

        let text = output.to_display_string();
        assert!(text.contains("=== Test Header ==="));
        assert!(text.contains("nginx (1.24.0) [system] -- HTTP server"));
        assert!(text.contains("---"));
        assert!(text.contains("OK: Done"));
        assert!(text.contains("ERROR: Something failed"));
        assert!(text.contains("WARN: Caution"));
    }

    #[test]
    fn test_install_plan_mixed_sources() {
        let plan = InstallPlan {
            steps: vec![
                InstallStep::MarketplaceInstall {
                    package: "my-agent".to_string(),
                    version: None,
                },
                InstallStep::FlutterInstall {
                    package: "my-app".to_string(),
                    version: None,
                },
                InstallStep::SystemInstall {
                    package: "curl".to_string(),
                    version: None,
                },
            ],
            requires_root: true,
            estimated_size_bytes: 0,
        };

        assert_eq!(plan.steps.len(), 3);
        assert_eq!(
            plan.steps[0],
            InstallStep::MarketplaceInstall {
                package: "my-agent".to_string(),
                version: None,
            }
        );
        assert_eq!(
            plan.steps[1],
            InstallStep::FlutterInstall {
                package: "my-app".to_string(),
                version: None,
            }
        );
        assert_eq!(
            plan.steps[2],
            InstallStep::SystemInstall {
                package: "curl".to_string(),
                version: None,
            }
        );
        assert!(plan.requires_root);
    }

    #[test]
    fn test_status_returns_valid_output() {
        let tmp = TempDir::new().unwrap();
        let config = ArkConfig {
            marketplace_dir: tmp.path().to_path_buf(),
            cache_dir: tmp.path().join("cache"),
            ..ArkConfig::default()
        };
        let mgr = ArkPackageManager::new(config).unwrap();
        let output = mgr.status();
        let text = output.to_display_string();

        assert!(text.contains("ark status"));
        assert!(text.contains(ARK_VERSION));
        assert!(text.contains("Sources"));
        assert!(text.contains("ark is operational"));
    }

    // -- PackageSource display (from nous, verify it works through ark) --

    #[test]
    fn test_package_source_display_through_output() {
        let output_line = ArkOutputLine::Package {
            name: "test".to_string(),
            version: "1.0".to_string(),
            source: PackageSource::System,
            description: "test pkg".to_string(),
        };
        let output = ArkOutput {
            lines: vec![output_line],
        };
        let text = output.to_display_string();
        assert!(text.contains("[system]"));

        let output_line = ArkOutputLine::Package {
            name: "test".to_string(),
            version: "1.0".to_string(),
            source: PackageSource::Marketplace,
            description: "test pkg".to_string(),
        };
        let output = ArkOutput {
            lines: vec![output_line],
        };
        let text = output.to_display_string();
        assert!(text.contains("[marketplace]"));

        let output_line = ArkOutputLine::Package {
            name: "test".to_string(),
            version: "1.0".to_string(),
            source: PackageSource::FlutterApp,
            description: "test pkg".to_string(),
        };
        let output = ArkOutput {
            lines: vec![output_line],
        };
        let text = output.to_display_string();
        assert!(text.contains("[flutter-app]"));
    }
}
