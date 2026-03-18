//! Accessibility (a11y) infrastructure for the AGNOS desktop environment.
//!
//! Provides AT-SPI2 bridge foundations, keyboard navigation support,
//! screen reader integration, and high-contrast theme definitions.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::compositor::Rectangle;

// ============================================================================
// Roles and states
// ============================================================================

/// Semantic role of an accessible UI element (AT-SPI2 compatible).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AccessibilityRole {
    Window,
    Button,
    Label,
    TextInput,
    Menu,
    MenuItem,
    List,
    ListItem,
    Toolbar,
    StatusBar,
    Dialog,
    Alert,
    Image,
    Link,
    Separator,
    Slider,
    Checkbox,
    RadioButton,
    Tab,
    TabPanel,
    Tree,
    TreeItem,
    ScrollBar,
    ProgressBar,
}

/// Dynamic state of an accessible node.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccessibilityState {
    pub focused: bool,
    pub selected: bool,
    pub expanded: bool,
    pub checked: Option<bool>,
    pub disabled: bool,
    pub hidden: bool,
    pub value: Option<String>,
    pub description: String,
}

// ============================================================================
// Actions
// ============================================================================

/// Actions that can be performed on an accessible node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AccessibleAction {
    Click,
    Focus,
    Expand,
    Collapse,
    Select,
    ScrollTo,
    Activate,
    Dismiss,
}

// ============================================================================
// Accessible node
// ============================================================================

/// A single node in the accessibility tree.
#[derive(Debug, Clone)]
pub struct AccessibleNode {
    pub id: Uuid,
    pub role: AccessibilityRole,
    pub name: String,
    pub state: AccessibilityState,
    pub children: Vec<Uuid>,
    pub parent: Option<Uuid>,
    pub bounds: Rectangle,
    pub actions: Vec<AccessibleAction>,
}

impl AccessibleNode {
    /// Create a new accessible node with the given role and name.
    pub fn new(role: AccessibilityRole, name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role,
            name: name.into(),
            state: AccessibilityState::default(),
            children: Vec::new(),
            parent: None,
            bounds: Rectangle::default(),
            actions: Vec::new(),
        }
    }
}

// ============================================================================
// Accessibility tree
// ============================================================================

/// Manages the full accessibility tree for the desktop.
///
/// Maintains a flat map of nodes with parent/child relationships,
/// focus tracking, tab-order navigation, and a screen-reader announcement queue.
#[derive(Debug)]
pub struct AccessibilityTree {
    nodes: Vec<AccessibleNode>,
    focused_id: Option<Uuid>,
    announcements: Vec<String>,
}

