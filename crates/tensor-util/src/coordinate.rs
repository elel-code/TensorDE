use std::{
    fmt,
    marker::PhantomData,
    ops::{Add, AddAssign, Sub, SubAssign},
};

#[derive(Clone, Copy, Debug)]
pub enum LogicalSpace {}

#[derive(Clone, Copy, Debug)]
pub enum PhysicalSpace {}

#[derive(Clone, Copy, Debug)]
pub enum BufferSpace {}

pub trait CoordinateScalar: Copy + Default + PartialOrd {
    const ZERO: Self;

    fn saturating_add(self, other: Self) -> Self;
    fn saturating_sub(self, other: Self) -> Self;
    fn as_f64(self) -> f64;
}

impl CoordinateScalar for i32 {
    const ZERO: Self = 0;

    #[inline]
    fn saturating_add(self, other: Self) -> Self {
        self.saturating_add(other)
    }

    #[inline]
    fn saturating_sub(self, other: Self) -> Self {
        self.saturating_sub(other)
    }

    #[inline]
    fn as_f64(self) -> f64 {
        f64::from(self)
    }
}

impl CoordinateScalar for f64 {
    const ZERO: Self = 0.0;

    #[inline]
    fn saturating_add(self, other: Self) -> Self {
        self + other
    }

    #[inline]
    fn saturating_sub(self, other: Self) -> Self {
        self - other
    }

    #[inline]
    fn as_f64(self) -> f64 {
        self
    }
}

#[repr(C)]
pub struct Point2D<N, Space> {
    pub x: N,
    pub y: N,
    space: PhantomData<Space>,
}

impl<N, Space> Point2D<N, Space> {
    pub const fn new(x: N, y: N) -> Self {
        Self {
            x,
            y,
            space: PhantomData,
        }
    }
}

impl<N: CoordinateScalar, Space> Point2D<N, Space> {
    #[inline]
    pub fn to_f64(self) -> Point2D<f64, Space> {
        Point2D::new(self.x.as_f64(), self.y.as_f64())
    }
}

impl<Space> Point2D<f64, Space> {
    #[inline]
    pub fn floor_i32(self) -> Point2D<i32, Space> {
        Point2D::new(self.x.floor() as i32, self.y.floor() as i32)
    }
}

impl<N, Space> From<(N, N)> for Point2D<N, Space> {
    #[inline]
    fn from((x, y): (N, N)) -> Self {
        Self::new(x, y)
    }
}

impl<N: CoordinateScalar, Space> Add for Point2D<N, Space> {
    type Output = Self;

    #[inline]
    fn add(self, other: Self) -> Self {
        Self::new(
            self.x.saturating_add(other.x),
            self.y.saturating_add(other.y),
        )
    }
}

impl<N: CoordinateScalar, Space> AddAssign for Point2D<N, Space> {
    #[inline]
    fn add_assign(&mut self, other: Self) {
        self.x = self.x.saturating_add(other.x);
        self.y = self.y.saturating_add(other.y);
    }
}

impl<N: CoordinateScalar, Space> Sub for Point2D<N, Space> {
    type Output = Self;

    #[inline]
    fn sub(self, other: Self) -> Self {
        Self::new(
            self.x.saturating_sub(other.x),
            self.y.saturating_sub(other.y),
        )
    }
}

impl<N: CoordinateScalar, Space> SubAssign for Point2D<N, Space> {
    #[inline]
    fn sub_assign(&mut self, other: Self) {
        self.x = self.x.saturating_sub(other.x);
        self.y = self.y.saturating_sub(other.y);
    }
}

impl<N: Clone, Space> Clone for Point2D<N, Space> {
    fn clone(&self) -> Self {
        Self::new(self.x.clone(), self.y.clone())
    }
}

impl<N: Copy, Space> Copy for Point2D<N, Space> {}

impl<N: fmt::Debug, Space> fmt::Debug for Point2D<N, Space> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Point2D")
            .field("x", &self.x)
            .field("y", &self.y)
            .finish()
    }
}

impl<N: Default, Space> Default for Point2D<N, Space> {
    fn default() -> Self {
        Self::new(N::default(), N::default())
    }
}

