//! Feature-gated live Wayland server integration.

#[cfg(feature = "wayland")]
mod wayland_live {
    use std::collections::HashMap as StdHashMap;
    use std::sync::Arc;

    use wayland_protocols::xdg::shell::server::{xdg_surface, xdg_toplevel, xdg_wm_base};
    use wayland_server::{
        backend::ClientId,
        protocol::{wl_buffer, wl_compositor, wl_output, wl_seat, wl_shm, wl_shm_pool, wl_surface},
        Client, DataInit, Dispatch, Display, DisplayHandle, GlobalDispatch, ListeningSocket, New,
        Resource,
    };

    use crate::compositor::{Compositor, InputEvent, SurfaceId};

    use super::super::protocol::ProtocolAction;
    use super::super::protocol::ProtocolBridge;
    use super::super::types::*;

    /// Per-surface user data stored on `wl_surface` resources.
    #[derive(Debug)]
    pub struct SurfaceData {
        pub surface_id: SurfaceId,
        pub client_id: u32,
    }

    /// Per-toplevel user data stored on `xdg_toplevel` resources.
    #[derive(Debug)]
    pub struct ToplevelData {
        pub surface_id: SurfaceId,
        pub client_id: u32,
    }

    /// Per-xdg_surface user data.
    #[derive(Debug)]
    pub struct XdgSurfaceData {
        pub surface_id: SurfaceId,
        pub client_id: u32,
    }

    /// Internal dispatch state — separated from Display so
    /// `Display::dispatch_clients(&mut inner)` can borrow correctly.
    pub struct WaylandInner {
        pub compositor: Arc<Compositor>,
        pub bridge: ProtocolBridge,
        pub dh: DisplayHandle,
        client_ids: StdHashMap<ClientId, u32>,
        next_client_id: u32,
    }

    impl WaylandInner {
        fn get_client_id(&mut self, client: &Client) -> u32 {
            let cid = client.id();
            if let Some(&id) = self.client_ids.get(&cid) {
                return id;
            }
            let id = self.next_client_id;
            self.next_client_id = self.next_client_id.wrapping_add(1);
            self.client_ids.insert(cid, id);
            id
        }
    }

    /// Main Wayland server state.
    ///
    /// Holds the `wayland_server::Display` separately from the dispatch state
    /// (`WaylandInner`) so that `dispatch_clients` can borrow the inner state
    /// mutably without conflicting with the display borrow.
    pub struct WaylandState {
        display: Display<WaylandInner>,
        inner: WaylandInner,
        socket_name: Option<String>,
    }

    impl WaylandState {
        /// Create a new Wayland server state bound to the given compositor.
        pub fn new(compositor: Arc<Compositor>) -> Result<Self, Box<dyn std::error::Error>> {
            let display: Display<WaylandInner> = Display::new()?;
            let dh = display.handle();

            Ok(Self {
                display,
                inner: WaylandInner {
                    compositor,
                    bridge: ProtocolBridge::new(),
                    dh,
                    client_ids: StdHashMap::new(),
                    next_client_id: 1,
                },
                socket_name: None,
            })
        }

        /// Start listening on a Wayland socket.
        pub fn listen(&mut self) -> Result<String, Box<dyn std::error::Error>> {
            let socket = ListeningSocket::bind_auto("wayland", 0..33)?;
            let name = socket
                .socket_name()
                .and_then(|n| n.to_str().map(String::from))
                .unwrap_or_else(|| "wayland-0".to_string());
            self.socket_name = Some(name.clone());
            Ok(name)
        }

        /// Dispatch one frame: accept clients, process events, apply to compositor.
        pub fn dispatch(&mut self) -> Result<Vec<ProtocolAction>, Box<dyn std::error::Error>> {
            self.display.dispatch_clients(&mut self.inner)?;
            let actions = self.inner.bridge.apply_actions(&self.inner.compositor);
            Ok(actions)
        }

        /// Handle an input event by routing through the bridge.
        pub fn handle_input(&mut self, event: &InputEvent) {
            self.inner.bridge.route_input(&self.inner.compositor, event);
        }

