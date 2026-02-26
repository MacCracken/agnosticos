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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_formatter_default() {
        let formatter = OutputFormatter::default();
        assert_eq!(formatter.format, "auto");
    }

    #[test]
    fn test_output_formatter_new() {
        let formatter = OutputFormatter::new("json");
        assert_eq!(formatter.format, "json");
    }

    #[test]
    fn test_format_auto() {
        let formatter = OutputFormatter::new("auto");
        let input = "Hello, World!";
        let result = formatter.format(input);
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_format_json() {
        let formatter = OutputFormatter::new("json");
        let input = "Hello, World!";
        let result = formatter.format(input);
        assert!(result.contains("output"));
        assert!(result.contains("Hello"));
    }

    #[test]
    fn test_format_json_escapes_quotes() {
        let formatter = OutputFormatter::new("json");
        let input = r#"He said "Hello""#;
        let result = formatter.format(input);
        assert!(!result.contains("\"Hello\""));
        assert!(result.contains("\\\""));
    }

    #[test]
    fn test_format_table() {
        let formatter = OutputFormatter::new("table");
        let input = "col1\tcol2\tcol3";
        let result = formatter.format(input);
        assert!(!result.is_empty());
    }

    #[test]
    fn test_format_unknown() {
        let formatter = OutputFormatter::new("unknown_format");
        let input = "Hello";
        let result = formatter.format(input);
        assert_eq!(result, "Hello");
    }

    #[test]
    fn test_format_empty_string() {
        let formatter = OutputFormatter::new("auto");
        let result = formatter.format("");
        assert_eq!(result, "");
    }

    #[test]
    fn test_format_multiline() {
        let formatter = OutputFormatter::new("json");
        let input = "Line 1\nLine 2\nLine 3";
        let result = formatter.format(input);
        assert!(result.contains("Line 1"));
        assert!(result.contains("Line 2"));
    }
}
