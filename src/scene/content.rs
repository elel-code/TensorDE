use tensor_util::{Rect, Size};

use crate::ecs::{SurfaceBufferId, SurfaceId};

/// Monotonic content generation for one surface.
///
/// A generation changes when Smithay observes new buffer damage, a buffer
/// replacement, or a mapping-state change.  It lets scene damage distinguish
/// two commits that reuse the same `wl_buffer` without leaking Smithay's commit
/// counter into ECS.
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
///
/// Toplevel and subsurface content follows the layout clip assigned to the
/// view. Popups are still owned by that view, but may extend beyond its tile
/// and are clipped only by the output viewport.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SurfaceLayer {
    #[default]
    View,
    Popup,
}

/// Affine mapping from surface-local unit coordinates to normalized buffer
/// coordinates.  Keeping this as integer coefficients makes the scene
/// contract value-only and lossless; the Vulkan boundary converts it to
/// floating-point push data immediately before recording a draw.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceUvTransform {
    pub origin: (i8, i8),
    pub axis_x: (i8, i8),
    pub axis_y: (i8, i8),
}

impl SurfaceTransform {
    /// Return the full-buffer UV mapping prescribed by the Wayland transform.
    ///
    /// The coefficients intentionally operate on normalized coordinates.  A
    /// future viewport/source-rectangle contract can compose another affine
    /// transform here without changing the renderer's draw-record shape.
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

/// Renderable state extracted from a live Wayland surface.
///
/// Vulkan and Wayland handles stay in their owners.  A scene snapshot carries
/// only stable identities and the dimensions needed to build a draw plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceContent {
    pub surface_id: SurfaceId,
    pub buffer_id: SurfaceBufferId,
    pub revision: ContentRevision,
    pub layer: SurfaceLayer,
    pub buffer_size: Size,
    /// Surface-local destination after buffer scale, transform, and viewport
    /// destination have been resolved by Smithay.
    pub local_geometry: Rect,
    pub buffer_scale: u32,
    pub transform: SurfaceTransform,
}

/// Index range into `SceneSnapshot`'s flat surface-content table.
///
/// Keeping this range on a scene node avoids embedding protocol-owned trees in
/// the node itself.  Subsurfaces and popups can be appended to the same table
/// without changing the renderer's stable view ordering.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ContentSpan {
    start: u32,
    len: u32,
}

impl ContentSpan {
    pub(crate) fn new(start: usize, len: usize) -> Option<Self> {
        Some(Self {
            start: u32::try_from(start).ok()?,
            len: u32::try_from(len).ok()?,
        })
    }

    pub const fn is_empty(self) -> bool {
        self.len == 0
    }

    pub const fn len(self) -> usize {
        self.len as usize
    }

    pub(crate) fn range(self) -> Option<std::ops::Range<usize>> {
        let start = self.start as usize;
        start.checked_add(self.len as usize).map(|end| start..end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uv_transforms_match_smithay_normalized_point_semantics() {
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
            assert_eq!(uv.origin, origin);
            assert_eq!(uv.axis_x, axis_x);
            assert_eq!(uv.axis_y, axis_y);
        }
    }

    #[test]
    fn revisions_wrap_without_aliasing_protocol_counters() {
        assert_eq!(ContentRevision::new(u64::MAX).next().get(), 0);
    }

    #[test]
    fn content_spans_reject_unrepresentable_tables() {
        assert_eq!(ContentSpan::new(3, 2).unwrap().range(), Some(3..5));
        assert!(ContentSpan::new(usize::MAX, 1).is_none());
    }
}
