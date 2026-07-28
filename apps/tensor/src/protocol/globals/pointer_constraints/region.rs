const MAX_RECTS: usize = 128;
const POSITION_EPSILON: f64 = 1.0 / 256.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RegionOpKind {
    Add,
    Subtract,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RegionOp {
    pub(super) kind: RegionOpKind,
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) width: i32,
    pub(super) height: i32,
}

#[derive(Debug)]
pub(super) enum ConstraintRegion {
    Unbounded,
    Rects(Box<[RegionRect]>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RegionRect {
    x0: i64,
    y0: i64,
    x1: i64,
    y1: i64,
}

impl ConstraintRegion {
    pub(super) const fn unbounded() -> Self {
        Self::Unbounded
    }

    pub(super) fn from_ops(ops: impl IntoIterator<Item = RegionOp>) -> Self {
        let mut rects = Vec::new();
        for op in ops {
            let Some(rect) = RegionRect::from_op(op) else {
                continue;
            };
            let result = match op.kind {
                RegionOpKind::Add => add_rect(&mut rects, rect),
                RegionOpKind::Subtract => subtract_rect(&mut rects, rect),
            };
            if result.is_err() {
                // A compositor is allowed to leave a constraint inactive. A
                // bounded representation prevents client-controlled region
                // complexity from entering the input hot path.
                return Self::Rects(Box::new([]));
            }
        }
        Self::Rects(rects.into_boxed_slice())
    }

    pub(super) fn contains(&self, point: (f64, f64)) -> bool {
        match self {
            Self::Unbounded => point.0.is_finite() && point.1.is_finite(),
            Self::Rects(rects) => rects.iter().any(|rect| rect.contains(point)),
        }
    }

    pub(super) fn confine(&self, current: (f64, f64), proposed: (f64, f64)) -> Option<(f64, f64)> {
        if !current.0.is_finite()
            || !current.1.is_finite()
            || !proposed.0.is_finite()
            || !proposed.1.is_finite()
        {
            return None;
        }
        let Self::Rects(rects) = self else {
            return Some(proposed);
        };
        if !rects.iter().any(|rect| rect.contains(current)) {
            return None;
        }

        let delta = (proposed.0 - current.0, proposed.1 - current.1);
        if delta.0 == 0.0 && delta.1 == 0.0 {
            return Some(current);
        }

        // Find the connected union of segment intervals containing t=0.
        // Rectangles were normalized off the hot path; the fixed stack array
        // and unstable sort keep this bounded and allocation-free.
        let mut intervals = [(0.0_f64, 0.0_f64); MAX_RECTS];
        let mut interval_count = 0;
        for rect in rects {
            if let Some(interval) = rect.segment_interval(current, delta) {
                intervals[interval_count] = interval;
                interval_count += 1;
            }
        }
        let intervals = &mut intervals[..interval_count];
        intervals.sort_unstable_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.1.total_cmp(&right.1))
        });
        let mut covered = 0.0_f64;
        for &(enter, exit) in intervals.iter() {
            if enter > covered + f64::EPSILON {
                break;
            }
            covered = covered.max(exit).min(1.0);
            if covered >= 1.0 {
                if self.contains(proposed) {
                    return Some(proposed);
                }
                break;
            }
        }

        let distance = delta.0.abs().max(delta.1.abs());
        let inset = if distance > 0.0 {
            (POSITION_EPSILON / distance).min(covered)
        } else {
            0.0
        };
        let t = (covered - inset).max(0.0);
        Some((current.0 + delta.0 * t, current.1 + delta.1 * t))
    }
}

impl RegionRect {
    fn from_op(op: RegionOp) -> Option<Self> {
        if op.width <= 0 || op.height <= 0 {
            return None;
        }
        let x0 = i64::from(op.x);
        let y0 = i64::from(op.y);
        Some(Self {
            x0,
            y0,
            x1: x0 + i64::from(op.width),
            y1: y0 + i64::from(op.height),
        })
    }

    fn contains(self, point: (f64, f64)) -> bool {
        point.0 >= self.x0 as f64
            && point.0 < self.x1 as f64
            && point.1 >= self.y0 as f64
            && point.1 < self.y1 as f64
    }

    fn segment_interval(self, point: (f64, f64), delta: (f64, f64)) -> Option<(f64, f64)> {
        let mut enter = 0.0_f64;
        let mut exit = 1.0_f64;
        clip_axis(
            point.0,
            delta.0,
            self.x0 as f64,
            self.x1 as f64,
            &mut enter,
            &mut exit,
        )?;
        clip_axis(
            point.1,
            delta.1,
            self.y0 as f64,
            self.y1 as f64,
            &mut enter,
            &mut exit,
        )?;
        (enter <= exit).then_some((enter, exit))
    }
}

