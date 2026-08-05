//! Map drm-rs values into Tensor-owned host types.

use drm::control::{Mode as DrmMode, ModeFlags, connector::SubPixel as DrmSubPixel};
use tensor_host::{PhysicalMode, SubpixelLayout};

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
}
