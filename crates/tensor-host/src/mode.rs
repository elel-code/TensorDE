//! Physical display mode (KMS / EDID units).

/// A scanout mode in physical pixels and millihertz refresh.
///
/// Matches the unit convention of DRM and of `wp_presentation` millihertz
/// clocks without depending on any compositor toolkit.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PhysicalMode {
    pub width: i32,
    pub height: i32,
    /// Vertical refresh in millihertz (e.g. 60 Hz → `60_000`).
    pub refresh_millihertz: i32,
}

impl PhysicalMode {
    #[inline]
    pub const fn new(width: i32, height: i32, refresh_millihertz: i32) -> Self {
        Self {
            width,
            height,
            refresh_millihertz,
        }
    }

    #[inline]
    pub const fn size(self) -> (i32, i32) {
        (self.width, self.height)
    }

    #[inline]
    pub fn is_usable(self) -> bool {
        self.width > 0 && self.height > 0 && self.refresh_millihertz > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usable_mode_requires_positive_geometry() {
        assert!(PhysicalMode::new(1920, 1080, 60_000).is_usable());
        assert!(!PhysicalMode::new(0, 1080, 60_000).is_usable());
        assert!(!PhysicalMode::new(1920, 1080, 0).is_usable());
    }
}
