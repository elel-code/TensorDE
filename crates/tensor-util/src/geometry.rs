#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(self) -> i32 {
        self.x
            .saturating_add(i32::try_from(self.width).unwrap_or(i32::MAX))
    }

    pub fn bottom(self) -> i32 {
        self.y
            .saturating_add(i32::try_from(self.height).unwrap_or(i32::MAX))
    }

    pub const fn size(self) -> Size {
        Size::new(self.width, self.height)
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if right <= left || bottom <= top {
            return None;
        }
        Some(Self::new(
            left,
            top,
            (right - left) as u32,
            (bottom - top) as u32,
        ))
    }

    pub fn translated(self, x: i32, y: i32) -> Self {
        Self::new(
            self.x.saturating_add(x),
            self.y.saturating_add(y),
            self.width,
            self.height,
        )
    }

    pub fn inflated(self, amount: u32) -> Self {
        let amount_i32 = i32::try_from(amount).unwrap_or(i32::MAX);
        Self::new(
            self.x.saturating_sub(amount_i32),
            self.y.saturating_sub(amount_i32),
            self.width.saturating_add(amount.saturating_mul(2)),
            self.height.saturating_add(amount.saturating_mul(2)),
        )
    }

    pub fn union(self, other: Self) -> Self {
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());
        Self::new(
            left,
            top,
            u32::try_from(right.saturating_sub(left)).unwrap_or(u32::MAX),
            u32::try_from(bottom.saturating_sub(top)).unwrap_or(u32::MAX),
        )
    }

    pub fn touches_or_overlaps(self, other: Self) -> bool {
        self.x <= other.right()
            && other.x <= self.right()
            && self.y <= other.bottom()
            && other.y <= self.bottom()
    }

    pub fn contains_rect(self, other: Self) -> bool {
        self.x <= other.x
            && self.y <= other.y
            && self.right() >= other.right()
            && self.bottom() >= other.bottom()
    }
}

pub fn split_evenly(length: u32, count: usize) -> Vec<u32> {
    if count == 0 {
        return Vec::new();
    }

    let count = u32::try_from(count).unwrap_or(u32::MAX);
    let base = length / count;
    let remainder = length % count;
    (0..count)
        .map(|index| base + u32::from(index < remainder))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_preserves_length_and_balances_remainder() {
        assert_eq!(split_evenly(10, 3), [4, 3, 3]);
        assert_eq!(split_evenly(10, 3).into_iter().sum::<u32>(), 10);
    }

    #[test]
    fn empty_split_is_safe() {
        assert!(split_evenly(10, 0).is_empty());
    }

    #[test]
    fn intersection_excludes_touching_and_disjoint_rectangles() {
        let viewport = Rect::new(10, 10, 100, 80);

        assert_eq!(
            viewport.intersection(Rect::new(0, 0, 40, 30)),
            Some(Rect::new(10, 10, 30, 20))
        );
        assert_eq!(viewport.intersection(Rect::new(110, 10, 20, 20)), None);
        assert_eq!(viewport.intersection(Rect::new(200, 200, 20, 20)), None);
    }

    #[test]
    fn effect_bounds_translate_inflate_and_merge_safely() {
        let rect = Rect::new(10, 20, 30, 40).translated(-5, 7).inflated(3);
        assert_eq!(rect, Rect::new(2, 24, 36, 46));

        let adjacent = Rect::new(38, 24, 10, 10);
        assert!(rect.touches_or_overlaps(adjacent));
        let union = rect.union(adjacent);
        assert_eq!(union, Rect::new(2, 24, 46, 46));
        assert!(union.contains_rect(rect));
        assert!(union.contains_rect(adjacent));
    }
}
