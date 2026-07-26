use tensor_util::{Rect, Size};

use crate::{SurfaceBufferId, SurfaceId};

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
    pub buffer_size: Size,
    pub local_geometry: Rect,
    pub buffer_scale: u32,
    pub transform: SurfaceTransform,
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
}
