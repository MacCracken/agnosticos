//! Tab-completion for shell commands, agent names, service names, and intents

use std::collections::BTreeSet;

/// Completion provider for the AI shell
pub struct CompletionEngine {
    /// Built-in commands
    builtins: BTreeSet<String>,
    /// Known intent keywords
    intent_keywords: BTreeSet<String>,
    /// Network tool names
    network_tools: BTreeSet<String>,
    /// Dynamically registered agent names
    agent_names: BTreeSet<String>,
    /// Dynamically registered service names
    service_names: BTreeSet<String>,
}

impl CompletionEngine {
    pub fn new() -> Self {
        let builtins = ["help", "clear", "exit", "quit", "history", "mode", "cd"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let intent_keywords = [
            "show", "list", "display", "view", "find", "search", "create", "remove", "copy",
            "move", "install", "scan", "audit", "agent", "service", "journal", "device", "mount",
            "unmount", "boot", "update", "start", "stop", "restart",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let network_tools = [
            "nmap",
            "masscan",
            "tcpdump",
            "wireshark",
            "tshark",
            "netcat",
            "ncat",
            "curl",
            "httpie",
            "dig",
            "drill",
            "nikto",
            "gobuster",
            "ffuf",
            "nuclei",
            "sqlmap",
            "aircrack-ng",
            "kismet",
            "mtr",
            "iperf3",
            "ss",
            "socat",
            "bettercap",
            "ngrep",
            "termshark",
            "p0f",
            "netdiscover",
            "arp-scan",
            "dnsx",
            "dnsrecon",
            "fierce",
            "wfuzz",
            "nethogs",
            "iftop",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        Self {
            builtins,
            intent_keywords,
            network_tools,
            agent_names: BTreeSet::new(),
            service_names: BTreeSet::new(),
        }
    }

    /// Register an agent name for completion
    pub fn register_agent(&mut self, name: String) {
        self.agent_names.insert(name);
    }

    /// Register a service name for completion
    pub fn register_service(&mut self, name: String) {
        self.service_names.insert(name);
    }

    /// Remove an agent name
    pub fn deregister_agent(&mut self, name: &str) {
        self.agent_names.remove(name);
    }

    /// Get completions for partial input
    pub fn complete(&self, partial: &str) -> Vec<String> {
        if partial.is_empty() {
            return Vec::new();
        }

        let lower = partial.to_lowercase();
        let mut results = Vec::new();

        // Search all sources
        for source in [
            &self.builtins,
            &self.intent_keywords,
            &self.network_tools,
            &self.agent_names,
            &self.service_names,
        ] {
            for item in source.range(lower.clone()..) {
                if item.starts_with(&lower) {
                    results.push(item.clone());
                } else {
                    break;
                }
            }
        }

        results.sort();
        results.dedup();
        results.truncate(20); // Cap suggestions
        results
    }

    /// Get context-aware completions (after a known prefix like "start service")
    pub fn complete_contextual(&self, words: &[&str]) -> Vec<String> {
        match words {
            ["start", partial] | ["stop", partial] | ["restart", partial] => self
                .service_names
                .iter()
                .filter(|s| s.starts_with(partial))
                .cloned()
                .collect(),
            ["agent", partial] | ["show", "agent", partial] => self
                .agent_names
                .iter()
                .filter(|s| s.starts_with(partial))
                .cloned()
                .collect(),
            ["scan", partial] | ["network", partial] => {
                let actions = ["ports", "web", "dns", "arp", "vuln", "bandwidth", "sockets"];
                actions
                    .iter()
                    .filter(|a| a.starts_with(partial))
                    .map(|a| a.to_string())
                    .collect()
            }
            ["mode", partial] => {
                let modes = ["human", "ai", "auto", "strict"];
                modes
                    .iter()
                    .filter(|m| m.starts_with(partial))
                    .map(|m| m.to_string())
                    .collect()
            }
            _ => {
                // Fall back to general completion on last word
                if let Some(last) = words.last() {
                    self.complete(last)
                } else {
                    Vec::new()
                }
            }
        }
    }

    /// Number of registered agents
    pub fn agent_count(&self) -> usize {
        self.agent_names.len()
    }

    /// Number of registered services
    pub fn service_count(&self) -> usize {
        self.service_names.len()
    }
}

impl Default for CompletionEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_has_builtins() {
        let engine = CompletionEngine::new();
        let completions = engine.complete("hel");
        assert!(completions.contains(&"help".to_string()));
    }

    #[test]
    fn test_new_has_intent_keywords() {
        let engine = CompletionEngine::new();
        let completions = engine.complete("sho");
        assert!(completions.contains(&"show".to_string()));
    }

    #[test]
    fn test_new_has_network_tools() {
        let engine = CompletionEngine::new();
        let completions = engine.complete("nma");
        assert!(completions.contains(&"nmap".to_string()));
    }

    #[test]
    fn test_complete_empty_returns_nothing() {
        let engine = CompletionEngine::new();
        let completions = engine.complete("");
        assert!(completions.is_empty());
    }

    #[test]
    fn test_complete_prefix_matching() {
        let engine = CompletionEngine::new();
        let completions = engine.complete("cu");
        assert!(completions.contains(&"curl".to_string()));
        // Should not contain items that don't start with "cu"
        assert!(!completions.contains(&"nmap".to_string()));
    }

    #[test]
    fn test_complete_case_insensitive() {
        let engine = CompletionEngine::new();
        // All items stored lowercase, input lowered on match
        let completions = engine.complete("HELP");
        // "HELP".to_lowercase() == "help" which matches "help"
        assert!(completions.contains(&"help".to_string()));
    }

    #[test]
    fn test_register_agent() {
        let mut engine = CompletionEngine::new();
        engine.register_agent("code-analyzer".to_string());
        assert_eq!(engine.agent_count(), 1);
        let completions = engine.complete("code");
        assert!(completions.contains(&"code-analyzer".to_string()));
    }

    #[test]
    fn test_register_service() {
        let mut engine = CompletionEngine::new();
        engine.register_service("llm-gateway".to_string());
        assert_eq!(engine.service_count(), 1);
        let completions = engine.complete("llm");
        assert!(completions.contains(&"llm-gateway".to_string()));
    }

    #[test]
    fn test_deregister_agent() {
        let mut engine = CompletionEngine::new();
        engine.register_agent("scanner".to_string());
        assert_eq!(engine.agent_count(), 1);
        engine.deregister_agent("scanner");
        assert_eq!(engine.agent_count(), 0);
        // Should no longer complete
        let completions = engine.complete("scanner");
        assert!(!completions.contains(&"scanner".to_string()));
    }

    #[test]
    fn test_contextual_service_completion() {
        let mut engine = CompletionEngine::new();
        engine.register_service("nginx".to_string());
        engine.register_service("node-api".to_string());

        let completions = engine.complete_contextual(&["start", "n"]);
        assert!(completions.contains(&"nginx".to_string()));
        assert!(completions.contains(&"node-api".to_string()));

        let completions = engine.complete_contextual(&["stop", "ng"]);
        assert!(completions.contains(&"nginx".to_string()));
        assert!(!completions.contains(&"node-api".to_string()));
    }

    #[test]
    fn test_contextual_agent_completion() {
        let mut engine = CompletionEngine::new();
        engine.register_agent("agent-alpha".to_string());
        engine.register_agent("agent-beta".to_string());

        let completions = engine.complete_contextual(&["agent", "agent-a"]);
        assert!(completions.contains(&"agent-alpha".to_string()));
        assert!(!completions.contains(&"agent-beta".to_string()));
    }

    #[test]
    fn test_contextual_mode_completion() {
        let engine = CompletionEngine::new();
        let completions = engine.complete_contextual(&["mode", "h"]);
        assert!(completions.contains(&"human".to_string()));
        assert!(!completions.contains(&"ai".to_string()));
    }

    #[test]
    fn test_contextual_scan_completion() {
        let engine = CompletionEngine::new();
        let completions = engine.complete_contextual(&["scan", "p"]);
        assert!(completions.contains(&"ports".to_string()));
        assert!(!completions.contains(&"web".to_string()));
    }

    #[test]
    fn test_contextual_fallback_to_general() {
        let engine = CompletionEngine::new();
        let completions = engine.complete_contextual(&["unknown_prefix", "hel"]);
        assert!(completions.contains(&"help".to_string()));
    }

    #[test]
    fn test_dedup_no_duplicates() {
        let mut engine = CompletionEngine::new();
        // "exit" is already a builtin; registering as agent shouldn't produce dups
        engine.register_agent("exit".to_string());
        let completions = engine.complete("exit");
        let count = completions.iter().filter(|s| *s == "exit").count();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_truncation_cap_at_20() {
        let mut engine = CompletionEngine::new();
        for i in 0..30 {
            engine.register_agent(format!("aaa-agent-{:02}", i));
        }
        let completions = engine.complete("aaa");
        assert!(completions.len() <= 20);
    }

    #[test]
    fn test_complete_no_match() {
        let engine = CompletionEngine::new();
        let completions = engine.complete("zzzznonexistent");
        assert!(completions.is_empty());
    }

    #[test]
    fn test_default_impl() {
        let engine = CompletionEngine::default();
        assert!(engine.agent_count() == 0);
        assert!(engine.service_count() == 0);
        // Builtins should still be present
        assert!(!engine.complete("hel").is_empty());
    }

    #[test]
    fn test_contextual_empty_words() {
        let engine = CompletionEngine::new();
        let completions = engine.complete_contextual(&[]);
        assert!(completions.is_empty());
    }

    #[test]
    fn test_multiple_sources_merged() {
        let mut engine = CompletionEngine::new();
        // "scan" is an intent keyword; register an agent starting with "scan"
        engine.register_agent("scan-bot".to_string());
        let completions = engine.complete("scan");
        assert!(completions.contains(&"scan".to_string())); // intent keyword
        assert!(completions.contains(&"scan-bot".to_string())); // agent
    }
}
