use anyhow::Result;

use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_marketplace(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::MarketplaceInstall { package } => Ok(Translation {
            command: "curl".to_string(),
            args: vec![
                "-s".to_string(),
                "-X".to_string(),
                "POST".to_string(),
                "http://127.0.0.1:8090/v1/marketplace/install".to_string(),
                "-H".to_string(),
                "Content-Type: application/json".to_string(),
                "-d".to_string(),
                format!(r#"{{"path":"{}"}}"#, package),
            ],
            description: format!("Install marketplace package: {}", package),
            permission: PermissionLevel::SystemWrite,
            explanation: "Installs a package from the marketplace".to_string(),
        }),

        Intent::MarketplaceUninstall { package } => Ok(Translation {
            command: "curl".to_string(),
            args: vec![
                "-s".to_string(),
                "-X".to_string(),
                "DELETE".to_string(),
                format!("http://127.0.0.1:8090/v1/marketplace/{}", package),
            ],
            description: format!("Uninstall marketplace package: {}", package),
            permission: PermissionLevel::SystemWrite,
            explanation: "Removes an installed marketplace package".to_string(),
        }),

        Intent::MarketplaceSearch { query } => Ok(Translation {
            command: "curl".to_string(),
            args: vec![
                "-s".to_string(),
                format!("http://127.0.0.1:8090/v1/marketplace/search?q={}", query),
            ],
            description: format!("Search marketplace for: {}", query),
            permission: PermissionLevel::Safe,
            explanation: "Searches installed marketplace packages".to_string(),
        }),

        Intent::MarketplaceList => Ok(Translation {
            command: "curl".to_string(),
            args: vec![
                "-s".to_string(),
                "http://127.0.0.1:8090/v1/marketplace/installed".to_string(),
            ],
            description: "List installed marketplace packages".to_string(),
            permission: PermissionLevel::Safe,
            explanation: "Shows all packages installed from the marketplace".to_string(),
        }),

        Intent::MarketplaceUpdate => Ok(Translation {
            command: "curl".to_string(),
            args: vec![
                "-s".to_string(),
                "http://127.0.0.1:8090/v1/marketplace/installed".to_string(),
            ],
            description: "Check for marketplace package updates".to_string(),
            permission: PermissionLevel::Safe,
            explanation: "Checks for available updates to installed packages".to_string(),
        }),

        _ => unreachable!("translate_marketplace called with non-marketplace intent"),
    }
}
