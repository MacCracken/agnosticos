//! Output formatting

pub struct OutputFormatter {
    format: String,
}

impl OutputFormatter {
    pub fn new(format: &str) -> Self {
        Self {
            format: format.to_string(),
        }
    }
    
    /// Format command output
    pub fn format(&self, output: &str) -> String {
        match self.format.as_str() {
            "json" => self.format_json(output),
            "table" => self.format_table(output),
            "auto" => self.format_auto(output),
            _ => output.to_string(),
        }
    }
    
    fn format_json(&self, output: &str) -> String {
        // Simple JSON wrapping
        format!("{{\"output\": \"{}\"}}", output.replace('"', "\\\""))
    }
    
    fn format_table(&self, output: &str) -> String {
        // Detect if output looks like a table
        if output.contains('\t') || output.contains("  ") {
            // Attempt to format as table
            output.to_string()
        } else {
            output.to_string()
        }
    }
    
    fn format_auto(&self, output: &str) -> String {
        // Auto-detect format based on content
        output.to_string()
    }
}

impl Default for OutputFormatter {
    fn default() -> Self {
        Self::new("auto")
    }
}
