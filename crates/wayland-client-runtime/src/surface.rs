use std::fmt;
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};

use bitflags::bitflags;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle, WindowHandle,
};
use smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface;
use smithay_client_toolkit::reexports::client::{Connection, Proxy};
use smithay_client_toolkit::reexports::protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::xdg::XdgSurface;
use smithay_client_toolkit::shell::xdg::dialog::Dialog;
use smithay_client_toolkit::shell::xdg::popup::Popup;
use smithay_client_toolkit::shell::xdg::window::Window;

use crate::fractional_scale::FractionalScaleSurface;
use crate::layer_shell::LayerProtocolSurface;
use crate::text_input::TextInputState;
use crate::toplevel_icon::AppliedToplevelIcon;
use crate::{InputSerial, LogicalPosition, LogicalRect, LogicalSize, PointerCaptureState};

/// Stable runtime identifier for a Wayland surface role.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SurfaceId(pub(crate) u64);

impl SurfaceId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SurfaceKind {
    Toplevel,
    Dialog,
    Popup,
    Layer,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum DecorationPreference {
    #[default]
    Server,
    Client,
    None,
}

#[derive(Clone, Debug)]
pub struct ToplevelAttributes {
    pub title: String,
    pub app_id: String,
    pub min_size: Option<LogicalSize>,
    pub max_size: Option<LogicalSize>,
    pub decorations: DecorationPreference,
}

impl Default for ToplevelAttributes {
    fn default() -> Self {
        Self {
            title: String::new(),
            app_id: String::new(),
            min_size: None,
            max_size: None,
            decorations: DecorationPreference::Server,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct DialogAttributes {
    pub toplevel: ToplevelAttributes,
    pub modal: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum PopupAnchor {
    #[default]
    None,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    BottomLeft,
    TopRight,
    BottomRight,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Gravity {
    #[default]
    None,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    BottomLeft,
    TopRight,
    BottomRight,
}

bitflags! {
    #[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
    pub struct ConstraintAdjustments: u8 {
        const SLIDE_X = 1 << 0;
        const SLIDE_Y = 1 << 1;
        const FLIP_X = 1 << 2;
        const FLIP_Y = 1 << 3;
        const RESIZE_X = 1 << 4;
        const RESIZE_Y = 1 << 5;
    }
}

/// Complete xdg-positioner state for a popup.
#[derive(Clone, Debug)]
pub struct PopupPositioner {
    pub size: LogicalSize,
    pub anchor_rect: LogicalRect,
    pub anchor: PopupAnchor,
    pub gravity: Gravity,
    pub constraints: ConstraintAdjustments,
    pub offset: LogicalPosition,
    pub reactive: bool,
    pub parent_size: Option<LogicalSize>,
    pub parent_configure: Option<u32>,
}

impl Default for PopupPositioner {
    fn default() -> Self {
        Self {
            size: LogicalSize::new(1, 1),
            anchor_rect: LogicalRect::new(0, 0, 1, 1),
            anchor: PopupAnchor::None,
            gravity: Gravity::None,
            constraints: ConstraintAdjustments::empty(),
            offset: LogicalPosition::ZERO,
            reactive: false,
            parent_size: None,
            parent_configure: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PopupAttributes {
    pub positioner: PopupPositioner,
    /// A recent pointer-press or touch-down serial requests an explicit popup grab.
    pub grab: Option<InputSerial>,
}

pub(crate) enum ProtocolSurface {
    Toplevel(Window),
    NativeDialog(Dialog),
    FallbackDialog(Window),
    Popup(Popup),
    Layer(LayerProtocolSurface),
}

impl ProtocolSurface {
    pub(crate) fn wl_surface(&self) -> &WlSurface {
        match self {
            Self::Toplevel(window) | Self::FallbackDialog(window) => window.wl_surface(),
            Self::NativeDialog(dialog) => dialog.wl_surface(),
            Self::Popup(popup) => popup.wl_surface(),
            Self::Layer(layer) => layer.wl_surface(),
        }
    }

    pub(crate) fn xdg_surface(
        &self,
    ) -> Option<
        &smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_surface::XdgSurface,
    > {
        match self {
            Self::Toplevel(window) | Self::FallbackDialog(window) => Some(window.xdg_surface()),
            Self::NativeDialog(dialog) => Some(dialog.xdg_surface()),
            Self::Popup(popup) => Some(popup.xdg_surface()),
            Self::Layer(_) => None,
        }
    }

    pub(crate) fn xdg_toplevel(
        &self,
    ) -> Option<
        &smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_toplevel::XdgToplevel,
    >{
        match self {
            Self::Toplevel(window) | Self::FallbackDialog(window) => Some(window.xdg_toplevel()),
            Self::NativeDialog(dialog) => Some(dialog.xdg_toplevel()),
            Self::Popup(_) | Self::Layer(_) => None,
        }
    }

    pub(crate) fn layer_surface(&self) -> Option<&LayerProtocolSurface> {
        match self {
            Self::Layer(layer) => Some(layer),
            _ => None,
        }
    }
}

pub(crate) struct ManagedBlur(pub(crate) ExtBackgroundEffectSurfaceV1);

impl Drop for ManagedBlur {
    fn drop(&mut self) {
        if self.0.is_alive() {
            self.0.destroy();
        }
    }
}

pub(crate) struct SurfaceShared {
    // Destruction order matters: extension role, xdg role, then parent.
    pub(crate) blur: Mutex<Option<ManagedBlur>>,
    pub(crate) fractional_scale: Option<FractionalScaleSurface>,
    pub(crate) pointer_capture: Mutex<PointerCaptureState>,
    pub(crate) text_input: Mutex<Option<TextInputState>>,
    pub(crate) toplevel_icon: Mutex<Option<AppliedToplevelIcon>>,
    pub(crate) protocol: ProtocolSurface,
    pub(crate) parent: Option<Arc<SurfaceShared>>,
    pub(crate) connection: Connection,
    pub(crate) id: SurfaceId,
    pub(crate) kind: SurfaceKind,
}

impl SurfaceShared {
    pub(crate) fn wl_surface(&self) -> &WlSurface {
        self.protocol.wl_surface()
    }
}

/// A renderer-facing lease on a live Wayland surface.
///
/// The lease keeps the protocol role, its `wl_surface`, its connection and all
/// ancestors alive. This makes it suitable for `wgpu::Instance::create_surface`
/// and `VK_KHR_wayland_surface` without either graphics API becoming a crate
/// dependency.
///
/// Backends: SCTK [`Runtime`] (shared graph) or native [`crate::NativeShell`].
#[derive(Clone)]
pub struct SurfaceHandle {
    backend: SurfaceHandleBackend,
}

#[derive(Clone)]
enum SurfaceHandleBackend {
    Sctk(Arc<SurfaceShared>),
    Native(crate::NativeSurfaceHandle),
}

impl SurfaceHandle {
    pub(crate) fn from_sctk(shared: Arc<SurfaceShared>) -> Self {
        Self {
            backend: SurfaceHandleBackend::Sctk(shared),
        }
    }

    pub(crate) fn from_native(handle: crate::NativeSurfaceHandle) -> Self {
        Self {
            backend: SurfaceHandleBackend::Native(handle),
        }
    }

    pub fn id(&self) -> SurfaceId {
        match &self.backend {
            SurfaceHandleBackend::Sctk(shared) => shared.id,
            SurfaceHandleBackend::Native(handle) => SurfaceId(u64::from(handle.id().0)),
        }
    }

    pub fn kind(&self) -> SurfaceKind {
        match &self.backend {
            SurfaceHandleBackend::Sctk(shared) => shared.kind,
            // Native path currently only leases toplevels / layers for wgpu.
            SurfaceHandleBackend::Native(_) => SurfaceKind::Toplevel,
        }
    }
}

impl fmt::Debug for SurfaceHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SurfaceHandle")
            .field("id", &self.id())
            .field("kind", &self.kind())
            .finish_non_exhaustive()
    }
}

impl HasWindowHandle for SurfaceHandle {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        match &self.backend {
            SurfaceHandleBackend::Sctk(shared) => {
                let pointer = shared.wl_surface().id().as_ptr();
                let pointer = NonNull::new(pointer.cast())
                    .expect("a live wl_surface proxy always has a non-null pointer");
                let raw = RawWindowHandle::Wayland(WaylandWindowHandle::new(pointer));
                // SAFETY: borrow cannot outlive `self`, which owns the surface.
                Ok(unsafe { WindowHandle::borrow_raw(raw) })
            }
            SurfaceHandleBackend::Native(handle) => handle.window_handle(),
        }
    }
}

impl HasDisplayHandle for SurfaceHandle {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        match &self.backend {
            SurfaceHandleBackend::Sctk(shared) => {
                let display = shared.connection.display();
                let pointer = display.id().as_ptr();
                let pointer = NonNull::new(pointer.cast())
                    .expect("a live wl_display proxy always has a non-null pointer");
                let raw = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(pointer));
                // SAFETY: `self` owns Connection for at least the returned borrow.
                Ok(unsafe { DisplayHandle::borrow_raw(raw) })
            }
            SurfaceHandleBackend::Native(handle) => handle.display_handle(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_renderer_handle<T: HasWindowHandle + HasDisplayHandle + Clone + Send + Sync>() {}

    #[test]
    fn surface_handle_meets_native_renderer_contract() {
        assert_renderer_handle::<SurfaceHandle>();
    }

    #[test]
    fn popup_positioner_defaults_are_protocol_valid() {
        let positioner = PopupPositioner::default();
        assert!(!positioner.size.is_empty());
        assert!(!positioner.anchor_rect.is_empty());
    }
}
