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

    pub const fn right(self) -> i32 {
        self.x.saturating_add(self.width as i32)
    }

    pub const fn bottom(self) -> i32 {
        self.y.saturating_add(self.height as i32)
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
}
