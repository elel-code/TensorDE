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

#[test]
fn response_and_event_envelopes_can_share_one_completed_read() {
    let mut frames = crate::ipc::encode(&ServerMessage::Response(Response::new(
        7,
        ResultBody::Accepted,
    )))
    .unwrap();
    frames.extend(
        crate::ipc::encode(&ServerMessage::Event(EventMessage::new(
            1,
            ServerEvent::ConfigReload(ConfigReloadEvent {
                request_id: 8,
                generation: 2,
                result: ConfigReloadEventResult::Applied,
            }),
        )))
        .unwrap(),
    );

    let messages = crate::ipc::FrameDecoder::new()
        .push::<ServerMessage>(&frames)
        .unwrap();
    assert_eq!(messages.len(), 2);
    assert!(matches!(messages[0], ServerMessage::Response(_)));
    assert!(matches!(messages[1], ServerMessage::Event(_)));
}
