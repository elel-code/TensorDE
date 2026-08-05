use tensor_util::{Rect, Size};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SizeConstraints {
    pub min: Size,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
}

impl SizeConstraints {
    pub const fn new(min: Size, max_width: Option<u32>, max_height: Option<u32>) -> Self {
        Self {
            min,
            max_width,
            max_height,
        }
    }

    pub fn constrain(self, desired: Size) -> Size {
        Size::new(
            constrain_axis(desired.width, self.min.width, self.max_width),
            constrain_axis(desired.height, self.min.height, self.max_height),
        )
    }
}

impl Default for SizeConstraints {
    fn default() -> Self {
        Self::new(Size::new(1, 1), None, None)
    }
}

fn constrain_axis(desired: u32, min: u32, max: Option<u32>) -> u32 {
    let max = max.unwrap_or(u32::MAX).max(min);
    desired.clamp(min, max)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutLength {
    Proportion { numerator: u32, denominator: u32 },
    Fixed(u32),
}

impl LayoutLength {
    pub const fn proportion(numerator: u32, denominator: u32) -> Self {
        let denominator = if denominator == 0 { 1 } else { denominator };
        let divisor = greatest_common_divisor(numerator, denominator);
        Self::Proportion {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        }
    }

    pub const fn fixed(pixels: u32) -> Self {
        Self::Fixed(pixels)
    }

    pub(crate) fn resolve(self, available: u32) -> u32 {
        match self {
            Self::Proportion {
                numerator,
                denominator,
            } => {
                let denominator = u64::from(denominator.max(1));
                let value = u64::from(available).saturating_mul(u64::from(numerator)) / denominator;
                u32::try_from(value).unwrap_or(u32::MAX)
            }
            Self::Fixed(pixels) => pixels,
        }
    }

    pub(crate) fn resolve_column(self, view_width: u32, gap: u32) -> u32 {
        match self {
            Self::Proportion { .. } => self
                .resolve(view_width.saturating_sub(gap))
                .saturating_sub(gap),
            Self::Fixed(pixels) => pixels,
        }
    }
}

const fn greatest_common_divisor(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    if left == 0 { 1 } else { left }
}

impl Default for LayoutLength {
    fn default() -> Self {
        Self::proportion(1, 2)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayoutItem {
    pub constraints: SizeConstraints,
    pub primary_size: Option<LayoutLength>,
}

impl LayoutItem {
    pub const fn new(constraints: SizeConstraints, primary_size: Option<LayoutLength>) -> Self {
        Self {
            constraints,
            primary_size,
        }
    }
}

impl Default for LayoutItem {
    fn default() -> Self {
        Self::new(SizeConstraints::default(), None)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayoutOptions {
    pub gap: u32,
    pub scrolling_default_width: LayoutLength,
    pub master_width: LayoutLength,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            gap: 8,
            scrolling_default_width: LayoutLength::proportion(1, 2),
            master_width: LayoutLength::proportion(55, 100),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LayoutState {
    pub horizontal_offset: i32,
}

impl LayoutState {
    pub(crate) fn reset_horizontal(&mut self) {
        self.horizontal_offset = 0;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayoutPlacement {
    pub geometry: Rect,
    pub visible: Option<Rect>,
}

impl LayoutPlacement {
    pub(crate) fn new(geometry: Rect, viewport: Rect) -> Self {
        Self {
            geometry,
            visible: geometry.intersection(viewport),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayoutSnapshot {
    pub viewport: Rect,
    pub placements: Vec<LayoutPlacement>,
    pub content_bounds: Rect,
    pub horizontal_offset: i32,
}

impl LayoutSnapshot {
    pub(crate) fn empty(area: Rect, state: LayoutState) -> Self {
        Self {
            viewport: area,
            placements: Vec::new(),
            content_bounds: Rect::new(area.x, area.y, 0, 0),
            horizontal_offset: state.horizontal_offset,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contradictory_maximum_never_undercuts_minimum() {
        let constraints = SizeConstraints::new(Size::new(300, 200), Some(100), Some(50));

        assert_eq!(
            constraints.constrain(Size::new(10, 10)),
            Size::new(300, 200)
        );
    }

    #[test]
    fn proportions_are_integer_and_zero_denominator_is_safe() {
        assert_eq!(LayoutLength::proportion(1, 3).resolve(100), 33);
        assert_eq!(LayoutLength::proportion(2, 0).resolve(100), 200);
        assert_eq!(
            LayoutLength::proportion(5_000, 10_000),
            LayoutLength::proportion(1, 2)
        );
    }

    #[test]
    fn placement_keeps_full_geometry_and_visible_clip_separate() {
        let placement = LayoutPlacement::new(Rect::new(80, 10, 40, 30), Rect::new(0, 0, 100, 100));

        assert_eq!(placement.geometry, Rect::new(80, 10, 40, 30));
        assert_eq!(placement.visible, Some(Rect::new(80, 10, 20, 30)));
    }
}
