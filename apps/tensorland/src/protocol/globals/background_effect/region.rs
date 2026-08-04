use std::sync::Arc;

use tensor_util::Rect;

use crate::scene::BackdropRegion;

use super::super::compositor::{RectangleKind, RegionAttributes};

const MAX_RECTS: usize = 128;

/// Immutable normalized protocol region in surface-local logical coordinates.
///
/// Wide endpoints preserve wl_region arithmetic until the region is clipped
/// to a real surface. The bounded, non-overlapping result makes later commits
/// and scene extraction independent of client-controlled operation count.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::protocol) struct BackgroundRegion(Arc<[WideRect]>);

impl BackgroundRegion {
    pub(in crate::protocol) fn from_attributes(attributes: RegionAttributes) -> Self {
        let mut rects = Vec::new();
        for (kind, rect) in attributes.rects {
            let added = WideRect::from_rect(rect);
            let result = match kind {
                RectangleKind::Add => add_rect(&mut rects, added),
                RectangleKind::Subtract => subtract_rect(&mut rects, added),
            };
            if result.is_err() {
                // An empty effect is a safe bounded response to pathological
                // client region complexity; it never expands blur outside the
                // requested pixels.
                return Self(Arc::from([]));
            }
        }
        Self(rects.into())
    }

    pub(in crate::protocol) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(in crate::protocol) fn to_scene_region(
        &self,
        surface_bounds: Rect,
        local_offset: (i32, i32),
    ) -> Option<BackdropRegion> {
        let bounds = WideRect::from_rect(surface_bounds);
        let rects = self
            .0
            .iter()
            .filter_map(|rect| rect.intersection(bounds))
            .filter_map(|rect| rect.to_rect())
            .map(|rect| rect.translated(local_offset.0, local_offset.1))
            .collect();
        BackdropRegion::new(rects)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WideRect {
    x0: i64,
    y0: i64,
    x1: i64,
    y1: i64,
}

impl WideRect {
    fn from_rect(rect: Rect) -> Self {
        let x0 = i64::from(rect.x);
        let y0 = i64::from(rect.y);
        Self {
            x0,
            y0,
            x1: x0 + i64::from(rect.width),
            y1: y0 + i64::from(rect.height),
        }
    }

    fn intersection(self, other: Self) -> Option<Self> {
        let intersection = Self {
            x0: self.x0.max(other.x0),
            y0: self.y0.max(other.y0),
            x1: self.x1.min(other.x1),
            y1: self.y1.min(other.y1),
        };
        (intersection.x0 < intersection.x1 && intersection.y0 < intersection.y1)
            .then_some(intersection)
    }

    fn to_rect(self) -> Option<Rect> {
        Some(Rect::new(
            i32::try_from(self.x0).ok()?,
            i32::try_from(self.y0).ok()?,
            u32::try_from(self.x1 - self.x0).ok()?,
            u32::try_from(self.y1 - self.y0).ok()?,
        ))
    }
}

fn add_rect(rects: &mut Vec<WideRect>, added: WideRect) -> Result<(), ()> {
    let mut fragments = vec![added];
    let mut scratch = Vec::new();
    for existing in rects.iter().copied() {
        scratch.clear();
        for fragment in fragments.drain(..) {
            subtract_into(fragment, existing, &mut scratch)?;
        }
        std::mem::swap(&mut fragments, &mut scratch);
        if fragments.is_empty() {
            return Ok(());
        }
    }
    if rects.len().saturating_add(fragments.len()) > MAX_RECTS {
        return Err(());
    }
    rects.extend(fragments);
    Ok(())
}

fn subtract_rect(rects: &mut Vec<WideRect>, removed: WideRect) -> Result<(), ()> {
    let mut result = Vec::with_capacity(rects.len().min(MAX_RECTS));
    for rect in rects.drain(..) {
        subtract_into(rect, removed, &mut result)?;
    }
    *rects = result;
    Ok(())
}

fn subtract_into(rect: WideRect, removed: WideRect, output: &mut Vec<WideRect>) -> Result<(), ()> {
    let Some(overlap) = rect.intersection(removed) else {
        return push_rect(output, rect);
    };
    if rect.y0 < overlap.y0 {
        push_rect(
            output,
            WideRect {
                y1: overlap.y0,
                ..rect
            },
        )?;
    }
    if overlap.y1 < rect.y1 {
        push_rect(
            output,
            WideRect {
                y0: overlap.y1,
                ..rect
            },
        )?;
    }
    if rect.x0 < overlap.x0 {
        push_rect(
            output,
            WideRect {
                x1: overlap.x0,
                y0: overlap.y0,
                y1: overlap.y1,
                ..rect
            },
        )?;
    }
    if overlap.x1 < rect.x1 {
        push_rect(
            output,
            WideRect {
                x0: overlap.x1,
                y0: overlap.y0,
                y1: overlap.y1,
                ..rect
            },
        )?;
    }
    Ok(())
}

fn push_rect(output: &mut Vec<WideRect>, rect: WideRect) -> Result<(), ()> {
    if output.len() == MAX_RECTS {
        return Err(());
    }
    output.push(rect);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attributes(rects: &[(RectangleKind, Rect)]) -> RegionAttributes {
        RegionAttributes {
            rects: rects.to_vec(),
        }
    }

    #[test]
    fn add_subtract_and_restore_are_exact_after_surface_clipping() {
        let region = BackgroundRegion::from_attributes(attributes(&[
            (RectangleKind::Add, Rect::new(-10, -10, 120, 120)),
            (RectangleKind::Subtract, Rect::new(20, 20, 60, 60)),
            (RectangleKind::Add, Rect::new(40, 40, 20, 20)),
        ]));
        let scene = region
            .to_scene_region(Rect::new(0, 0, 100, 100), (5, 7))
            .unwrap();

        let area = scene
            .rects()
            .iter()
            .map(|rect| u64::from(rect.width) * u64::from(rect.height))
            .sum::<u64>();
        assert_eq!(area, 6_800);
        assert!(scene.rects().contains(&Rect::new(45, 47, 20, 20)));
    }

    #[test]
    fn pathological_region_complexity_fails_closed() {
        let attrs = RegionAttributes {
            rects: (0..=MAX_RECTS)
                .map(|index| (RectangleKind::Add, Rect::new(index as i32 * 2, 0, 1, 1)))
                .collect(),
        };
        let region = BackgroundRegion::from_attributes(attrs);

        assert!(region.is_empty());
        assert!(
            region
                .to_scene_region(Rect::new(0, 0, 512, 512), (0, 0))
                .is_none()
        );
    }
}