fn clip_axis(
    point: f64,
    delta: f64,
    min: f64,
    max: f64,
    enter: &mut f64,
    exit: &mut f64,
) -> Option<()> {
    if delta == 0.0 {
        return (point >= min && point <= max).then_some(());
    }
    let first = (min - point) / delta;
    let second = (max - point) / delta;
    let (axis_enter, axis_exit) = if first <= second {
        (first, second)
    } else {
        (second, first)
    };
    *enter = enter.max(axis_enter);
    *exit = exit.min(axis_exit);
    (*enter <= *exit).then_some(())
}

fn add_rect(rects: &mut Vec<RegionRect>, added: RegionRect) -> Result<(), ()> {
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

fn subtract_rect(rects: &mut Vec<RegionRect>, removed: RegionRect) -> Result<(), ()> {
    let mut result = Vec::with_capacity(rects.len().min(MAX_RECTS));
    for rect in rects.drain(..) {
        subtract_into(rect, removed, &mut result)?;
    }
    *rects = result;
    Ok(())
}

fn subtract_into(
    rect: RegionRect,
    removed: RegionRect,
    output: &mut Vec<RegionRect>,
) -> Result<(), ()> {
    let x0 = rect.x0.max(removed.x0);
    let y0 = rect.y0.max(removed.y0);
    let x1 = rect.x1.min(removed.x1);
    let y1 = rect.y1.min(removed.y1);
    if x0 >= x1 || y0 >= y1 {
        return push_rect(output, rect);
    }

    if rect.y0 < y0 {
        push_rect(output, RegionRect { y1: y0, ..rect })?;
    }
    if y1 < rect.y1 {
        push_rect(output, RegionRect { y0: y1, ..rect })?;
    }
    if rect.x0 < x0 {
        push_rect(
            output,
            RegionRect {
                x1: x0,
                y0,
                y1,
                ..rect
            },
        )?;
    }
    if x1 < rect.x1 {
        push_rect(
            output,
            RegionRect {
                x0: x1,
                y0,
                y1,
                ..rect
            },
        )?;
    }
    Ok(())
}

fn push_rect(output: &mut Vec<RegionRect>, rect: RegionRect) -> Result<(), ()> {
    if output.len() == MAX_RECTS {
        return Err(());
    }
    output.push(rect);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add(x: i32, y: i32, width: i32, height: i32) -> RegionOp {
        RegionOp {
            kind: RegionOpKind::Add,
            x,
            y,
            width,
            height,
        }
    }

    fn subtract(x: i32, y: i32, width: i32, height: i32) -> RegionOp {
        RegionOp {
            kind: RegionOpKind::Subtract,
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn ordered_region_ops_normalize_without_overlap() {
        let region = ConstraintRegion::from_ops([
            add(0, 0, 100, 100),
            subtract(20, 20, 60, 60),
            add(40, 40, 20, 20),
        ]);
        assert!(region.contains((10.0, 10.0)));
        assert!(!region.contains((30.0, 30.0)));
        assert!(region.contains((50.0, 50.0)));
        assert!(!region.contains((100.0, 50.0)));
    }

    #[test]
    fn confinement_stops_before_holes_and_outer_edges() {
        let region = ConstraintRegion::from_ops([add(0, 0, 100, 100), subtract(40, 0, 20, 100)]);
        let at_hole = region.confine((10.0, 50.0), (90.0, 50.0)).unwrap();
        assert!(at_hole.0 < 40.0);
        assert!(at_hole.0 > 39.0);
        let at_edge = region.confine((10.0, 50.0), (-20.0, 50.0)).unwrap();
        assert!(at_edge.0 >= 0.0);
        assert!(at_edge.0 < 1.0);
    }

    #[test]
    fn adjacent_rectangles_form_one_continuous_region() {
        let region = ConstraintRegion::from_ops([add(0, 0, 50, 100), add(50, 0, 50, 100)]);
        assert_eq!(
            region.confine((10.0, 50.0), (90.0, 50.0)),
            Some((90.0, 50.0))
        );
    }

    #[test]
    fn pathological_regions_fail_closed() {
        let ops = (0..=MAX_RECTS).map(|index| add(index as i32 * 2, 0, 1, 1));
        let region = ConstraintRegion::from_ops(ops);
        assert!(!region.contains((0.0, 0.0)));
        assert_eq!(region.confine((0.0, 0.0), (1.0, 0.0)), None);
    }
}
