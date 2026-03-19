//! Tests for the Wayland module.

use super::*;
use std::sync::Arc;
use uuid::Uuid;

use crate::compositor::{Compositor, InputEvent, Rectangle};

// -- ShmFormat tests --

#[test]
fn test_shm_format_bpp() {
    assert_eq!(ShmFormat::Argb8888.bpp(), 4);
    assert_eq!(ShmFormat::Xrgb8888.bpp(), 4);
}

#[test]
fn test_shm_supported_formats() {
    let fmts = ShmFormat::supported_formats();
    assert!(fmts.contains(&ShmFormat::Argb8888));
    assert!(fmts.contains(&ShmFormat::Xrgb8888));
}

// -- ShmBufferInfo validation tests --

#[test]
fn test_shm_buffer_validate_ok() {
    let info = ShmBufferInfo {
        width: 100,
        height: 100,
        stride: 400,
        format: ShmFormat::Argb8888,
        offset: 0,
    };
    assert!(info.validate().is_ok());
}

#[test]
fn test_shm_buffer_validate_zero_width() {
    let info = ShmBufferInfo {
        width: 0,
        height: 100,
        stride: 400,
        format: ShmFormat::Argb8888,
        offset: 0,
    };
    assert!(info.validate().is_err());
}

#[test]
fn test_shm_buffer_validate_stride_too_small() {
    let info = ShmBufferInfo {
        width: 100,
        height: 100,
        stride: 100, // needs 400
        format: ShmFormat::Argb8888,
        offset: 0,
    };
    assert!(info.validate().is_err());
}

#[test]
fn test_shm_buffer_total_bytes() {
    let info = ShmBufferInfo {
        width: 100,
        height: 50,
        stride: 400,
        format: ShmFormat::Argb8888,
        offset: 64,
    };
    assert_eq!(info.total_bytes(), Some(64 + 400 * 50));
}

// -- OutputInfo tests --

