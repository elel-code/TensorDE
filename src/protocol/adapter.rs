//! Transitional conversions at the Smithay protocol adapter boundary.

use smithay::output::{Mode as SmithayMode, Subpixel as SmithaySubpixel};
use tensor_host::{PhysicalMode, SubpixelLayout};

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
}
