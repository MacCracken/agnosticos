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
        serde_json::json!({"output": output}).to_string()
    }

    fn format_table(&self, output: &str) -> String {
        let lines: Vec<&str> = output.lines().collect();
        if lines.is_empty() {
            return output.to_string();
        }

        // Split each line into columns (tab-separated or multi-space-separated)
        let rows: Vec<Vec<&str>> = lines
            .iter()
            .map(|line| {
                if line.contains('\t') {
                    line.split('\t').collect()
                } else {
                    line.split("  ")
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .collect()
                }
            })
            .collect();

        if rows.is_empty() {
            return output.to_string();
        }

        // Find maximum column count and widths
        let max_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        if max_cols <= 1 {
            // Not tabular data, return as-is
            return output.to_string();
        }

        let mut col_widths = vec![0usize; max_cols];
        for row in &rows {
            for (i, cell) in row.iter().enumerate() {
                col_widths[i] = col_widths[i].max(cell.len());
            }
        }

        // Render aligned columns
        let mut result = String::new();
        for row in &rows {
            for (i, cell) in row.iter().enumerate() {
                if i > 0 {
                    result.push_str("  ");
                }
                let width = col_widths.get(i).copied().unwrap_or(0);
                result.push_str(&format!("{:<width$}", cell, width = width));
            }
            result.push('\n');
        }

        // Remove trailing newline to match input convention
        if result.ends_with('\n') {
            result.pop();
        }

        result
    }

    fn format_auto(&self, output: &str) -> String {
        // Auto-detect format based on content
        let trimmed = output.trim();

        // If it looks like JSON, format as JSON
        if (trimmed.starts_with('{') && trimmed.ends_with('}'))
            || (trimmed.starts_with('[') && trimmed.ends_with(']'))
        {
            return self.format_json(output);
        }

        // If it contains tabs or multi-space separators with multiple lines, format as table
        let lines: Vec<&str> = output.lines().collect();
        if lines.len() > 1 && lines.iter().any(|l| l.contains('\t') || l.contains("  ")) {
            return self.format_table(output);
        }

        // Otherwise return as-is
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
    fn test_format_auto_plain_text() {
        let formatter = OutputFormatter::new("auto");
        let input = "Hello, World!";
        let result = formatter.format(input);
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_format_auto_detects_json_object() {
        let formatter = OutputFormatter::new("auto");
        let input = r#"{"key": "value"}"#;
        let result = formatter.format(input);
        assert!(result.contains("output"));
    }

    #[test]
    fn test_format_auto_detects_json_array() {
        let formatter = OutputFormatter::new("auto");
        let input = r#"[1, 2, 3]"#;
        let result = formatter.format(input);
        assert!(result.contains("output"));
    }

    #[test]
    fn test_format_auto_detects_table() {
        let formatter = OutputFormatter::new("auto");
        let input = "col1\tcol2\tcol3\nval1\tval2\tval3";
        let result = formatter.format(input);
        // Should be aligned
        assert!(result.contains("col1"));
        assert!(result.contains("val1"));
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
        // serde_json properly escapes quotes
        assert!(result.contains("\\\""));
    }

    #[test]
    fn test_format_table_tab_separated() {
        let formatter = OutputFormatter::new("table");
        let input = "name\tage\tcity\nAlice\t30\tNYC\nBob\t25\tLA";
        let result = formatter.format(input);
        // Columns should be aligned
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 3);
        // All "age" column values should start at the same position
        let pos_0 = lines[0].find("age").unwrap();
        let pos_1 = lines[1].find("30").unwrap();
        assert_eq!(pos_0, pos_1);
    }

    #[test]
    fn test_format_table_multi_space_separated() {
        let formatter = OutputFormatter::new("table");
        let input = "PID  USER  CPU\n123  root  50\n456  www   10";
        let result = formatter.format(input);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_format_table_single_column_passthrough() {
        let formatter = OutputFormatter::new("table");
        let input = "just\nsome\nlines";
        let result = formatter.format(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_format_table_empty() {
        let formatter = OutputFormatter::new("table");
        let result = formatter.format("");
        assert_eq!(result, "");
    }

    #[test]
    fn test_format_table_uneven_columns() {
        let formatter = OutputFormatter::new("table");
        let input = "a\tb\tc\n1\t2";
        let result = formatter.format(input);
        assert!(result.contains("a"));
        assert!(result.contains("1"));
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

    #[test]
    fn test_format_auto_single_line_no_table() {
        let formatter = OutputFormatter::new("auto");
        let input = "simple single line";
        let result = formatter.format(input);
        assert_eq!(result, input);
    }
}
