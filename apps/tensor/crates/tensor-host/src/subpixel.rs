//! Subpixel layout of a physical panel (EDID / KMS).

/// Panel subpixel arrangement. Independent of Wayland `wl_output` enums so
/// adapters can map without leaking protocol crates into policy.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum SubpixelLayout {
    #[default]
    Unknown,
    None,
    HorizontalRgb,
    HorizontalBgr,
    VerticalRgb,
    VerticalBgr,
}

impl SubpixelLayout {
    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::None => "none",
            Self::HorizontalRgb => "horizontal-rgb",
            Self::HorizontalBgr => "horizontal-bgr",
            Self::VerticalRgb => "vertical-rgb",
            Self::VerticalBgr => "vertical-bgr",
        }
    }
}