#[test]
fn test_output_default() {
    let out = OutputInfo::default();
    assert_eq!(out.width_px, 1920);
    assert_eq!(out.height_px, 1080);
    assert_eq!(out.refresh_mhz, 60_000);
    assert_eq!(out.scale, 1);
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn test_output_logical_size() {
    let mut out = OutputInfo::default();
    out.scale = 2;
    assert_eq!(out.logical_size(), (960, 540));
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn test_output_logical_size_zero_scale() {
    let mut out = OutputInfo::default();
    out.scale = 0;
    assert_eq!(out.logical_size(), (1920, 1080));
}

#[test]
fn test_output_refresh_hz() {
    let out = OutputInfo::default();
    assert!((out.refresh_hz() - 60.0).abs() < 0.01);
}

#[test]
fn test_output_dpi() {
    let out = OutputInfo::default();
    // 1920px / (530mm / 25.4) ~= 92 DPI
    assert!(out.dpi_x() > 80.0 && out.dpi_x() < 110.0);
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn test_output_dpi_zero_mm() {
    let mut out = OutputInfo::default();
    out.width_mm = 0;
    assert_eq!(out.dpi_x(), 96.0);
}

#[test]
fn test_output_from_rectangle() {
    let rect = Rectangle {
        x: 0,
        y: 0,
        width: 2560,
        height: 1440,
    };
    let out = OutputInfo::from_rectangle(&rect);
    assert_eq!(out.width_px, 2560);
    assert_eq!(out.height_px, 1440);
}

// -- SurfaceMap tests --

#[test]
fn test_surface_map_register_and_lookup() {
    let mut map = SurfaceMap::new();
    let sid = Uuid::new_v4();
    let proto = map.register(sid);
    assert_eq!(map.get_internal(proto), Some(&sid));
    assert_eq!(map.get_proto(&sid), Some(proto));
    assert_eq!(map.len(), 1);
}

#[test]
fn test_surface_map_register_idempotent() {
    let mut map = SurfaceMap::new();
    let sid = Uuid::new_v4();
    let p1 = map.register(sid);
    let p2 = map.register(sid);
    assert_eq!(p1, p2);
    assert_eq!(map.len(), 1);
}

#[test]
fn test_surface_map_unregister() {
    let mut map = SurfaceMap::new();
    let sid = Uuid::new_v4();
    let proto = map.register(sid);
    let removed = map.unregister(&sid);
    assert_eq!(removed, Some(proto));
    assert!(map.is_empty());
    assert_eq!(map.get_internal(proto), None);
}

#[test]
fn test_surface_map_unregister_proto() {
    let mut map = SurfaceMap::new();
    let sid = Uuid::new_v4();
    let proto = map.register(sid);
    let removed = map.unregister_proto(proto);
    assert_eq!(removed, Some(sid));
    assert!(map.is_empty());
}

// -- SeatCapabilities tests --

#[test]
fn test_seat_capabilities_bitmask_roundtrip() {
    let caps = SeatCapabilities {
        pointer: true,
        keyboard: true,
        touch: true,
    };
    let mask = caps.to_bitmask();
    assert_eq!(mask, 7);
    let caps2 = SeatCapabilities::from_bitmask(mask);
    assert_eq!(caps, caps2);
}

#[test]
fn test_seat_capabilities_default() {
    let caps = SeatCapabilities::default();
    assert!(caps.pointer);
    assert!(caps.keyboard);
    assert!(!caps.touch);
    assert_eq!(caps.to_bitmask(), 3);
}

// -- ModifierState tests --

#[test]
fn test_modifier_state_roundtrip() {
    let mods = ModifierState {
        shift: true,
        ctrl: true,
        alt: false,
        logo: false,
        caps_lock: false,
        num_lock: false,
    };
    let raw = mods.to_raw();
    let mods2 = ModifierState::from_raw(raw);
    assert_eq!(mods, mods2);
}

#[test]
fn test_modifier_state_empty() {
    let mods = ModifierState::default();
    assert!(mods.is_empty());
    assert_eq!(mods.to_raw(), 0);
}

#[test]
fn test_modifier_state_all() {
    let mods = ModifierState {
        shift: true,
        ctrl: true,
        alt: true,
        logo: true,
        caps_lock: true,
        num_lock: true,
    };
    assert!(!mods.is_empty());
    let raw = mods.to_raw();
    let mods2 = ModifierState::from_raw(raw);
    assert_eq!(mods, mods2);
}

// -- SerialCounter tests --

#[test]
fn test_serial_counter() {
    let mut counter = SerialCounter::new();
    assert_eq!(counter.current(), 0);
    assert_eq!(counter.next_serial(), 1);
    assert_eq!(counter.next_serial(), 2);
    assert_eq!(counter.current(), 2);
}

// -- ClientRegistry tests --

#[test]
fn test_client_registry_register_unregister() {
    let mut reg = ClientRegistry::new();
    assert!(reg.is_empty());
    let id = reg.register();
    assert_eq!(reg.len(), 1);
    assert!(reg.get(id).is_some());
    let info = reg.unregister(id);
    assert!(info.is_some());
    assert!(reg.is_empty());
}

#[test]
fn test_client_registry_with_pid() {
    let mut reg = ClientRegistry::new();
    let id = reg.register_with_pid(42);
    assert_eq!(reg.get(id).unwrap().pid, Some(42));
}

#[test]
fn test_client_registry_find_by_surface() {
    let mut reg = ClientRegistry::new();
    let cid = reg.register();
    let sid = Uuid::new_v4();
    reg.get_mut(cid).unwrap().add_surface(sid);
    assert_eq!(reg.find_by_surface(&sid), Some(cid));
    assert_eq!(reg.find_by_surface(&Uuid::new_v4()), None);
}

// -- XdgToplevelTracker tests --

#[test]
fn test_toplevel_tracker_lifecycle() {
    let sid = Uuid::new_v4();
    let mut tracker = XdgToplevelTracker::new(sid);
    assert!(!tracker.configured);
    assert!(!tracker.mapped);

    // Cannot map before configure
    assert!(!tracker.map());

    // Send configure
    let cfg = ToplevelConfigure::initial(sid, 1);
    tracker.send_configure(cfg);
    assert!(!tracker.configured);

    // Ack configure
    assert!(tracker.ack_configure(1));
    assert!(tracker.configured);

    // Now can map
    assert!(tracker.map());
    assert!(tracker.mapped);

    // Map again -> false (already mapped)
    assert!(!tracker.map());

    // Unmap
    tracker.unmap();
    assert!(!tracker.mapped);
}

#[test]
fn test_toplevel_constrain_size() {
    let sid = Uuid::new_v4();
    let mut tracker = XdgToplevelTracker::new(sid);
    tracker.min_size = Some((100, 100));
    tracker.max_size = Some((800, 600));

    assert_eq!(tracker.constrain_size(50, 50), (100, 100));
    assert_eq!(tracker.constrain_size(400, 300), (400, 300));
    assert_eq!(tracker.constrain_size(1000, 1000), (800, 600));
}

#[test]
fn test_toplevel_ack_wrong_serial() {
    let sid = Uuid::new_v4();
    let mut tracker = XdgToplevelTracker::new(sid);
    let cfg = ToplevelConfigure::initial(sid, 5);
    tracker.send_configure(cfg);
    assert!(!tracker.ack_configure(99));
    assert!(!tracker.configured);
}

// -- ToplevelConfigure tests --

#[test]
fn test_toplevel_configure_maximized() {
    let sid = Uuid::new_v4();
    let out = OutputInfo::default();
    let cfg = ToplevelConfigure::maximized(sid, &out, 1);
    assert!(cfg.is_maximized());
    assert!(cfg.is_activated());
    assert_eq!(cfg.width, 1920);
    assert_eq!(cfg.height, 1080);
}

#[test]
fn test_toplevel_configure_initial() {
    let sid = Uuid::new_v4();
    let cfg = ToplevelConfigure::initial(sid, 1);
    assert_eq!(cfg.width, 0);
    assert_eq!(cfg.height, 0);
    assert!(cfg.is_activated());
    assert!(!cfg.is_maximized());
}

// -- Input mapping tests --

#[test]
fn test_map_input_mouse_move() {
    let event = InputEvent::MouseMove { x: 100, y: 200 };
    let result = map_input_to_pointer_event(&event);
    assert!(result.is_some());
    match result.unwrap() {
        WaylandPointerEvent::Motion { x, y } => {
            assert_eq!(x, 100.0);
            assert_eq!(y, 200.0);
        }
        _ => panic!("Expected Motion"),
    }
}

#[test]
fn test_map_input_mouse_click() {
    let event = InputEvent::MouseClick {
        button: 1,
        x: 50,
        y: 75,
    };
    let result = map_input_to_pointer_event(&event);
    assert!(result.is_some());
    match result.unwrap() {
        WaylandPointerEvent::Button {
            button,
            x,
            y,
            pressed,
        } => {
            assert_eq!(button, 1);
            assert_eq!(x, 50.0);
            assert_eq!(y, 75.0);
            assert!(pressed);
        }
        _ => panic!("Expected Button"),
    }
}

#[test]
fn test_map_input_key_to_keyboard() {
    let event = InputEvent::KeyPress {
        keycode: 30,
        modifiers: 0x05, // shift + ctrl
    };
    let result = map_input_to_keyboard_event(&event);
    assert!(result.is_some());
    match result.unwrap() {
        WaylandKeyboardEvent::Key {
            keycode,
            modifiers,
            pressed,
        } => {
            assert_eq!(keycode, 30);
            assert!(modifiers.shift);
            assert!(modifiers.ctrl);
            assert!(pressed);
        }
        _ => panic!("Expected Key"),
    }
}

#[test]
fn test_map_input_irrelevant_events() {
    let event = InputEvent::KeyPress {
        keycode: 1,
        modifiers: 0,
    };
    assert!(map_input_to_pointer_event(&event).is_none());

    let event = InputEvent::MouseMove { x: 0, y: 0 };
    assert!(map_input_to_keyboard_event(&event).is_none());
}

// -- PointerFocus tests --

#[test]
fn test_pointer_focus_set_and_motion() {
    let mut focus = PointerFocus::default();
    let sid = Uuid::new_v4();
    assert!(focus.set_focus(Some(sid), 10.0, 20.0, 1));
    assert_eq!(focus.surface_id, Some(sid));

    // Same surface -> no change
    assert!(!focus.set_focus(Some(sid), 15.0, 25.0, 2));

    focus.motion(30.0, 40.0);
    assert_eq!(focus.surface_x, 30.0);
    assert_eq!(focus.surface_y, 40.0);
}

// -- KeyboardFocus tests --

#[test]
fn test_keyboard_focus() {
    let mut focus = KeyboardFocus::default();
    let sid = Uuid::new_v4();
    assert!(focus.set_focus(Some(sid), 1));
    assert!(!focus.set_focus(Some(sid), 2)); // same surface
    assert!(focus.set_focus(None, 3)); // changed

    focus.set_modifiers(ModifierState {
        shift: true,
        ..Default::default()
    });
    assert!(focus.modifiers.shift);
}

// -- WaylandState stub tests --

#[test]
fn test_wayland_state_stub_new() {
    let comp = Arc::new(Compositor::new());
    let state = WaylandState::new(comp);
    assert!(state.is_ok());
    let state = state.unwrap();
    assert!(state.bridge.surface_map.is_empty());
    assert!(state.bridge.clients.is_empty());
    assert_eq!(state.bridge.serial.current(), 0);
}

#[cfg(not(feature = "wayland"))]
#[test]
fn test_wayland_state_stub_listen() {
    let comp = Arc::new(Compositor::new());
    let mut state = WaylandState::new(comp).unwrap();
    let name = state.listen().unwrap();
    assert!(name.contains("stub"));
    assert_eq!(state.socket_name, Some(name));
}

#[cfg(not(feature = "wayland"))]
#[test]
fn test_wayland_state_stub_dispatch() {
    let comp = Arc::new(Compositor::new());
    let mut state = WaylandState::new(comp).unwrap();
    let actions = state.dispatch().unwrap();
    assert!(actions.is_empty());
}

// -- ProtocolBridge tests --

#[test]
fn test_bridge_client_lifecycle() {
    let mut bridge = ProtocolBridge::new();
    assert_eq!(bridge.client_count(), 0);

    let id = bridge.client_connect(Some(1234));
    assert_eq!(bridge.client_count(), 1);
    assert_eq!(bridge.clients.get(id).unwrap().pid, Some(1234));

    bridge.client_disconnect(id);
    assert_eq!(bridge.client_count(), 0);
}

#[test]
fn test_bridge_surface_creation() {
    let mut bridge = ProtocolBridge::new();
    let client_id = bridge.client_connect(None);

    let (surface_id, proto_id) = bridge.create_surface(client_id).unwrap();
    assert_eq!(bridge.surface_count(), 1);
    assert_eq!(bridge.surface_map.get_internal(proto_id), Some(&surface_id));

    // Client should track the surface
    assert_eq!(bridge.clients.get(client_id).unwrap().surfaces.len(), 1);
}

#[test]
fn test_bridge_toplevel_lifecycle() {
    let mut bridge = ProtocolBridge::new();
    let client_id = bridge.client_connect(None);
    let (surface_id, _) = bridge.create_surface(client_id).unwrap();

    // Create toplevel — sends initial configure
    let configure = bridge.create_toplevel(surface_id, client_id);
    assert!(configure.is_activated());
    assert_eq!(configure.width, 0); // initial = client picks size
    let serial = configure.serial;

    // Ack configure
    assert!(bridge.ack_configure(surface_id, serial));
    assert!(bridge.toplevels.get(&surface_id).unwrap().configured);

    // First commit maps the window
    assert!(bridge.surface_commit(surface_id));
    assert!(bridge.toplevels.get(&surface_id).unwrap().mapped);
    assert_eq!(bridge.mapped_toplevel_count(), 1);
}

#[test]
fn test_bridge_set_title_and_app_id() {
    let mut bridge = ProtocolBridge::new();
    let cid = bridge.client_connect(None);
    let (sid, _) = bridge.create_surface(cid).unwrap();
    bridge.create_toplevel(sid, cid);

    bridge.set_title(sid, "My Window".to_string());
    bridge.set_app_id(sid, "com.example.app".to_string());

    let tracker = bridge.toplevels.get(&sid).unwrap();
    assert_eq!(tracker.title.as_deref(), Some("My Window"));
    assert_eq!(tracker.app_id.as_deref(), Some("com.example.app"));
}

#[test]
fn test_bridge_maximize() {
    let mut bridge = ProtocolBridge::new();
    let cid = bridge.client_connect(None);
    let (sid, _) = bridge.create_surface(cid).unwrap();
    bridge.create_toplevel(sid, cid);

    bridge.set_maximized(sid, true);
    let tracker = bridge.toplevels.get(&sid).unwrap();
    let pending = tracker.pending_configure.as_ref().unwrap();
    assert!(pending.is_maximized());
    assert_eq!(pending.width, 1920);
    assert_eq!(pending.height, 1080);
}

#[test]
fn test_bridge_fullscreen() {
    let mut bridge = ProtocolBridge::new();
    let cid = bridge.client_connect(None);
    let (sid, _) = bridge.create_surface(cid).unwrap();
    bridge.create_toplevel(sid, cid);

    bridge.set_fullscreen(sid, true);
    let tracker = bridge.toplevels.get(&sid).unwrap();
    let pending = tracker.pending_configure.as_ref().unwrap();
    assert!(pending.states.contains(&XdgToplevelState::Fullscreen));
}

#[test]
fn test_bridge_size_bounds() {
    let mut bridge = ProtocolBridge::new();
    let cid = bridge.client_connect(None);
    let (sid, _) = bridge.create_surface(cid).unwrap();
    bridge.create_toplevel(sid, cid);

    bridge.set_size_bounds(sid, Some((200, 150)), Some((800, 600)));
    let tracker = bridge.toplevels.get(&sid).unwrap();
    assert_eq!(tracker.min_size, Some((200, 150)));
    assert_eq!(tracker.max_size, Some((800, 600)));
    assert_eq!(tracker.constrain_size(100, 100), (200, 150));
    assert_eq!(tracker.constrain_size(1000, 1000), (800, 600));
}

#[test]
fn test_bridge_destroy_surface() {
    let mut bridge = ProtocolBridge::new();
    let cid = bridge.client_connect(None);
    let (sid, _) = bridge.create_surface(cid).unwrap();
    bridge.create_toplevel(sid, cid);

    bridge.destroy_surface(sid);
    assert_eq!(bridge.surface_count(), 0);
    assert!(!bridge.toplevels.contains_key(&sid));
}

#[test]
fn test_bridge_client_disconnect_cleans_surfaces() {
    let mut bridge = ProtocolBridge::new();
    let cid = bridge.client_connect(None);
    let (sid1, _) = bridge.create_surface(cid).unwrap();
    let (sid2, _) = bridge.create_surface(cid).unwrap();
    bridge.create_toplevel(sid1, cid);
    bridge.create_toplevel(sid2, cid);

    let removed = bridge.client_disconnect(cid);
    assert_eq!(removed.len(), 2);
    assert_eq!(bridge.surface_count(), 0);
    assert_eq!(bridge.mapped_toplevel_count(), 0);
}

#[test]
fn test_bridge_ack_wrong_serial() {
    let mut bridge = ProtocolBridge::new();
    let cid = bridge.client_connect(None);
    let (sid, _) = bridge.create_surface(cid).unwrap();
    bridge.create_toplevel(sid, cid);

    assert!(!bridge.ack_configure(sid, 9999));
    assert!(!bridge.toplevels.get(&sid).unwrap().configured);
}

#[test]
fn test_bridge_commit_before_ack_does_not_map() {
    let mut bridge = ProtocolBridge::new();
    let cid = bridge.client_connect(None);
    let (sid, _) = bridge.create_surface(cid).unwrap();
    bridge.create_toplevel(sid, cid);

    // Commit without acking configure should not map
    assert!(!bridge.surface_commit(sid));
    assert!(!bridge.toplevels.get(&sid).unwrap().mapped);
}

#[test]
fn test_bridge_drain_actions() {
    let mut bridge = ProtocolBridge::new();
    let cid = bridge.client_connect(None);
    let (sid, _) = bridge.create_surface(cid).unwrap();
    bridge.create_toplevel(sid, cid);

    let actions = bridge.drain_actions();
    assert!(!actions.is_empty());

    // Second drain should be empty
    let actions2 = bridge.drain_actions();
    assert!(actions2.is_empty());
}

#[test]
fn test_bridge_disconnect_clears_focus() {
    let mut bridge = ProtocolBridge::new();
    let cid = bridge.client_connect(None);
    let (sid, _) = bridge.create_surface(cid).unwrap();
    bridge.create_toplevel(sid, cid);

    // Set focus to this surface
    let serial = bridge.serial.next_serial();
    bridge
        .pointer_focus
        .set_focus(Some(sid), 50.0, 50.0, serial);
    bridge.keyboard_focus.set_focus(Some(sid), serial);

    bridge.client_disconnect(cid);
    assert_eq!(bridge.pointer_focus.surface_id, None);
    assert_eq!(bridge.keyboard_focus.surface_id, None);
}

#[test]
fn test_bridge_input_routing() {
    let comp = Compositor::new();
    let mut bridge = ProtocolBridge::new();

    // Route a mouse move — should produce a ForwardPointer action
    let event = InputEvent::MouseMove { x: 100, y: 200 };
    bridge.route_input(&comp, &event);

    let actions = bridge.drain_actions();
    let has_pointer = actions
        .iter()
        .any(|a| matches!(a, ProtocolAction::ForwardPointer { .. }));
    assert!(has_pointer);
}

#[test]
fn test_bridge_multiple_clients() {
    let mut bridge = ProtocolBridge::new();
    let c1 = bridge.client_connect(Some(100));
    let c2 = bridge.client_connect(Some(200));

    let (_s1, _) = bridge.create_surface(c1).unwrap();
    let (s2, _) = bridge.create_surface(c2).unwrap();

    assert_eq!(bridge.client_count(), 2);
    assert_eq!(bridge.surface_count(), 2);

    bridge.client_disconnect(c1);
    assert_eq!(bridge.client_count(), 1);
    assert_eq!(bridge.surface_count(), 1);
    assert!(bridge.surface_map.get_proto(&s2).is_some());
}

#[test]
fn test_bridge_request_move_and_resize() {
    let mut bridge = ProtocolBridge::new();
    let cid = bridge.client_connect(None);
    let (sid, _) = bridge.create_surface(cid).unwrap();

    bridge.request_move(sid);
    bridge.request_resize(sid, 4); // right edge

    let actions = bridge.drain_actions();
    assert!(actions
        .iter()
        .any(|a| matches!(a, ProtocolAction::RequestMove { .. })));
    assert!(actions
        .iter()
        .any(|a| matches!(a, ProtocolAction::RequestResize { edge: 4, .. })));
}

#[test]
fn test_bridge_set_minimized() {
    let mut bridge = ProtocolBridge::new();
    let cid = bridge.client_connect(None);
    let (sid, _) = bridge.create_surface(cid).unwrap();

    bridge.set_minimized(sid);
    let actions = bridge.drain_actions();
    assert!(actions
        .iter()
        .any(|a| matches!(a, ProtocolAction::SetMinimized { .. })));
}

// -- ClientInfo tests --

#[test]
fn test_client_info_surfaces() {
    let mut client = ClientInfo::new(1);
    let s1 = Uuid::new_v4();
    let s2 = Uuid::new_v4();

    client.add_surface(s1);
    client.add_surface(s1); // duplicate
    assert_eq!(client.surfaces.len(), 1);

    client.add_surface(s2);
    assert_eq!(client.surfaces.len(), 2);

    client.remove_surface(&s1);
    assert_eq!(client.surfaces.len(), 1);
    assert_eq!(client.surfaces[0], s2);
}

// -- Protocol extension tests --

fn test_surface_id() -> crate::compositor::SurfaceId {
    uuid::Uuid::new_v4()
}

// ── DataDeviceManager ──────────────────────────────────────────

#[test]
fn test_data_device_manager_new() {
    let mgr = DataDeviceManager::new();
    assert!(mgr.selections.is_empty());
    assert!(mgr.drag_source.is_none());
}

#[test]
fn test_data_device_manager_default() {
    let mgr = DataDeviceManager::default();
    assert!(mgr.selections.is_empty());
    assert!(mgr.drag_source.is_none());
}

#[test]
fn test_set_selection() {
    let mut mgr = DataDeviceManager::new();
    let sid = test_surface_id();
    mgr.set_selection(sid, vec!["text/plain".into()], 1);
    let offer = mgr.get_selection(&sid).unwrap();
    assert_eq!(offer.mime_types, vec!["text/plain"]);
    assert_eq!(offer.source_surface, sid);
    assert_eq!(offer.serial, 1);
}

#[test]
fn test_set_selection_overwrite() {
    let mut mgr = DataDeviceManager::new();
    let sid = test_surface_id();
    mgr.set_selection(sid, vec!["text/plain".into()], 1);
    mgr.set_selection(sid, vec!["text/html".into()], 2);
    let offer = mgr.get_selection(&sid).unwrap();
    assert_eq!(offer.mime_types, vec!["text/html"]);
    assert_eq!(offer.serial, 2);
}

#[test]
fn test_clear_selection() {
    let mut mgr = DataDeviceManager::new();
    let sid = test_surface_id();
    mgr.set_selection(sid, vec!["text/plain".into()], 1);
    mgr.clear_selection(&sid);
    assert!(mgr.get_selection(&sid).is_none());
}

#[test]
fn test_clear_selection_nonexistent() {
    let mut mgr = DataDeviceManager::new();
    let sid = test_surface_id();
    mgr.clear_selection(&sid); // should not panic
    assert!(mgr.get_selection(&sid).is_none());
}

#[test]
fn test_start_and_end_drag() {
    let mut mgr = DataDeviceManager::new();
    let src = test_surface_id();
    let icon = test_surface_id();
    mgr.start_drag(src, Some(icon), vec!["text/uri-list".into()]);
    let drag = mgr.drag_source.as_ref().unwrap();
    assert_eq!(drag.source_surface, src);
    assert_eq!(drag.icon_surface, Some(icon));
    assert!(drag.active);
    assert_eq!(drag.position, (0.0, 0.0));
    assert_eq!(drag.mime_types, vec!["text/uri-list"]);

    mgr.end_drag();
    assert!(mgr.drag_source.is_none());
}

#[test]
fn test_start_drag_no_icon() {
    let mut mgr = DataDeviceManager::new();
    let src = test_surface_id();
    mgr.start_drag(src, None, vec![]);
    let drag = mgr.drag_source.as_ref().unwrap();
    assert!(drag.icon_surface.is_none());
    assert!(drag.mime_types.is_empty());
}

#[test]
fn test_end_drag_when_none() {
    let mut mgr = DataDeviceManager::new();
    mgr.end_drag(); // should not panic
    assert!(mgr.drag_source.is_none());
}

#[test]
fn test_multiple_surfaces_selection() {
    let mut mgr = DataDeviceManager::new();
    let s1 = test_surface_id();
    let s2 = test_surface_id();
    mgr.set_selection(s1, vec!["a".into()], 1);
    mgr.set_selection(s2, vec!["b".into()], 2);
    assert_eq!(mgr.selections.len(), 2);
    assert_eq!(mgr.get_selection(&s1).unwrap().mime_types, vec!["a"]);
    assert_eq!(mgr.get_selection(&s2).unwrap().mime_types, vec!["b"]);
}

// ── TextInputState ─────────────────────────────────────────────

#[test]
fn test_text_input_new() {
    let ti = TextInputState::new();
    assert!(ti.surface_id.is_none());
    assert!(!ti.enabled);
    assert_eq!(ti.content_type, ContentType::Normal);
    assert!(ti.surrounding_text.is_empty());
    assert_eq!(ti.cursor_position, 0);
    assert!(ti.preedit.is_none());
}

#[test]
fn test_text_input_default() {
    let ti = TextInputState::default();
    assert!(!ti.enabled);
    assert_eq!(ti.content_type, ContentType::Normal);
}

#[test]
fn test_content_type_default() {
    assert_eq!(ContentType::default(), ContentType::Normal);
}

#[test]
fn test_text_input_enable_disable() {
    let mut ti = TextInputState::new();
    let sid = test_surface_id();
    ti.enable(sid);
    assert!(ti.enabled);
    assert_eq!(ti.surface_id, Some(sid));

    ti.disable();
    assert!(!ti.enabled);
    assert!(ti.surface_id.is_none());
    assert!(ti.preedit.is_none());
}

#[test]
fn test_text_input_disable_clears_preedit() {
    let mut ti = TextInputState::new();
    let sid = test_surface_id();
    ti.enable(sid);
    ti.preedit = Some(PreeditState {
        text: "pre".into(),
        cursor_begin: 0,
        cursor_end: 3,
    });
    ti.disable();
    assert!(ti.preedit.is_none());
}

#[test]
fn test_set_surrounding_text() {
    let mut ti = TextInputState::new();
    ti.set_surrounding_text("hello world".into(), 5);
    assert_eq!(ti.surrounding_text, "hello world");
    assert_eq!(ti.cursor_position, 5);
}

#[test]
fn test_commit_preedit() {
    let mut ti = TextInputState::new();
    ti.preedit = Some(PreeditState {
        text: "composing".into(),
        cursor_begin: 0,
        cursor_end: 9,
    });
    let text = ti.commit_preedit();
    assert_eq!(text, Some("composing".to_string()));
    assert!(ti.preedit.is_none());
}

#[test]
fn test_commit_preedit_none() {
    let mut ti = TextInputState::new();
    assert_eq!(ti.commit_preedit(), None);
}

#[test]
fn test_clear_preedit() {
    let mut ti = TextInputState::new();
    ti.preedit = Some(PreeditState {
        text: "x".into(),
        cursor_begin: 0,
        cursor_end: 1,
    });
    ti.clear_preedit();
    assert!(ti.preedit.is_none());
}

#[test]
fn test_clear_preedit_when_none() {
    let mut ti = TextInputState::new();
    ti.clear_preedit(); // should not panic
    assert!(ti.preedit.is_none());
}

#[test]
fn test_content_type_variants() {
    let variants = [
        ContentType::Normal,
        ContentType::Password,
        ContentType::Email,
        ContentType::Number,
        ContentType::Phone,
        ContentType::Url,
        ContentType::Terminal,
    ];
    assert_eq!(variants.len(), 7);
    // All are distinct
    for (i, a) in variants.iter().enumerate() {
        for (j, b) in variants.iter().enumerate() {
            if i != j {
                assert_ne!(a, b);
            }
        }
    }
}

// ── DecorationMode / DecorationState ───────────────────────────

#[test]
fn test_decoration_mode_default() {
    assert_eq!(DecorationMode::default(), DecorationMode::ServerSide);
}

#[test]
fn test_decoration_state_new() {
    let sid = test_surface_id();
    let ds = DecorationState::new(sid);
    assert_eq!(ds.surface_id, sid);
    assert_eq!(ds.preferred, DecorationMode::ServerSide);
    assert_eq!(ds.current, DecorationMode::ServerSide);
}

#[test]
fn test_decoration_negotiate_server_side() {
    let sid = test_surface_id();
    let mut ds = DecorationState::new(sid);
    ds.preferred = DecorationMode::ServerSide;
    let mode = ds.negotiate();
    assert_eq!(mode, DecorationMode::ServerSide);
    assert_eq!(ds.current, DecorationMode::ServerSide);
}

#[test]
fn test_decoration_negotiate_client_side() {
    let sid = test_surface_id();
    let mut ds = DecorationState::new(sid);
    ds.preferred = DecorationMode::ClientSide;
    let mode = ds.negotiate();
    assert_eq!(mode, DecorationMode::ClientSide);
    assert_eq!(ds.current, DecorationMode::ClientSide);
}

#[test]
fn test_decoration_mode_equality() {
    assert_eq!(DecorationMode::ClientSide, DecorationMode::ClientSide);
    assert_eq!(DecorationMode::ServerSide, DecorationMode::ServerSide);
    assert_ne!(DecorationMode::ClientSide, DecorationMode::ServerSide);
}

// ── ViewportState ──────────────────────────────────────────────

#[test]
fn test_viewport_state_new() {
    let sid = test_surface_id();
    let vs = ViewportState::new(sid);
    assert_eq!(vs.surface_id, sid);
    assert!(vs.source.is_none());
    assert!(vs.destination.is_none());
}

#[test]
fn test_viewport_set_source() {
    let sid = test_surface_id();
    let mut vs = ViewportState::new(sid);
    vs.set_source(10.0, 20.0, 100.0, 200.0);
    let src = vs.source.unwrap();
    assert_eq!(src.x, 10.0);
    assert_eq!(src.y, 20.0);
    assert_eq!(src.width, 100.0);
    assert_eq!(src.height, 200.0);
}

#[test]
fn test_viewport_set_destination() {
    let sid = test_surface_id();
    let mut vs = ViewportState::new(sid);
    vs.set_destination(800, 600);
    assert_eq!(vs.destination, Some((800, 600)));
}

#[test]
fn test_viewport_effective_size_destination() {
    let sid = test_surface_id();
    let mut vs = ViewportState::new(sid);
    vs.set_source(0.0, 0.0, 1920.0, 1080.0);
    vs.set_destination(960, 540);
    assert_eq!(vs.effective_size(), Some((960, 540)));
}

#[test]
fn test_viewport_effective_size_source_only() {
    let sid = test_surface_id();
    let mut vs = ViewportState::new(sid);
    vs.set_source(0.0, 0.0, 1920.0, 1080.0);
    assert_eq!(vs.effective_size(), Some((1920, 1080)));
}

#[test]
fn test_viewport_effective_size_none() {
    let sid = test_surface_id();
    let vs = ViewportState::new(sid);
    assert_eq!(vs.effective_size(), None);
}

#[test]
fn test_viewport_source_fractional() {
    let sid = test_surface_id();
    let mut vs = ViewportState::new(sid);
    vs.set_source(0.5, 0.5, 99.5, 49.5);
    // effective_size truncates to u32
    assert_eq!(vs.effective_size(), Some((99, 49)));
}

// ── FractionalScale ────────────────────────────────────────────

#[test]
fn test_fractional_scale_new() {
    let sid = test_surface_id();
    let fs = FractionalScale::new(sid, 120);
    assert_eq!(fs.surface_id, sid);
    assert_eq!(fs.scale_120, 120);
}

#[test]
fn test_fractional_scale_factor_1x() {
    let sid = test_surface_id();
    let fs = FractionalScale::new(sid, 120);
    assert!((fs.scale_factor() - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_fractional_scale_factor_125() {
    let sid = test_surface_id();
    let fs = FractionalScale::new(sid, 150);
    assert!((fs.scale_factor() - 1.25).abs() < f64::EPSILON);
}

#[test]
fn test_fractional_scale_factor_2x() {
    let sid = test_surface_id();
    let fs = FractionalScale::new(sid, 240);
    assert!((fs.scale_factor() - 2.0).abs() < f64::EPSILON);
}

#[test]
fn test_fractional_scale_from_scale() {
    let sid = test_surface_id();
    let fs = FractionalScale::from_scale(sid, 1.5);
    assert_eq!(fs.scale_120, 180);
    assert!((fs.scale_factor() - 1.5).abs() < f64::EPSILON);
}

#[test]
fn test_fractional_scale_from_scale_1x() {
    let sid = test_surface_id();
    let fs = FractionalScale::from_scale(sid, 1.0);
    assert_eq!(fs.scale_120, 120);
}

#[test]
fn test_fractional_scale_from_scale_rounding() {
    let sid = test_surface_id();
    // 1.33333... * 120 = 160.0 (rounds to 160)
    let fs = FractionalScale::from_scale(sid, 1.3333333333);
    assert_eq!(fs.scale_120, 160);
}

#[test]
fn test_fractional_scale_zero() {
    let sid = test_surface_id();
    let fs = FractionalScale::new(sid, 0);
    assert!((fs.scale_factor()).abs() < f64::EPSILON);
}

// ── ProtocolAction variants ────────────────────────────────────

#[test]
fn test_protocol_action_set_selection() {
    let sid = test_surface_id();
    let action = ProtocolAction::SetSelection {
        surface_id: sid,
        mime_types: vec!["text/plain".into()],
    };
    match action {
        ProtocolAction::SetSelection {
            surface_id,
            mime_types,
        } => {
            assert_eq!(surface_id, sid);
            assert_eq!(mime_types, vec!["text/plain"]);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn test_protocol_action_start_drag() {
    let src = test_surface_id();
    let icon = test_surface_id();
    let action = ProtocolAction::StartDrag {
        source: src,
        icon: Some(icon),
        mime_types: vec![],
    };
    match action {
        ProtocolAction::StartDrag {
            source,
            icon: i,
            mime_types,
        } => {
            assert_eq!(source, src);
            assert_eq!(i, Some(icon));
            assert!(mime_types.is_empty());
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn test_protocol_action_text_input_enable() {
    let sid = test_surface_id();
    let action = ProtocolAction::TextInputEnable { surface_id: sid };
    matches!(action, ProtocolAction::TextInputEnable { .. });
}

#[test]
fn test_protocol_action_text_input_disable() {
    let sid = test_surface_id();
    let action = ProtocolAction::TextInputDisable { surface_id: sid };
    matches!(action, ProtocolAction::TextInputDisable { .. });
}

#[test]
fn test_protocol_action_text_input_commit() {
    let sid = test_surface_id();
    let action = ProtocolAction::TextInputCommit {
        surface_id: sid,
        text: "hello".into(),
    };
    match action {
        ProtocolAction::TextInputCommit { text, .. } => assert_eq!(text, "hello"),
        _ => panic!("wrong variant"),
    }
}

#[test]
fn test_protocol_action_set_decoration_mode() {
    let sid = test_surface_id();
    let action = ProtocolAction::SetDecorationMode {
        surface_id: sid,
        mode: DecorationMode::ClientSide,
    };
    match action {
        ProtocolAction::SetDecorationMode { mode, .. } => {
            assert_eq!(mode, DecorationMode::ClientSide);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn test_protocol_action_set_viewport() {
    let sid = test_surface_id();
    let action = ProtocolAction::SetViewport {
        surface_id: sid,
        source: Some(ViewportSource {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        }),
        destination: Some((50, 50)),
    };
    match action {
        ProtocolAction::SetViewport {
            source,
            destination,
            ..
        } => {
            assert!(source.is_some());
            assert_eq!(destination, Some((50, 50)));
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn test_protocol_action_set_fractional_scale() {
    let sid = test_surface_id();
    let action = ProtocolAction::SetFractionalScale {
        surface_id: sid,
        scale_120: 180,
    };
    match action {
        ProtocolAction::SetFractionalScale { scale_120, .. } => {
            assert_eq!(scale_120, 180);
        }
        _ => panic!("wrong variant"),
    }
}