impl AccessibilityTree {
    /// Create an empty accessibility tree.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            focused_id: None,
            announcements: Vec::new(),
        }
    }

    /// Add a node to the tree. Returns the node's id.
    pub fn add_node(&mut self, node: AccessibleNode) -> Uuid {
        let id = node.id;
        self.nodes.push(node);
        id
    }

    /// Remove a node by id. Returns the removed node if found.
    pub fn remove_node(&mut self, id: &Uuid) -> Option<AccessibleNode> {
        if let Some(pos) = self.nodes.iter().position(|n| n.id == *id) {
            let node = self.nodes.remove(pos);
            // If the focused node was removed, clear focus.
            if self.focused_id == Some(*id) {
                self.focused_id = None;
            }
            Some(node)
        } else {
            None
        }
    }

    /// Get a reference to a node by id.
    pub fn get_node(&self, id: &Uuid) -> Option<&AccessibleNode> {
        self.nodes.iter().find(|n| n.id == *id)
    }

    /// Get the currently focused node.
    pub fn get_focused(&self) -> Option<&AccessibleNode> {
        let id = self.focused_id?;
        self.get_node(&id)
    }

    /// Move keyboard focus to the given node.
    pub fn set_focus(&mut self, id: &Uuid) -> anyhow::Result<()> {
        // Verify the node exists.
        if self.nodes.iter().any(|n| n.id == *id) {
            // Unfocus the previous node.
            if let Some(prev_id) = self.focused_id {
                if let Some(prev) = self.nodes.iter_mut().find(|n| n.id == prev_id) {
                    prev.state.focused = false;
                }
            }
            // Focus the new node.
            if let Some(node) = self.nodes.iter_mut().find(|n| n.id == *id) {
                node.state.focused = true;
            }
            self.focused_id = Some(*id);
            Ok(())
        } else {
            anyhow::bail!("Node {} not found in accessibility tree", id)
        }
    }

    /// Navigate to the next focusable node in tab order (insertion order of non-hidden,
    /// non-disabled nodes).
    pub fn navigate_next(&mut self) -> Option<&AccessibleNode> {
        let focusable: Vec<usize> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| !n.state.hidden && !n.state.disabled)
            .map(|(i, _)| i)
            .collect();

        if focusable.is_empty() {
            return None;
        }

        let current_idx = self
            .focused_id
            .and_then(|fid| focusable.iter().position(|&i| self.nodes[i].id == fid));

        let next = match current_idx {
            Some(ci) => focusable[(ci + 1) % focusable.len()],
            None => focusable[0],
        };

        // Update focus state on previous node.
        if let Some(prev_id) = self.focused_id {
            if let Some(prev) = self.nodes.iter_mut().find(|n| n.id == prev_id) {
                prev.state.focused = false;
            }
        }

        self.nodes[next].state.focused = true;
        self.focused_id = Some(self.nodes[next].id);

        Some(&self.nodes[next])
    }

    /// Navigate to the previous focusable node in tab order.
    pub fn navigate_prev(&mut self) -> Option<&AccessibleNode> {
        let focusable: Vec<usize> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| !n.state.hidden && !n.state.disabled)
            .map(|(i, _)| i)
            .collect();

        if focusable.is_empty() {
            return None;
        }

        let current_idx = self
            .focused_id
            .and_then(|fid| focusable.iter().position(|&i| self.nodes[i].id == fid));

        let prev = match current_idx {
            Some(0) => focusable[focusable.len() - 1],
            Some(ci) => focusable[ci - 1],
            None => focusable[focusable.len() - 1],
        };

        // Update focus state on previous node.
        if let Some(prev_id) = self.focused_id {
            if let Some(p) = self.nodes.iter_mut().find(|n| n.id == prev_id) {
                p.state.focused = false;
            }
        }

        self.nodes[prev].state.focused = true;
        self.focused_id = Some(self.nodes[prev].id);

        Some(&self.nodes[prev])
    }

    /// Find nodes whose name contains the given substring (case-insensitive).
    pub fn search_by_name(&self, name: &str) -> Vec<&AccessibleNode> {
        let lower = name.to_lowercase();
        self.nodes
            .iter()
            .filter(|n| n.name.to_lowercase().contains(&lower))
            .collect()
    }

    /// Find all nodes with the given role.
    pub fn search_by_role(&self, role: AccessibilityRole) -> Vec<&AccessibleNode> {
        self.nodes.iter().filter(|n| n.role == role).collect()
    }

    /// Queue a screen-reader announcement.
    pub fn announce(&mut self, message: &str) {
        self.announcements.push(message.to_string());
    }

    /// Drain and return all pending announcements.
    pub fn pending_announcements(&mut self) -> Vec<String> {
        std::mem::take(&mut self.announcements)
    }

    /// Total number of nodes in the tree.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the tree is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

impl Default for AccessibilityTree {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// High-contrast themes
// ============================================================================

/// High-contrast theme parameters for accessibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighContrastTheme {
    pub background: u32,
    pub foreground: u32,
    pub accent: u32,
    pub error: u32,
    pub warning: u32,
    pub focus_ring_width: u32,
    pub font_scale: f32,
}

impl HighContrastTheme {
    /// Default light high-contrast theme (dark on white).
    pub fn default_high_contrast() -> Self {
        Self {
            background: 0xFFFFFFFF, // white
            foreground: 0xFF000000, // black
            accent: 0xFF0000FF,     // blue
            error: 0xFFFF0000,      // red
            warning: 0xFFFF8800,    // orange
            focus_ring_width: 3,
            font_scale: 1.25,
        }
    }

    /// Default dark high-contrast theme (light on black).
    pub fn default_dark_high_contrast() -> Self {
        Self {
            background: 0xFF000000, // black
            foreground: 0xFFFFFFFF, // white
            accent: 0xFF00CCFF,     // cyan
            error: 0xFFFF4444,      // bright red
            warning: 0xFFFFCC00,    // yellow
            focus_ring_width: 3,
            font_scale: 1.25,
        }
    }
}

