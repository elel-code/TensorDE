//! Transitional conversions at the Smithay protocol adapter boundary.

use smithay::{
    backend::allocator::{
        Format as SmithayDrmFormat, Fourcc as SmithayFourcc, Modifier as SmithayModifier,
    },
    output::{Mode as SmithayMode, Subpixel as SmithaySubpixel},
};
use tensor_host::{DrmFormat, Fourcc, Modifier, PhysicalMode, SubpixelLayout};

#[inline]
pub(super) fn output_mode(mode: PhysicalMode) -> SmithayMode {
    SmithayMode {
        size: (mode.width, mode.height).into(),
        refresh: mode.refresh_millihertz,
    }
}

#[inline]
pub(super) fn output_subpixel(layout: SubpixelLayout) -> SmithaySubpixel {
    match layout {
        SubpixelLayout::Unknown => SmithaySubpixel::Unknown,
        SubpixelLayout::None => SmithaySubpixel::None,
        SubpixelLayout::HorizontalRgb => SmithaySubpixel::HorizontalRgb,
        SubpixelLayout::HorizontalBgr => SmithaySubpixel::HorizontalBgr,
        SubpixelLayout::VerticalRgb => SmithaySubpixel::VerticalRgb,
        SubpixelLayout::VerticalBgr => SmithaySubpixel::VerticalBgr,
    }
}

#[inline]
pub(super) fn host_drm_format(format: SmithayDrmFormat) -> DrmFormat {
    DrmFormat::new(
        Fourcc::from_raw(format.code as u32),
        Modifier::from_raw(u64::from(format.modifier)),
    )
}

#[inline]
pub(super) fn smithay_drm_format(format: DrmFormat) -> SmithayDrmFormat {
    SmithayDrmFormat {
        code: SmithayFourcc::try_from(format.code.raw()).unwrap_or(SmithayFourcc::Xrgb8888),
        modifier: SmithayModifier::from(format.modifier.raw()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_subpixel_preserves_every_host_layout() {
        for layout in [
            SubpixelLayout::Unknown,
            SubpixelLayout::None,
            SubpixelLayout::HorizontalRgb,
            SubpixelLayout::HorizontalBgr,
            SubpixelLayout::VerticalRgb,
            SubpixelLayout::VerticalBgr,
        ] {
            let adapter = output_subpixel(layout);
            let roundtrip = match adapter {
                SmithaySubpixel::Unknown => SubpixelLayout::Unknown,
                SmithaySubpixel::None => SubpixelLayout::None,
                SmithaySubpixel::HorizontalRgb => SubpixelLayout::HorizontalRgb,
                SmithaySubpixel::HorizontalBgr => SubpixelLayout::HorizontalBgr,
                SmithaySubpixel::VerticalRgb => SubpixelLayout::VerticalRgb,
                SmithaySubpixel::VerticalBgr => SubpixelLayout::VerticalBgr,
            };
            assert_eq!(roundtrip, layout);
        }
    }

    #[test]
    fn drm_format_preserves_explicit_modifier() {
        let format = DrmFormat::new(Fourcc::XRGB8888, Modifier::from_raw(9));
        assert_eq!(host_drm_format(smithay_drm_format(format)), format);
    }
}
