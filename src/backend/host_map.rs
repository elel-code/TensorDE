//! Map Tensor host types to DRM and Smithay adapter types.
//!
//! Policy modules must not import Smithay. Only this file (and other adapter
//! modules) may convert.

use drm::control::{Mode as DrmMode, ModeFlags, connector::SubPixel as DrmSubPixel};
use smithay::{
    backend::allocator::{
        Format as SmithayDrmFormat, Fourcc as SmithayFourcc, Modifier as SmithayModifier,
    },
    output::{Mode as SmithayMode, Subpixel as SmithaySubpixel},
};
use tensor_host::{DrmFormat, Fourcc, Modifier, PhysicalMode, SubpixelLayout};

/// Convert a drm-rs mode into a pure physical mode without a Smithay hop.
#[inline]
pub(crate) fn physical_mode_from_drm(mode: DrmMode) -> PhysicalMode {
    let (width, height) = mode.size();
    PhysicalMode::new(
        i32::from(width),
        i32::from(height),
        drm_refresh_millihertz(
            mode.clock(),
            mode.hsync().2,
            mode.vsync().2,
            mode.flags(),
            mode.vscan(),
        ),
    )
}

fn drm_refresh_millihertz(
    clock_khz: u32,
    horizontal_total: u16,
    vertical_total: u16,
    flags: ModeFlags,
    vertical_scan: u16,
) -> i32 {
    let horizontal_total = u64::from(horizontal_total);
    let vertical_total = u64::from(vertical_total);
    if horizontal_total == 0 || vertical_total == 0 {
        return 0;
    }
    let mut refresh =
        (u64::from(clock_khz) * 1_000_000 / horizontal_total + vertical_total / 2) / vertical_total;
    if flags.contains(ModeFlags::INTERLACE) {
        refresh *= 2;
    }
    if flags.contains(ModeFlags::DBLSCAN) {
        refresh /= 2;
    }
    if vertical_scan > 1 {
        refresh /= u64::from(vertical_scan);
    }
    i32::try_from(refresh).unwrap_or(i32::MAX)
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
pub(crate) fn subpixel_from_drm(subpixel: DrmSubPixel) -> SubpixelLayout {
    match subpixel {
        DrmSubPixel::Unknown | DrmSubPixel::NotImplemented => SubpixelLayout::Unknown,
        DrmSubPixel::None => SubpixelLayout::None,
        DrmSubPixel::HorizontalRgb => SubpixelLayout::HorizontalRgb,
        DrmSubPixel::HorizontalBgr => SubpixelLayout::HorizontalBgr,
        DrmSubPixel::VerticalRgb => SubpixelLayout::VerticalRgb,
        DrmSubPixel::VerticalBgr => SubpixelLayout::VerticalBgr,
        _ => SubpixelLayout::Unknown,
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
    fn drm_refresh_uses_full_timings_and_scan_flags() {
        assert_eq!(
            drm_refresh_millihertz(148_500, 2200, 1125, ModeFlags::empty(), 0),
            60_000
        );
        assert_eq!(
            drm_refresh_millihertz(74_250, 2200, 1125, ModeFlags::INTERLACE, 0),
            60_000
        );
        assert_eq!(
            drm_refresh_millihertz(148_500, 2200, 1125, ModeFlags::DBLSCAN, 2),
            15_000
        );
        assert_eq!(
            drm_refresh_millihertz(148_500, 0, 1125, ModeFlags::empty(), 0),
            0
        );
    }

    #[test]
    fn smithay_subpixel_roundtrip() {
        for layout in [
            SubpixelLayout::Unknown,
            SubpixelLayout::None,
            SubpixelLayout::HorizontalRgb,
            SubpixelLayout::HorizontalBgr,
            SubpixelLayout::VerticalRgb,
            SubpixelLayout::VerticalBgr,
        ] {
            let smithay = smithay_subpixel(layout);
            assert_eq!(smithay_subpixel_to_host(smithay), layout);
        }
    }

    #[test]
    fn format_roundtrip_preserves_modifier() {
        let format = DrmFormat::new(Fourcc::XRGB8888, Modifier::from_raw(9));
        assert_eq!(host_drm_format(smithay_drm_format(format)), format);
    }

    fn smithay_subpixel_to_host(subpixel: SmithaySubpixel) -> SubpixelLayout {
        match subpixel {
            SmithaySubpixel::Unknown => SubpixelLayout::Unknown,
            SmithaySubpixel::None => SubpixelLayout::None,
            SmithaySubpixel::HorizontalRgb => SubpixelLayout::HorizontalRgb,
            SmithaySubpixel::HorizontalBgr => SubpixelLayout::HorizontalBgr,
            SmithaySubpixel::VerticalRgb => SubpixelLayout::VerticalRgb,
            SmithaySubpixel::VerticalBgr => SubpixelLayout::VerticalBgr,
        }
    }
}
