use std::sync::Arc;

use lyon::math::point;
use lyon::tessellation::{
    FillRule, FillVertex, FillVertexConstructor, LineCap, LineJoin, StrokeVertex,
    StrokeVertexConstructor,
};

use super::sample_svg_gradient;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct SvgVertex {
    pub(crate) position: [f32; 2],
    pub(crate) color: [f32; 4],
}

pub(crate) struct SvgGeometry {
    pub(crate) vertices: Vec<SvgVertex>,
    pub(crate) indices: Vec<u32>,
}

#[derive(Clone, Copy)]
pub(super) struct SvgAffine {
    pub(super) a: f32,
    pub(super) b: f32,
    pub(super) c: f32,
    pub(super) d: f32,
    pub(super) e: f32,
    pub(super) f: f32,
}

impl SvgAffine {
    pub(super) const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub(super) fn then(self, rhs: Self) -> Self {
        Self {
            a: rhs.a * self.a + rhs.c * self.b,
            b: rhs.b * self.a + rhs.d * self.b,
            c: rhs.a * self.c + rhs.c * self.d,
            d: rhs.b * self.c + rhs.d * self.d,
            e: rhs.a * self.e + rhs.c * self.f + rhs.e,
            f: rhs.b * self.e + rhs.d * self.f + rhs.f,
        }
    }

    fn point(self, p: lyon::math::Point) -> lyon::math::Point {
        point(
            self.a * p.x + self.c * p.y + self.e,
            self.b * p.x + self.d * p.y + self.f,
        )
    }

    fn inverse(self) -> Option<Self> {
        let determinant = self.a * self.d - self.b * self.c;
        if determinant.abs() <= f32::EPSILON {
            return None;
        }
        let inverse = 1.0 / determinant;
        Some(Self {
            a: self.d * inverse,
            b: -self.b * inverse,
            c: -self.c * inverse,
            d: self.a * inverse,
            e: (self.c * self.f - self.d * self.e) * inverse,
            f: (self.b * self.e - self.a * self.f) * inverse,
        })
    }
}

#[derive(Clone)]
pub(super) struct SvgGradientStop {
    pub(super) offset: f32,
    pub(super) color: [f32; 4],
}

#[derive(Clone)]
pub(super) enum SvgPaint {
    Solid([f32; 4]),
    Linear {
        start: [f32; 2],
        end: [f32; 2],
        object_bounding_box: bool,
        transform: SvgAffine,
        stops: Arc<[SvgGradientStop]>,
    },
    Radial {
        center: [f32; 2],
        radius: f32,
        object_bounding_box: bool,
        transform: SvgAffine,
        stops: Arc<[SvgGradientStop]>,
    },
}

impl SvgPaint {
    fn color_at(&self, position: lyon::math::Point, bounds: lyon::math::Box2D) -> [f32; 4] {
        match self {
            Self::Solid(color) => *color,
            Self::Linear {
                start,
                end,
                object_bounding_box,
                transform,
                stops,
            } => {
                let p = transform
                    .inverse()
                    .unwrap_or(SvgAffine::IDENTITY)
                    .point(position);
                let (start, end) = if *object_bounding_box {
                    let size = bounds.size();
                    (
                        point(
                            bounds.min.x + start[0] * size.width,
                            bounds.min.y + start[1] * size.height,
                        ),
                        point(
                            bounds.min.x + end[0] * size.width,
                            bounds.min.y + end[1] * size.height,
                        ),
                    )
                } else {
                    (point(start[0], start[1]), point(end[0], end[1]))
                };
                let axis = end - start;
                let denominator = axis.square_length();
                let offset = if denominator > f32::EPSILON {
                    ((p - start).dot(axis) / denominator).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                sample_svg_gradient(stops, offset)
            }
            Self::Radial {
                center,
                radius,
                object_bounding_box,
                transform,
                stops,
            } => {
                let p = transform
                    .inverse()
                    .unwrap_or(SvgAffine::IDENTITY)
                    .point(position);
                let (center, radius) = if *object_bounding_box {
                    let size = bounds.size();
                    (
                        point(
                            bounds.min.x + center[0] * size.width,
                            bounds.min.y + center[1] * size.height,
                        ),
                        radius * size.width.max(size.height),
                    )
                } else {
                    (point(center[0], center[1]), *radius)
                };
                let offset = if radius > f32::EPSILON {
                    (p - center).length() / radius
                } else {
                    0.0
                };
                sample_svg_gradient(stops, offset.clamp(0.0, 1.0))
            }
        }
    }
}

#[derive(Clone)]
pub(super) struct SvgPaintState {
    pub(super) transform: SvgAffine,
    pub(super) color: [f32; 4],
    pub(super) fill: Option<SvgPaint>,
    pub(super) stroke: Option<SvgPaint>,
    pub(super) stroke_width: f32,
    pub(super) stroke_cap: LineCap,
    pub(super) stroke_join: LineJoin,
    pub(super) fill_rule: FillRule,
    pub(super) opacity: f32,
}

impl Default for SvgPaintState {
    fn default() -> Self {
        Self {
            transform: SvgAffine::IDENTITY,
            color: [0.0, 0.0, 0.0, 1.0],
            fill: Some(SvgPaint::Solid([0.0, 0.0, 0.0, 1.0])),
            stroke: None,
            stroke_width: 1.0,
            stroke_cap: LineCap::Butt,
            stroke_join: LineJoin::Miter,
            fill_rule: FillRule::NonZero,
            opacity: 1.0,
        }
    }
}

pub(super) struct SvgVertexCtor {
    pub(super) transform: SvgAffine,
    pub(super) paint: SvgPaint,
    pub(super) bounds: lyon::math::Box2D,
    pub(super) opacity: f32,
}

impl SvgVertexCtor {
    fn make(&self, p: lyon::math::Point) -> SvgVertex {
        let mut color = self.paint.color_at(p, self.bounds);
        color[3] *= self.opacity;
        color[0] *= color[3];
        color[1] *= color[3];
        color[2] *= color[3];
        let transformed = self.transform.point(p);
        SvgVertex {
            position: [transformed.x, transformed.y],
            color,
        }
    }
}

impl FillVertexConstructor<SvgVertex> for SvgVertexCtor {
    fn new_vertex(&mut self, vertex: FillVertex<'_>) -> SvgVertex {
        self.make(vertex.position())
    }
}

impl StrokeVertexConstructor<SvgVertex> for SvgVertexCtor {
    fn new_vertex(&mut self, vertex: StrokeVertex<'_, '_>) -> SvgVertex {
        self.make(vertex.position())
    }
}

#[derive(Clone, Copy)]
pub(crate) struct SvgIntrinsicSize {
    pub(crate) width: f32,
    pub(crate) height: f32,
}