impl<N: PartialEq, Space> PartialEq for Point2D<N, Space> {
    fn eq(&self, other: &Self) -> bool {
        self.x == other.x && self.y == other.y
    }
}

impl<N: Eq, Space> Eq for Point2D<N, Space> {}

#[repr(C)]
pub struct Size2D<N, Space> {
    pub w: N,
    pub h: N,
    space: PhantomData<Space>,
}

impl<N, Space> Size2D<N, Space> {
    pub const fn new(w: N, h: N) -> Self {
        Self {
            w,
            h,
            space: PhantomData,
        }
    }
}

impl<N: CoordinateScalar, Space> Size2D<N, Space> {
    #[inline]
    pub fn to_f64(self) -> Size2D<f64, Space> {
        Size2D::new(self.w.as_f64(), self.h.as_f64())
    }
}

impl<Space> Size2D<f64, Space> {
    #[inline]
    pub fn round_i32(self) -> Size2D<i32, Space> {
        Size2D::new(self.w.round() as i32, self.h.round() as i32)
    }

    #[inline]
    pub fn ceil_i32(self) -> Size2D<i32, Space> {
        Size2D::new(self.w.ceil() as i32, self.h.ceil() as i32)
    }
}

impl Size2D<f64, PhysicalSpace> {
    #[inline]
    pub fn to_logical(self, scale: f64) -> Size2D<f64, LogicalSpace> {
        Size2D::new(self.w / scale, self.h / scale)
    }
}

impl<N, Space> From<(N, N)> for Size2D<N, Space> {
    #[inline]
    fn from((w, h): (N, N)) -> Self {
        Self::new(w, h)
    }
}

impl<N: Clone, Space> Clone for Size2D<N, Space> {
    fn clone(&self) -> Self {
        Self::new(self.w.clone(), self.h.clone())
    }
}

impl<N: Copy, Space> Copy for Size2D<N, Space> {}

impl<N: fmt::Debug, Space> fmt::Debug for Size2D<N, Space> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Size2D")
            .field("w", &self.w)
            .field("h", &self.h)
            .finish()
    }
}

impl<N: Default, Space> Default for Size2D<N, Space> {
    fn default() -> Self {
        Self::new(N::default(), N::default())
    }
}

impl<N: PartialEq, Space> PartialEq for Size2D<N, Space> {
    fn eq(&self, other: &Self) -> bool {
        self.w == other.w && self.h == other.h
    }
}

impl<N: Eq, Space> Eq for Size2D<N, Space> {}

impl<N: CoordinateScalar, Space> Add<Size2D<N, Space>> for Point2D<N, Space> {
    type Output = Self;

    #[inline]
    fn add(self, size: Size2D<N, Space>) -> Self {
        Self::new(self.x.saturating_add(size.w), self.y.saturating_add(size.h))
    }
}

#[repr(C)]
pub struct Rect2D<N, Space> {
    pub loc: Point2D<N, Space>,
    pub size: Size2D<N, Space>,
}

impl<N, Space> Rect2D<N, Space> {
    pub const fn new(loc: Point2D<N, Space>, size: Size2D<N, Space>) -> Self {
        Self { loc, size }
    }
}

impl<N: CoordinateScalar, Space> Rect2D<N, Space> {
    #[inline]
    pub fn from_size(size: Size2D<N, Space>) -> Self {
        Self::new(Point2D::new(N::ZERO, N::ZERO), size)
    }

    #[inline]
    pub fn zero() -> Self {
        Self::from_size(Size2D::new(N::ZERO, N::ZERO))
    }

    #[inline]
    pub fn to_f64(self) -> Rect2D<f64, Space> {
        Rect2D::new(self.loc.to_f64(), self.size.to_f64())
    }

    #[inline]
    pub fn contains(self, point: Point2D<N, Space>) -> bool {
        point.x >= self.loc.x
            && point.x < self.loc.x.saturating_add(self.size.w)
            && point.y >= self.loc.y
            && point.y < self.loc.y.saturating_add(self.size.h)
    }

