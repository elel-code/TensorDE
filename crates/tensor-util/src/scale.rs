use super::{Rect, Size};

/// Output scale represented in the units used by `wp_fractional_scale_v1`.
///
/// Keeping the denominator fixed avoids independently rounding the Smithay,
/// scene, damage, and Vulkan coordinate domains.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OutputScale(u32);

impl OutputScale {
    pub const DENOMINATOR: u32 = 120;
    pub const MIN_UNITS: u32 = 12;
    pub const MAX_UNITS: u32 = 1_200;
    pub const ONE: Self = Self(Self::DENOMINATOR);

    pub fn from_f64(value: f64) -> Option<Self> {
        if !value.is_finite() || !(0.1..=10.0).contains(&value) {
            return None;
        }
        let units = (value * f64::from(Self::DENOMINATOR)).round() as u32;
        Self::from_units(units)
    }

    pub const fn from_units(units: u32) -> Option<Self> {
        if units >= Self::MIN_UNITS && units <= Self::MAX_UNITS {
            Some(Self(units))
        } else {
            None
        }
    }

    pub const fn units(self) -> u32 {
        self.0
    }

    pub fn as_f64(self) -> f64 {
        f64::from(self.0) / f64::from(Self::DENOMINATOR)
    }

    pub fn physical_length_round(self, logical: u32) -> u32 {
        let numerator = u64::from(logical) * u64::from(self.0);
        let rounded = (numerator + u64::from(Self::DENOMINATOR / 2)) / u64::from(Self::DENOMINATOR);
        u32::try_from(rounded).unwrap_or(u32::MAX)
    }

    pub fn logical_size_ceil(self, physical: Size) -> Size {
        Size::new(
            self.logical_length_ceil(physical.width),
            self.logical_length_ceil(physical.height),
        )
    }

    pub fn logical_length_ceil(self, physical: u32) -> u32 {
        let numerator = u64::from(physical) * u64::from(Self::DENOMINATOR);
        u32::try_from(numerator.div_ceil(u64::from(self.0))).unwrap_or(u32::MAX)
    }

    /// Map both rectangle edges independently to the nearest physical pixel.
    /// Adjacent logical rectangles therefore keep a shared physical edge.
    pub fn physical_rect_round(self, logical: Rect) -> Rect {
        self.map_rect(
            logical,
            |edge| round_ratio(edge, self.0),
            |edge| round_ratio(edge, self.0),
        )
    }

    /// Map a logical rectangle to a physical rectangle that fully covers it.
    /// This is used for damage and scissors, where dropping a partial edge
    /// pixel would leave stale output content.
    pub fn physical_rect_cover(self, logical: Rect) -> Rect {
        self.map_rect(
            logical,
            |edge| floor_ratio(edge, self.0),
            |edge| ceil_ratio(edge, self.0),
        )
    }

    fn map_rect(
        self,
        logical: Rect,
        lower: impl Fn(i32) -> i32,
        upper: impl Fn(i32) -> i32,
    ) -> Rect {
        let left = lower(logical.x);
        let top = lower(logical.y);
        let right = if logical.width == 0 {
            left
        } else {
            upper(logical.right())
        };
        let bottom = if logical.height == 0 {
            top
        } else {
            upper(logical.bottom())
        };
        Rect::new(
            left,
            top,
            u32::try_from(right.saturating_sub(left)).unwrap_or(u32::MAX),
            u32::try_from(bottom.saturating_sub(top)).unwrap_or(u32::MAX),
        )
    }
}

impl Default for OutputScale {
    fn default() -> Self {
        Self::ONE
    }
}

fn round_ratio(value: i32, units: u32) -> i32 {
    let numerator = i64::from(value) * i64::from(units);
    let denominator = i64::from(OutputScale::DENOMINATOR);
    let rounded = if numerator >= 0 {
        (numerator + denominator / 2) / denominator
    } else {
        -((-numerator + denominator / 2) / denominator)
    };
    clamp_i64_to_i32(rounded)
}

fn floor_ratio(value: i32, units: u32) -> i32 {
    let numerator = i64::from(value) * i64::from(units);
    clamp_i64_to_i32(numerator.div_euclid(i64::from(OutputScale::DENOMINATOR)))
}

fn ceil_ratio(value: i32, units: u32) -> i32 {
    let numerator = i64::from(value) * i64::from(units);
    let denominator = i64::from(OutputScale::DENOMINATOR);
    clamp_i64_to_i32(-(-numerator).div_euclid(denominator))
}

fn clamp_i64_to_i32(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantizes_to_wayland_fractional_scale_units() {
        let scale = OutputScale::from_f64(1.31).unwrap();
        assert_eq!(scale.units(), 157);
        assert_eq!(scale.as_f64(), 157.0 / 120.0);
        assert!(OutputScale::from_f64(0.0).is_none());
        assert!(OutputScale::from_f64(f64::NAN).is_none());
    }

    #[test]
    fn logical_size_uses_smithay_compatible_ceil_rounding() {
        let scale = OutputScale::from_f64(1.25).unwrap();
        assert_eq!(
            scale.logical_size_ceil(Size::new(1920, 1080)),
            Size::new(1536, 864)
        );
        assert_eq!(
            scale.logical_size_ceil(Size::new(101, 51)),
            Size::new(81, 41)
        );
    }

    #[test]
    fn rounded_rectangles_preserve_shared_edges() {
        let scale = OutputScale::from_f64(1.25).unwrap();
        let left = scale.physical_rect_round(Rect::new(0, 0, 1, 10));
        let right = scale.physical_rect_round(Rect::new(1, 0, 1, 10));
        assert_eq!(left.right(), right.x);
        assert_eq!(right.right(), 3);
    }

    #[test]
    fn covering_rectangles_include_partial_and_negative_edges() {
        let scale = OutputScale::from_f64(1.25).unwrap();
        assert_eq!(
            scale.physical_rect_cover(Rect::new(-1, -1, 2, 2)),
            Rect::new(-2, -2, 4, 4)
        );
    }
}