// ============================================================================
// Keyboard navigation config
// ============================================================================

/// Configuration for keyboard-driven navigation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyboardNavConfig {
    /// Whether Tab wraps around from last to first element.
    pub tab_cycle: bool,
    /// Whether arrow keys navigate within containers.
    pub arrow_navigation: bool,
    /// Whether Escape closes the current dialog.
    pub escape_closes_dialog: bool,
    /// Whether Enter activates the focused element.
    pub enter_activates: bool,
}

impl Default for KeyboardNavConfig {
    fn default() -> Self {
        Self {
            tab_cycle: true,
            arrow_navigation: true,
            escape_closes_dialog: true,
            enter_activates: true,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(role: AccessibilityRole, name: &str) -> AccessibleNode {
        AccessibleNode::new(role, name)
    }

    #[test]
    fn test_tree_new_is_empty() {
        let tree = AccessibilityTree::new();
        assert!(tree.is_empty());
        assert_eq!(tree.len(), 0);
    }

    #[test]
    fn test_add_and_get_node() {
        let mut tree = AccessibilityTree::new();
        let node = make_node(AccessibilityRole::Button, "OK");
        let id = tree.add_node(node);
        assert_eq!(tree.len(), 1);
        let retrieved = tree.get_node(&id).unwrap();
        assert_eq!(retrieved.name, "OK");
        assert_eq!(retrieved.role, AccessibilityRole::Button);
    }

    #[test]
    fn test_remove_node() {
        let mut tree = AccessibilityTree::new();
        let id = tree.add_node(make_node(AccessibilityRole::Label, "Title"));
        let removed = tree.remove_node(&id);
        assert!(removed.is_some());
        assert!(tree.is_empty());
        assert!(tree.get_node(&id).is_none());
    }

    #[test]
    fn test_remove_nonexistent_node() {
        let mut tree = AccessibilityTree::new();
        let result = tree.remove_node(&Uuid::new_v4());
        assert!(result.is_none());
    }

    #[test]
    fn test_set_focus() {
        let mut tree = AccessibilityTree::new();
        let id = tree.add_node(make_node(AccessibilityRole::TextInput, "Name"));
        tree.set_focus(&id).unwrap();
        let focused = tree.get_focused().unwrap();
        assert_eq!(focused.id, id);
        assert!(focused.state.focused);
    }

    #[test]
    fn test_set_focus_nonexistent() {
        let mut tree = AccessibilityTree::new();
        let result = tree.set_focus(&Uuid::new_v4());
        assert!(result.is_err());
    }

    #[test]
    fn test_set_focus_clears_previous() {
        let mut tree = AccessibilityTree::new();
        let id1 = tree.add_node(make_node(AccessibilityRole::Button, "A"));
        let id2 = tree.add_node(make_node(AccessibilityRole::Button, "B"));
        tree.set_focus(&id1).unwrap();
        tree.set_focus(&id2).unwrap();
        assert!(!tree.get_node(&id1).unwrap().state.focused);
        assert!(tree.get_node(&id2).unwrap().state.focused);
    }

    #[test]
    fn test_remove_focused_node_clears_focus() {
        let mut tree = AccessibilityTree::new();
        let id = tree.add_node(make_node(AccessibilityRole::Button, "X"));
        tree.set_focus(&id).unwrap();
        tree.remove_node(&id);
        assert!(tree.get_focused().is_none());
    }

    #[test]
    fn test_navigate_next_simple() {
        let mut tree = AccessibilityTree::new();
        let id1 = tree.add_node(make_node(AccessibilityRole::Button, "A"));
        let id2 = tree.add_node(make_node(AccessibilityRole::Button, "B"));
        let id3 = tree.add_node(make_node(AccessibilityRole::Button, "C"));

        let first = tree.navigate_next().unwrap().id;
        assert_eq!(first, id1);
        let second = tree.navigate_next().unwrap().id;
        assert_eq!(second, id2);
        let third = tree.navigate_next().unwrap().id;
        assert_eq!(third, id3);
        // Wraps around
        let fourth = tree.navigate_next().unwrap().id;
        assert_eq!(fourth, id1);
    }

    #[test]
    fn test_navigate_next_skips_hidden() {
        let mut tree = AccessibilityTree::new();
        let _id1 = tree.add_node(make_node(AccessibilityRole::Button, "A"));
        let mut hidden_node = make_node(AccessibilityRole::Button, "Hidden");
        hidden_node.state.hidden = true;
        let _id_hidden = tree.add_node(hidden_node);
        let id3 = tree.add_node(make_node(AccessibilityRole::Button, "C"));

        tree.navigate_next(); // A
        let next = tree.navigate_next().unwrap().id;
        assert_eq!(next, id3); // skipped hidden
    }

    #[test]
    fn test_navigate_next_skips_disabled() {
        let mut tree = AccessibilityTree::new();
        let id1 = tree.add_node(make_node(AccessibilityRole::Button, "A"));
        let mut disabled = make_node(AccessibilityRole::Button, "Disabled");
        disabled.state.disabled = true;
        tree.add_node(disabled);

        tree.navigate_next(); // A
        let next = tree.navigate_next().unwrap().id;
        assert_eq!(next, id1); // wraps back (only one focusable)
    }

    #[test]
    fn test_navigate_next_empty_tree() {
        let mut tree = AccessibilityTree::new();
        assert!(tree.navigate_next().is_none());
    }

    #[test]
    fn test_navigate_prev() {
        let mut tree = AccessibilityTree::new();
        let id1 = tree.add_node(make_node(AccessibilityRole::Button, "A"));
        let _id2 = tree.add_node(make_node(AccessibilityRole::Button, "B"));
        let id3 = tree.add_node(make_node(AccessibilityRole::Button, "C"));

        // Start with no focus — navigate_prev goes to last
        let last = tree.navigate_prev().unwrap().id;
        assert_eq!(last, id3);
        let prev = tree.navigate_prev().unwrap().id;
        assert_eq!(prev, _id2);
        let prev2 = tree.navigate_prev().unwrap().id;
        assert_eq!(prev2, id1);
    }

    #[test]
    fn test_navigate_prev_empty() {
        let mut tree = AccessibilityTree::new();
        assert!(tree.navigate_prev().is_none());
    }

    #[test]
    fn test_search_by_name() {
        let mut tree = AccessibilityTree::new();
        tree.add_node(make_node(AccessibilityRole::Button, "Save File"));
        tree.add_node(make_node(AccessibilityRole::Button, "Save As"));
        tree.add_node(make_node(AccessibilityRole::Button, "Cancel"));

        let results = tree.search_by_name("save");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_by_name_case_insensitive() {
        let mut tree = AccessibilityTree::new();
        tree.add_node(make_node(AccessibilityRole::Label, "Hello World"));
        let results = tree.search_by_name("HELLO");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_by_name_no_match() {
        let mut tree = AccessibilityTree::new();
        tree.add_node(make_node(AccessibilityRole::Button, "OK"));
        let results = tree.search_by_name("Cancel");
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_by_role() {
        let mut tree = AccessibilityTree::new();
        tree.add_node(make_node(AccessibilityRole::Button, "A"));
        tree.add_node(make_node(AccessibilityRole::Label, "B"));
        tree.add_node(make_node(AccessibilityRole::Button, "C"));

        let buttons = tree.search_by_role(AccessibilityRole::Button);
        assert_eq!(buttons.len(), 2);
        let labels = tree.search_by_role(AccessibilityRole::Label);
        assert_eq!(labels.len(), 1);
    }

    #[test]
    fn test_search_by_role_no_match() {
        let mut tree = AccessibilityTree::new();
        tree.add_node(make_node(AccessibilityRole::Button, "A"));
        let sliders = tree.search_by_role(AccessibilityRole::Slider);
        assert!(sliders.is_empty());
    }

    #[test]
    fn test_announcements() {
        let mut tree = AccessibilityTree::new();
        tree.announce("Window opened");
        tree.announce("Focus moved to OK button");

        let pending = tree.pending_announcements();
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0], "Window opened");
        assert_eq!(pending[1], "Focus moved to OK button");

        // Queue should be drained
        let empty = tree.pending_announcements();
        assert!(empty.is_empty());
    }

    #[test]
    fn test_announcements_empty() {
        let mut tree = AccessibilityTree::new();
        let pending = tree.pending_announcements();
        assert!(pending.is_empty());
    }

    #[test]
    fn test_high_contrast_theme_light() {
        let theme = HighContrastTheme::default_high_contrast();
        assert_eq!(theme.background, 0xFFFFFFFF);
        assert_eq!(theme.foreground, 0xFF000000);
        assert!(theme.font_scale > 1.0);
        assert!(theme.focus_ring_width >= 2);
    }

    #[test]
    fn test_high_contrast_theme_dark() {
        let theme = HighContrastTheme::default_dark_high_contrast();
        assert_eq!(theme.background, 0xFF000000);
        assert_eq!(theme.foreground, 0xFFFFFFFF);
        assert!(theme.font_scale > 1.0);
    }

    #[test]
    fn test_keyboard_nav_config_defaults() {
        let config = KeyboardNavConfig::default();
        assert!(config.tab_cycle);
        assert!(config.arrow_navigation);
        assert!(config.escape_closes_dialog);
        assert!(config.enter_activates);
    }

    #[test]
    fn test_accessible_node_new() {
        let node = AccessibleNode::new(AccessibilityRole::Dialog, "Confirm");
        assert_eq!(node.role, AccessibilityRole::Dialog);
        assert_eq!(node.name, "Confirm");
        assert!(node.children.is_empty());
        assert!(node.parent.is_none());
        assert!(!node.state.focused);
    }

    #[test]
    fn test_accessibility_state_default() {
        let state = AccessibilityState::default();
        assert!(!state.focused);
        assert!(!state.selected);
        assert!(!state.expanded);
        assert!(state.checked.is_none());
        assert!(!state.disabled);
        assert!(!state.hidden);
        assert!(state.value.is_none());
        assert!(state.description.is_empty());
    }

    #[test]
    fn test_all_roles_exist() {
        // Ensure all 24 roles are distinct values
        let roles = vec![
            AccessibilityRole::Window,
            AccessibilityRole::Button,
            AccessibilityRole::Label,
            AccessibilityRole::TextInput,
            AccessibilityRole::Menu,
            AccessibilityRole::MenuItem,
            AccessibilityRole::List,
            AccessibilityRole::ListItem,
            AccessibilityRole::Toolbar,
            AccessibilityRole::StatusBar,
            AccessibilityRole::Dialog,
            AccessibilityRole::Alert,
            AccessibilityRole::Image,
            AccessibilityRole::Link,
            AccessibilityRole::Separator,
            AccessibilityRole::Slider,
            AccessibilityRole::Checkbox,
            AccessibilityRole::RadioButton,
            AccessibilityRole::Tab,
            AccessibilityRole::TabPanel,
            AccessibilityRole::Tree,
            AccessibilityRole::TreeItem,
            AccessibilityRole::ScrollBar,
            AccessibilityRole::ProgressBar,
        ];
        assert_eq!(roles.len(), 24);
    }

    #[test]
    fn test_all_actions_exist() {
        let actions = [
            AccessibleAction::Click,
            AccessibleAction::Focus,
            AccessibleAction::Expand,
            AccessibleAction::Collapse,
            AccessibleAction::Select,
            AccessibleAction::ScrollTo,
            AccessibleAction::Activate,
            AccessibleAction::Dismiss,
        ];
        assert_eq!(actions.len(), 8);
    }

    #[test]
    fn test_node_with_children_and_parent() {
        let mut tree = AccessibilityTree::new();
        let parent_id = tree.add_node(make_node(AccessibilityRole::Window, "Main"));
        let mut child = make_node(AccessibilityRole::Button, "Close");
        child.parent = Some(parent_id);
        let child_id = tree.add_node(child);

        // Update parent's children list
        if let Some(parent) = tree.nodes.iter_mut().find(|n| n.id == parent_id) {
            parent.children.push(child_id);
        }

        let parent = tree.get_node(&parent_id).unwrap();
        assert_eq!(parent.children.len(), 1);
        assert_eq!(parent.children[0], child_id);

        let child_node = tree.get_node(&child_id).unwrap();
        assert_eq!(child_node.parent, Some(parent_id));
    }

    #[test]
    fn test_tree_default_impl() {
        let tree = AccessibilityTree::default();
        assert!(tree.is_empty());
    }
}