    #[inline]
    pub fn overlaps(self, other: Self) -> bool {
        self.loc.x < other.loc.x.saturating_add(other.size.w)
            && other.loc.x < self.loc.x.saturating_add(self.size.w)
            && self.loc.y < other.loc.y.saturating_add(other.size.h)
            && other.loc.y < self.loc.y.saturating_add(self.size.h)
    }

    #[inline]
    pub fn intersection(self, other: Self) -> Option<Self> {
        if !self.overlaps(other) {
            return None;
        }
        let left = scalar_max(self.loc.x, other.loc.x);
        let top = scalar_max(self.loc.y, other.loc.y);
        let right = scalar_min(
            self.loc.x.saturating_add(self.size.w),
            other.loc.x.saturating_add(other.size.w),
        );
        let bottom = scalar_min(
            self.loc.y.saturating_add(self.size.h),
            other.loc.y.saturating_add(other.size.h),
        );
        Some(Self::new(
            Point2D::new(left, top),
            Size2D::new(right.saturating_sub(left), bottom.saturating_sub(top)),
        ))
    }

    #[inline]
    pub fn union(self, other: Self) -> Self {
        let left = scalar_min(self.loc.x, other.loc.x);
        let top = scalar_min(self.loc.y, other.loc.y);
        let right = scalar_max(
            self.loc.x.saturating_add(self.size.w),
            other.loc.x.saturating_add(other.size.w),
        );
        let bottom = scalar_max(
            self.loc.y.saturating_add(self.size.h),
            other.loc.y.saturating_add(other.size.h),
        );
        Self::new(
            Point2D::new(left, top),
            Size2D::new(right.saturating_sub(left), bottom.saturating_sub(top)),
        )
    }
}

impl<N: Clone, Space> Clone for Rect2D<N, Space> {
    fn clone(&self) -> Self {
        Self::new(self.loc.clone(), self.size.clone())
    }
}

impl<N: Copy, Space> Copy for Rect2D<N, Space> {}

impl<N: fmt::Debug, Space> fmt::Debug for Rect2D<N, Space> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Rect2D")
            .field("loc", &self.loc)
            .field("size", &self.size)
            .finish()
    }
}

impl<N: Default, Space> Default for Rect2D<N, Space> {
    fn default() -> Self {
        Self::new(Point2D::default(), Size2D::default())
    }
}

impl<N: PartialEq, Space> PartialEq for Rect2D<N, Space> {
    fn eq(&self, other: &Self) -> bool {
        self.loc == other.loc && self.size == other.size
    }
}

impl<N: Eq, Space> Eq for Rect2D<N, Space> {}

#[inline]
fn scalar_min<N: PartialOrd>(left: N, right: N) -> N {
    if left < right { left } else { right }
}

#[inline]
fn scalar_max<N: PartialOrd>(left: N, right: N) -> N {
    if left > right { left } else { right }
}

pub type LogicalPoint<N> = Point2D<N, LogicalSpace>;
pub type LogicalSize<N> = Size2D<N, LogicalSpace>;
pub type LogicalRect<N> = Rect2D<N, LogicalSpace>;
pub type PhysicalSize<N> = Size2D<N, PhysicalSpace>;
pub type BufferSize<N> = Size2D<N, BufferSpace>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_rect_operations_are_half_open_and_saturating() {
        let first = LogicalRect::new((10, 20).into(), (30, 40).into());
        let second = LogicalRect::new((20, 10).into(), (40, 30).into());

        assert!(first.contains((10, 20).into()));
        assert!(!first.contains((40, 60).into()));
        assert_eq!(
            first.intersection(second),
            Some(LogicalRect::new((20, 20).into(), (20, 20).into()))
        );
        assert_eq!(
            first.union(second),
            LogicalRect::new((10, 10).into(), (50, 50).into())
        );
    }

    #[test]
    fn physical_size_rounds_only_at_the_requested_boundary() {
        let logical = PhysicalSize::new(1920, 1080)
            .to_f64()
            .to_logical(1.25)
            .ceil_i32();
        assert_eq!(logical, LogicalSize::new(1536, 864));
    }
}
