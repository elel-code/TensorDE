//! raw-window-handle adapters for native surfaces (wgpu / Vulkan).
//!
//! Handles expose `wl_display` + `wl_surface` via raw-window-handle 0.6 so
//! renderers can create `VK_KHR_wayland_surface` objects (or wgpu's Wayland
//! backend) without depending on this crate's protocol types.

use std::fmt;
use std::ptr::NonNull;

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle, WindowHandle,
};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Proxy};

use super::types::NativeSurfaceId;
use crate::surface::SurfaceKind;

/// Renderer lease for a native surface: owns connection + surface proxies long
/// enough for wgpu / Vulkan surface creation.
///
/// # Vulkan
///
/// With `ash` (or similar), create the surface from the raw handles:
///
/// ```ignore
/// use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
///
/// let display = handle.display_handle()?.as_raw();
/// let window = handle.window_handle()?.as_raw();
/// let (wl_display, wl_surface) = match (display, window) {
///     (RawDisplayHandle::Wayland(d), RawWindowHandle::Wayland(w)) => (d.display, w.surface),
///     _ => unreachable!("native handle is always Wayland"),
/// };
/// // vkCreateWaylandSurfaceKHR(instance, &VkWaylandSurfaceCreateInfoKHR {
/// //     display: wl_display.as_ptr(), surface: wl_surface.as_ptr(), ...
/// // })
/// ```
///
/// Keep this handle (or a clone) alive for as long as the Vulkan / wgpu
/// surface exists — dropping it can destroy the underlying `wl_surface`.
#[derive(Clone)]
pub struct NativeSurfaceHandle {
    connection: Connection,
    surface: WlSurface,
    id: NativeSurfaceId,
    kind: SurfaceKind,
}

impl NativeSurfaceHandle {
    pub(crate) fn new(
        connection: Connection,
        surface: WlSurface,
        id: NativeSurfaceId,
        kind: SurfaceKind,
    ) -> Self {
        Self {
            connection,
            surface,
            id,
            kind,
        }
    }

    pub fn id(&self) -> NativeSurfaceId {
        self.id
    }

    pub fn kind(&self) -> SurfaceKind {
        self.kind
    }

    /// Borrow the live `wl_surface` proxy (for protocol extensions the
    /// renderer may need alongside Vulkan).
    pub fn wl_surface(&self) -> &WlSurface {
        &self.surface
    }

    /// Borrow the Wayland connection that owns this surface's display.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }
}

impl fmt::Debug for NativeSurfaceHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeSurfaceHandle")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl HasWindowHandle for NativeSurfaceHandle {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let pointer = self.surface.id().as_ptr();
        let pointer = NonNull::new(pointer.cast())
            .expect("a live wl_surface proxy always has a non-null pointer");
        let raw = RawWindowHandle::Wayland(WaylandWindowHandle::new(pointer));
        // SAFETY: borrow cannot outlive `self`, which keeps the surface proxy alive.
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

impl HasDisplayHandle for NativeSurfaceHandle {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let display = self.connection.display();
        let pointer = display.id().as_ptr();
        let pointer = NonNull::new(pointer.cast())
            .expect("a live wl_display proxy always has a non-null pointer");
        let raw = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(pointer));
        // SAFETY: `self` owns Connection for at least the returned borrow.
        Ok(unsafe { DisplayHandle::borrow_raw(raw) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_renderer_handle<T: HasWindowHandle + HasDisplayHandle + Clone + Send + Sync>() {}

    #[test]
    fn native_surface_handle_meets_renderer_contract() {
        assert_renderer_handle::<NativeSurfaceHandle>();
    }
}
