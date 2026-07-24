use super::{FocusOutline, LinearRgba16};

/// Global compositor-owned appearance values consumed during ECS extraction.
///
/// These values deliberately describe scene geometry rather than client-side
/// decorations. They therefore apply equally to native Wayland and rootless
/// XWayland views and remain independent of a renderer implementation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SceneAppearance {
    pub focus_ring: FocusRingStyle,
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
