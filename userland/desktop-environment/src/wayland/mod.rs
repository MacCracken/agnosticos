//! Wayland protocol integration for AGNOS compositor.
//!
//! Bridges the internal compositor (scene graph, renderer, window management)
//! to the Wayland wire protocol, allowing real Wayland clients to connect.
//!
//! When the `wayland` feature is enabled, this module provides:
//! - [`WaylandState`] — the main server state holding the `wayland_server::Display`
//! - Surface tracking (wl_surface -> internal [`SurfaceId`])
//! - XDG Shell handling (xdg_wm_base, xdg_surface, xdg_toplevel)
//! - SHM buffer support (wl_shm shared-memory pixel buffers)
//! - Seat/Input forwarding (wl_seat, wl_keyboard, wl_pointer)
//! - Output advertising (wl_output screen geometry)
//!
//! Without the feature, a compile-time stub is provided so dependent code
//! still builds.

pub mod types;
pub mod protocol;
pub mod server;
pub mod stub;
pub mod popups;

#[cfg(test)]
mod tests;

// Re-export all public types from sub-modules for backward compatibility.
pub use types::*;
pub use protocol::*;
pub use popups::*;

// Feature-gated re-exports from server/stub.
#[cfg(feature = "wayland")]
pub use server::{WaylandState, start_server, WaylandServer, WaylandServerConfig, WaylandServerEvent};

#[cfg(not(feature = "wayland"))]
pub use stub::WaylandState;
