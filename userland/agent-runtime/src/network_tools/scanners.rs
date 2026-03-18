//! Vulnerability assessment and web fuzzing wrappers.

use anyhow::Result;

use super::runner::NetworkToolRunner;
use super::types::{NetworkTool, NetworkToolConfig, ToolOutput};

/// Vulnerability assessment wrapping nuclei and nikto.
#[derive(Debug)]
pub struct VulnAssessor {
    pub(super) runner: NetworkToolRunner,
    pub(super) use_nikto: bool,
    pub(super) severity_filter: Option<String>,
    pub(super) tags: Vec<String>,
}

impl VulnAssessor {
    pub fn new() -> Self {
        Self {
            runner: NetworkToolRunner {
                allow_dangerous: false,
                external_only: false,
            },
            use_nikto: false,
            severity_filter: None,
            tags: Vec::new(),
        }
    }

    /// Use nikto instead of nuclei.
    pub fn use_nikto(mut self, yes: bool) -> Self {
        self.use_nikto = yes;
        self
    }

    /// Filter by severity (nuclei: "critical,high,medium").
    pub fn severity(mut self, sev: &str) -> Self {
        self.severity_filter = Some(sev.to_string());
        self
    }

    /// Filter by template tags (nuclei: "cve", "rce", "sqli").
    pub fn tag(mut self, tag: &str) -> Self {
        self.tags.push(tag.to_string());
        self
    }

    /// Build argument list.
    pub fn build_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        if self.use_nikto {
            args.push("-h".into());
            return args;
        }

        // nuclei args
        if let Some(ref sev) = self.severity_filter {
            args.push("-severity".into());
            args.push(sev.clone());
        }
        for tag in &self.tags {
            args.push("-tags".into());
            args.push(tag.clone());
        }
        args.push("-u".into());
        args
    }

    /// Run vulnerability scan and return raw output summary.
    /// Structured parsing of nuclei/nikto results is deferred to beta.
    pub async fn scan(&self, target: &str) -> Result<ToolOutput> {
        let tool = if self.use_nikto {
            NetworkTool::WebScan
        } else {
            NetworkTool::VulnScanner
        };
        let config = NetworkToolConfig::for_tool(tool);
        let mut args = self.build_args();
        args.push(target.to_string());
        self.runner.run(&config, &args, None).await
    }
}

impl Default for VulnAssessor {
    fn default() -> Self {
        Self::new()
    }
}

/// Web directory/path fuzzing wrapping gobuster, ffuf, and dirb.
#[derive(Debug)]
pub struct WebFuzzer {
    pub(super) runner: NetworkToolRunner,
    pub(super) tool_choice: NetworkTool,
    pub(super) wordlist: Option<String>,
    pub(super) extensions: Vec<String>,
    pub(super) threads: Option<u32>,
    pub(super) status_codes: Option<String>,
}

impl WebFuzzer {
    pub fn new() -> Self {
        Self {
            runner: NetworkToolRunner::new(),
            tool_choice: NetworkTool::DirBust,
            wordlist: None,
            extensions: Vec::new(),
            threads: None,
            status_codes: None,
        }
    }

    /// Use ffuf instead of gobuster.
    pub fn use_ffuf(mut self) -> Self {
        self.tool_choice = NetworkTool::DirFuzz;
        self
    }

    /// Set wordlist path.
    pub fn wordlist(mut self, path: &str) -> Self {
        self.wordlist = Some(path.to_string());
        self
    }

    /// Add file extensions to check (e.g., "php", "html", "js").
    pub fn extension(mut self, ext: &str) -> Self {
        self.extensions.push(ext.to_string());
        self
    }

    /// Set thread count.
    pub fn threads(mut self, n: u32) -> Self {
        self.threads = Some(n);
        self
    }

    /// Filter by HTTP status codes (e.g., "200,301,302").
    pub fn status_codes(mut self, codes: &str) -> Self {
        self.status_codes = Some(codes.to_string());
        self
    }

    /// Build argument list.
    pub fn build_args(&self, target_url: &str) -> Vec<String> {
        let mut args = Vec::new();

        match self.tool_choice {
            NetworkTool::DirBust => {
                // gobuster
                args.push("dir".into());
                args.push("-u".into());
                args.push(target_url.to_string());
                if let Some(ref wl) = self.wordlist {
                    args.push("-w".into());
                    args.push(wl.clone());
                }
                if !self.extensions.is_empty() {
                    args.push("-x".into());
                    args.push(self.extensions.join(","));
                }
                if let Some(n) = self.threads {
                    args.push("-t".into());
                    args.push(n.to_string());
                }
                if let Some(ref codes) = self.status_codes {
                    args.push("-s".into());
                    args.push(codes.clone());
                }
            }
            NetworkTool::DirFuzz => {
                // ffuf
                args.push("-u".into());
                args.push(format!("{}/FUZZ", target_url));
                if let Some(ref wl) = self.wordlist {
                    args.push("-w".into());
                    args.push(wl.clone());
                }
                if !self.extensions.is_empty() {
                    args.push("-e".into());
                    args.push(format!(".{}", self.extensions.join(",.")));
                }
                if let Some(n) = self.threads {
                    args.push("-t".into());
                    args.push(n.to_string());
                }
                if let Some(ref codes) = self.status_codes {
                    args.push("-mc".into());
                    args.push(codes.clone());
                }
            }
            _ => {}
        }

        args
    }

    /// Run fuzzing and return raw output.
    pub async fn fuzz(&self, target_url: &str) -> Result<ToolOutput> {
        let config = NetworkToolConfig::for_tool(self.tool_choice);
        let args = self.build_args(target_url);
        self.runner.run(&config, &args, None).await
    }
}

impl Default for WebFuzzer {
    fn default() -> Self {
        Self::new()
    }
}
