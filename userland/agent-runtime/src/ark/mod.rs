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
//!
//! Submodules:
//! - **types**: All types, enums, and data structures
//! - **tests**: Unit tests

pub mod types;

#[cfg(test)]
mod tests;

// Re-export all public types so external consumers see the same flat API
// as they did when this was a single ark.rs file.
pub use types::{
    ArkCommand, ArkConfig, ArkOutput, ArkOutputLine, ArkResult, InstallPlan, InstallStep,
    IntegrityIssue, IntegrityIssueType, PackageDb, PackageDbEntry, PlanExecutionResult, StepResult,
    Transaction, TransactionId, TransactionLog, TransactionOp, TransactionOpStatus,
    TransactionOpType, TransactionStatus, DEFAULT_TRANSACTION_LOG_PATH,
};

use anyhow::{bail, Context, Result};
use tracing::{debug, info, warn};

use crate::nous::{
    AvailableUpdate, InstalledPackage, NousResolver, PackageSource, ResolvedPackage,
};

/// Ark version string.
pub const ARK_VERSION: &str = "0.1.0";

// ---------------------------------------------------------------------------
// ArkPackageManager — the main engine
// ---------------------------------------------------------------------------

/// The main ark package management engine.
pub struct ArkPackageManager {
    pub(crate) config: ArkConfig,
    resolver: NousResolver,
}

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
            ArkCommand::GroupInstall { group, force } => {
                let meta = group_meta_package(group).unwrap_or_else(|| {
                    warn!(group = %group, "Unknown group, trying as package name");
                    // Fall back to treating the group name as a package name
                    // (e.g., `ark install --group agnos-desktop` works too)
                    group.as_str()
                });
                info!(group = %group, meta_package = %meta, "Installing package group");
                self.install(&[meta.to_string()], *force)
            }
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
                PackageSource::Community => {
                    // Community packages are built locally via takumi, installed like marketplace.
                    plan.steps.push(InstallStep::MarketplaceInstall {
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
                        PackageSource::Community => {
                            plan.steps.push(InstallStep::MarketplaceInstall {
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
                        PackageSource::Community => {
                            plan.steps
                                .push(InstallStep::MarketplaceRemove { package: pkg.name });
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
            let mut group: Option<String> = None;
            let mut packages = Vec::new();
            let mut i = 0;
            while i < rest.len() {
                match rest[i] {
                    "--force" | "-f" => force = true,
                    "--group" | "-g" => {
                        i += 1;
                        if i >= rest.len() {
                            bail!("--group requires a group name (e.g., desktop, ai, edge)");
                        }
                        group = Some(rest[i].to_string());
                    }
                    arg if !arg.starts_with('-') => packages.push(arg.to_string()),
                    arg => bail!("Unknown flag for install: {}", arg),
                }
                i += 1;
            }
            if let Some(g) = group {
                Ok(ArkCommand::GroupInstall { group: g, force })
            } else {
                if packages.is_empty() {
                    bail!("install requires at least one package name or --group <name>");
                }
                Ok(ArkCommand::Install { packages, force })
            }
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

/// Well-known package groups and their meta-package mappings.
/// `ark install --group <name>` resolves to installing the meta-package.
pub fn group_meta_package(group: &str) -> Option<&'static str> {
    match group {
        "desktop" => Some("agnos-desktop"),
        "ai" | "ml" => Some("agnos-ai"),
        "edge" | "iot" => Some("agnos-edge-agent"),
        _ => None,
    }
}
