//! User-defined command aliases

use std::collections::HashMap;

/// Manages shell aliases (name -> expansion)
pub struct AliasManager {
    aliases: HashMap<String, String>,
}

impl AliasManager {
    pub fn new() -> Self {
        Self {
            aliases: HashMap::new(),
        }
    }

    pub fn from_config(aliases: HashMap<String, String>) -> Self {
        Self { aliases }
    }

    /// Define or update an alias
    pub fn set(&mut self, name: String, expansion: String) {
        self.aliases.insert(name, expansion);
    }

    /// Remove an alias
    pub fn remove(&mut self, name: &str) -> bool {
        self.aliases.remove(name).is_some()
    }

    /// Expand input if it starts with a known alias
    pub fn expand(&self, input: &str) -> String {
        let trimmed = input.trim();
        // Check if the first word matches an alias
        let first_word = trimmed.split_whitespace().next().unwrap_or("");
        if let Some(expansion) = self.aliases.get(first_word) {
            let rest = trimmed.strip_prefix(first_word).unwrap_or("").trim();
            if rest.is_empty() {
                expansion.clone()
            } else {
                format!("{} {}", expansion, rest)
            }
        } else {
            input.to_string()
        }
    }

    /// List all aliases
    pub fn list(&self) -> Vec<(&str, &str)> {
        let mut pairs: Vec<_> = self
            .aliases
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        pairs.sort_by_key(|(k, _)| *k);
        pairs
    }

    /// Get alias count
    pub fn count(&self) -> usize {
        self.aliases.len()
    }

    /// Check if an alias exists
    pub fn contains(&self, name: &str) -> bool {
        self.aliases.contains_key(name)
    }
}

impl Default for AliasManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_empty() {
        let mgr = AliasManager::new();
        assert_eq!(mgr.count(), 0);
        assert!(mgr.list().is_empty());
    }

    #[test]
    fn test_set_and_contains() {
        let mut mgr = AliasManager::new();
        mgr.set("ll".to_string(), "ls -la".to_string());
        assert!(mgr.contains("ll"));
        assert!(!mgr.contains("la"));
    }

    #[test]
    fn test_set_overwrites() {
        let mut mgr = AliasManager::new();
        mgr.set("ll".to_string(), "ls -la".to_string());
        mgr.set("ll".to_string(), "ls -lah".to_string());
        assert_eq!(mgr.count(), 1);
        assert_eq!(mgr.expand("ll"), "ls -lah");
    }

    #[test]
    fn test_remove_existing() {
        let mut mgr = AliasManager::new();
        mgr.set("ll".to_string(), "ls -la".to_string());
        assert!(mgr.remove("ll"));
        assert!(!mgr.contains("ll"));
        assert_eq!(mgr.count(), 0);
    }

    #[test]
    fn test_remove_nonexistent() {
        let mut mgr = AliasManager::new();
        assert!(!mgr.remove("nope"));
    }

    #[test]
    fn test_expand_simple() {
        let mut mgr = AliasManager::new();
        mgr.set("ll".to_string(), "ls -la".to_string());
        assert_eq!(mgr.expand("ll"), "ls -la");
    }

    #[test]
    fn test_expand_with_args() {
        let mut mgr = AliasManager::new();
        mgr.set("ll".to_string(), "ls -la".to_string());
        assert_eq!(mgr.expand("ll /tmp"), "ls -la /tmp");
    }

    #[test]
    fn test_expand_non_alias_passthrough() {
        let mgr = AliasManager::new();
        assert_eq!(mgr.expand("cat foo.txt"), "cat foo.txt");
    }

    #[test]
    fn test_expand_empty_input() {
        let mgr = AliasManager::new();
        assert_eq!(mgr.expand(""), "");
    }

    #[test]
    fn test_expand_whitespace_only() {
        let mgr = AliasManager::new();
        assert_eq!(mgr.expand("   "), "   ");
    }

    #[test]
    fn test_list_sorted() {
        let mut mgr = AliasManager::new();
        mgr.set("zz".to_string(), "zzz".to_string());
        mgr.set("aa".to_string(), "aaa".to_string());
        mgr.set("mm".to_string(), "mmm".to_string());
        let list = mgr.list();
        assert_eq!(list[0].0, "aa");
        assert_eq!(list[1].0, "mm");
        assert_eq!(list[2].0, "zz");
    }

    #[test]
    fn test_from_config() {
        let mut map = HashMap::new();
        map.insert("gs".to_string(), "git status".to_string());
        map.insert("gp".to_string(), "git push".to_string());
        let mgr = AliasManager::from_config(map);
        assert_eq!(mgr.count(), 2);
        assert!(mgr.contains("gs"));
        assert!(mgr.contains("gp"));
        assert_eq!(mgr.expand("gs"), "git status");
    }

    #[test]
    fn test_count() {
        let mut mgr = AliasManager::new();
        assert_eq!(mgr.count(), 0);
        mgr.set("a".to_string(), "b".to_string());
        assert_eq!(mgr.count(), 1);
        mgr.set("c".to_string(), "d".to_string());
        assert_eq!(mgr.count(), 2);
    }

    #[test]
    fn test_default_impl() {
        let mgr = AliasManager::default();
        assert_eq!(mgr.count(), 0);
    }

    #[test]
    fn test_expand_partial_word_no_match() {
        let mut mgr = AliasManager::new();
        mgr.set("ll".to_string(), "ls -la".to_string());
        // "llama" should NOT match alias "ll" (first word is "llama", not "ll")
        assert_eq!(mgr.expand("llama"), "llama");
    }

    #[test]
    fn test_expand_with_multiple_args() {
        let mut mgr = AliasManager::new();
        mgr.set("g".to_string(), "git".to_string());
        assert_eq!(
            mgr.expand("g commit -m 'hello world'"),
            "git commit -m 'hello world'"
        );
    }
}
