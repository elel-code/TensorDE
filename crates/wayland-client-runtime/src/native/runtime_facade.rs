//! Thin Fika-facing facade over [`NativeShell`].
//!
//! Mirrors the subset of SCTK [`crate::Runtime`] that the platform event loop
//! needs for a basic toplevel + input + frame path. Full feature parity
//! (dialogs, blur, icons, rich DnD) remains on the SCTK backend until ported.

use std::collections::HashMap;
use std::os::fd::AsRawFd;
use std::time::Duration;

use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::Shape as CursorShape;

use crate::event::Event;
use crate::geometry::{LogicalPosition, LogicalSize};
use crate::input::CursorIcon;
use crate::native::event_map::{NativeEventMapState, SurfaceIdMap};
use crate::native::shell::{NativeShell, NativeSurfaceId};
use crate::runtime::{RuntimeCapabilities, RuntimeError, WakeHandle};
use crate::surface::{SurfaceHandle, SurfaceId, ToplevelAttributes};
use crate::wake_fd::EventFdWake;
use crate::{BlurState, NativeError, TextInputState, ToplevelIcon};

/// Native Compio-free shell wrapped for Fika's event loop.
pub struct NativeRuntime {
    shell: NativeShell,
    surfaces: SurfaceIdMap,
    map_state: NativeEventMapState,
    native_ids: HashMap<SurfaceId, NativeSurfaceId>,
    wake: std::sync::Arc<EventFdWake>,
    wake_handle: WakeHandle,
    capabilities: RuntimeCapabilities,
    public_events: Vec<Event>,
}

impl NativeRuntime {
    pub fn connect() -> Result<Self, RuntimeError> {
        let shell = NativeShell::connect_to_env().map_err(map_native_error)?;
        let caps = shell.capabilities();
        let wake = std::sync::Arc::new(
            EventFdWake::new().map_err(|e| RuntimeError::EventLoop(e.to_string()))?,
        );
        let wake_handle = WakeHandle::from_event_fd(wake.clone());
        Ok(Self {
            shell,
            surfaces: SurfaceIdMap::new(),
            map_state: NativeEventMapState::default(),
            native_ids: HashMap::new(),
            wake,
            wake_handle,
            capabilities: RuntimeCapabilities {
                fractional_scale: caps.fractional_scale && caps.viewporter,
                cursor_shape: caps.cursor_shape,
                text_input_v3: caps.text_input,
                layer_shell_v1: caps.layer_shell,
                xdg_activation_v1: caps.activation,
                pointer_gestures_v1: caps.pointer_gestures,
                pointer_gesture_hold_v1: caps.pointer_gesture_hold,
                relative_pointer_v1: caps.relative_pointer,
                popup_reposition: false,
                ..RuntimeCapabilities::default()
            },
            public_events: Vec::with_capacity(128),
        })
    }

    pub fn wake_handle(&self) -> WakeHandle {
        self.wake_handle.clone()
    }

    pub fn capabilities(&self) -> RuntimeCapabilities {
        self.capabilities
    }

    pub fn drain_events_into(&mut self, target: &mut Vec<Event>) {
        self.public_events.clear();
        self.shell.drain_public_events(
            &mut self.surfaces,
            &mut self.map_state,
            &mut self.public_events,
        );
        target.append(&mut self.public_events);
    }

    /// Poll display + wake fd. `None` waits indefinitely.
    pub fn dispatch(&mut self, timeout: Option<Duration>) -> Result<(), RuntimeError> {
        self.shell
            .connection()
            .flush()
            .map_err(map_native_error)?;
        let _ = self.shell.dispatch_pending().map_err(map_native_error)?;

        let display_fd = self.shell.connection().as_fd().as_raw_fd();
        let wake_fd = self.wake.as_raw_fd();

        let mut pollfds = [
            libc::pollfd {
                fd: display_fd,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: wake_fd,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let timeout_ms = match timeout {
            None => -1,
            Some(d) if d.is_zero() => 0,
            Some(d) => d.as_millis().min(i32::MAX as u128) as i32,
        };
        // SAFETY: poll on two valid fds owned by this process.
        let n = unsafe { libc::poll(pollfds.as_mut_ptr(), 2, timeout_ms) };
        if n < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                return Ok(());
            }
            return Err(RuntimeError::EventLoop(err.to_string()));
        }
        if pollfds[1].revents != 0 {
            self.wake.drain();
        }
        if pollfds[0].revents != 0 {
            if let Some(guard) = self.shell.connection().connection().prepare_read() {
                match guard.read() {
                    Ok(_) => {}
                    Err(wayland_client::backend::WaylandError::Io(err))
                        if err.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(error) => {
                        return Err(RuntimeError::EventLoop(error.to_string()));
                    }
                }
            }
            let _ = self.shell.dispatch_pending().map_err(map_native_error)?;
        }
        Ok(())
    }

    pub fn create_toplevel(
        &mut self,
        attributes: ToplevelAttributes,
    ) -> Result<SurfaceId, RuntimeError> {
        // Prefer min_size as the initial buffer/viewport hint when Fika sets it;
        // otherwise a desktop-typical default until the first configure.
        let width = attributes
            .min_size
            .map(|s| s.width)
            .filter(|&w| w > 0)
            .or_else(|| attributes.max_size.map(|s| s.width).filter(|&w| w > 0))
            .unwrap_or(800)
            .max(1);
        let height = attributes
            .min_size
            .map(|s| s.height)
            .filter(|&h| h > 0)
            .or_else(|| attributes.max_size.map(|s| s.height).filter(|&h| h > 0))
            .unwrap_or(600)
            .max(1);
        let native = self
            .shell
            .create_toplevel_gpu(attributes.title, attributes.app_id, width, height)
            .map_err(map_native_error)?;
        let public = self.surfaces.intern(native);
        self.native_ids.insert(public, native);
        Ok(public)
    }

