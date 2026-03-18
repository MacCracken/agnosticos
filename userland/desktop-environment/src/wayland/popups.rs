//! XDG Popup / Positioner support.

use std::collections::HashMap;

use uuid::Uuid;

use crate::compositor::{Rectangle, SurfaceId};

/// Edge anchor for popup positioning.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, Default,
)]
pub enum Edge {
    #[default]
    None,
    Top,
    Bottom,
    Left,
    Right,
}

/// Bitflags-style constraint adjustment for popup repositioning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConstraintAdjustment {
    pub slide_x: bool,
    pub slide_y: bool,
    pub flip_x: bool,
    pub flip_y: bool,
    pub resize_x: bool,
    pub resize_y: bool,
}

impl ConstraintAdjustment {
    /// All adjustments disabled.
    pub fn new() -> Self {
        Self {
            slide_x: false,
            slide_y: false,
            flip_x: false,
            flip_y: false,
            resize_x: false,
            resize_y: false,
        }
    }

    /// Slide adjustments on both axes.
    pub fn slide() -> Self {
        Self {
            slide_x: true,
            slide_y: true,
            ..Self::new()
        }
    }

    /// Flip adjustments on both axes.
    pub fn flip() -> Self {
        Self {
            flip_x: true,
            flip_y: true,
            ..Self::new()
        }
    }

    /// All adjustments enabled.
    pub fn all() -> Self {
        Self {
            slide_x: true,
            slide_y: true,
            flip_x: true,
            flip_y: true,
            resize_x: true,
            resize_y: true,
        }
    }
}

impl Default for ConstraintAdjustment {
    fn default() -> Self {
        Self::new()
    }
}

/// Describes how a popup should be positioned relative to its parent.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PopupPosition {
    pub anchor_rect: Rectangle,
    pub anchor_edge: Edge,
    pub gravity: Edge,
    pub offset_x: i32,
    pub offset_y: i32,
    pub constraint_adjustment: ConstraintAdjustment,
}

impl Default for PopupPosition {
    fn default() -> Self {
        Self {
            anchor_rect: Rectangle::default(),
            anchor_edge: Edge::None,
            gravity: Edge::None,
            offset_x: 0,
            offset_y: 0,
            constraint_adjustment: ConstraintAdjustment::new(),
        }
    }
}

/// An XDG popup surface.
#[derive(Debug, Clone)]
pub struct Popup {
    pub id: SurfaceId,
    pub parent: SurfaceId,
    pub position: PopupPosition,
    pub size: Rectangle,
    pub visible: bool,
    pub grab: bool,
}

/// Manages popup lifecycle and positioning.
#[derive(Debug)]
pub struct PopupManager {
    popups: HashMap<SurfaceId, Popup>,
    next_counter: u64,
}

impl PopupManager {
    /// Create a new popup manager.
    pub fn new() -> Self {
        Self {
            popups: HashMap::new(),
            next_counter: 0,
        }
    }

    /// Create a new popup attached to the given parent surface.
    /// Returns the id assigned to the new popup.
    pub fn create_popup(&mut self, parent: SurfaceId, position: PopupPosition) -> SurfaceId {
        let id = Uuid::new_v4();
        let popup = Popup {
            id,
            parent,
            position,
            size: Rectangle {
                x: 0,
                y: 0,
                width: 200,
                height: 100,
            },
            visible: true,
            grab: false,
        };
        self.popups.insert(id, popup);
        self.next_counter += 1;
        id
    }

    /// Dismiss (close) a popup by id. Returns the removed popup if found.
    pub fn dismiss_popup(&mut self, id: &SurfaceId) -> Option<Popup> {
        self.popups.remove(id)
    }

    /// Dismiss all popups.
    pub fn dismiss_all(&mut self) {
        self.popups.clear();
    }

    /// Get a reference to a popup by id.
    pub fn get_popup(&self, id: &SurfaceId) -> Option<&Popup> {
        self.popups.get(id)
    }

    /// List all visible popups.
    pub fn active_popups(&self) -> Vec<&Popup> {
        self.popups.values().filter(|p| p.visible).collect()
    }

    /// Reposition an existing popup.
    pub fn reposition(&mut self, id: &SurfaceId, position: PopupPosition) -> anyhow::Result<()> {
        if let Some(popup) = self.popups.get_mut(id) {
            popup.position = position;
            Ok(())
        } else {
            anyhow::bail!("Popup {} not found", id)
        }
    }

    /// Total number of managed popups.
    pub fn len(&self) -> usize {
        self.popups.len()
    }

    /// Whether there are no popups.
    pub fn is_empty(&self) -> bool {
        self.popups.is_empty()
    }
}