        /// Initialize all global protocol objects on the display.
        pub fn init_globals(&mut self) {
            let dh = &self.inner.dh;
            dh.create_global::<WaylandInner, wl_compositor::WlCompositor, ()>(5, ());
            dh.create_global::<WaylandInner, wl_shm::WlShm, ()>(1, ());
            dh.create_global::<WaylandInner, wl_seat::WlSeat, ()>(7, ());
            dh.create_global::<WaylandInner, wl_output::WlOutput, ()>(4, ());
            dh.create_global::<WaylandInner, xdg_wm_base::XdgWmBase, ()>(3, ());
        }
    }

    // ========================================================================
    // GlobalDispatch impls — called when a client binds to a global
    // ========================================================================

    impl GlobalDispatch<wl_compositor::WlCompositor, ()> for WaylandInner {
        fn bind(
            _state: &mut Self,
            _client: &Client,
            resource: New<wl_compositor::WlCompositor>,
            _global_data: &(),
            _dhandle: &DisplayHandle,
            data_init: &mut DataInit<'_, Self>,
        ) {
            data_init.init(resource, ());
        }
    }

    impl GlobalDispatch<wl_shm::WlShm, ()> for WaylandInner {
        fn bind(
            _state: &mut Self,
            _client: &Client,
            resource: New<wl_shm::WlShm>,
            _global_data: &(),
            _dhandle: &DisplayHandle,
            data_init: &mut DataInit<'_, Self>,
        ) {
            let shm = data_init.init(resource, ());
            shm.format(wl_shm::Format::Argb8888);
            shm.format(wl_shm::Format::Xrgb8888);
        }
    }

    impl GlobalDispatch<wl_seat::WlSeat, ()> for WaylandInner {
        fn bind(
            state: &mut Self,
            _client: &Client,
            resource: New<wl_seat::WlSeat>,
            _global_data: &(),
            _dhandle: &DisplayHandle,
            data_init: &mut DataInit<'_, Self>,
        ) {
            let seat = data_init.init(resource, ());
            let caps = state.bridge.seat_caps;
            seat.capabilities(wl_seat::Capability::from_bits_truncate(caps.to_bitmask()));
        }
    }

    impl GlobalDispatch<wl_output::WlOutput, ()> for WaylandInner {
        fn bind(
            state: &mut Self,
            _client: &Client,
            resource: New<wl_output::WlOutput>,
            _global_data: &(),
            _dhandle: &DisplayHandle,
            data_init: &mut DataInit<'_, Self>,
        ) {
            let output = data_init.init(resource, ());
            let info = &state.bridge.output;

            output.geometry(
                info.x,
                info.y,
                info.width_mm as i32,
                info.height_mm as i32,
                wl_output::Subpixel::Unknown,
                info.make.clone(),
                info.model.clone(),
                wl_output::Transform::Normal,
            );
            output.mode(
                wl_output::Mode::Current | wl_output::Mode::Preferred,
                info.width_px as i32,
                info.height_px as i32,
                info.refresh_mhz as i32,
            );
            output.scale(info.scale as i32);
            output.done();
        }
    }

    impl GlobalDispatch<xdg_wm_base::XdgWmBase, ()> for WaylandInner {
        fn bind(
            _state: &mut Self,
            _client: &Client,
            resource: New<xdg_wm_base::XdgWmBase>,
            _global_data: &(),
            _dhandle: &DisplayHandle,
            data_init: &mut DataInit<'_, Self>,
        ) {
            data_init.init(resource, ());
        }
    }

    // ========================================================================
    // Dispatch: wl_compositor — creates wl_surface instances
    // ========================================================================