    pub fn surface_handle(&self, surface: SurfaceId) -> Option<SurfaceHandle> {
        let native = *self.native_ids.get(&surface)?;
        self.shell.public_surface_handle(native).ok()
    }

    pub fn set_title(&mut self, surface: SurfaceId, title: String) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .set_title(native, title)
            .map_err(map_native_error)
    }

    pub fn set_min_size(
        &mut self,
        _surface: SurfaceId,
        _size: Option<LogicalSize>,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }

    pub fn set_max_size(
        &mut self,
        _surface: SurfaceId,
        _size: Option<LogicalSize>,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }

    pub fn set_cursor(&mut self, icon: CursorIcon) -> Result<(), RuntimeError> {
        if !self.shell.has_cursor_shape() {
            return Err(RuntimeError::Unsupported("wp_cursor_shape_manager_v1"));
        }
        let shape = match icon {
            CursorIcon::Default => CursorShape::Default,
            CursorIcon::Pointer => CursorShape::Pointer,
            CursorIcon::Text => CursorShape::Text,
            CursorIcon::ColResize => CursorShape::ColResize,
        };
        self.shell
            .set_cursor_shape(shape)
            .map_err(map_native_error)
    }

    pub fn request_frame(&mut self, surface: SurfaceId) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .request_frame(native)
            .map_err(map_native_error)
    }

    pub fn commit(&mut self, surface: SurfaceId) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .commit_surface(native)
            .map_err(map_native_error)
    }

    pub fn set_buffer_scale(&mut self, surface: SurfaceId, factor: i32) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .set_buffer_scale(native, factor)
            .map_err(map_native_error)
    }

    pub fn set_viewport_destination(
        &mut self,
        surface: SurfaceId,
        size: Option<LogicalSize>,
    ) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        if let Some(size) = size {
            self.shell
                .set_viewport_destination(native, size.width as i32, size.height as i32)
                .map_err(map_native_error)?;
        }
        Ok(())
    }

    pub fn set_window_geometry(
        &mut self,
        surface: SurfaceId,
        _origin: LogicalPosition,
        size: LogicalSize,
    ) -> Result<(), RuntimeError> {
        self.set_viewport_destination(surface, Some(size))
    }

    pub fn destroy_surface(&mut self, surface: SurfaceId) -> Result<Vec<SurfaceId>, RuntimeError> {
        let Some(native) = self.native_ids.remove(&surface) else {
            return Err(RuntimeError::SurfaceNotFound(surface));
        };
        self.surfaces.remove(native);
        self.shell
            .destroy_toplevel(native)
            .map_err(map_native_error)?;
        Ok(vec![surface])
    }

    pub fn set_text_input_state(
        &mut self,
        surface: SurfaceId,
        state: Option<&TextInputState>,
    ) -> Result<(), RuntimeError> {
        if !self.shell.has_text_input() {
            return Err(RuntimeError::Unsupported("text_input_v3"));
        }
        let native = self.native(surface)?;
        if state.is_some() {
            self.shell
                .enable_text_input(native)
                .map_err(map_native_error)
        } else {
            self.shell.disable_text_input().map_err(map_native_error)
        }
    }

    pub fn request_user_attention(&mut self, surface: SurfaceId) -> Result<(), RuntimeError> {
        if !self.shell.has_activation() {
            return Err(RuntimeError::Unsupported("xdg_activation_v1"));
        }
        let native = self.native(surface)?;
        self.shell
            .request_activation_token(native, None)
            .map_err(map_native_error)
    }

    pub fn set_blur(&mut self, _surface: SurfaceId, _state: BlurState) -> Result<(), RuntimeError> {
        Err(RuntimeError::Unsupported("ext_background_effect"))
    }

    pub fn set_toplevel_icon(
        &mut self,
        _surface: SurfaceId,
        _icon: Option<ToplevelIcon>,
    ) -> Result<(), RuntimeError> {
        Err(RuntimeError::Unsupported("xdg_toplevel_icon_v1"))
    }

    pub fn set_pointer_gestures_enabled(
        &mut self,
        _surface: SurfaceId,
        _enabled: bool,
    ) -> Result<(), RuntimeError> {
        if self.capabilities.pointer_gestures_v1 {
            Ok(())
        } else {
            Err(RuntimeError::Unsupported("zwp_pointer_gestures_v1"))
        }
    }

    fn native(&self, surface: SurfaceId) -> Result<NativeSurfaceId, RuntimeError> {
        self.native_ids
            .get(&surface)
            .copied()
            .ok_or(RuntimeError::SurfaceNotFound(surface))
    }
}

fn map_native_error(error: NativeError) -> RuntimeError {
    match error {
        NativeError::Connect(msg) => RuntimeError::Connect(msg),
        NativeError::Registry(msg) => RuntimeError::Registry(msg),
        NativeError::Protocol(msg) => RuntimeError::Protocol(msg),
        NativeError::Io(msg) => RuntimeError::EventLoop(msg),
    }
}
