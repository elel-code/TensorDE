//! raw-window-handle adapters for native surfaces (wgpu / Vulkan).
//!
//! Handles expose `wl_display` + `wl_surface` via raw-window-handle 0.6 so
//! renderers can create `VK_KHR_wayland_surface` objects (or wgpu's Wayland
//! backend) without depending on this crate's protocol types.

use std::fmt;
use std::ptr::NonNull;
use std::sync::Arc;

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle, WindowHandle,
};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Proxy};
use wayland_protocols::ext::session_lock::v1::client::ext_session_lock_surface_v1::ExtSessionLockSurfaceV1;
use wayland_protocols::xdg::dialog::v1::client::xdg_dialog_v1::XdgDialogV1;
use wayland_protocols::xdg::shell::client::{
    xdg_popup::XdgPopup, xdg_surface::XdgSurface, xdg_toplevel::XdgToplevel,
};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::ZwlrLayerSurfaceV1;

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
pub(crate) struct NativeSurfaceLease {
    connection: Connection,
    surface: WlSurface,
    id: NativeSurfaceId,
    kind: SurfaceKind,
    role: NativeSurfaceRoleLease,
    _parent: Option<Arc<NativeSurfaceLease>>,
}

enum NativeSurfaceRoleLease {
    Toplevel {
        dialog: Option<XdgDialogV1>,
        toplevel: XdgToplevel,
        xdg: XdgSurface,
    },
    Popup {
        popup: XdgPopup,
        xdg: XdgSurface,
    },
    Layer(ZwlrLayerSurfaceV1),
    SessionLock(ExtSessionLockSurfaceV1),
}

impl NativeSurfaceLease {
    fn new(
        connection: Connection,
        surface: WlSurface,
        id: NativeSurfaceId,
        kind: SurfaceKind,
        role: NativeSurfaceRoleLease,
        parent: Option<Arc<Self>>,
    ) -> Self {
        Self {
            connection,
            surface,
            id,
            kind,
            role,
            _parent: parent,
        }
    }

    #[allow(clippy::too_many_arguments)] // mirrors the complete xdg role lease
    pub(crate) fn toplevel(
        connection: Connection,
        surface: WlSurface,
        id: NativeSurfaceId,
        kind: SurfaceKind,
        xdg: XdgSurface,
        toplevel: XdgToplevel,
        dialog: Option<XdgDialogV1>,
        parent: Option<Arc<Self>>,
    ) -> Self {
        Self::new(
            connection,
            surface,
            id,
            kind,
            NativeSurfaceRoleLease::Toplevel {
                dialog,
                toplevel,
                xdg,
            },
            parent,
        )
    }

    pub(crate) fn popup(
        connection: Connection,
        surface: WlSurface,
        id: NativeSurfaceId,
        xdg: XdgSurface,
        popup: XdgPopup,
        parent: Arc<Self>,
    ) -> Self {
        Self::new(
            connection,
            surface,
            id,
            SurfaceKind::Popup,
            NativeSurfaceRoleLease::Popup { popup, xdg },
            Some(parent),
        )
    }

    pub(crate) fn layer(
        connection: Connection,
        surface: WlSurface,
        id: NativeSurfaceId,
        layer: ZwlrLayerSurfaceV1,
    ) -> Self {
        Self::new(
            connection,
            surface,
            id,
            SurfaceKind::Layer,
            NativeSurfaceRoleLease::Layer(layer),
            None,
        )
    }

    pub(crate) fn session_lock(
        connection: Connection,
        surface: WlSurface,
        id: NativeSurfaceId,
        role: ExtSessionLockSurfaceV1,
    ) -> Self {
        Self::new(
            connection,
            surface,
            id,
            SurfaceKind::SessionLock,
            NativeSurfaceRoleLease::SessionLock(role),
            None,
        )
    }
}

impl Drop for NativeSurfaceLease {
    fn drop(&mut self) {
        match &self.role {
            NativeSurfaceRoleLease::Toplevel {
                dialog,
                toplevel,
                xdg,
            } => {
                if let Some(dialog) = dialog {
                    dialog.destroy();
                }
                toplevel.destroy();
                xdg.destroy();
            }
            NativeSurfaceRoleLease::Popup { popup, xdg } => {
                popup.destroy();
                xdg.destroy();
            }
            NativeSurfaceRoleLease::Layer(layer) => layer.destroy(),
            NativeSurfaceRoleLease::SessionLock(role) => role.destroy(),
        }
        self.surface.destroy();
        let _ = self.connection.flush();
    }
}

/// Cloneable renderer-facing lease for one complete Wayland surface role.
#[derive(Clone)]
pub struct NativeSurfaceHandle {
    lease: Arc<NativeSurfaceLease>,
}

impl NativeSurfaceHandle {
    pub(crate) fn from_lease(lease: Arc<NativeSurfaceLease>) -> Self {
        Self { lease }
    }

    pub fn id(&self) -> NativeSurfaceId {
        self.lease.id
    }

    pub fn kind(&self) -> SurfaceKind {
        self.lease.kind
    }

    /// Borrow the live `wl_surface` proxy (for protocol extensions the
    /// renderer may need alongside Vulkan).
    pub fn wl_surface(&self) -> &WlSurface {
        &self.lease.surface
    }

    /// Borrow the Wayland connection that owns this surface's display.
    pub fn connection(&self) -> &Connection {
        &self.lease.connection
    }

    /// Raw `wl_display*` for `VK_KHR_wayland_surface` creation.
    pub fn display_ptr(&self) -> NonNull<std::ffi::c_void> {
        let display = self.lease.connection.display();
        NonNull::new(display.id().as_ptr().cast())
            .expect("a live wl_display proxy always has a non-null pointer")
    }

    /// Raw `wl_surface*` for `VK_KHR_wayland_surface` creation.
    pub fn surface_ptr(&self) -> NonNull<std::ffi::c_void> {
        NonNull::new(self.lease.surface.id().as_ptr().cast())
            .expect("a live wl_surface proxy always has a non-null pointer")
    }

    /// Wayland protocol object id of the leased `wl_surface`.
    pub fn protocol_id(&self) -> u32 {
        self.lease.surface.id().protocol_id()
    }
}

impl fmt::Debug for NativeSurfaceHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeSurfaceHandle")
            .field("id", &self.id())
            .field("kind", &self.kind())
            .finish_non_exhaustive()
    }
}

impl HasWindowHandle for NativeSurfaceHandle {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let pointer = self.lease.surface.id().as_ptr();
        let pointer = NonNull::new(pointer.cast())
            .expect("a live wl_surface proxy always has a non-null pointer");
        let raw = RawWindowHandle::Wayland(WaylandWindowHandle::new(pointer));
        // SAFETY: borrow cannot outlive `self`, which keeps the surface proxy alive.
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

impl HasDisplayHandle for NativeSurfaceHandle {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let display = self.lease.connection.display();
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
