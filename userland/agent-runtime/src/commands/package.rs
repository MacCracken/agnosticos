//! Package management command handler.

use std::path::Path;

use anyhow::Result;

use crate::cli::PackageCommands;
use super::truncate;

pub async fn handle_package_command(action: PackageCommands, data_dir: &Path) -> Result<()> {
    let pkg_dir = data_dir.join("packages");
    let mut mgr = crate::package_manager::PackageManager::new(&pkg_dir)?;

    match action {
        PackageCommands::Install { source } => {
            // Validate first
            let package = mgr.validate_package(&source)?;

            // Show consent prompt
            println!("{}", crate::package_manager::consent_prompt(&package));
            println!();

            // Install
            let result = mgr.install(&source)?;
            if let Some(ref prev) = result.upgraded_from {
                println!(
                    "Upgraded '{}' from v{} to v{}",
                    result.name, prev, result.version
                );
            } else {
                println!("Installed '{}' v{}", result.name, result.version);
            }
            println!("  Location: {}", result.install_dir.display());
        }
        PackageCommands::Uninstall { name } => {
            let result = mgr.uninstall(&name)?;
            println!(
                "Uninstalled '{}' v{} ({} files removed)",
                result.name, result.version, result.files_removed
            );
        }
        PackageCommands::List => {
            let packages = mgr.list_installed();
            if packages.is_empty() {
                println!("No packages installed.");
                return Ok(());
            }
            println!(
                "{:<25} {:<12} {:<20} DESCRIPTION",
                "NAME", "VERSION", "AUTHOR"
            );
            println!("{}", "-".repeat(80));
            for pkg in &packages {
                println!(
                    "{:<25} {:<12} {:<20} {}",
                    pkg.name,
                    pkg.version,
                    pkg.author,
                    truncate(&pkg.description, 30),
                );
            }
            println!("\nTotal: {} package(s)", packages.len());
        }
        PackageCommands::Info { name } => match mgr.get_info(&name) {
            Some(info) => {
                println!("Package: {}", info.manifest.name);
                println!("  Version:     {}", info.manifest.version);
                println!("  Author:      {}", info.manifest.author);
                println!("  Description: {}", info.manifest.description);
                println!(
                    "  Installed:   {}",
                    info.installed_at.format("%Y-%m-%d %H:%M:%S UTC")
                );
                println!("  Location:    {}", info.install_dir.display());
                println!("  Binary:      {}", info.binary_path.display());
                println!("  Hash:        {}", info.binary_hash);
                if !info.manifest.requested_permissions.is_empty() {
                    println!("  Permissions: {:?}", info.manifest.requested_permissions);
                }
                println!("  Network:     {:?}", info.manifest.network_scope);
                println!("  Auto-update: {}", info.auto_update);
            }
            None => {
                anyhow::bail!("Package '{}' is not installed", name);
            }
        },
        PackageCommands::Search { query } => {
            let results = mgr.search(&query);
            if results.is_empty() {
                println!("No packages matching '{}'.", query);
                return Ok(());
            }
            for pkg in &results {
                println!("{} v{} — {}", pkg.name, pkg.version, pkg.description);
            }
        }
        PackageCommands::Verify { name } => match mgr.verify(&name) {
            Ok(true) => println!("Package '{}' integrity OK.", name),
            Ok(false) => println!(
                "Package '{}' integrity FAILED — binary may have been modified.",
                name
            ),
            Err(e) => anyhow::bail!("{}", e),
        },
        PackageCommands::Validate { source } => {
            let package = mgr.validate_package(&source)?;
            println!("Package valid:");
            println!("  Name:    {}", package.manifest.name);
            println!("  Version: {}", package.manifest.version);
            println!(
                "  Binary:  {} bytes (hash: {})",
                package.binary_size, package.binary_hash
            );
            println!("{}", crate::package_manager::consent_prompt(&package));
        }
    }

    Ok(())
}
