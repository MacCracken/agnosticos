use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tracing::{info, warn, error};
use tracing_subscriber::EnvFilter;
use clap::Parser;

mod compositor;
mod shell;
mod ai_features;
mod apps;
mod security_ui;

use compositor::{Compositor, WindowState, ContextType};
use shell::{DesktopShell, Notification, NotificationPriority};
use ai_features::{AIDesktopFeatures, AISuggestion, SuggestionType, ContextEvent, ContextEventType};
use apps::DesktopApplications;
use security_ui::{SecurityUI, SecurityAlert, ThreatLevel, SecurityLevel};

#[derive(Debug, clap::Parser)]
#[command(name = "desktop-environment")]
#[command(author = "AGNOS Team")]
#[command(version = "0.1.0")]
#[command(about = "AGNOS Desktop Environment", long_about = None)]
struct Args {
    #[arg(short, long, default_value = "wayland")]
    #[arg(help = "Display server backend (wayland, x11)")]
    backend: String,

    #[arg(short, long)]
    #[arg(help = "Start in kiosk mode")]
    kiosk: bool,

    #[arg(short, long)]
    #[arg(help = "Disable AI features")]
    no_ai: bool,

    #[arg(short, long)]
    #[arg(help = "Enable secure mode")]
    secure: bool,
}

struct DesktopEnvironment {
    compositor: Arc<Compositor>,
    shell: Arc<DesktopShell>,
    ai_features: Arc<AIDesktopFeatures>,
    apps: Arc<tokio::sync::Mutex<DesktopApplications>>,
    security_ui: Arc<SecurityUI>,
    running: Arc<tokio::sync::Mutex<bool>>,
}

impl DesktopEnvironment {
    async fn new(args: &Args) -> Self {
        info!("Initializing AGNOS Desktop Environment");

        let compositor = Arc::new(Compositor::new());
        let shell = Arc::new(DesktopShell::new());
        let ai_features = Arc::new(AIDesktopFeatures::new());
        let apps = Arc::new(tokio::sync::Mutex::new(DesktopApplications::new()));
        let security_ui = Arc::new(SecurityUI::new());
        let running = Arc::new(tokio::sync::Mutex::new(false));

        if args.secure {
            compositor.set_secure_mode(true);
            security_ui.set_security_level(SecurityLevel::Elevated);
        }

        if !args.no_ai {
            info!("AI features enabled");
        }

        Self {
            compositor,
            shell,
            ai_features,
            apps,
            security_ui,
            running,
        }
    }

    async fn initialize(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Initializing compositor backend");

        info!("Loading desktop shell");
        self.shell.show_notification(Notification {
            app_name: "AGNOS Desktop".to_string(),
            title: "Welcome to AGNOS".to_string(),
            body: "Your AI-native desktop environment".to_string(),
            priority: NotificationPriority::Normal,
            timestamp: chrono::Utc::now(),
            requires_action: false,
            is_agent_related: false,
            ..Default::default()
        });

        info!("Initializing AI desktop features");
        self.ai_features.set_proactive_mode(true);
        self.ai_features.set_ambient_enabled(true);

        info!("Starting system applications");

        Ok(())
    }

    async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Desktop environment running");
        *self.running.lock().await = true;

        let mut interval = tokio::time::interval(Duration::from_secs(5));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.update_system_status().await;
                }
                _ = signal::ctrl_c() => {
                    info!("Received shutdown signal");
                    break;
                }
            }
        }

        self.shutdown().await;
        Ok(())
    }

    async fn update_system_status(&self) {
        let status = shell::SystemStatus {
            cpu_usage: 25.0,
            memory_usage: 40.0,
            disk_usage: 60.0,
            battery_level: Some(85),
            network_status: shell::NetworkStatus::Connected,
            agent_count: 2,
        };
        self.shell.update_system_status(status);

        self.ai_features.update_context(ContextEvent {
            id: uuid::Uuid::new_v4(),
            event_type: ContextEventType::UserPresent,
            source: "desktop".to_string(),
            timestamp: chrono::Utc::now(),
            metadata: Default::default(),
        });
    }

    async fn shutdown(&self) {
        info!("Shutting down desktop environment");
        *self.running.lock().await = false;

        self.shell.show_notification(Notification {
            app_name: "AGNOS Desktop".to_string(),
            title: "Shutting down".to_string(),
            body: "Goodbye!".to_string(),
            priority: NotificationPriority::Low,
            timestamp: chrono::Utc::now(),
            requires_action: false,
            is_agent_related: false,
            ..Default::default()
        });

        info!("Desktop environment stopped");
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    info!("AGNOS Desktop Environment v0.1.0");
    info!("Backend: {}", args.backend);

    let desktop = DesktopEnvironment::new(&args).await;
    desktop.initialize().await?;

    desktop.run().await?;

    Ok(())
}
