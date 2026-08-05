//! Deterministic compositor-owned overview geometry and hit testing.
//!
//! The plan contains only value data. It references stable view IDs and never
//! duplicates client buffers, renderer descriptors, or Wayland resources.

#[cfg(test)]
mod tests;

use tensor_util::{Point, Rect, Size, split_evenly};

use crate::ecs::{OverviewView, ViewId, WorkspaceId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverviewOptions {
    pub outer_gap: u32,
    pub workspace_gap: u32,
}

impl OverviewOptions {
    pub const fn new(outer_gap: u32, workspace_gap: u32) -> Self {
        Self {
            outer_gap,
            workspace_gap,
        }
    }
}

impl Default for OverviewOptions {
    fn default() -> Self {
        Self::new(24, 16)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverviewWorkspaceSource<'a> {
    pub id: WorkspaceId,
    /// Stable back-to-front scene order.
    pub views: &'a [OverviewView],
}

impl<'a> OverviewWorkspaceSource<'a> {
    pub const fn new(id: WorkspaceId, views: &'a [OverviewView]) -> Self {
        Self { id, views }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverviewViewPlan {
    pub id: ViewId,
    pub root: ViewId,
    pub source_geometry: Rect,
    pub geometry: Rect,
    pub clip: Rect,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverviewWorkspacePlan {
    pub id: WorkspaceId,
    pub geometry: Rect,
    /// Stable back-to-front scene order.
    pub views: Vec<OverviewViewPlan>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverviewPlan {
    pub area: Rect,
    pub workspaces: Vec<OverviewWorkspacePlan>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverviewHit {
    View {
        workspace: WorkspaceId,
        view: ViewId,
        root: ViewId,
    },
    Workspace {
        workspace: WorkspaceId,
    },
}

impl OverviewPlan {
    pub fn compile(
        area: Rect,
        options: OverviewOptions,
        sources: &[OverviewWorkspaceSource<'_>],
    ) -> Option<Self> {
        if area.width == 0 || area.height == 0 {
            return None;
        }
        let content = inset_non_empty(area, options.outer_gap);
        let cells = workspace_cells(content, options.workspace_gap, sources.len());
        let workspaces = sources
            .iter()
            .zip(cells)
            .map(|(source, cell)| {
                let workspace_geometry = fit_rect(area.size(), cell);
                let views = source
                    .views
                    .iter()
                    .filter_map(|view| {
                        let source_geometry = view.geometry?;
                        let geometry = transform_rect(source_geometry, area, workspace_geometry);
                        let clip = geometry.intersection(workspace_geometry)?;
                        Some(OverviewViewPlan {
                            id: view.id,
                            root: view.root,
                            source_geometry,
                            geometry,
                            clip,
                        })
                    })
                    .collect();
                OverviewWorkspacePlan {
                    id: source.id,
                    geometry: workspace_geometry,
                    views,
                }
            })
            .collect();
        Some(Self { area, workspaces })
    }

    /// Hit test the exact geometry used for overview rendering.
    ///
    /// Workspaces and their view lists are stored back-to-front, so reverse
    /// iteration selects the visually foremost target without rebuilding a
    /// separate input index.
    pub fn hit_test(&self, point: Point) -> Option<OverviewHit> {
        for workspace in self.workspaces.iter().rev() {
            if !contains_point(workspace.geometry, point) {
                continue;
            }
            for view in workspace.views.iter().rev() {
                if contains_point(view.clip, point) {
                    return Some(OverviewHit::View {
                        workspace: workspace.id,
                        view: view.id,
                        root: view.root,
                    });
                }
            }
            return Some(OverviewHit::Workspace {
                workspace: workspace.id,
            });
        }
        None
    }

    pub fn workspace(&self, id: WorkspaceId) -> Option<&OverviewWorkspacePlan> {
        self.workspaces.iter().find(|workspace| workspace.id == id)
    }
}

fn inset_non_empty(area: Rect, requested: u32) -> Rect {
    let horizontal = requested.min(area.width.saturating_sub(1) / 2);
    let vertical = requested.min(area.height.saturating_sub(1) / 2);
    Rect::new(
        area.x
            .saturating_add(i32::try_from(horizontal).unwrap_or(i32::MAX)),
        area.y
            .saturating_add(i32::try_from(vertical).unwrap_or(i32::MAX)),
        area.width.saturating_sub(horizontal.saturating_mul(2)),
        area.height.saturating_sub(vertical.saturating_mul(2)),
    )
}

fn workspace_cells(area: Rect, requested_gap: u32, count: usize) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }
    let columns = choose_columns(area.size(), requested_gap, count);
    let rows = count.div_ceil(columns);
    let gap_x = bounded_gap(area.width, requested_gap, columns);
    let gap_y = bounded_gap(area.height, requested_gap, rows);
    let columns_u32 = u32::try_from(columns).unwrap_or(u32::MAX);
    let rows_u32 = u32::try_from(rows).unwrap_or(u32::MAX);
    let widths = split_evenly(
        area.width
            .saturating_sub(gap_x.saturating_mul(columns_u32.saturating_sub(1))),
        columns,
    );
    let heights = split_evenly(
        area.height
            .saturating_sub(gap_y.saturating_mul(rows_u32.saturating_sub(1))),
        rows,
    );
    let mut cells = Vec::with_capacity(count);
    let mut y = area.y;
    for height in heights {
        let mut x = area.x;
        for width in widths.iter().copied() {
            if cells.len() == count {
                return cells;
            }
            cells.push(Rect::new(x, y, width, height));
            x = x
                .saturating_add(i32::try_from(width).unwrap_or(i32::MAX))
                .saturating_add(i32::try_from(gap_x).unwrap_or(i32::MAX));
        }
        y = y
            .saturating_add(i32::try_from(height).unwrap_or(i32::MAX))
            .saturating_add(i32::try_from(gap_y).unwrap_or(i32::MAX));
    }
    cells
}

fn choose_columns(area: Size, gap: u32, count: usize) -> usize {
    let mut best = GridCandidate::new(area, gap, count, 1);
    for columns in 2..=count {
        let candidate = GridCandidate::new(area, gap, count, columns);
        if candidate.better_than(best) {
            best = candidate;
        }
    }
    best.columns
}

#[derive(Clone, Copy)]
struct GridCandidate {
    columns: usize,
    empty_cells: usize,
    card_area: u64,
}

impl GridCandidate {
    fn new(area: Size, gap: u32, count: usize, columns: usize) -> Self {
        let rows = count.div_ceil(columns);
        let gap_x = bounded_gap(area.width, gap, columns);
        let gap_y = bounded_gap(area.height, gap, rows);
        let columns_u32 = u32::try_from(columns).unwrap_or(u32::MAX);
        let rows_u32 = u32::try_from(rows).unwrap_or(u32::MAX);
        let width = area
            .width
            .saturating_sub(gap_x.saturating_mul(columns_u32.saturating_sub(1)))
            / columns_u32.max(1);
        let height = area
            .height
            .saturating_sub(gap_y.saturating_mul(rows_u32.saturating_sub(1)))
            / rows_u32.max(1);
        let card = fit_size(area, Size::new(width, height));
        Self {
            columns,
            empty_cells: columns * rows - count,
            card_area: u64::from(card.width) * u64::from(card.height),
        }
    }

    fn better_than(self, current: Self) -> bool {
        self.card_area > current.card_area
            || (self.card_area == current.card_area
                && (self.empty_cells, self.columns) < (current.empty_cells, current.columns))
    }
}

fn bounded_gap(length: u32, requested: u32, count: usize) -> u32 {
    if count <= 1 {
        return 0;
    }
    let count = u32::try_from(count).unwrap_or(u32::MAX);
    requested.min(length.saturating_sub(count) / count.saturating_sub(1).max(1))
}

fn fit_rect(aspect: Size, bounds: Rect) -> Rect {
    let size = fit_size(aspect, bounds.size());
    let x = bounds.x.saturating_add(
        i32::try_from(bounds.width.saturating_sub(size.width) / 2).unwrap_or(i32::MAX),
    );
    let y = bounds.y.saturating_add(
        i32::try_from(bounds.height.saturating_sub(size.height) / 2).unwrap_or(i32::MAX),
    );
    Rect::new(x, y, size.width, size.height)
}

fn fit_size(aspect: Size, bounds: Size) -> Size {
    if aspect.width == 0 || aspect.height == 0 || bounds.width == 0 || bounds.height == 0 {
        return Size::new(0, 0);
    }
    if u64::from(bounds.width) * u64::from(aspect.height)
        <= u64::from(bounds.height) * u64::from(aspect.width)
    {
        Size::new(
            bounds.width,
            scale_unsigned(aspect.height, bounds.width, aspect.width).max(1),
        )
    } else {
        Size::new(
            scale_unsigned(aspect.width, bounds.height, aspect.height).max(1),
            bounds.height,
        )
    }
}

fn transform_rect(rect: Rect, source: Rect, destination: Rect) -> Rect {
    let left = transform_edge(
        rect.x,
        source.x,
        source.width,
        destination.x,
        destination.width,
        false,
    );
    let top = transform_edge(
        rect.y,
        source.y,
        source.height,
        destination.y,
        destination.height,
        false,
    );
    let right = transform_edge(
        rect.right(),
        source.x,
        source.width,
        destination.x,
        destination.width,
        true,
    );
    let bottom = transform_edge(
        rect.bottom(),
        source.y,
        source.height,
        destination.y,
        destination.height,
        true,
    );
    Rect::new(
        left,
        top,
        u32::try_from(right.saturating_sub(left)).unwrap_or(u32::MAX),
        u32::try_from(bottom.saturating_sub(top)).unwrap_or(u32::MAX),
    )
}

fn transform_edge(
    value: i32,
    source_origin: i32,
    source_length: u32,
    destination_origin: i32,
    destination_length: u32,
    ceil: bool,
) -> i32 {
    let numerator =
        (i128::from(value) - i128::from(source_origin)) * i128::from(destination_length);
    let denominator = i128::from(source_length.max(1));
    let scaled = if ceil {
        -(-numerator).div_euclid(denominator)
    } else {
        numerator.div_euclid(denominator)
    };
    let value = i128::from(destination_origin) + scaled;
    i32::try_from(value).unwrap_or(if value.is_negative() {
        i32::MIN
    } else {
        i32::MAX
    })
}

fn scale_unsigned(value: u32, numerator: u32, denominator: u32) -> u32 {
    let value =
        u64::from(value).saturating_mul(u64::from(numerator)) / u64::from(denominator.max(1));
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn contains_point(rect: Rect, point: Point) -> bool {
    point.x >= rect.x && point.x < rect.right() && point.y >= rect.y && point.y < rect.bottom()
}
