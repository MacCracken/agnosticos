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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_history_get_recent() {
        let mut entries = VecDeque::new();
        entries.push_back("cmd1".to_string());
        entries.push_back("cmd2".to_string());
        entries.push_back("cmd3".to_string());
        
        let history = CommandHistory {
            entries,
            file: PathBuf::from("/tmp/test"),
            max_size: 100,
        };
        
        let recent = history.get_recent(2);
        assert_eq!(recent.len(), 2);
    }

    #[test]
    fn test_history_search() {
        let mut entries = VecDeque::new();
        entries.push_back("ls -la".to_string());
        entries.push_back("cd /home".to_string());
        entries.push_back("git status".to_string());
        
        let history = CommandHistory {
            entries,
            file: PathBuf::from("/tmp/test"),
            max_size: 100,
        };
        
        let results = history.search("git");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_history_search_no_results() {
        let mut entries = VecDeque::new();
        entries.push_back("ls".to_string());
        
        let history = CommandHistory {
            entries,
            file: PathBuf::from("/tmp/test"),
            max_size: 100,
        };
        
        let results = history.search("xyz");
        assert!(results.is_empty());
    }

    #[test]
    fn test_history_get_recent_empty() {
        let entries = VecDeque::new();
        let history = CommandHistory {
            entries,
            file: PathBuf::from("/tmp/test"),
            max_size: 100,
        };
        
        let recent = history.get_recent(5);
        assert!(recent.is_empty());
    }

    #[test]
    fn test_history_search_multiple() {
        let mut entries = VecDeque::new();
        entries.push_back("git commit -m".to_string());
        entries.push_back("git push".to_string());
        entries.push_back("git status".to_string());
        entries.push_back("ls".to_string());
        
        let history = CommandHistory {
            entries,
            file: PathBuf::from("/tmp/test"),
            max_size: 100,
        };
        
        let results = history.search("git");
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_history_max_size() {
        let entries = VecDeque::new();
        let history = CommandHistory {
            entries,
            file: PathBuf::from("/tmp/test"),
            max_size: 100,
        };
        
        assert_eq!(history.max_size, 100);
    }
}