    impl Dispatch<wl_compositor::WlCompositor, ()> for WaylandInner {
        fn request(
            state: &mut Self,
            client: &Client,
            _resource: &wl_compositor::WlCompositor,
            request: wl_compositor::Request,
            _data: &(),
            _dhandle: &DisplayHandle,
            data_init: &mut DataInit<'_, Self>,
        ) {
            match request {
                wl_compositor::Request::CreateSurface { id } => {
                    let client_id = state.get_client_id(client);
                    if let Some((surface_id, _proto_id)) = state.bridge.create_surface(client_id) {
                        data_init.init(
                            id,
                            SurfaceData {
                                surface_id,
                                client_id,
                            },
                        );
                    }
                }
                wl_compositor::Request::CreateRegion { id: _ } => {
                    // Region management — no-op for basic compositor
                }
                _ => {}
            }
        }
    }

    // ========================================================================
    // Dispatch: wl_surface — surface commit, damage, attach
    // ========================================================================

    impl Dispatch<wl_surface::WlSurface, SurfaceData> for WaylandInner {
        fn request(
            state: &mut Self,
            _client: &Client,
            _resource: &wl_surface::WlSurface,
            request: wl_surface::Request,
            data: &SurfaceData,
            _dhandle: &DisplayHandle,
            _data_init: &mut DataInit<'_, Self>,
        ) {
            match request {
                wl_surface::Request::Commit => {
                    state.bridge.surface_commit(data.surface_id);
                }
                wl_surface::Request::Destroy => {
                    state.bridge.destroy_surface(data.surface_id);
                }
                wl_surface::Request::Attach {
                    buffer: _,
                    x: _,
                    y: _,
                } => {}
                wl_surface::Request::Damage {
                    x: _,
                    y: _,
                    width: _,
                    height: _,
                } => {}
                wl_surface::Request::Frame { callback: _ } => {}
                wl_surface::Request::SetInputRegion { region: _ } => {}
                wl_surface::Request::SetOpaqueRegion { region: _ } => {}
                wl_surface::Request::SetBufferTransform { transform: _ } => {}
                wl_surface::Request::SetBufferScale { scale: _ } => {}
                wl_surface::Request::DamageBuffer {
                    x: _,
                    y: _,
                    width: _,
                    height: _,
                } => {}
                wl_surface::Request::Offset { x: _, y: _ } => {}
                _ => {}
            }
        }
    }

    // ========================================================================
    // Dispatch: wl_shm — shared memory buffer management
    // ========================================================================

    impl Dispatch<wl_shm::WlShm, ()> for WaylandInner {
        fn request(
            _state: &mut Self,
            _client: &Client,
            _resource: &wl_shm::WlShm,
            request: wl_shm::Request,
            _data: &(),
            _dhandle: &DisplayHandle,
            data_init: &mut DataInit<'_, Self>,
        ) {
            match request {
                wl_shm::Request::CreatePool { id, fd: _, size: _ } => {
                    data_init.init(id, ());
                }
                _ => {}
            }
        }
    }

    impl Dispatch<wl_shm_pool::WlShmPool, ()> for WaylandInner {
        fn request(
            _state: &mut Self,
            _client: &Client,
            _resource: &wl_shm_pool::WlShmPool,
            request: wl_shm_pool::Request,
            _data: &(),
            _dhandle: &DisplayHandle,
            data_init: &mut DataInit<'_, Self>,
        ) {
            match request {
                wl_shm_pool::Request::CreateBuffer {
                    id,
                    offset: _,
                    width: _,
                    height: _,
                    stride: _,
                    format: _,
                } => {
                    data_init.init(id, ());
                }
                wl_shm_pool::Request::Resize { size: _ } => {}
                wl_shm_pool::Request::Destroy => {}
                _ => {}
            }
        }
    }

    impl Dispatch<wl_buffer::WlBuffer, ()> for WaylandInner {
        fn request(
            _state: &mut Self,
            _client: &Client,
            _resource: &wl_buffer::WlBuffer,
            request: wl_buffer::Request,
            _data: &(),
            _dhandle: &DisplayHandle,
            _data_init: &mut DataInit<'_, Self>,
        ) {
            match request {
                wl_buffer::Request::Destroy => {}
                _ => {}
            }
        }
    }

