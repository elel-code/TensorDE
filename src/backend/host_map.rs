//! Map Tensor host types ↔ Smithay / DRM toolkit types at the adapter edge.
//!
//! Policy modules must not import Smithay. Only this file (and other adapter
//! modules) may convert.

use smithay::{
    backend::allocator::{
        Format as SmithayDrmFormat, Fourcc as SmithayFourcc, Modifier as SmithayModifier,
    },
    output::{Mode as SmithayMode, Subpixel as SmithaySubpixel},
};
use tensor_host::{DrmFormat, Fourcc, Modifier, PhysicalMode, SubpixelLayout};

/// Convert a Smithay output mode into a pure physical mode.
#[inline]
pub(crate) fn physical_mode_from_smithay(mode: SmithayMode) -> PhysicalMode {
    PhysicalMode::new(mode.size.w, mode.size.h, mode.refresh)
}

/// Convert a pure physical mode into a Smithay output mode (protocol advertise).
#[inline]
pub(crate) fn smithay_mode(mode: PhysicalMode) -> SmithayMode {
    SmithayMode {
        size: (mode.width, mode.height).into(),
        refresh: mode.refresh_millihertz,
    }
}

#[inline]
pub(crate) fn subpixel_from_smithay(subpixel: SmithaySubpixel) -> SubpixelLayout {
    match subpixel {
        SmithaySubpixel::Unknown => SubpixelLayout::Unknown,
        SmithaySubpixel::None => SubpixelLayout::None,
        SmithaySubpixel::HorizontalRgb => SubpixelLayout::HorizontalRgb,
        SmithaySubpixel::HorizontalBgr => SubpixelLayout::HorizontalBgr,
        SmithaySubpixel::VerticalRgb => SubpixelLayout::VerticalRgb,
        SmithaySubpixel::VerticalBgr => SubpixelLayout::VerticalBgr,
    }
}

#[inline]
pub(crate) fn smithay_subpixel(layout: SubpixelLayout) -> SmithaySubpixel {
    match layout {
        SubpixelLayout::Unknown => SmithaySubpixel::Unknown,
        SubpixelLayout::None => SmithaySubpixel::None,
        SubpixelLayout::HorizontalRgb => SmithaySubpixel::HorizontalRgb,
        SubpixelLayout::HorizontalBgr => SmithaySubpixel::HorizontalBgr,
        SubpixelLayout::VerticalRgb => SmithaySubpixel::VerticalRgb,
        SubpixelLayout::VerticalBgr => SmithaySubpixel::VerticalBgr,
    }
}

/// Convert a Smithay/allocator format into a pure host format.
#[inline]
pub(crate) fn host_drm_format(format: SmithayDrmFormat) -> DrmFormat {
    DrmFormat::new(
        Fourcc::from_raw(format.code as u32),
        Modifier::from_raw(u64::from(format.modifier)),
    )
}

/// Convert a host format into a Smithay/allocator format (GBM / KMS edge).
#[inline]
pub(crate) fn smithay_drm_format(format: DrmFormat) -> SmithayDrmFormat {
    SmithayDrmFormat {
        code: smithay_fourcc(format.code),
        modifier: SmithayModifier::from(format.modifier.raw()),
    }
}

#[inline]
pub(crate) fn smithay_fourcc(code: Fourcc) -> SmithayFourcc {
    SmithayFourcc::try_from(code.raw()).unwrap_or(SmithayFourcc::Xrgb8888)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_roundtrip_preserves_refresh() {
        let mode = PhysicalMode::new(2560, 1440, 165_000);
        assert_eq!(physical_mode_from_smithay(smithay_mode(mode)), mode);
    }

    #[test]
    fn subpixel_roundtrip() {
        for layout in [
            SubpixelLayout::Unknown,
            SubpixelLayout::None,
            SubpixelLayout::HorizontalRgb,
            SubpixelLayout::HorizontalBgr,
            SubpixelLayout::VerticalRgb,
            SubpixelLayout::VerticalBgr,
        ] {
            assert_eq!(subpixel_from_smithay(smithay_subpixel(layout)), layout);
        }
    }

    #[test]
    fn format_roundtrip_preserves_modifier() {
        let format = DrmFormat::new(Fourcc::XRGB8888, Modifier::from_raw(9));
        assert_eq!(host_drm_format(smithay_drm_format(format)), format);
    }
}
