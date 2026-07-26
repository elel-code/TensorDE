//! raw-window-handle adapters for native toplevel surfaces (wgpu / Vulkan).

use std::fmt;
use std::ptr::NonNull;

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle, WindowHandle,
};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Proxy};

use super::types::NativeSurfaceId;

/// Renderer lease for a native toplevel: owns connection + surface proxies long
/// enough for wgpu surface creation.
#[derive(Clone)]
pub struct NativeSurfaceHandle {
    connection: Connection,
    surface: WlSurface,
    id: NativeSurfaceId,
}

impl NativeSurfaceHandle {
    pub(crate) fn new(connection: Connection, surface: WlSurface, id: NativeSurfaceId) -> Self {
        Self {
            connection,
            surface,
            id,
        }
    }

    pub fn id(&self) -> NativeSurfaceId {
        self.id
    }
}

impl fmt::Debug for NativeSurfaceHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeSurfaceHandle")
            .field("id", &self.id)
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