    // ========================================================================
    // Dispatch: wl_seat — input device capabilities
    // ========================================================================

    impl Dispatch<wl_seat::WlSeat, ()> for WaylandInner {
        fn request(
            state: &mut Self,
            _client: &Client,
            resource: &wl_seat::WlSeat,
            request: wl_seat::Request,
            _data: &(),
            _dhandle: &DisplayHandle,
            _data_init: &mut DataInit<'_, Self>,
        ) {
            match request {
                wl_seat::Request::GetPointer { id: _ } => {}
                wl_seat::Request::GetKeyboard { id: _ } => {}
                wl_seat::Request::GetTouch { id: _ } => {}
                wl_seat::Request::Release => {}
                _ => {}
            }
            let caps = state.bridge.seat_caps;
            resource.capabilities(wl_seat::Capability::from_bits_truncate(caps.to_bitmask()));
        }
    }

    // ========================================================================
    // Dispatch: wl_output — screen geometry advertising
    // ========================================================================

    impl Dispatch<wl_output::WlOutput, ()> for WaylandInner {
        fn request(
            _state: &mut Self,
            _client: &Client,
            _resource: &wl_output::WlOutput,
            request: wl_output::Request,
            _data: &(),
            _dhandle: &DisplayHandle,
            _data_init: &mut DataInit<'_, Self>,
        ) {
            match request {
                wl_output::Request::Release => {}
                _ => {}
            }
        }
    }

    // ========================================================================
    // Dispatch: xdg_wm_base — XDG Shell entry point
    // ========================================================================

    impl Dispatch<xdg_wm_base::XdgWmBase, ()> for WaylandInner {
        fn request(
            _state: &mut Self,
            _client: &Client,
            _resource: &xdg_wm_base::XdgWmBase,
            request: xdg_wm_base::Request,
            _data: &(),
            _dhandle: &DisplayHandle,
            data_init: &mut DataInit<'_, Self>,
        ) {
            match request {
                xdg_wm_base::Request::GetXdgSurface { id, surface } => {
                    let Some(sdata): Option<&SurfaceData> = surface.data() else {
                        tracing::error!("GetXdgSurface: surface missing data, rejecting");
                        return;
                    };
                    data_init.init(
                        id,
                        XdgSurfaceData {
                            surface_id: sdata.surface_id,
                            client_id: sdata.client_id,
                        },
                    );
                }
                xdg_wm_base::Request::Pong { serial: _ } => {}
                xdg_wm_base::Request::CreatePositioner { id: _ } => {}
                xdg_wm_base::Request::Destroy => {}
                _ => {}
            }
        }
    }

    // ========================================================================
    // Dispatch: xdg_surface — surface role assignment
    // ========================================================================

    impl Dispatch<xdg_surface::XdgSurface, XdgSurfaceData> for WaylandInner {
        fn request(
            state: &mut Self,
            _client: &Client,
            _resource: &xdg_surface::XdgSurface,
            request: xdg_surface::Request,
            data: &XdgSurfaceData,
            _dhandle: &DisplayHandle,
            data_init: &mut DataInit<'_, Self>,
        ) {
            match request {
                xdg_surface::Request::GetToplevel { id } => {
                    state
                        .bridge
                        .create_toplevel(data.surface_id, data.client_id);
                    data_init.init(
                        id,
                        ToplevelData {
                            surface_id: data.surface_id,
                            client_id: data.client_id,
                        },
                    );
                }
                xdg_surface::Request::AckConfigure { serial } => {
                    state.bridge.ack_configure(data.surface_id, serial);
                }
                xdg_surface::Request::SetWindowGeometry {
                    x: _,
                    y: _,
                    width: _,
                    height: _,
                } => {}
                xdg_surface::Request::GetPopup { .. } => {}
                xdg_surface::Request::Destroy => {}
                _ => {}
            }
        }
    }

    // ========================================================================
    // Dispatch: xdg_toplevel — window management requests
    // ========================================================================

