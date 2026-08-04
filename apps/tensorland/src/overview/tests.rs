use super::*;
use crate::ecs::OverviewViewKind;

fn view(id: u64, root: u64, geometry: Rect, stacking_order: u64) -> OverviewView {
    OverviewView {
        id: ViewId::new(id),
        root: ViewId::new(root),
        geometry: Some(geometry),
        focused: false,
        kind: OverviewViewKind::Tiled,
        stacking_order,
    }
}

#[test]
fn grid_is_integer_deterministic_and_bounded() {
    let area = Rect::new(100, 50, 1200, 800);
    let empty = [];
    let sources = (0..5)
        .map(|id| OverviewWorkspaceSource::new(WorkspaceId::new(id), &empty))
        .collect::<Vec<_>>();

    let first = OverviewPlan::compile(area, OverviewOptions::default(), &sources).unwrap();
    let second = OverviewPlan::compile(area, OverviewOptions::default(), &sources).unwrap();

    assert_eq!(first, second);
    assert_eq!(first.workspaces.len(), 5);
    assert!(first.workspaces.iter().all(|workspace| {
        area.contains_rect(workspace.geometry)
            && workspace.geometry.width > 0
            && workspace.geometry.height > 0
    }));
    for (index, left) in first.workspaces.iter().enumerate() {
        assert!(
            first.workspaces[index + 1..]
                .iter()
                .all(|right| left.geometry.intersection(right.geometry).is_none())
        );
    }
}

#[test]
fn view_transform_and_clip_share_the_workspace_plan() {
    let area = Rect::new(100, 200, 1000, 500);
    let views = [view(1, 1, Rect::new(0, 100, 700, 700), 1)];
    let sources = [OverviewWorkspaceSource::new(WorkspaceId::new(0), &views)];

    let plan = OverviewPlan::compile(area, OverviewOptions::new(0, 0), &sources).unwrap();
    let workspace = &plan.workspaces[0];
    let view = workspace.views[0];

    assert_eq!(workspace.geometry, area);
    assert!(view.geometry.x < workspace.geometry.x);
    assert_eq!(
        view.clip,
        view.geometry.intersection(workspace.geometry).unwrap()
    );
    assert!(workspace.geometry.contains_rect(view.clip));
}

#[test]
fn hit_test_prefers_frontmost_view_and_preserves_family() {
    let area = Rect::new(0, 0, 800, 600);
    let views = [
        view(10, 10, Rect::new(100, 100, 400, 300), 1),
        view(11, 10, Rect::new(200, 150, 300, 250), 2),
    ];
    let sources = [OverviewWorkspaceSource::new(WorkspaceId::new(3), &views)];
    let plan = OverviewPlan::compile(area, OverviewOptions::new(0, 0), &sources).unwrap();

    assert_eq!(
        plan.hit_test(Point::new(250, 200)),
        Some(OverviewHit::View {
            workspace: WorkspaceId::new(3),
            view: ViewId::new(11),
            root: ViewId::new(10),
        })
    );
    assert_eq!(
        plan.hit_test(Point::new(10, 10)),
        Some(OverviewHit::Workspace {
            workspace: WorkspaceId::new(3),
        })
    );
    assert_eq!(plan.hit_test(Point::new(800, 600)), None);
}

#[test]
fn missing_geometry_never_creates_a_false_input_target() {
    let area = Rect::new(0, 0, 640, 480);
    let mut missing = view(7, 7, area, 1);
    missing.geometry = None;
    let views = [missing];
    let sources = [OverviewWorkspaceSource::new(WorkspaceId::new(0), &views)];

    let plan = OverviewPlan::compile(area, OverviewOptions::new(0, 0), &sources).unwrap();

    assert!(plan.workspaces[0].views.is_empty());
    assert_eq!(
        plan.hit_test(Point::new(100, 100)),
        Some(OverviewHit::Workspace {
            workspace: WorkspaceId::new(0),
        })
    );
}

#[test]
fn tiny_areas_saturate_gaps_without_underflow() {
    let area = Rect::new(i32::MAX - 2, i32::MAX - 2, 2, 2);
    let empty = [];
    let sources = (0..4)
        .map(|id| OverviewWorkspaceSource::new(WorkspaceId::new(id), &empty))
        .collect::<Vec<_>>();

    let plan =
        OverviewPlan::compile(area, OverviewOptions::new(u32::MAX, u32::MAX), &sources).unwrap();

    assert_eq!(plan.workspaces.len(), 4);
    assert!(
        plan.workspaces
            .iter()
            .all(|workspace| { workspace.geometry.width > 0 && workspace.geometry.height > 0 })
    );
}
