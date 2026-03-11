//! Output capture for agent stdout/stderr.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// Ring buffer for capturing agent stdout/stderr output.
/// Queryable via API and shell (`agent logs <id>`).
#[derive(Debug, Clone)]
pub struct OutputCapture {
    buffer: VecDeque<OutputLine>,
    pub(super) max_lines: usize,
}

/// A single line of captured output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputLine {
    pub timestamp: String,
    pub stream: OutputStream,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

impl OutputCapture {
    pub fn new(max_lines: usize) -> Self {
        Self {
            buffer: VecDeque::new(),
            max_lines,
        }
    }

    /// Append a line to the buffer
    pub fn push(&mut self, stream: OutputStream, content: String) {
        let line = OutputLine {
            timestamp: chrono::Utc::now().to_rfc3339(),
            stream,
            content,
        };
        self.buffer.push_back(line);
        while self.buffer.len() > self.max_lines {
            self.buffer.pop_front();
        }
    }

    /// Get the last N lines
    pub fn tail(&self, n: usize) -> Vec<&OutputLine> {
        let skip = self.buffer.len().saturating_sub(n);
        self.buffer.iter().skip(skip).collect()
    }

    /// Get all lines
    pub fn all(&self) -> Vec<&OutputLine> {
        self.buffer.iter().collect()
    }

    /// Get lines from a specific stream only
    pub fn filter_stream(&self, stream: OutputStream) -> Vec<&OutputLine> {
        self.buffer.iter().filter(|l| l.stream == stream).collect()
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Number of lines in the buffer
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Format output for display
    pub fn format_display(&self, n: usize) -> String {
        let lines = self.tail(n);
        if lines.is_empty() {
            return "(no output captured)".to_string();
        }

        lines
            .iter()
            .map(|l| {
                let prefix = match l.stream {
                    OutputStream::Stdout => "OUT",
                    OutputStream::Stderr => "ERR",
                };
                format!("[{}] {} | {}", l.timestamp, prefix, l.content)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Default for OutputCapture {
    fn default() -> Self {
        Self::new(1000)
    }
}
