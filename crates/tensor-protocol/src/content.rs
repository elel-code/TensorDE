use tensor_util::{Rect, Size};

use crate::{SurfaceBufferId, SurfaceColorState, SurfaceId};

/// Monotonic content generation for one surface.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct ContentRevision(u64);

impl ContentRevision {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

/// Value-only equivalent of the Wayland buffer transform.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SurfaceTransform {
    #[default]
    Normal,
    Rotate90,
    Rotate180,
    Rotate270,
    Flipped,
    Flipped90,
    Flipped180,
    Flipped270,
}

impl SurfaceTransform {
    pub const fn swaps_axes(self) -> bool {
        matches!(
            self,
            Self::Rotate90 | Self::Rotate270 | Self::Flipped90 | Self::Flipped270
        )
    }
}

/// Placement policy for one surface inside a view scene.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SurfaceLayer {
    #[default]
    View,
    Popup,
}

/// Affine mapping from surface-local unit coordinates to normalized buffer
/// coordinates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceUvTransform {
    pub origin: (i8, i8),
    pub axis_x: (i8, i8),
    pub axis_y: (i8, i8),
}

/// Exact Wayland 24.8 fixed-point crop rectangle in post-transform,
/// post-buffer-scale coordinates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceSourceRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl SurfaceSourceRect {
    pub const FIXED_SCALE: i64 = 256;

    pub const fn from_raw_fixed(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub const fn raw_fixed(self) -> [i32; 4] {
        [self.x, self.y, self.width, self.height]
    }

    pub fn as_f64(self) -> [f64; 4] {
        let scale = Self::FIXED_SCALE as f64;
        [
            f64::from(self.x) / scale,
            f64::from(self.y) / scale,
            f64::from(self.width) / scale,
            f64::from(self.height) / scale,
        ]
    }

    pub const fn has_integer_size(self) -> bool {
        self.width % Self::FIXED_SCALE as i32 == 0 && self.height % Self::FIXED_SCALE as i32 == 0
    }

    pub const fn integer_size(self) -> (i32, i32) {
        (
            self.width / Self::FIXED_SCALE as i32,
            self.height / Self::FIXED_SCALE as i32,
        )
    }

    pub fn fits_within(self, width: i32, height: i32) -> bool {
        let x = i64::from(self.x);
        let y = i64::from(self.y);
        let source_width = i64::from(self.width);
        let source_height = i64::from(self.height);
        x >= 0
            && y >= 0
            && source_width > 0
            && source_height > 0
            && x + source_width <= i64::from(width) * Self::FIXED_SCALE
            && y + source_height <= i64::from(height) * Self::FIXED_SCALE
    }
}

/// Bit-stable affine mapping from surface-local unit coordinates to normalized
/// source-buffer coordinates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceSampleTransform {
    components: [u32; 6],
}

impl SurfaceSampleTransform {
    pub const IDENTITY: Self = Self::new((0.0, 0.0), (1.0, 0.0), (0.0, 1.0));

    pub const fn new(origin: (f32, f32), axis_x: (f32, f32), axis_y: (f32, f32)) -> Self {
        Self {
            components: [
                origin.0.to_bits(),
                origin.1.to_bits(),
                axis_x.0.to_bits(),
                axis_x.1.to_bits(),
                axis_y.0.to_bits(),
                axis_y.1.to_bits(),
            ],
        }
    }

    pub fn origin(self) -> (f32, f32) {
        (
            f32::from_bits(self.components[0]),
            f32::from_bits(self.components[1]),
        )
    }

    pub fn axis_x(self) -> (f32, f32) {
        (
            f32::from_bits(self.components[2]),
            f32::from_bits(self.components[3]),
        )
    }

    pub fn axis_y(self) -> (f32, f32) {
        (
            f32::from_bits(self.components[4]),
            f32::from_bits(self.components[5]),
        )
    }

    pub fn for_surface(
        buffer_size: Size,
        buffer_scale: u32,
        transform: SurfaceTransform,
        source: Option<SurfaceSourceRect>,
    ) -> Self {
        let base = transform.uv_transform();
        let Some(source) = source else {
            return Self::from_discrete(base);
        };
        let scale = buffer_scale.max(1);
        let (transformed_width, transformed_height) = match transform {
            SurfaceTransform::Rotate90
            | SurfaceTransform::Rotate270
            | SurfaceTransform::Flipped90
            | SurfaceTransform::Flipped270 => (buffer_size.height, buffer_size.width),
            _ => (buffer_size.width, buffer_size.height),
        };
        let logical_width = transformed_width / scale;
        let logical_height = transformed_height / scale;
        if logical_width == 0 || logical_height == 0 {
            return Self::from_discrete(base);
        }

        let [x, y, width, height] = source.as_f64();
        let x = (x / f64::from(logical_width)) as f32;
        let y = (y / f64::from(logical_height)) as f32;
        let width = (width / f64::from(logical_width)) as f32;
        let height = (height / f64::from(logical_height)) as f32;
        let base_origin = (f32::from(base.origin.0), f32::from(base.origin.1));
        let base_x = (f32::from(base.axis_x.0), f32::from(base.axis_x.1));
        let base_y = (f32::from(base.axis_y.0), f32::from(base.axis_y.1));
        Self::new(
            (
                base_origin.0 + base_x.0 * x + base_y.0 * y,
                base_origin.1 + base_x.1 * x + base_y.1 * y,
            ),
            (base_x.0 * width, base_x.1 * width),
            (base_y.0 * height, base_y.1 * height),
        )
    }

