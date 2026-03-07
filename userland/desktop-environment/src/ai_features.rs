use chrono::Timelike;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tracing::{debug, info};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AIFeatureError {
    #[error("Context not found: {0}")]
    ContextNotFound(String),
    #[error("Agent not found: {0}")]
    AgentNotFound(Uuid),
    #[error("Model error: {0}")]
    ModelError(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestionType {
    WindowPlacement,
    ContextSwitch,
    TaskRecommendation,
    ResourceOptimization,
    SecurityAlert,
    Productivity,
}

#[derive(Debug, Clone)]
pub struct AISuggestion {
    pub id: Uuid,
    pub suggestion_type: SuggestionType,
    pub title: String,
    pub description: String,
    pub confidence: f32,
    pub action: Option<String>,
    pub is_dismissed: bool,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Default for AISuggestion {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            suggestion_type: SuggestionType::Productivity,
            title: String::new(),
            description: String::new(),
            confidence: 0.0,
            action: None,
            is_dismissed: false,
            timestamp: chrono::Utc::now(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentHUDState {
    pub agent_id: Uuid,
    pub agent_name: String,
    pub status: AgentStatus,
    pub current_task: String,
    pub progress: f32,
    pub last_activity: chrono::DateTime<chrono::Utc>,
    pub resource_usage: ResourceMetrics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentStatus {
    Idle,
    Thinking,
    Acting,
    Waiting,
    Error,
}

#[derive(Debug, Clone)]
pub struct ResourceMetrics {
    pub cpu_percent: f32,
    pub memory_mb: u64,
    pub gpu_percent: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct ContextEvent {
    pub id: Uuid,
    pub event_type: ContextEventType,
    pub source: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextEventType {
    WindowOpened,
    WindowClosed,
    AppSwitched,
    FileOpened,
    CommandExecuted,
    MeetingStarted,
    MeetingEnded,
    UserAway,
    UserPresent,
}

#[derive(Debug)]
pub struct AIDesktopFeatures {
    suggestions: Arc<RwLock<Vec<AISuggestion>>>,
    agent_hud: Arc<RwLock<HashMap<Uuid, AgentHUDState>>>,
    context_history: Arc<RwLock<Vec<ContextEvent>>>,
    current_context: Arc<RwLock<DesktopContext>>,
    proactive_mode: Arc<RwLock<bool>>,
    ambient_enabled: Arc<RwLock<bool>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopContext {
    pub context_type: ContextType,
    pub active_apps: Vec<String>,
    pub open_files: Vec<String>,
    pub recent_commands: Vec<String>,
    pub time_of_day: TimeOfDay,
    pub user_activity_level: ActivityLevel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextType {
    Development,
    Writing,
    Design,
    Communication,
    Browsing,
    Gaming,
    Idle,
    Meeting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeOfDay {
    Morning,
    Afternoon,
    Evening,
    Night,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityLevel {
    High,
    Medium,
    Low,
    Idle,
}

impl Default for DesktopContext {
    fn default() -> Self {
        Self {
            context_type: ContextType::Idle,
            active_apps: Vec::new(),
            open_files: Vec::new(),
            recent_commands: Vec::new(),
            time_of_day: TimeOfDay::Afternoon,
            user_activity_level: ActivityLevel::Medium,
        }
    }
}

impl Default for AIDesktopFeatures {
    fn default() -> Self {
        Self::new()
    }
}

impl AIDesktopFeatures {
    pub fn new() -> Self {
        Self {
            suggestions: Arc::new(RwLock::new(Vec::new())),
            agent_hud: Arc::new(RwLock::new(HashMap::new())),
            context_history: Arc::new(RwLock::new(Vec::new())),
            current_context: Arc::new(RwLock::new(DesktopContext::default())),
            proactive_mode: Arc::new(RwLock::new(true)),
            ambient_enabled: Arc::new(RwLock::new(true)),
        }
    }

    pub fn analyze_context(&self) -> DesktopContext {
        let mut context = self.current_context.write().unwrap();

        let hour = chrono::Utc::now().hour();
        context.time_of_day = match hour {
            5..=11 => TimeOfDay::Morning,
            12..=16 => TimeOfDay::Afternoon,
            17..=20 => TimeOfDay::Evening,
            _ => TimeOfDay::Night,
        };

        info!(
            "Context analyzed: {:?} at {:?}",
            context.context_type, context.time_of_day
        );
        context.clone()
    }

    pub fn update_context(&self, event: ContextEvent) {
        {
            let mut history = self.context_history.write().unwrap();
            history.push(event.clone());

            let len = history.len();
            if len > 100 {
                history.drain(0..len - 100);
            }
        }

        {
            let mut context = self.current_context.write().unwrap();
            match event.event_type {
                ContextEventType::WindowOpened => {
                    if let Some(app) = event.metadata.get("app") {
                        if !context.active_apps.contains(app) {
                            context.active_apps.push(app.clone());
                        }
                    }
                }
                ContextEventType::FileOpened => {
                    if let Some(file) = event.metadata.get("file") {
                        context.open_files.push(file.clone());
                        let len = context.open_files.len();
                        if len > 10 {
                            context.open_files.drain(0..len - 10);
                        }
                    }
                }
                _ => {}
            }
        }

        self.detect_context_type();
        let context = self.current_context.read().unwrap();
        debug!("Context updated: {:?}", context.context_type);
    }

    fn detect_context_type(&self) {
        let mut context = self.current_context.write().unwrap();

        let has_dev_tools = context
            .active_apps
            .iter()
            .any(|app| app.contains("code") || app.contains("terminal") || app.contains("editor"));
        let has_design = context
            .active_apps
            .iter()
            .any(|app| app.contains("gimp") || app.contains("figma") || app.contains("blender"));
        let has_comm = context
            .active_apps
            .iter()
            .any(|app| app.contains("slack") || app.contains("discord") || app.contains("mail"));

        context.context_type = if has_dev_tools {
            ContextType::Development
        } else if has_design {
            ContextType::Design
        } else if has_comm {
            ContextType::Communication
        } else if context.active_apps.iter().any(|a| a.contains("browser")) {
            ContextType::Browsing
        } else {
            ContextType::Idle
        };
    }

    pub fn generate_suggestion(
        &self,
        suggestion_type: SuggestionType,
        title: String,
        description: String,
        confidence: f32,
    ) -> AISuggestion {
        AISuggestion {
            id: Uuid::new_v4(),
            suggestion_type,
            title,
            description,
            confidence,
            action: None,
            is_dismissed: false,
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn add_suggestion(&self, suggestion: AISuggestion) {
        let title = suggestion.title.clone();
        let mut suggestions = self.suggestions.write().unwrap();

        suggestions.retain(|s| {
            s.suggestion_type != suggestion.suggestion_type || s.title != suggestion.title
        });

        suggestions.push(suggestion);
        suggestions.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        debug!("Suggestion added: {}", title);
    }

    pub fn proactive_suggestions(&self) -> Vec<AISuggestion> {
        let _context = self.analyze_context();
        let suggestions = self.suggestions.write().unwrap();

        let relevant: Vec<_> = suggestions
            .iter()
            .filter(|s| !s.is_dismissed && s.confidence > 0.5)
            .cloned()
            .collect();

        relevant
    }

    pub fn smart_window_placement(&self, _app_id: &str) -> (i32, i32, u32, u32) {
        let context = self.current_context.read().unwrap();
        match context.context_type {
            ContextType::Development => (100, 100, 1400, 900),
            ContextType::Design => (50, 50, 1820, 1000),
            ContextType::Communication => (200, 500, 600, 400),
            _ => (100, 100, 1200, 800),
        }
    }

    pub fn register_agent_hud(&self, agent_id: Uuid, name: String) {
        let name_clone = name.clone();
        let mut hud = self.agent_hud.write().unwrap();
        hud.insert(
            agent_id,
            AgentHUDState {
                agent_id,
                agent_name: name_clone,
                status: AgentStatus::Idle,
                current_task: String::new(),
                progress: 0.0,
                last_activity: chrono::Utc::now(),
                resource_usage: ResourceMetrics {
                    cpu_percent: 0.0,
                    memory_mb: 0,
                    gpu_percent: None,
                },
            },
        );
        info!("Agent HUD registered: {}", name);
    }

    pub fn update_agent_hud(
        &self,
        agent_id: Uuid,
        status: AgentStatus,
        task: String,
        progress: f32,
    ) {
        let mut hud = self.agent_hud.write().unwrap();
        if let Some(state) = hud.get_mut(&agent_id) {
            state.status = status;
            state.current_task = task;
            state.progress = progress;
            state.last_activity = chrono::Utc::now();
        }
    }

    pub fn unregister_agent_hud(&self, agent_id: Uuid) {
        let mut hud = self.agent_hud.write().unwrap();
        hud.remove(&agent_id);
        debug!("Agent HUD unregistered: {}", agent_id);
    }

    pub fn get_agent_hud_states(&self) -> Vec<AgentHUDState> {
        self.agent_hud.read().unwrap().values().cloned().collect()
    }

    pub fn set_proactive_mode(&self, enabled: bool) {
        *self.proactive_mode.write().unwrap() = enabled;
        info!("Proactive mode: {}", enabled);
    }

    pub fn set_ambient_enabled(&self, enabled: bool) {
        *self.ambient_enabled.write().unwrap() = enabled;
        info!("Ambient intelligence: {}", enabled);
    }

    pub fn dismiss_suggestion(&self, suggestion_id: Uuid) {
        let mut suggestions = self.suggestions.write().unwrap();
        if let Some(s) = suggestions.iter_mut().find(|s| s.id == suggestion_id) {
            s.is_dismissed = true;
            debug!("Suggestion dismissed: {}", suggestion_id);
        }
    }

    pub fn get_context(&self) -> DesktopContext {
        self.current_context.read().unwrap().clone()
    }

    pub fn suggest_workspace_switch(
        &self,
        _from: usize,
        to: usize,
        reason: String,
    ) -> AISuggestion {
        self.generate_suggestion(
            SuggestionType::ContextSwitch,
            format!("Switch to Workspace {}", to + 1),
            reason,
            0.75,
        )
    }

    pub fn optimize_resources(&self) -> AISuggestion {
        self.generate_suggestion(
            SuggestionType::ResourceOptimization,
            "Resource Optimization".to_string(),
            "Consider suspending idle agents to free resources".to_string(),
            0.65,
        )
    }

    /// Start a background polling loop that periodically fetches agent state
    /// from the registration API at `http://localhost:8090/v1/agents` and
    /// updates the HUD.
    pub fn start_hud_polling(&self, interval: std::time::Duration) -> tokio::task::JoinHandle<()> {
        let agent_hud = self.agent_hud.clone();

        tokio::spawn(async move {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_default();

            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;

                match Self::poll_agents(&client).await {
                    Ok(agents) => {
                        let mut hud = agent_hud.write().unwrap();
                        // Clear stale entries and replace with fresh data
                        hud.clear();
                        for agent in agents {
                            hud.insert(agent.agent_id, agent);
                        }
                        debug!("HUD updated with {} agents", hud.len());
                    }
                    Err(e) => {
                        debug!("HUD poll failed (agent API may be down): {}", e);
                    }
                }
            }
        })
    }

    /// Poll the agent registration API and convert responses to HUD state.
    async fn poll_agents(
        client: &reqwest::Client,
    ) -> Result<Vec<AgentHUDState>, Box<dyn std::error::Error + Send + Sync>> {
        let resp = client.get("http://localhost:8090/v1/agents").send().await?;

        if !resp.status().is_success() {
            return Err(format!("API returned {}", resp.status()).into());
        }

        let body: serde_json::Value = resp.json().await?;
        let mut states = Vec::new();

        if let Some(agents) = body["agents"].as_array() {
            for agent in agents {
                let id_str = agent["id"].as_str().unwrap_or_default();
                let agent_id = uuid::Uuid::parse_str(id_str).unwrap_or_else(|_| Uuid::new_v4());

                let status_str = agent["status"].as_str().unwrap_or("unknown");
                let status = match status_str {
                    "running" => AgentStatus::Acting,
                    "registered" => AgentStatus::Idle,
                    "error" | "failed" => AgentStatus::Error,
                    "waiting" => AgentStatus::Waiting,
                    _ => AgentStatus::Idle,
                };

                let last_hb = agent["last_heartbeat"]
                    .as_str()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(chrono::Utc::now);

                states.push(AgentHUDState {
                    agent_id,
                    agent_name: agent["name"].as_str().unwrap_or("unknown").to_string(),
                    status,
                    current_task: agent["current_task"].as_str().unwrap_or("").to_string(),
                    progress: 0.0,
                    last_activity: last_hb,
                    resource_usage: ResourceMetrics {
                        cpu_percent: agent["cpu_percent"].as_f64().unwrap_or(0.0) as f32,
                        memory_mb: agent["memory_mb"].as_u64().unwrap_or(0),
                        gpu_percent: None,
                    },
                });
            }
        }

        Ok(states)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suggestion_type_variants() {
        assert!(matches!(
            SuggestionType::WindowPlacement,
            SuggestionType::WindowPlacement
        ));
        assert!(matches!(
            SuggestionType::ContextSwitch,
            SuggestionType::ContextSwitch
        ));
        assert!(matches!(
            SuggestionType::TaskRecommendation,
            SuggestionType::TaskRecommendation
        ));
        assert!(matches!(
            SuggestionType::SecurityAlert,
            SuggestionType::SecurityAlert
        ));
    }

    #[test]
    fn test_ai_suggestion_default() {
        let suggestion = AISuggestion::default();
        assert_eq!(suggestion.suggestion_type, SuggestionType::Productivity);
        assert!(!suggestion.is_dismissed);
        assert_eq!(suggestion.confidence, 0.0);
    }

    #[test]
    fn test_ai_suggestion_custom() {
        let suggestion = AISuggestion {
            id: Uuid::new_v4(),
            suggestion_type: SuggestionType::TaskRecommendation,
            title: "Take a break".to_string(),
            description: "You've been working for 2 hours".to_string(),
            confidence: 0.85,
            action: Some("break".to_string()),
            is_dismissed: false,
            timestamp: chrono::Utc::now(),
        };
        assert_eq!(suggestion.confidence, 0.85);
        assert!(suggestion.action.is_some());
    }

    #[test]
    fn test_agent_status_variants() {
        assert!(matches!(AgentStatus::Idle, AgentStatus::Idle));
        assert!(matches!(AgentStatus::Thinking, AgentStatus::Thinking));
        assert!(matches!(AgentStatus::Acting, AgentStatus::Acting));
        assert!(matches!(AgentStatus::Waiting, AgentStatus::Waiting));
        assert!(matches!(AgentStatus::Error, AgentStatus::Error));
    }

    #[test]
    fn test_agent_hud_state() {
        let state = AgentHUDState {
            agent_id: Uuid::new_v4(),
            agent_name: "test-agent".to_string(),
            status: AgentStatus::Thinking,
            current_task: "Processing data".to_string(),
            progress: 0.5,
            last_activity: chrono::Utc::now(),
            resource_usage: ResourceMetrics {
                cpu_percent: 25.0,
                memory_mb: 512,
                gpu_percent: Some(50.0),
            },
        };
        assert_eq!(state.status, AgentStatus::Thinking);
        assert_eq!(state.progress, 0.5);
    }

    #[test]
    fn test_resource_metrics() {
        let metrics = ResourceMetrics {
            cpu_percent: 30.0,
            memory_mb: 1024,
            gpu_percent: Some(75.0),
        };
        assert_eq!(metrics.cpu_percent, 30.0);
        assert_eq!(metrics.memory_mb, 1024);
        assert!(metrics.gpu_percent.is_some());
    }

    #[test]
    fn test_time_of_day_variants() {
        assert!(matches!(TimeOfDay::Morning, TimeOfDay::Morning));
        assert!(matches!(TimeOfDay::Afternoon, TimeOfDay::Afternoon));
        assert!(matches!(TimeOfDay::Evening, TimeOfDay::Evening));
        assert!(matches!(TimeOfDay::Night, TimeOfDay::Night));
    }

    #[test]
    fn test_activity_level_variants() {
        assert!(matches!(ActivityLevel::Low, ActivityLevel::Low));
        assert!(matches!(ActivityLevel::Medium, ActivityLevel::Medium));
        assert!(matches!(ActivityLevel::High, ActivityLevel::High));
        assert!(matches!(ActivityLevel::Idle, ActivityLevel::Idle));
    }

    #[test]
    fn test_context_type_variants() {
        assert!(matches!(ContextType::Development, ContextType::Development));
        assert!(matches!(
            ContextType::Communication,
            ContextType::Communication
        ));
        assert!(matches!(ContextType::Design, ContextType::Design));
    }

    #[test]
    fn test_context_event_type_variants() {
        assert!(matches!(
            ContextEventType::WindowOpened,
            ContextEventType::WindowOpened
        ));
        assert!(matches!(
            ContextEventType::WindowClosed,
            ContextEventType::WindowClosed
        ));
        assert!(matches!(
            ContextEventType::FileOpened,
            ContextEventType::FileOpened
        ));
    }

    #[test]
    fn test_ai_desktop_features_new() {
        let features = AIDesktopFeatures::new();
        assert_eq!(features.get_agent_hud_states().len(), 0);
    }

    #[test]
    fn test_ai_desktop_features_analyze_context() {
        let features = AIDesktopFeatures::new();
        let context = features.analyze_context();
        assert!(matches!(context.context_type, ContextType::Idle));
    }

    #[test]
    fn test_ai_desktop_features_update_context() {
        let features = AIDesktopFeatures::new();
        let mut metadata = HashMap::new();
        metadata.insert("app".to_string(), "vscode".to_string());
        let event = ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::WindowOpened,
            source: "compositor".to_string(),
            timestamp: chrono::Utc::now(),
            metadata,
        };
        features.update_context(event);
        let context = features.get_context();
        assert!(context.active_apps.contains(&"vscode".to_string()));
    }

    #[test]
    fn test_ai_desktop_features_generate_suggestion() {
        let features = AIDesktopFeatures::new();
        let suggestion = features.generate_suggestion(
            SuggestionType::TaskRecommendation,
            "Take a break".to_string(),
            "You've been coding for 2 hours".to_string(),
            0.85,
        );
        assert_eq!(suggestion.title, "Take a break");
        assert_eq!(suggestion.confidence, 0.85);
    }

    #[test]
    fn test_ai_desktop_features_add_suggestion() {
        let features = AIDesktopFeatures::new();
        let suggestion = features.generate_suggestion(
            SuggestionType::Productivity,
            "Test".to_string(),
            "Desc".to_string(),
            0.7,
        );
        features.add_suggestion(suggestion);
        let suggestions = features.proactive_suggestions();
        assert!(!suggestions.is_empty());
    }

    #[test]
    fn test_ai_desktop_features_smart_window_placement() {
        let features = AIDesktopFeatures::new();
        let (_x, _y, w, h) = features.smart_window_placement("test");
        assert!(w > 0);
        assert!(h > 0);
    }

    #[test]
    fn test_ai_desktop_features_register_agent_hud() {
        let features = AIDesktopFeatures::new();
        let agent_id = Uuid::new_v4();
        features.register_agent_hud(agent_id, "test-agent".to_string());
        let states = features.get_agent_hud_states();
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].agent_name, "test-agent");
    }

    #[test]
    fn test_ai_desktop_features_update_agent_hud() {
        let features = AIDesktopFeatures::new();
        let agent_id = Uuid::new_v4();
        features.register_agent_hud(agent_id, "test-agent".to_string());
        features.update_agent_hud(
            agent_id,
            AgentStatus::Thinking,
            "Processing".to_string(),
            0.5,
        );
        let states = features.get_agent_hud_states();
        assert_eq!(states[0].status, AgentStatus::Thinking);
        assert_eq!(states[0].progress, 0.5);
    }

    #[test]
    fn test_ai_desktop_features_unregister_agent_hud() {
        let features = AIDesktopFeatures::new();
        let agent_id = Uuid::new_v4();
        features.register_agent_hud(agent_id, "test-agent".to_string());
        assert_eq!(features.get_agent_hud_states().len(), 1);
        features.unregister_agent_hud(agent_id);
        assert!(features.get_agent_hud_states().is_empty());
    }

    #[test]
    fn test_ai_desktop_features_set_proactive_mode() {
        let features = AIDesktopFeatures::new();
        features.set_proactive_mode(false);
        features.set_proactive_mode(true);
    }

    #[test]
    fn test_ai_desktop_features_set_ambient_enabled() {
        let features = AIDesktopFeatures::new();
        features.set_ambient_enabled(false);
        features.set_ambient_enabled(true);
    }

    #[test]
    fn test_ai_desktop_features_dismiss_suggestion() {
        let features = AIDesktopFeatures::new();
        let suggestion = features.generate_suggestion(
            SuggestionType::Productivity,
            "Test".to_string(),
            "Desc".to_string(),
            0.8,
        );
        let id = suggestion.id;
        features.add_suggestion(suggestion);
        features.dismiss_suggestion(id);
    }

    #[test]
    fn test_ai_desktop_features_suggest_workspace_switch() {
        let features = AIDesktopFeatures::new();
        let suggestion = features.suggest_workspace_switch(0, 1, "Better focus".to_string());
        assert_eq!(suggestion.suggestion_type, SuggestionType::ContextSwitch);
    }

    #[test]
    fn test_ai_desktop_features_optimize_resources() {
        let features = AIDesktopFeatures::new();
        let suggestion = features.optimize_resources();
        assert_eq!(
            suggestion.suggestion_type,
            SuggestionType::ResourceOptimization
        );
    }

    #[test]
    fn test_desktop_context_default() {
        let context = DesktopContext::default();
        assert!(matches!(context.context_type, ContextType::Idle));
        assert!(context.active_apps.is_empty());
    }

    #[test]
    fn test_ai_feature_error_variants() {
        let err = AIFeatureError::ContextNotFound("test".to_string());
        assert!(err.to_string().contains("not found"));
        let err = AIFeatureError::AgentNotFound(Uuid::nil());
        assert!(err.to_string().contains("not found"));
        let err = AIFeatureError::ModelError("test".to_string());
        assert!(err.to_string().contains("Model error"));
    }

    // --- detect_context_type tests ---

    #[test]
    fn test_detect_context_type_development_code() {
        let features = AIDesktopFeatures::new();
        let mut metadata = HashMap::new();
        metadata.insert("app".to_string(), "vscode".to_string());
        features.update_context(ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::WindowOpened,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata,
        });
        let ctx = features.get_context();
        // "vscode" contains "code" → Development
        assert_eq!(ctx.context_type, ContextType::Development);
    }

    #[test]
    fn test_detect_context_type_development_terminal() {
        let features = AIDesktopFeatures::new();
        let mut metadata = HashMap::new();
        metadata.insert("app".to_string(), "terminal".to_string());
        features.update_context(ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::WindowOpened,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata,
        });
        let ctx = features.get_context();
        assert_eq!(ctx.context_type, ContextType::Development);
    }

    #[test]
    fn test_detect_context_type_development_editor() {
        let features = AIDesktopFeatures::new();
        let mut metadata = HashMap::new();
        metadata.insert("app".to_string(), "text-editor".to_string());
        features.update_context(ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::WindowOpened,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata,
        });
        let ctx = features.get_context();
        assert_eq!(ctx.context_type, ContextType::Development);
    }

    #[test]
    fn test_detect_context_type_design_figma() {
        let features = AIDesktopFeatures::new();
        let mut metadata = HashMap::new();
        metadata.insert("app".to_string(), "figma".to_string());
        features.update_context(ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::WindowOpened,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata,
        });
        let ctx = features.get_context();
        assert_eq!(ctx.context_type, ContextType::Design);
    }

    #[test]
    fn test_detect_context_type_design_gimp() {
        let features = AIDesktopFeatures::new();
        let mut metadata = HashMap::new();
        metadata.insert("app".to_string(), "gimp".to_string());
        features.update_context(ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::WindowOpened,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata,
        });
        let ctx = features.get_context();
        assert_eq!(ctx.context_type, ContextType::Design);
    }

    #[test]
    fn test_detect_context_type_design_blender() {
        let features = AIDesktopFeatures::new();
        let mut metadata = HashMap::new();
        metadata.insert("app".to_string(), "blender".to_string());
        features.update_context(ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::WindowOpened,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata,
        });
        let ctx = features.get_context();
        assert_eq!(ctx.context_type, ContextType::Design);
    }

    #[test]
    fn test_detect_context_type_communication_slack() {
        let features = AIDesktopFeatures::new();
        let mut metadata = HashMap::new();
        metadata.insert("app".to_string(), "slack".to_string());
        features.update_context(ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::WindowOpened,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata,
        });
        let ctx = features.get_context();
        assert_eq!(ctx.context_type, ContextType::Communication);
    }

    #[test]
    fn test_detect_context_type_communication_discord() {
        let features = AIDesktopFeatures::new();
        let mut metadata = HashMap::new();
        metadata.insert("app".to_string(), "discord".to_string());
        features.update_context(ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::WindowOpened,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata,
        });
        let ctx = features.get_context();
        assert_eq!(ctx.context_type, ContextType::Communication);
    }

    #[test]
    fn test_detect_context_type_communication_mail() {
        let features = AIDesktopFeatures::new();
        let mut metadata = HashMap::new();
        metadata.insert("app".to_string(), "mail".to_string());
        features.update_context(ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::WindowOpened,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata,
        });
        let ctx = features.get_context();
        assert_eq!(ctx.context_type, ContextType::Communication);
    }

    #[test]
    fn test_detect_context_type_browsing() {
        let features = AIDesktopFeatures::new();
        let mut metadata = HashMap::new();
        metadata.insert("app".to_string(), "browser".to_string());
        features.update_context(ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::WindowOpened,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata,
        });
        let ctx = features.get_context();
        assert_eq!(ctx.context_type, ContextType::Browsing);
    }

    #[test]
    fn test_detect_context_type_idle_unknown_app() {
        let features = AIDesktopFeatures::new();
        let mut metadata = HashMap::new();
        metadata.insert("app".to_string(), "calculator".to_string());
        features.update_context(ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::WindowOpened,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata,
        });
        let ctx = features.get_context();
        assert_eq!(ctx.context_type, ContextType::Idle);
    }

    // --- update_context event type tests ---

    #[test]
    fn test_update_context_window_closed() {
        let features = AIDesktopFeatures::new();
        features.update_context(ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::WindowClosed,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata: HashMap::new(),
        });
        // WindowClosed doesn't change active_apps (falls through to _ branch)
        let ctx = features.get_context();
        assert!(ctx.active_apps.is_empty());
    }

    #[test]
    fn test_update_context_app_switched() {
        let features = AIDesktopFeatures::new();
        features.update_context(ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::AppSwitched,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata: HashMap::new(),
        });
        let ctx = features.get_context();
        assert!(ctx.active_apps.is_empty());
    }

    #[test]
    fn test_update_context_command_executed() {
        let features = AIDesktopFeatures::new();
        features.update_context(ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::CommandExecuted,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata: HashMap::new(),
        });
        let ctx = features.get_context();
        assert!(ctx.active_apps.is_empty());
    }

    #[test]
    fn test_update_context_file_opened() {
        let features = AIDesktopFeatures::new();
        let mut metadata = HashMap::new();
        metadata.insert("file".to_string(), "/home/user/doc.txt".to_string());
        features.update_context(ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::FileOpened,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata,
        });
        let ctx = features.get_context();
        assert!(ctx.open_files.contains(&"/home/user/doc.txt".to_string()));
    }

    #[test]
    fn test_update_context_file_opened_cap_at_10() {
        let features = AIDesktopFeatures::new();
        for i in 0..15 {
            let mut metadata = HashMap::new();
            metadata.insert("file".to_string(), format!("file_{}.txt", i));
            features.update_context(ContextEvent {
                id: Uuid::new_v4(),
                event_type: ContextEventType::FileOpened,
                source: "test".to_string(),
                timestamp: chrono::Utc::now(),
                metadata,
            });
        }
        let ctx = features.get_context();
        assert!(ctx.open_files.len() <= 10);
        // Last file should be present
        assert!(ctx.open_files.contains(&"file_14.txt".to_string()));
    }

    #[test]
    fn test_update_context_window_opened_no_app_metadata() {
        let features = AIDesktopFeatures::new();
        // WindowOpened without "app" metadata — should not add to active_apps
        features.update_context(ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::WindowOpened,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata: HashMap::new(),
        });
        let ctx = features.get_context();
        assert!(ctx.active_apps.is_empty());
    }

    #[test]
    fn test_update_context_window_opened_duplicate_app() {
        let features = AIDesktopFeatures::new();
        let mut metadata = HashMap::new();
        metadata.insert("app".to_string(), "vscode".to_string());
        features.update_context(ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::WindowOpened,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata: metadata.clone(),
        });
        features.update_context(ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::WindowOpened,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata,
        });
        let ctx = features.get_context();
        // Should not add duplicate
        assert_eq!(ctx.active_apps.iter().filter(|a| *a == "vscode").count(), 1);
    }

    // --- proactive_suggestions tests ---

    #[test]
    fn test_proactive_suggestions_filters_dismissed() {
        let features = AIDesktopFeatures::new();
        let s1 = features.generate_suggestion(
            SuggestionType::Productivity,
            "S1".to_string(),
            "D1".to_string(),
            0.9,
        );
        let s1_id = s1.id;
        let s2 = features.generate_suggestion(
            SuggestionType::TaskRecommendation,
            "S2".to_string(),
            "D2".to_string(),
            0.8,
        );
        features.add_suggestion(s1);
        features.add_suggestion(s2);
        features.dismiss_suggestion(s1_id);
        let suggestions = features.proactive_suggestions();
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].title, "S2");
    }

    #[test]
    fn test_proactive_suggestions_filters_low_confidence() {
        let features = AIDesktopFeatures::new();
        let low = features.generate_suggestion(
            SuggestionType::Productivity,
            "Low".to_string(),
            "Desc".to_string(),
            0.3, // below 0.5 threshold
        );
        let high = features.generate_suggestion(
            SuggestionType::TaskRecommendation,
            "High".to_string(),
            "Desc".to_string(),
            0.9,
        );
        features.add_suggestion(low);
        features.add_suggestion(high);
        let suggestions = features.proactive_suggestions();
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].title, "High");
    }

    #[test]
    fn test_proactive_suggestions_empty() {
        let features = AIDesktopFeatures::new();
        let suggestions = features.proactive_suggestions();
        assert!(suggestions.is_empty());
    }

    // --- dismiss_suggestion tests ---

    #[test]
    fn test_dismiss_suggestion_marks_dismissed() {
        let features = AIDesktopFeatures::new();
        let s = features.generate_suggestion(
            SuggestionType::SecurityAlert,
            "Alert".to_string(),
            "Desc".to_string(),
            0.95,
        );
        let id = s.id;
        features.add_suggestion(s);
        features.dismiss_suggestion(id);
        // After dismiss, proactive_suggestions should not include it
        let suggestions = features.proactive_suggestions();
        assert!(suggestions.iter().all(|s| s.id != id));
    }

    #[test]
    fn test_dismiss_nonexistent_suggestion() {
        let features = AIDesktopFeatures::new();
        // Should not panic
        features.dismiss_suggestion(Uuid::new_v4());
    }

    // --- analyze_context tests ---

    #[test]
    fn test_analyze_context_returns_valid() {
        let features = AIDesktopFeatures::new();
        let ctx = features.analyze_context();
        // time_of_day should be set based on current hour
        let valid = matches!(
            ctx.time_of_day,
            TimeOfDay::Morning | TimeOfDay::Afternoon | TimeOfDay::Evening | TimeOfDay::Night
        );
        assert!(valid);
    }

    // --- set_proactive_mode / set_ambient_enabled toggle tests ---

    #[test]
    fn test_set_proactive_mode_toggle() {
        let features = AIDesktopFeatures::new();
        features.set_proactive_mode(false);
        assert!(!*features.proactive_mode.read().unwrap());
        features.set_proactive_mode(true);
        assert!(*features.proactive_mode.read().unwrap());
    }

    #[test]
    fn test_set_ambient_enabled_toggle() {
        let features = AIDesktopFeatures::new();
        features.set_ambient_enabled(false);
        assert!(!*features.ambient_enabled.read().unwrap());
        features.set_ambient_enabled(true);
        assert!(*features.ambient_enabled.read().unwrap());
    }

    // --- add_suggestion dedup + sort tests ---

    #[test]
    fn test_add_suggestion_deduplicates_same_type_and_title() {
        let features = AIDesktopFeatures::new();
        let s1 = features.generate_suggestion(
            SuggestionType::Productivity,
            "Same".to_string(),
            "First".to_string(),
            0.7,
        );
        let s2 = features.generate_suggestion(
            SuggestionType::Productivity,
            "Same".to_string(),
            "Second".to_string(),
            0.9,
        );
        features.add_suggestion(s1);
        features.add_suggestion(s2);
        let suggestions = features.proactive_suggestions();
        // Should have only one with title "Same"
        let same_count = suggestions.iter().filter(|s| s.title == "Same").count();
        assert_eq!(same_count, 1);
        // The remaining one should be the second (higher confidence)
        assert_eq!(suggestions[0].description, "Second");
    }

    #[test]
    fn test_add_suggestion_sorts_by_confidence_descending() {
        let features = AIDesktopFeatures::new();
        let low = features.generate_suggestion(
            SuggestionType::Productivity,
            "Low".to_string(),
            "D".to_string(),
            0.6,
        );
        let high = features.generate_suggestion(
            SuggestionType::TaskRecommendation,
            "High".to_string(),
            "D".to_string(),
            0.95,
        );
        let mid = features.generate_suggestion(
            SuggestionType::SecurityAlert,
            "Mid".to_string(),
            "D".to_string(),
            0.8,
        );
        features.add_suggestion(low);
        features.add_suggestion(high);
        features.add_suggestion(mid);
        let suggestions = features.proactive_suggestions();
        assert_eq!(suggestions.len(), 3);
        assert!(suggestions[0].confidence >= suggestions[1].confidence);
        assert!(suggestions[1].confidence >= suggestions[2].confidence);
    }

    // --- context history cap at 100 ---

    #[test]
    fn test_context_history_capped_at_100() {
        let features = AIDesktopFeatures::new();
        for i in 0..120 {
            features.update_context(ContextEvent {
                id: Uuid::new_v4(),
                event_type: ContextEventType::UserPresent,
                source: format!("test-{}", i),
                timestamp: chrono::Utc::now(),
                metadata: HashMap::new(),
            });
        }
        let history = features.context_history.read().unwrap();
        assert!(history.len() <= 100);
    }

    // --- smart_window_placement for different context types ---

    #[test]
    fn test_smart_window_placement_development() {
        let features = AIDesktopFeatures::new();
        // Set context to Development
        let mut metadata = HashMap::new();
        metadata.insert("app".to_string(), "code".to_string());
        features.update_context(ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::WindowOpened,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata,
        });
        let (x, y, w, h) = features.smart_window_placement("test");
        assert_eq!((x, y, w, h), (100, 100, 1400, 900));
    }

    #[test]
    fn test_smart_window_placement_design() {
        let features = AIDesktopFeatures::new();
        let mut metadata = HashMap::new();
        metadata.insert("app".to_string(), "figma".to_string());
        features.update_context(ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::WindowOpened,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata,
        });
        let (x, y, w, h) = features.smart_window_placement("test");
        assert_eq!((x, y, w, h), (50, 50, 1820, 1000));
    }

    #[test]
    fn test_smart_window_placement_communication() {
        let features = AIDesktopFeatures::new();
        let mut metadata = HashMap::new();
        metadata.insert("app".to_string(), "slack".to_string());
        features.update_context(ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::WindowOpened,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata,
        });
        let (x, y, w, h) = features.smart_window_placement("test");
        assert_eq!((x, y, w, h), (200, 500, 600, 400));
    }

    // --- register/update/unregister agent HUD ---

    #[test]
    fn test_register_multiple_agents_hud() {
        let features = AIDesktopFeatures::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        features.register_agent_hud(id1, "agent-1".to_string());
        features.register_agent_hud(id2, "agent-2".to_string());
        assert_eq!(features.get_agent_hud_states().len(), 2);
    }

    #[test]
    fn test_update_agent_hud_nonexistent() {
        let features = AIDesktopFeatures::new();
        // Updating a non-existent agent should not panic
        features.update_agent_hud(Uuid::new_v4(), AgentStatus::Error, "task".to_string(), 0.0);
        assert!(features.get_agent_hud_states().is_empty());
    }

    #[test]
    fn test_resource_metrics_no_gpu() {
        let metrics = ResourceMetrics {
            cpu_percent: 0.0,
            memory_mb: 0,
            gpu_percent: None,
        };
        assert!(metrics.gpu_percent.is_none());
    }

    #[test]
    fn test_desktop_context_default_fields() {
        let ctx = DesktopContext::default();
        assert!(ctx.active_apps.is_empty());
        assert!(ctx.open_files.is_empty());
        assert!(ctx.recent_commands.is_empty());
        assert_eq!(ctx.time_of_day, TimeOfDay::Afternoon);
        assert_eq!(ctx.user_activity_level, ActivityLevel::Medium);
    }

    #[test]
    fn test_optimize_resources_confidence() {
        let features = AIDesktopFeatures::new();
        let suggestion = features.optimize_resources();
        assert_eq!(suggestion.confidence, 0.65);
        assert!(!suggestion.is_dismissed);
    }

    #[test]
    fn test_suggest_workspace_switch_fields() {
        let features = AIDesktopFeatures::new();
        let suggestion = features.suggest_workspace_switch(0, 2, "Focus needed".to_string());
        assert_eq!(suggestion.confidence, 0.75);
        assert!(suggestion.title.contains("3")); // workspace 2 + 1
        assert_eq!(suggestion.description, "Focus needed");
    }

    #[test]
    fn test_context_event_type_meeting_started() {
        assert!(matches!(
            ContextEventType::MeetingStarted,
            ContextEventType::MeetingStarted
        ));
    }

    #[test]
    fn test_context_event_type_meeting_ended() {
        assert!(matches!(
            ContextEventType::MeetingEnded,
            ContextEventType::MeetingEnded
        ));
    }

    #[test]
    fn test_context_event_type_user_away() {
        assert!(matches!(
            ContextEventType::UserAway,
            ContextEventType::UserAway
        ));
    }

    #[test]
    fn test_context_type_writing() {
        assert!(matches!(ContextType::Writing, ContextType::Writing));
    }

    #[test]
    fn test_context_type_gaming() {
        assert!(matches!(ContextType::Gaming, ContextType::Gaming));
    }

    #[test]
    fn test_context_type_meeting() {
        assert!(matches!(ContextType::Meeting, ContextType::Meeting));
    }

    #[test]
    fn test_register_agent_hud_initial_state() {
        let features = AIDesktopFeatures::new();
        let id = Uuid::new_v4();
        features.register_agent_hud(id, "my-agent".to_string());
        let states = features.get_agent_hud_states();
        let state = &states[0];
        assert_eq!(state.agent_id, id);
        assert_eq!(state.agent_name, "my-agent");
        assert_eq!(state.status, AgentStatus::Idle);
        assert!(state.current_task.is_empty());
        assert_eq!(state.progress, 0.0);
        assert_eq!(state.resource_usage.cpu_percent, 0.0);
        assert_eq!(state.resource_usage.memory_mb, 0);
        assert!(state.resource_usage.gpu_percent.is_none());
    }

    #[test]
    fn test_unregister_nonexistent_agent_no_panic() {
        let features = AIDesktopFeatures::new();
        features.unregister_agent_hud(Uuid::new_v4());
        assert!(features.get_agent_hud_states().is_empty());
    }

    #[test]
    fn test_analyze_context_updates_time_of_day() {
        let features = AIDesktopFeatures::new();
        let ctx = features.analyze_context();
        let hour = chrono::Utc::now().hour();
        let expected = match hour {
            5..=11 => TimeOfDay::Morning,
            12..=16 => TimeOfDay::Afternoon,
            17..=20 => TimeOfDay::Evening,
            _ => TimeOfDay::Night,
        };
        assert_eq!(ctx.time_of_day, expected);
    }

    #[test]
    fn test_suggestion_dedup_different_types_same_title_kept() {
        let features = AIDesktopFeatures::new();
        let s1 = features.generate_suggestion(
            SuggestionType::Productivity,
            "Same Title".to_string(),
            "D1".to_string(),
            0.8,
        );
        let s2 = features.generate_suggestion(
            SuggestionType::SecurityAlert,
            "Same Title".to_string(),
            "D2".to_string(),
            0.9,
        );
        features.add_suggestion(s1);
        features.add_suggestion(s2);
        let results = features.proactive_suggestions();
        // Different types with same title should both be kept
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_suggestion_boundary_confidence_at_threshold() {
        let features = AIDesktopFeatures::new();
        let at_threshold = features.generate_suggestion(
            SuggestionType::Productivity,
            "Exactly 0.5".to_string(),
            "D".to_string(),
            0.5, // exactly at threshold -- should be excluded (> 0.5 required)
        );
        features.add_suggestion(at_threshold);
        let results = features.proactive_suggestions();
        assert!(results.is_empty());
    }

    #[test]
    fn test_suggestion_just_above_threshold() {
        let features = AIDesktopFeatures::new();
        let above = features.generate_suggestion(
            SuggestionType::Productivity,
            "Above".to_string(),
            "D".to_string(),
            0.51,
        );
        features.add_suggestion(above);
        let results = features.proactive_suggestions();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_update_agent_hud_updates_last_activity() {
        let features = AIDesktopFeatures::new();
        let id = Uuid::new_v4();
        features.register_agent_hud(id, "agent".to_string());
        let before = chrono::Utc::now();
        features.update_agent_hud(id, AgentStatus::Acting, "task".to_string(), 0.75);
        let states = features.get_agent_hud_states();
        let state = states.iter().find(|s| s.agent_id == id).unwrap();
        assert!(state.last_activity >= before);
        assert_eq!(state.status, AgentStatus::Acting);
        assert_eq!(state.current_task, "task");
        assert_eq!(state.progress, 0.75);
    }

    #[test]
    fn test_register_agent_overwrites_existing() {
        let features = AIDesktopFeatures::new();
        let id = Uuid::new_v4();
        features.register_agent_hud(id, "first-name".to_string());
        features.register_agent_hud(id, "second-name".to_string());
        let states = features.get_agent_hud_states();
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].agent_name, "second-name");
    }

    #[test]
    fn test_smart_window_placement_default_context() {
        let features = AIDesktopFeatures::new();
        // Default context is Idle, so should get the catch-all placement
        let (x, y, w, h) = features.smart_window_placement("anything");
        assert_eq!((x, y, w, h), (100, 100, 1200, 800));
    }

    #[test]
    fn test_context_event_clone_and_debug() {
        let event = ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::UserPresent,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata: HashMap::new(),
        };
        let cloned = event.clone();
        assert_eq!(cloned.event_type, ContextEventType::UserPresent);
        let debug = format!("{:?}", cloned);
        assert!(debug.contains("UserPresent"));
    }

    #[test]
    fn test_resource_metrics_clone_and_debug() {
        let metrics = ResourceMetrics {
            cpu_percent: 99.9,
            memory_mb: 8192,
            gpu_percent: Some(50.0),
        };
        let cloned = metrics.clone();
        assert_eq!(cloned.cpu_percent, 99.9);
        assert_eq!(cloned.memory_mb, 8192);
        assert_eq!(cloned.gpu_percent, Some(50.0));
        let debug = format!("{:?}", cloned);
        assert!(debug.contains("8192"));
    }

    #[test]
    fn test_multiple_file_open_events_ordering() {
        let features = AIDesktopFeatures::new();
        for i in 0..12 {
            let mut metadata = HashMap::new();
            metadata.insert("file".to_string(), format!("/path/file_{}.rs", i));
            features.update_context(ContextEvent {
                id: Uuid::new_v4(),
                event_type: ContextEventType::FileOpened,
                source: "test".to_string(),
                timestamp: chrono::Utc::now(),
                metadata,
            });
        }
        let ctx = features.get_context();
        assert_eq!(ctx.open_files.len(), 10);
        // Oldest files (0, 1) should be evicted; newest (11) should be present
        assert!(ctx.open_files.contains(&"/path/file_11.rs".to_string()));
        assert!(!ctx.open_files.contains(&"/path/file_0.rs".to_string()));
    }

    #[test]
    fn test_file_opened_without_file_metadata_ignored() {
        let features = AIDesktopFeatures::new();
        features.update_context(ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::FileOpened,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            metadata: HashMap::new(), // no "file" key
        });
        let ctx = features.get_context();
        assert!(ctx.open_files.is_empty());
    }

    #[test]
    fn test_ai_feature_error_debug() {
        let err = AIFeatureError::ContextNotFound("workspace-5".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("ContextNotFound"));
        assert!(debug.contains("workspace-5"));
    }

    #[test]
    fn test_desktop_context_equality() {
        let ctx1 = DesktopContext::default();
        let ctx2 = DesktopContext::default();
        assert_eq!(ctx1, ctx2);

        let ctx3 = DesktopContext {
            context_type: ContextType::Development,
            ..DesktopContext::default()
        };
        assert_ne!(ctx1, ctx3);
    }

    #[test]
    fn test_agent_hud_state_clone_and_debug() {
        let state = AgentHUDState {
            agent_id: Uuid::nil(),
            agent_name: "debug-agent".to_string(),
            status: AgentStatus::Error,
            current_task: "crashing".to_string(),
            progress: 0.0,
            last_activity: chrono::Utc::now(),
            resource_usage: ResourceMetrics {
                cpu_percent: 0.0,
                memory_mb: 0,
                gpu_percent: None,
            },
        };
        let cloned = state.clone();
        assert_eq!(cloned.agent_name, "debug-agent");
        assert_eq!(cloned.status, AgentStatus::Error);
        let debug = format!("{:?}", cloned);
        assert!(debug.contains("debug-agent"));
    }
}
