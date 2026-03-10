//! Command history management

use anyhow::Result;
use std::collections::VecDeque;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

pub struct CommandHistory {
    pub(crate) entries: VecDeque<String>,
    pub(crate) file: PathBuf,
    pub(crate) max_size: usize,
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
        let content: String = self
            .entries
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        if let Some(parent) = self.file.parent() {
            let parent_existed = parent.exists();
            tokio::fs::create_dir_all(parent).await?;
            // Restrict newly-created parent directory to owner-only access (0700)
            if !parent_existed {
                tokio::fs::set_permissions(parent, Permissions::from_mode(0o700)).await?;
            }
        }

        tokio::fs::write(&self.file, content).await?;
        // Restrict history file to owner-only read/write (0600)
        tokio::fs::set_permissions(&self.file, Permissions::from_mode(0o600)).await?;
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

    #[tokio::test]
    async fn test_history_add() {
        let mut history = CommandHistory {
            entries: VecDeque::new(),
            file: PathBuf::from("/tmp/test_add"),
            max_size: 100,
        };

        history.add("ls -la").await.unwrap();
        history.add("cd /home").await.unwrap();
        assert_eq!(history.entries.len(), 2);
    }

    #[tokio::test]
    async fn test_history_add_no_duplicate() {
        let mut history = CommandHistory {
            entries: VecDeque::new(),
            file: PathBuf::from("/tmp/test_dup"),
            max_size: 100,
        };

        history.add("ls").await.unwrap();
        history.add("ls").await.unwrap();
        assert_eq!(history.entries.len(), 1);
    }

    #[tokio::test]
    async fn test_history_add_trim() {
        let mut history = CommandHistory {
            entries: VecDeque::new(),
            file: PathBuf::from("/tmp/test_trim"),
            max_size: 3,
        };

        history.add("cmd1").await.unwrap();
        history.add("cmd2").await.unwrap();
        history.add("cmd3").await.unwrap();
        history.add("cmd4").await.unwrap();
        assert_eq!(history.entries.len(), 3);
        assert_eq!(history.entries[0], "cmd2");
    }

    #[tokio::test]
    async fn test_history_save_and_load() {
        let dir = std::env::temp_dir().join("agnos_history_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("history.txt");
        let _ = std::fs::remove_file(&path);

        let mut history = CommandHistory {
            entries: VecDeque::new(),
            file: path.clone(),
            max_size: 100,
        };

        history.add("ls").await.unwrap();
        history.add("pwd").await.unwrap();
        history.save().await.unwrap();

        let loaded = CommandHistory::new(&path).await.unwrap();
        assert_eq!(loaded.entries.len(), 2);
        assert_eq!(loaded.entries[0], "ls");
        assert_eq!(loaded.entries[1], "pwd");

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn test_history_save_permissions() {
        let dir = std::env::temp_dir().join("agnos_history_perm_test");
        let _ = std::fs::remove_dir_all(&dir);

        let path = dir.join("history.txt");

        let mut history = CommandHistory {
            entries: VecDeque::new(),
            file: path.clone(),
            max_size: 100,
        };

        history.add("secret-command").await.unwrap();
        history.save().await.unwrap();

        // Verify file permissions are 0600
        let file_meta = std::fs::metadata(&path).unwrap();
        let file_mode = file_meta.permissions().mode() & 0o777;
        assert_eq!(
            file_mode, 0o600,
            "history file should have 0600 permissions, got {:o}",
            file_mode
        );

        // Verify parent directory permissions are 0700
        let dir_meta = std::fs::metadata(&dir).unwrap();
        let dir_mode = dir_meta.permissions().mode() & 0o777;
        assert_eq!(
            dir_mode, 0o700,
            "history directory should have 0700 permissions, got {:o}",
            dir_mode
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_history_new_nonexistent_file() {
        let path = PathBuf::from("/tmp/agnos_nonexistent_history_12345.txt");
        let _ = std::fs::remove_file(&path);
        let history = CommandHistory::new(&path).await.unwrap();
        assert!(history.entries.is_empty());
    }
}
