use super::*;

#[test]
fn maximum_overview_plan_fits_the_bounded_frame() {
    let geometry = OverviewGeometrySnapshot {
        x: i32::MIN,
        y: i32::MAX,
        width: u32::MAX,
        height: u32::MAX,
    };
    let views = (0..MAX_OVERVIEW_VIEWS)
        .map(|index| OverviewViewSnapshot {
            id: index as u64,
            root: index as u64,
            foreign_toplevel_identifier: Some("tensor-ffffffffffffffff".to_owned()),
            source_geometry: Some(geometry),
            geometry: Some(geometry),
            clip: Some(geometry),
            focused: index == 0,
            kind: OverviewViewKindSnapshot::Attached,
            stacking_order: u64::MAX,
        })
        .collect();
    let response = Response::new(
        u64::MAX,
        ResultBody::Overview(OverviewSnapshot {
            active_workspace: u32::MAX,
            area: Some(geometry),
            truncated: true,
            workspaces: vec![OverviewWorkspaceSnapshot {
                index: u32::MAX,
                name: "x".repeat(64),
                hidden: true,
                minimize_target: true,
                geometry: Some(geometry),
                view_count: MAX_OVERVIEW_VIEWS,
                views,
            }],
        }),
    );

    crate::ipc::encode(&response).expect("the declared overview prefix must fit one IPC frame");
}