    impl Dispatch<xdg_toplevel::XdgToplevel, ToplevelData> for WaylandInner {
        fn request(
            state: &mut Self,
            _client: &Client,
            _resource: &xdg_toplevel::XdgToplevel,
            request: xdg_toplevel::Request,
            data: &ToplevelData,
            _dhandle: &DisplayHandle,
            _data_init: &mut DataInit<'_, Self>,
        ) {
            match request {
                xdg_toplevel::Request::SetTitle { title } => {
                    state.bridge.set_title(data.surface_id, title);
                }
                xdg_toplevel::Request::SetAppId { app_id } => {
                    state.bridge.set_app_id(data.surface_id, app_id);
                }
                xdg_toplevel::Request::SetMaxSize { width, height } => {
                    let max = if width > 0 && height > 0 {
                        Some((width as u32, height as u32))
                    } else {
                        None
                    };
                    state.bridge.set_size_bounds(data.surface_id, None, max);
                }
                xdg_toplevel::Request::SetMinSize { width, height } => {
                    let min = if width > 0 && height > 0 {
                        Some((width as u32, height as u32))
                    } else {
                        None
                    };
                    state.bridge.set_size_bounds(data.surface_id, min, None);
                }
                xdg_toplevel::Request::SetMaximized => {
                    state.bridge.set_maximized(data.surface_id, true);
                }
                xdg_toplevel::Request::UnsetMaximized => {
                    state.bridge.set_maximized(data.surface_id, false);
                }
                xdg_toplevel::Request::SetFullscreen { output: _ } => {
                    state.bridge.set_fullscreen(data.surface_id, true);
                }
                xdg_toplevel::Request::UnsetFullscreen => {
                    state.bridge.set_fullscreen(data.surface_id, false);
                }
                xdg_toplevel::Request::SetMinimized => {
                    state.bridge.set_minimized(data.surface_id);
                }
                xdg_toplevel::Request::Move { seat: _, serial: _ } => {
                    state.bridge.request_move(data.surface_id);
                }
                xdg_toplevel::Request::Resize {
                    seat: _,
                    serial: _,
                    edges,
                } => {
                    state.bridge.request_resize(data.surface_id, edges.into());
                }
                xdg_toplevel::Request::SetParent { parent: _ } => {}
                xdg_toplevel::Request::ShowWindowMenu {
                    seat: _,
                    serial: _,
                    x: _,
                    y: _,
                } => {}
                xdg_toplevel::Request::Destroy => {
                    state.bridge.destroy_surface(data.surface_id);
                }
                _ => {}
            }
        }
    }
}

#[cfg(feature = "wayland")]
pub use wayland_live::WaylandState;

// ============================================================================
// Wayland server accept loop (feature-gated)
// ============================================================================

#[cfg(feature = "wayland")]
mod wayland_accept {
    use std::path::{Path, PathBuf};

    use tokio::net::UnixListener;
    use tokio::sync::mpsc;

    /// Event dispatched when a new Wayland client connects.
    #[derive(Debug)]
    pub enum WaylandServerEvent {
        /// A new client connection was accepted.
        ClientConnected {
            /// File descriptor of the accepted Unix socket.
            fd: std::os::unix::io::RawFd,
            /// Client credentials (PID, if obtainable).
            pid: Option<u32>,
        },
        /// The server socket was shut down.
        Shutdown,
    }

    /// Configuration for the Wayland accept-loop server.
    #[derive(Debug, Clone)]
    pub struct WaylandServerConfig {
        /// Path to the Unix socket. Defaults to `/run/user/{uid}/wayland-0`.
        pub socket_path: PathBuf,
    }

    impl Default for WaylandServerConfig {
        fn default() -> Self {
            let uid = unsafe { libc::getuid() };
            Self {
                socket_path: PathBuf::from(format!("/run/user/{}/wayland-0", uid)),
            }
        }
    }

    impl WaylandServerConfig {
        /// Create a config with a custom socket path.
        pub fn with_path(path: impl Into<PathBuf>) -> Self {
            Self {
                socket_path: path.into(),
            }
        }
    }