impl Default for PopupManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod popup_tests {
    use super::*;

    fn default_position() -> PopupPosition {
        PopupPosition::default()
    }

    #[test]
    fn test_popup_manager_new_empty() {
        let mgr = PopupManager::new();
        assert!(mgr.is_empty());
        assert_eq!(mgr.len(), 0);
    }

    #[test]
    fn test_create_popup() {
        let mut mgr = PopupManager::new();
        let parent = Uuid::new_v4();
        let id = mgr.create_popup(parent, default_position());
        assert_eq!(mgr.len(), 1);
        let popup = mgr.get_popup(&id).unwrap();
        assert_eq!(popup.parent, parent);
        assert!(popup.visible);
    }

    #[test]
    fn test_dismiss_popup() {
        let mut mgr = PopupManager::new();
        let parent = Uuid::new_v4();
        let id = mgr.create_popup(parent, default_position());
        let removed = mgr.dismiss_popup(&id);
        assert!(removed.is_some());
        assert!(mgr.is_empty());
    }

    #[test]
    fn test_dismiss_nonexistent() {
        let mut mgr = PopupManager::new();
        let result = mgr.dismiss_popup(&Uuid::new_v4());
        assert!(result.is_none());
    }

    #[test]
    fn test_dismiss_all() {
        let mut mgr = PopupManager::new();
        let parent = Uuid::new_v4();
        mgr.create_popup(parent, default_position());
        mgr.create_popup(parent, default_position());
        mgr.create_popup(parent, default_position());
        assert_eq!(mgr.len(), 3);
        mgr.dismiss_all();
        assert!(mgr.is_empty());
    }

    #[test]
    fn test_active_popups() {
        let mut mgr = PopupManager::new();
        let parent = Uuid::new_v4();
        let id1 = mgr.create_popup(parent, default_position());
        let _id2 = mgr.create_popup(parent, default_position());
        // Hide one
        mgr.popups.get_mut(&id1).unwrap().visible = false;
        let active = mgr.active_popups();
        assert_eq!(active.len(), 1);
    }

    #[test]
    fn test_reposition() {
        let mut mgr = PopupManager::new();
        let parent = Uuid::new_v4();
        let id = mgr.create_popup(parent, default_position());

        let new_pos = PopupPosition {
            offset_x: 50,
            offset_y: 100,
            ..default_position()
        };
        mgr.reposition(&id, new_pos).unwrap();
        let popup = mgr.get_popup(&id).unwrap();
        assert_eq!(popup.position.offset_x, 50);
        assert_eq!(popup.position.offset_y, 100);
    }

    #[test]
    fn test_reposition_nonexistent() {
        let mut mgr = PopupManager::new();
        let result = mgr.reposition(&Uuid::new_v4(), default_position());
        assert!(result.is_err());
    }

    #[test]
    fn test_constraint_adjustment_new() {
        let ca = ConstraintAdjustment::new();
        assert!(!ca.slide_x);
        assert!(!ca.slide_y);
        assert!(!ca.flip_x);
        assert!(!ca.flip_y);
        assert!(!ca.resize_x);
        assert!(!ca.resize_y);
    }

    #[test]
    fn test_constraint_adjustment_slide() {
        let ca = ConstraintAdjustment::slide();
        assert!(ca.slide_x);
        assert!(ca.slide_y);
        assert!(!ca.flip_x);
    }

    #[test]
    fn test_constraint_adjustment_flip() {
        let ca = ConstraintAdjustment::flip();
        assert!(ca.flip_x);
        assert!(ca.flip_y);
        assert!(!ca.slide_x);
    }

    #[test]
    fn test_constraint_adjustment_all() {
        let ca = ConstraintAdjustment::all();
        assert!(ca.slide_x && ca.slide_y && ca.flip_x && ca.flip_y && ca.resize_x && ca.resize_y);
    }

    #[test]
    fn test_edge_variants() {
        let edges = [Edge::None, Edge::Top, Edge::Bottom, Edge::Left, Edge::Right];
        assert_eq!(edges.len(), 5);
        assert_eq!(Edge::default(), Edge::None);
    }

    #[test]
    fn test_popup_default_manager() {
        let mgr = PopupManager::default();
        assert!(mgr.is_empty());
    }

    #[test]
    fn test_popup_position_default() {
        let pos = PopupPosition::default();
        assert_eq!(pos.anchor_edge, Edge::None);
        assert_eq!(pos.gravity, Edge::None);
        assert_eq!(pos.offset_x, 0);
        assert_eq!(pos.offset_y, 0);
    }
}
