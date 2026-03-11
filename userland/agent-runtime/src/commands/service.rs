//! Service management command handler.

use std::path::Path;

use anyhow::Result;

use crate::cli::ServiceCommands;
use crate::service_manager::ServiceManager;

pub async fn handle_service_command(action: ServiceCommands, config_dir: &Path) -> Result<()> {
    let services_dir = config_dir.join("services");
    let mgr = ServiceManager::new(&services_dir);
    mgr.load_definitions().await?;

    match action {
        ServiceCommands::List => {
            let services = mgr.list_services().await;
            if services.is_empty() {
                println!("No services configured.");
                println!("Add service definitions to: {}", services_dir.display());
                return Ok(());
            }
            println!(
                "{:<20} {:<10} {:<8} {:<10} {:<8} DESCRIPTION",
                "NAME", "STATE", "PID", "UPTIME", "RESTARTS"
            );
            println!("{}", "-".repeat(80));
            for svc in &services {
                println!(
                    "{:<20} {:<10} {:<8} {:<10} {:<8} {}",
                    svc.name,
                    svc.state.to_string(),
                    svc.pid.map_or("-".to_string(), |p| p.to_string()),
                    svc.uptime_display(),
                    svc.restart_count,
                    svc.description,
                );
            }
            println!("\nTotal: {} service(s)", services.len());
        }
        ServiceCommands::Start { name } => {
            mgr.start_service(&name).await?;
            println!("Service '{}' started.", name);
        }
        ServiceCommands::Stop { name } => {
            mgr.stop_service(&name).await?;
            println!("Service '{}' stopped.", name);
        }
        ServiceCommands::Restart { name } => {
            mgr.restart_service(&name).await?;
            println!("Service '{}' restarted.", name);
        }
        ServiceCommands::Status { name } => match mgr.get_status(&name).await {
            Some(status) => {
                println!("Service: {}", status.name);
                println!("  State:       {}", status.state);
                println!(
                    "  PID:         {}",
                    status.pid.map_or("-".to_string(), |p| p.to_string())
                );
                println!("  Uptime:      {}", status.uptime_display());
                println!("  Restarts:    {}", status.restart_count);
                println!("  Enabled:     {}", status.enabled);
                println!(
                    "  Exit Code:   {}",
                    status.exit_code.map_or("-".to_string(), |c| c.to_string())
                );
                if !status.description.is_empty() {
                    println!("  Description: {}", status.description);
                }
            }
            None => {
                anyhow::bail!("Unknown service: {}", name);
            }
        },
        ServiceCommands::Enable { name } => {
            mgr.enable_service(&name).await?;
            println!("Service '{}' enabled.", name);
        }
        ServiceCommands::Disable { name } => {
            mgr.disable_service(&name).await?;
            println!("Service '{}' disabled.", name);
        }
        ServiceCommands::Boot => {
            mgr.boot().await?;
            println!("All enabled services started.");
        }
    }

    Ok(())
}
