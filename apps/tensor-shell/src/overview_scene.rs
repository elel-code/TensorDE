use std::collections::BTreeMap;

use tensor_ipc::land::{OverviewGeometrySnapshot, OverviewSnapshot};
use vulkan_renderer::{Extent2D, Rect2D};
use wayland_client_runtime::{LogicalRect, LogicalSize};

use crate::overview::OverviewServiceSnapshot;
use crate::panel::PanelDraw;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OverviewHit {
    View(u64),
    CloseView(u64),
    Workspace(u32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct OverviewInteraction {
    pub hovered: Option<OverviewHit>,
    pub pressed: Option<OverviewHit>,
    pub press_position: Option<(f64, f64)>,
    pub dragging: bool,
    pub drop_workspace: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OverviewStatus {
    Pending,
    Ready,
    Unavailable,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OverviewItem {
    hit: OverviewHit,
    bounds: LogicalRect,
    focused: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OverviewScene {
    extent: LogicalSize,
    status: OverviewStatus,
    workspaces: Vec<OverviewItem>,
    views: Vec<OverviewItem>,
    controls: Vec<OverviewItem>,
    view_workspaces: BTreeMap<u64, u32>,
}

impl OverviewScene {
    pub(crate) fn build(extent: LogicalSize, snapshot: &OverviewServiceSnapshot) -> Self {
        let mut scene = Self {
            extent,
            status: status(snapshot),
            workspaces: Vec::new(),
            views: Vec::new(),
            controls: Vec::new(),
            view_workspaces: BTreeMap::new(),
        };
        let OverviewServiceSnapshot::Ready(snapshot) = snapshot else {
            return scene;
        };
        scene.populate(snapshot);
        scene
    }

    fn populate(&mut self, snapshot: &OverviewSnapshot) {
        let area = snapshot.area.unwrap_or(OverviewGeometrySnapshot {
            x: 0,
            y: 0,
            width: self.extent.width,
            height: self.extent.height,
        });
        if area.width == 0 || area.height == 0 || self.extent.is_empty() {
            return;
        }
        for workspace in &snapshot.workspaces {
            let Some(geometry) = workspace.geometry else {
                continue;
            };
            if let Some(bounds) = map_geometry(geometry, area, self.extent) {
                self.workspaces.push(OverviewItem {
                    hit: OverviewHit::Workspace(workspace.index),
                    bounds,
                    focused: workspace.index == snapshot.active_workspace,
                });
            }
            let mut views = workspace.views.iter().collect::<Vec<_>>();
            views.sort_by_key(|view| view.stacking_order);
            for view in views {
                let Some(geometry) = view.clip.or(view.geometry) else {
                    continue;
                };
                let Some(bounds) = map_geometry(geometry, area, self.extent) else {
                    continue;
                };
                self.views.push(OverviewItem {
                    hit: OverviewHit::View(view.id),
                    bounds,
                    focused: view.focused,
                });
                self.view_workspaces.insert(view.id, workspace.index);
                if let Some(bounds) = close_control_bounds(bounds) {
                    self.controls.push(OverviewItem {
                        hit: OverviewHit::CloseView(view.id),
                        bounds,
                        focused: false,
                    });
                }
            }
        }
    }

    pub(crate) fn hit_test(&self, position: (f64, f64)) -> Option<OverviewHit> {
        if !valid_position(position) {
            return None;
        }
        self.controls
            .iter()
            .rev()
            .chain(self.views.iter().rev())
            .chain(self.workspaces.iter().rev())
            .find(|item| contains(item.bounds, position))
            .map(|item| item.hit)
    }

    pub(crate) fn drop_workspace(&self, view: u64, position: (f64, f64)) -> Option<u32> {
        if !valid_position(position) {
            return None;
        }
        let source = self.view_workspaces.get(&view).copied()?;
        self.workspaces
            .iter()
            .rev()
            .find(|item| contains(item.bounds, position))
            .and_then(|item| match item.hit {
                OverviewHit::Workspace(index) if index != source => Some(index),
                OverviewHit::Workspace(_) | OverviewHit::View(_) | OverviewHit::CloseView(_) => {
                    None
                }
            })
    }

    pub(crate) fn physical_draws(
        &self,
        physical_extent: Extent2D,
        interaction: OverviewInteraction,
    ) -> Vec<PanelDraw> {
        if self.status != OverviewStatus::Ready {
            return status_draw(self.extent, physical_extent, self.status)
                .into_iter()
                .collect();
        }
        let mut draws = Vec::with_capacity(
            self.workspaces.len() + self.views.len() + self.controls.len().saturating_mul(6),
        );
        for item in self.workspaces.iter().chain(&self.views) {
            push_item_draw(&mut draws, *item, self.extent, physical_extent, interaction);
        }
        for item in &self.controls {
            push_item_draw(&mut draws, *item, self.extent, physical_extent, interaction);
            push_close_glyph(&mut draws, item.bounds, self.extent, physical_extent);
        }
        draws
    }
}

fn valid_position(position: (f64, f64)) -> bool {
    position.0.is_finite() && position.1.is_finite()
}

fn close_control_bounds(view: LogicalRect) -> Option<LogicalRect> {
    const SIZE: u32 = 18;
    const MARGIN: u32 = 4;
    if view.size.width < SIZE + MARGIN * 2 || view.size.height < SIZE + MARGIN * 2 {
        return None;
    }
    let x_offset = i32::try_from(view.size.width - SIZE - MARGIN).ok()?;
    let margin = i32::try_from(MARGIN).ok()?;
    Some(LogicalRect::new(
        view.origin.x.checked_add(x_offset)?,
        view.origin.y.checked_add(margin)?,
        SIZE,
        SIZE,
    ))
}

fn push_item_draw(
    draws: &mut Vec<PanelDraw>,
    item: OverviewItem,
    logical_extent: LogicalSize,
    physical_extent: Extent2D,
    interaction: OverviewInteraction,
) {
    if let Some(rect) = physical_rect(item.bounds, logical_extent, physical_extent) {
        draws.push(PanelDraw {
            rect,
            color: item_color(item, interaction),
        });
    }
}

fn push_close_glyph(
    draws: &mut Vec<PanelDraw>,
    bounds: LogicalRect,
    logical_extent: LogicalSize,
    physical_extent: Extent2D,
) {
    const GLYPH_SQUARES: [(i32, i32); 5] = [(4, 4), (11, 4), (7, 7), (4, 11), (11, 11)];
    for (x, y) in GLYPH_SQUARES {
        let logical = LogicalRect::new(
            bounds.origin.x.saturating_add(x),
            bounds.origin.y.saturating_add(y),
            3,
            3,
        );
        if let Some(rect) = physical_rect(logical, logical_extent, physical_extent) {
            draws.push(PanelDraw {
                rect,
                color: [0.98, 0.98, 0.98, 1.0],
            });
        }
    }
}

fn status(snapshot: &OverviewServiceSnapshot) -> OverviewStatus {
    match snapshot {
        OverviewServiceSnapshot::Pending => OverviewStatus::Pending,
        OverviewServiceSnapshot::Ready(_) => OverviewStatus::Ready,
        OverviewServiceSnapshot::Unavailable => OverviewStatus::Unavailable,
        OverviewServiceSnapshot::Failed => OverviewStatus::Failed,
    }
}

fn map_geometry(
    geometry: OverviewGeometrySnapshot,
    area: OverviewGeometrySnapshot,
    extent: LogicalSize,
) -> Option<LogicalRect> {
    let area_left = i64::from(area.x);
    let area_top = i64::from(area.y);
    let area_right = area_left.checked_add(i64::from(area.width))?;
    let area_bottom = area_top.checked_add(i64::from(area.height))?;
    let left = i64::from(geometry.x).max(area_left);
    let top = i64::from(geometry.y).max(area_top);
    let right = i64::from(geometry.x)
        .checked_add(i64::from(geometry.width))?
        .min(area_right);
    let bottom = i64::from(geometry.y)
        .checked_add(i64::from(geometry.height))?
        .min(area_bottom);
    if right <= left || bottom <= top {
        return None;
    }
    let x = scale_offset(left - area_left, area.width, extent.width);
    let y = scale_offset(top - area_top, area.height, extent.height);
    let right = scale_offset(right - area_left, area.width, extent.width);
    let bottom = scale_offset(bottom - area_top, area.height, extent.height);
    (right > x && bottom > y).then(|| {
        LogicalRect::new(
            i32::try_from(x).unwrap_or(i32::MAX),
            i32::try_from(y).unwrap_or(i32::MAX),
            right - x,
            bottom - y,
        )
    })
}

fn scale_offset(value: i64, source: u32, target: u32) -> u32 {
    let value = u64::try_from(value).unwrap_or(0);
    let scaled = value.saturating_mul(u64::from(target)) / u64::from(source.max(1));
    u32::try_from(scaled).unwrap_or(u32::MAX).min(target)
}

fn contains(bounds: LogicalRect, position: (f64, f64)) -> bool {
    let left = f64::from(bounds.origin.x);
    let top = f64::from(bounds.origin.y);
    let right = left + f64::from(bounds.size.width);
    let bottom = top + f64::from(bounds.size.height);
    position.0 >= left && position.0 < right && position.1 >= top && position.1 < bottom
}

fn physical_rect(
    logical: LogicalRect,
    logical_extent: LogicalSize,
    physical_extent: Extent2D,
) -> Option<Rect2D> {
    if logical_extent.is_empty() || physical_extent.is_empty() {
        return None;
    }
    let left = scale_edge(
        logical.origin.x.max(0) as u32,
        logical_extent.width,
        physical_extent.width,
    );
    let top = scale_edge(
        logical.origin.y.max(0) as u32,
        logical_extent.height,
        physical_extent.height,
    );
    let right = scale_edge(
        logical.origin.x.max(0) as u32 + logical.size.width,
        logical_extent.width,
        physical_extent.width,
    );
    let bottom = scale_edge(
        logical.origin.y.max(0) as u32 + logical.size.height,
        logical_extent.height,
        physical_extent.height,
    );
    (right > left && bottom > top).then(|| {
        Rect2D::new(
            i32::try_from(left).unwrap_or(i32::MAX),
            i32::try_from(top).unwrap_or(i32::MAX),
            right - left,
            bottom - top,
        )
    })
}

fn scale_edge(value: u32, logical: u32, physical: u32) -> u32 {
    let scaled = u64::from(value) * u64::from(physical) / u64::from(logical.max(1));
    u32::try_from(scaled).unwrap_or(u32::MAX).min(physical)
}

fn item_color(item: OverviewItem, interaction: OverviewInteraction) -> [f32; 4] {
    if interaction.pressed == Some(item.hit) {
        return [0.22, 0.48, 0.52, 0.98];
    }
    if let OverviewHit::Workspace(index) = item.hit
        && interaction.drop_workspace == Some(index)
    {
        return [0.10, 0.42, 0.31, 0.98];
    }
    if interaction.hovered == Some(item.hit) {
        return [0.17, 0.34, 0.38, 0.98];
    }
    match item.hit {
        OverviewHit::View(_) if item.focused => [0.12, 0.34, 0.28, 0.98],
        OverviewHit::View(_) => [0.12, 0.15, 0.19, 0.98],
        OverviewHit::CloseView(_) => [0.56, 0.10, 0.12, 0.98],
        OverviewHit::Workspace(_) if item.focused => [0.08, 0.18, 0.20, 0.94],
        OverviewHit::Workspace(_) => [0.06, 0.075, 0.095, 0.90],
    }
}

fn status_draw(
    logical_extent: LogicalSize,
    physical_extent: Extent2D,
    status: OverviewStatus,
) -> Option<PanelDraw> {
    if logical_extent.is_empty() {
        return None;
    }
    let width = logical_extent.width.min(240);
    let height = logical_extent.height.min(24);
    let logical = LogicalRect::new(
        ((logical_extent.width - width) / 2) as i32,
        ((logical_extent.height - height) / 2) as i32,
        width,
        height,
    );
    let color = match status {
        OverviewStatus::Pending => [0.18, 0.19, 0.21, 0.94],
        OverviewStatus::Unavailable => [0.31, 0.22, 0.08, 0.96],
        OverviewStatus::Failed => [0.42, 0.09, 0.11, 0.98],
        OverviewStatus::Ready => return None,
    };
    physical_rect(logical, logical_extent, physical_extent).map(|rect| PanelDraw { rect, color })
}

#[cfg(test)]
mod tests {
    use tensor_ipc::land::{
        OverviewViewKindSnapshot, OverviewViewSnapshot, OverviewWorkspaceSnapshot,
    };

    use super::*;

    fn ready_snapshot() -> OverviewServiceSnapshot {
        OverviewServiceSnapshot::Ready(OverviewSnapshot {
            active_workspace: 0,
            area: Some(OverviewGeometrySnapshot {
                x: 100,
                y: 50,
                width: 1_000,
                height: 500,
            }),
            truncated: false,
            workspaces: vec![OverviewWorkspaceSnapshot {
                index: 0,
                name: "1".into(),
                hidden: false,
                minimize_target: false,
                geometry: Some(OverviewGeometrySnapshot {
                    x: 100,
                    y: 50,
                    width: 500,
                    height: 500,
                }),
                view_count: 1,
                views: vec![OverviewViewSnapshot {
                    id: 42,
                    root: 42,
                    foreign_toplevel_identifier: None,
                    source_geometry: None,
                    geometry: Some(OverviewGeometrySnapshot {
                        x: 200,
                        y: 100,
                        width: 200,
                        height: 100,
                    }),
                    clip: None,
                    focused: true,
                    kind: OverviewViewKindSnapshot::Tiled,
                    stacking_order: 0,
                }],
            }],
        })
    }

    #[test]
    fn snapshot_geometry_drives_draws_and_hit_testing() {
        let scene = OverviewScene::build(LogicalSize::new(1_000, 500), &ready_snapshot());
        assert_eq!(scene.hit_test((150.0, 75.0)), Some(OverviewHit::View(42)));
        assert_eq!(
            scene.hit_test((282.0, 56.0)),
            Some(OverviewHit::CloseView(42))
        );
        assert_eq!(
            scene.hit_test((450.0, 250.0)),
            Some(OverviewHit::Workspace(0))
        );
        assert_eq!(
            scene
                .physical_draws(Extent2D::new(2_000, 1_000), Default::default())
                .len(),
            8
        );
    }

    #[test]
    fn drag_targets_ignore_view_stacking_and_reject_the_source_workspace() {
        let mut snapshot = match ready_snapshot() {
            OverviewServiceSnapshot::Ready(snapshot) => snapshot,
            _ => unreachable!(),
        };
        snapshot.workspaces.push(OverviewWorkspaceSnapshot {
            index: 1,
            name: "2".into(),
            hidden: false,
            minimize_target: false,
            geometry: Some(OverviewGeometrySnapshot {
                x: 600,
                y: 50,
                width: 500,
                height: 500,
            }),
            view_count: 0,
            views: Vec::new(),
        });
        let scene = OverviewScene::build(
            LogicalSize::new(1_000, 500),
            &OverviewServiceSnapshot::Ready(snapshot),
        );

        assert_eq!(scene.drop_workspace(42, (750.0, 250.0)), Some(1));
        assert_eq!(scene.drop_workspace(42, (150.0, 75.0)), None);
    }

    #[test]
    fn out_of_area_geometry_is_clipped_before_scaling() {
        let mapped = map_geometry(
            OverviewGeometrySnapshot {
                x: 50,
                y: 25,
                width: 100,
                height: 100,
            },
            OverviewGeometrySnapshot {
                x: 100,
                y: 50,
                width: 1_000,
                height: 500,
            },
            LogicalSize::new(1_000, 500),
        )
        .unwrap();
        assert_eq!(mapped, LogicalRect::new(0, 0, 50, 75));
    }

    #[test]
    fn unavailable_service_has_a_bounded_status_draw() {
        let scene = OverviewScene::build(
            LogicalSize::new(800, 600),
            &OverviewServiceSnapshot::Unavailable,
        );
        let draws = scene.physical_draws(Extent2D::new(800, 600), Default::default());
        assert_eq!(draws.len(), 1);
        assert_eq!(draws[0].rect, Rect2D::new(280, 288, 240, 24));
    }
}
