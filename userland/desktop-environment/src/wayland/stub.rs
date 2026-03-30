//! Feature-gated stub Wayland server (when `wayland` feature is NOT enabled).

#[cfg(not(feature = "wayland"))]
mod wayland_stub {
    use std::sync::Arc;

    use crate::compositor::{Compositor, InputEvent};

    use super::super::protocol::{ProtocolAction, ProtocolBridge};

    /// Stub Wayland server state (no real protocol, for compilation only).
    /// Uses the same [`ProtocolBridge`] as the live mode for consistent behavior.
    pub struct WaylandState {
        pub compositor: Arc<Compositor>,
        pub bridge: ProtocolBridge,
        pub socket_name: Option<String>,
    }

    impl WaylandState {
        /// Create stub state. No real Wayland socket is opened.
        pub fn new(compositor: Arc<Compositor>) -> Result<Self, Box<dyn std::error::Error>> {
            Ok(Self {
                compositor,
                bridge: ProtocolBridge::new(),
                socket_name: None,
            })
        }

        /// Access the protocol bridge (for testing/inspection).
        pub fn bridge(&self) -> &ProtocolBridge {
            &self.bridge
        }

        /// Stub: returns a fake socket name.
        pub fn listen(&mut self) -> Result<String, Box<dyn std::error::Error>> {
            let name = "wayland-stub-0".to_string();
            self.socket_name = Some(name.clone());
            Ok(name)
        }

        /// Dispatch a frame: apply pending bridge actions to the compositor.
        /// In stub mode this processes any programmatically enqueued actions.
        pub fn dispatch(&mut self) -> Result<Vec<ProtocolAction>, Box<dyn std::error::Error>> {
            let actions = self.bridge.apply_actions(&self.compositor);
            Ok(actions)
        }

        /// Handle an input event by routing through the bridge.
        pub fn handle_input(&mut self, event: &InputEvent) {
            self.bridge.route_input(&self.compositor, event);
        }
    }
}

#[cfg(not(feature = "wayland"))]
pub use wayland_stub::WaylandState;
