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
        let cpu_usage = read_cpu_usage().await.unwrap_or(0.0);
        let memory_usage = read_memory_usage().await.unwrap_or(0.0);
        let disk_usage = read_disk_usage().unwrap_or(0.0);

        let status = shell::SystemStatus {
            cpu_usage,
            memory_usage,
            disk_usage,
            battery_level: None,
            network_status: shell::NetworkStatus::Connected,
            agent_count: 0,
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

/// Parse CPU jiffies from a `/proc/stat` "cpu" line.
/// Returns (total, idle).
fn parse_cpu_line(line: &str) -> Option<(u64, u64)> {
    let fields: Vec<u64> = line
        .split_whitespace()
        .skip(1) // skip "cpu"
        .filter_map(|s| s.parse().ok())
        .collect();
    if fields.len() < 4 {
        return None;
    }
    // fields: user, nice, system, idle, iowait, irq, softirq, steal ...
    let idle = fields[3] + fields.get(4).copied().unwrap_or(0);
    let total: u64 = fields.iter().sum();
    Some((total, idle))
}

/// Read CPU usage percentage by sampling /proc/stat twice with a 100ms gap.
async fn read_cpu_usage() -> Option<f32> {
    let stat1 = tokio::fs::read_to_string("/proc/stat").await.ok()?;
    let line1 = stat1.lines().find(|l| l.starts_with("cpu "))?;
    let (total1, idle1) = parse_cpu_line(line1)?;

    tokio::time::sleep(Duration::from_millis(100)).await;

    let stat2 = tokio::fs::read_to_string("/proc/stat").await.ok()?;
    let line2 = stat2.lines().find(|l| l.starts_with("cpu "))?;
    let (total2, idle2) = parse_cpu_line(line2)?;

    let total_delta = total2.saturating_sub(total1) as f64;
    let idle_delta = idle2.saturating_sub(idle1) as f64;

    if total_delta == 0.0 {
        return Some(0.0);
    }
    Some(((total_delta - idle_delta) / total_delta * 100.0) as f32)
}

/// Read memory usage percentage from /proc/meminfo.
async fn read_memory_usage() -> Option<f32> {
    let meminfo = tokio::fs::read_to_string("/proc/meminfo").await.ok()?;
    let mut mem_total: Option<u64> = None;
    let mut mem_available: Option<u64> = None;

    for line in meminfo.lines() {
        if line.starts_with("MemTotal:") {
            mem_total = line.split_whitespace().nth(1).and_then(|v| v.parse().ok());
        } else if line.starts_with("MemAvailable:") {
            mem_available = line.split_whitespace().nth(1).and_then(|v| v.parse().ok());
        }
        if mem_total.is_some() && mem_available.is_some() {
            break;
        }
    }

    let total = mem_total? as f64;
    let available = mem_available? as f64;
    if total == 0.0 {
        return Some(0.0);
    }
    Some(((total - available) / total * 100.0) as f32)
}

/// Read disk usage percentage for the root filesystem using statvfs.
fn read_disk_usage() -> Option<f32> {
    let path = std::ffi::CString::new("/").ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let ret = unsafe { libc::statvfs(path.as_ptr(), &mut stat) };
    if ret != 0 {
        return None;
    }
    let total = stat.f_blocks as f64;
    let available = stat.f_bavail as f64;
    if total == 0.0 {
        return Some(0.0);
    }
    Some(((total - available) / total * 100.0) as f32)
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
