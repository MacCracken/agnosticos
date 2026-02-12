//! Command history management

use anyhow::Result;
use std::collections::VecDeque;
use std::path::PathBuf;

pub struct CommandHistory {
    entries: VecDeque<String>,
    file: PathBuf,
    max_size: usize,
}

impl CommandHistory {
    pub async fn new(file: &PathBuf) -> Result<Self> {
        let mut entries = VecDeque::new();
        
        // Load existing history
        if file.exists() {
            let content = tokio::fs::read_to_string(file).await?;
            for line in content.lines() {
                entries.push_back(line.to_string());
            }
        }
        
        Ok(Self {
            entries,
            file: file.clone(),
            max_size: 10000,
        })
    }
    
    pub async fn add(&mut self, command: &str) -> Result<()> {
        // Don't add duplicates at the end
        if self.entries.back() != Some(&command.to_string()) {
            self.entries.push_back(command.to_string());
            
            // Trim if too large
            while self.entries.len() > self.max_size {
                self.entries.pop_front();
            }
        }
        
        Ok(())
    }
    
    pub fn get_recent(&self, n: usize) -> Vec<&String> {
        self.entries.iter().rev().take(n).collect()
    }
    
    pub fn search(&self, query: &str) -> Vec<&String> {
        self.entries
            .iter()
            .filter(|entry| entry.contains(query))
            .collect()
    }
    
    pub async fn save(&self) -> Result<()> {
        let content: String = self.entries.iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        
        if let Some(parent) = self.file.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        
        tokio::fs::write(&self.file, content).await?;
        Ok(())
    }
}

impl Drop for CommandHistory {
    fn drop(&mut self) {
        // Try to save on drop
        let _ = tokio::runtime::Handle::try_current()
            .map(|rt| rt.block_on(self.save()));
    }
}