    /// Wayland Unix socket server that accepts client connections and
    /// dispatches events to the compositor.
    ///
    /// This wraps a tokio [`UnixListener`] and produces [`WaylandServerEvent`]s
    /// on a channel that the compositor event loop can consume.
    #[derive(Debug)]
    pub struct WaylandServer {
        /// Path where the socket is bound.
        pub socket_path: PathBuf,
        /// Sender for server events (client connections, shutdown).
        event_tx: mpsc::Sender<WaylandServerEvent>,
        /// Receiver for server events — consumed by the compositor loop.
        event_rx: Option<mpsc::Receiver<WaylandServerEvent>>,
    }

    impl WaylandServer {
        /// Create a new server bound to the configured socket path.
        ///
        /// The socket file is created (any pre-existing file at the path is removed).
        /// Call [`start_server`] to begin the accept loop.
        pub fn new(config: WaylandServerConfig) -> std::io::Result<Self> {
            // Remove stale socket if it exists
            if config.socket_path.exists() {
                std::fs::remove_file(&config.socket_path)?;
            }

            // Ensure parent directory exists
            if let Some(parent) = config.socket_path.parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent)?;
                }
            }

            let (tx, rx) = mpsc::channel(64);
            Ok(Self {
                socket_path: config.socket_path,
                event_tx: tx,
                event_rx: Some(rx),
            })
        }

        /// Take the event receiver. Can only be called once; subsequent calls
        /// return `None`.
        pub fn take_event_rx(&mut self) -> Option<mpsc::Receiver<WaylandServerEvent>> {
            self.event_rx.take()
        }

        /// Get a clone of the event sender (for the accept loop task).
        pub fn event_tx(&self) -> mpsc::Sender<WaylandServerEvent> {
            self.event_tx.clone()
        }

        /// Socket path this server is bound to.
        pub fn path(&self) -> &Path {
            &self.socket_path
        }
    }

    /// Start the Wayland client accept loop.
    ///
    /// Binds a [`UnixListener`] at the given path and spawns a tokio task that
    /// accepts connections, extracts peer credentials (PID), and sends
    /// [`WaylandServerEvent::ClientConnected`] events on the provided channel.
    ///
    /// The task runs until the sender is dropped or an unrecoverable error occurs,
    /// at which point it sends [`WaylandServerEvent::Shutdown`].
    pub async fn start_server(
        socket_path: PathBuf,
        event_tx: mpsc::Sender<WaylandServerEvent>,
    ) -> std::io::Result<()> {
        let listener = UnixListener::bind(&socket_path)?;
        tracing::info!(path = %socket_path.display(), "Wayland server listening");

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _addr)) => {
                        // Try to extract peer credentials for the PID
                        let pid = stream
                            .peer_cred()
                            .ok()
                            .and_then(|cred| cred.pid().map(|p| p as u32));

                        use std::os::unix::io::AsRawFd;
                        let fd = stream.as_raw_fd();

                        if event_tx
                            .send(WaylandServerEvent::ClientConnected { fd, pid })
                            .await
                            .is_err()
                        {
                            tracing::debug!("Event channel closed, shutting down accept loop");
                            break;
                        }

                        tracing::debug!(
                            pid = ?pid,
                            "Accepted Wayland client connection"
                        );

                        // Keep the stream alive — in a real compositor this would be
                        // handed off to the protocol dispatch layer. For now we leak
                        // the fd intentionally so the connection stays open.
                        std::mem::forget(stream);
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to accept Wayland client");
                        // Transient errors (EMFILE, ENFILE) — continue.
                        // Fatal errors will manifest as repeated failures and the
                        // channel will eventually be dropped.
                        continue;
                    }
                }
            }

            let _ = event_tx.send(WaylandServerEvent::Shutdown).await;
            tracing::info!("Wayland server accept loop exited");
        });

        Ok(())
    }
}

#[cfg(feature = "wayland")]
pub use wayland_accept::{start_server, WaylandServer, WaylandServerConfig, WaylandServerEvent};