    fn from_discrete(transform: SurfaceUvTransform) -> Self {
        Self::new(
            (f32::from(transform.origin.0), f32::from(transform.origin.1)),
            (f32::from(transform.axis_x.0), f32::from(transform.axis_x.1)),
            (f32::from(transform.axis_y.0), f32::from(transform.axis_y.1)),
        )
    }
}

impl SurfaceTransform {
    pub const fn uv_transform(self) -> SurfaceUvTransform {
        match self {
            Self::Normal => SurfaceUvTransform {
                origin: (0, 0),
                axis_x: (1, 0),
                axis_y: (0, 1),
            },
            Self::Rotate90 => SurfaceUvTransform {
                origin: (1, 0),
                axis_x: (0, 1),
                axis_y: (-1, 0),
            },
            Self::Rotate180 => SurfaceUvTransform {
                origin: (1, 1),
                axis_x: (-1, 0),
                axis_y: (0, -1),
            },
            Self::Rotate270 => SurfaceUvTransform {
                origin: (0, 1),
                axis_x: (0, -1),
                axis_y: (1, 0),
            },
            Self::Flipped => SurfaceUvTransform {
                origin: (1, 0),
                axis_x: (-1, 0),
                axis_y: (0, 1),
            },
            Self::Flipped90 => SurfaceUvTransform {
                origin: (0, 0),
                axis_x: (0, 1),
                axis_y: (1, 0),
            },
            Self::Flipped180 => SurfaceUvTransform {
                origin: (0, 1),
                axis_x: (1, 0),
                axis_y: (0, -1),
            },
            Self::Flipped270 => SurfaceUvTransform {
                origin: (1, 1),
                axis_x: (0, -1),
                axis_y: (-1, 0),
            },
        }
    }
}

/// Renderable state extracted from a live protocol surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceContent {
    pub surface_id: SurfaceId,
    pub buffer_id: SurfaceBufferId,
    pub revision: ContentRevision,
    pub layer: SurfaceLayer,
    pub alpha: SurfaceAlpha,
    /// Committed colorimetry and pixel representation for this exact surface.
    pub color: SurfaceColorState,
    pub local_geometry: Rect,
    pub sample_transform: SurfaceSampleTransform,
}

/// Exact client-provided alpha multiplier for one protocol surface.
///
/// The wire protocol uses the complete `u32` range, so the scene boundary
/// preserves all 32 bits and converts to floating point only while preparing
/// the Vulkan push constants.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceAlpha(u32);

impl SurfaceAlpha {
    pub const TRANSPARENT: Self = Self(0);
    pub const OPAQUE: Self = Self(u32::MAX);

    pub const fn from_raw(value: u32) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }

    pub fn as_f32(self) -> f32 {
        self.0 as f32 / u32::MAX as f32
    }
}

impl Default for SurfaceAlpha {
    fn default() -> Self {
        Self::OPAQUE
    }
}

/// Protocol-neutral content hint supplied by `wp_content_type_v1`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SurfaceContentType {
    #[default]
    None,
    Photo,
    Video,
    Game,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uv_transforms_cover_every_wayland_orientation() {
        let transforms = [
            (SurfaceTransform::Normal, (0, 0), (1, 0), (0, 1)),
            (SurfaceTransform::Rotate90, (1, 0), (0, 1), (-1, 0)),
            (SurfaceTransform::Rotate180, (1, 1), (-1, 0), (0, -1)),
            (SurfaceTransform::Rotate270, (0, 1), (0, -1), (1, 0)),
            (SurfaceTransform::Flipped, (1, 0), (-1, 0), (0, 1)),
            (SurfaceTransform::Flipped90, (0, 0), (0, 1), (1, 0)),
            (SurfaceTransform::Flipped180, (0, 1), (1, 0), (0, -1)),
            (SurfaceTransform::Flipped270, (1, 1), (0, -1), (-1, 0)),
        ];
        for (transform, origin, axis_x, axis_y) in transforms {
            let uv = transform.uv_transform();
            assert_eq!((uv.origin, uv.axis_x, uv.axis_y), (origin, axis_x, axis_y));
        }
    }

    #[test]
    fn revisions_wrap_without_aliasing_adapter_commit_tokens() {
        assert_eq!(ContentRevision::new(u64::MAX).next().get(), 0);
    }

    #[test]
    fn fixed_source_bounds_use_checked_wide_arithmetic() {
        let source = SurfaceSourceRect::from_raw_fixed(256, 512, 768, 1024);
        assert_eq!(source.as_f64(), [1.0, 2.0, 3.0, 4.0]);
        assert!(source.has_integer_size());
        assert_eq!(source.integer_size(), (3, 4));
        assert!(source.fits_within(4, 6));
        assert!(!source.fits_within(3, 6));
    }

    #[test]
    fn cropped_sampling_composes_after_scale_and_rotation() {
        let source = SurfaceSourceRect::from_raw_fixed(5 * 256, 10 * 256, 10 * 256, 20 * 256);
        let sample = SurfaceSampleTransform::for_surface(
            Size::new(200, 100),
            2,
            SurfaceTransform::Rotate90,
            Some(source),
        );
        assert_close(sample.origin(), (0.9, 0.1));
        assert_close(sample.axis_x(), (0.0, 0.2));
        assert_close(sample.axis_y(), (-0.2, 0.0));
    }

    fn assert_close(actual: (f32, f32), expected: (f32, f32)) {
        assert!((actual.0 - expected.0).abs() < 0.000_01);
        assert!((actual.1 - expected.1).abs() < 0.000_01);
    }
}
