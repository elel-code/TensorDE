use std::fmt;

use bitflags::bitflags;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};

use crate::{InputSerial, LogicalPosition, LogicalRect, LogicalSize};

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
    /// Preferred initial logical size (native / configure-before-first-buffer).
    pub initial_size: Option<LogicalSize>,
    pub min_size: Option<LogicalSize>,
    pub max_size: Option<LogicalSize>,
    pub decorations: DecorationPreference,
}

impl Default for ToplevelAttributes {
    fn default() -> Self {
        Self {
            title: String::new(),
            app_id: String::new(),
            initial_size: None,
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

/// A renderer-facing lease on a live Wayland surface.
///
/// The lease keeps the protocol role, its `wl_surface`, its connection and all
/// ancestors alive. Suitable for wgpu / Vulkan Wayland surfaces.
#[derive(Clone)]
pub struct SurfaceHandle {
    native: crate::NativeSurfaceHandle,
}

impl SurfaceHandle {
    pub(crate) fn from_native(handle: crate::NativeSurfaceHandle) -> Self {
        Self { native: handle }
    }

    pub fn id(&self) -> SurfaceId {
        SurfaceId(u64::from(self.native.id().0))
    }

    pub fn kind(&self) -> SurfaceKind {
        // Native leases currently expose toplevels for wgpu; dialog/popup/layer
        // still share the same handle shape.
        SurfaceKind::Toplevel
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
        self.native.window_handle()
    }
}

impl HasDisplayHandle for SurfaceHandle {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.native.display_handle()
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
