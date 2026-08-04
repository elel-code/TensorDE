use super::{FocusOutline, LinearRgba16, ShadowStyle};

/// Global compositor-owned appearance values consumed during ECS extraction.
///
/// These values deliberately describe scene geometry rather than client-side
/// decorations. They therefore apply equally to native Wayland and rootless
/// XWayland views and remain independent of a renderer implementation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SceneAppearance {
    pub focus_ring: FocusRingStyle,
    pub window_shadow: WindowShadowStyle,
    pub window_corners: WindowCornerStyle,
}

/// Appearance policy for the compositor-owned active-view focus ring.
///
/// Niri's four-logical-pixel default is intentional here: with Tensor's
/// default eight-pixel gaps it reaches the middle of each gap without covering
/// client content, and remains clearly visible on fractional-scale outputs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocusRingStyle {
    pub enabled: bool,
    pub width: u32,
    pub color: LinearRgba16,
}

impl FocusRingStyle {
    pub const DEFAULT_COLOR: LinearRgba16 = FocusOutline::DEFAULT.color;

    /// Resolve the policy into frame-local draw data. Disabled, transparent,
    /// and zero-width rings intentionally generate no compositor geometry.
    pub const fn outline(self) -> Option<FocusOutline> {
        if self.enabled && self.width > 0 && self.color.alpha > 0 {
            Some(FocusOutline {
                width: self.width,
                color: self.color,
            })
        } else {
            None
        }
    }
}

impl Default for FocusRingStyle {
    fn default() -> Self {
        Self {
            enabled: true,
            width: FocusOutline::DEFAULT.width,
            color: Self::DEFAULT_COLOR,
        }
    }
}

/// Global compositor-owned shadow policy for native and XWayland views.
///
/// It resolves once at the configuration boundary and remains a compact
/// renderer-independent value. Disabled shadows produce no scene draw.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowShadowStyle {
    pub enabled: bool,
    pub offset_x: i32,
    pub offset_y: i32,
    pub blur_radius: u32,
    pub spread: u32,
    pub color: LinearRgba16,
}

impl WindowShadowStyle {
    pub const DEFAULT_COLOR: LinearRgba16 = LinearRgba16::new(0, 0, 0, 0x7070);

    pub const fn effect(self) -> Option<ShadowStyle> {
        if self.enabled && self.color.alpha > 0 {
            Some(ShadowStyle {
                offset_x: self.offset_x,
                offset_y: self.offset_y,
                blur_radius: self.blur_radius,
                spread: self.spread,
                color: self.color,
            })
        } else {
            None
        }
    }
}

impl Default for WindowShadowStyle {
    fn default() -> Self {
        Self {
            enabled: false,
            offset_x: 0,
            offset_y: 6,
            blur_radius: 18,
            spread: 0,
            color: Self::DEFAULT_COLOR,
        }
    }
}

/// Global rounded-clip policy shared by client sampling, focus, and shadow.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WindowCornerStyle {
    pub radius: u32,
}
