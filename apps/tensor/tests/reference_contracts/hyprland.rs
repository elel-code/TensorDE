//! Contracts adapted from Hyprland's monitor/layout/IPC tests under
//! `references/tensor/hyprland/tests` and `hyprtester/src/tests`.

use tensor_compositor::{
    ecs::{CompositorWorld, ViewId, WorkspaceId},
    ipc::{CodecError, Command, FrameDecoder, Request, Response, ResultBody, encode},
    layout::{LayoutEngine, LayoutKind, Rect},
};

#[test]
fn layout_names_are_explicit_and_legacy_classic_is_not_an_alias() {
    for (name, kind) in [
        ("scrolling-1d", LayoutKind::Scrolling1D),
        ("spatial-2d", LayoutKind::Spatial2D),
        ("master-stack", LayoutKind::MasterStack),
    ] {
        assert_eq!(name.parse::<LayoutKind>().unwrap(), kind);
        assert_eq!(kind.name(), name);
    }
    assert!("classic".parse::<LayoutKind>().is_err());
}

#[test]
fn multi_output_like_layout_inputs_are_deterministic_for_each_policy() {
    let workspace = WorkspaceId::new(4);
    let mut world = CompositorWorld::new();
    for value in [30, 10, 20, 40] {
        world.spawn_view(ViewId::new(value), workspace).unwrap();
    }

    for kind in [
        LayoutKind::Scrolling1D,
        LayoutKind::Spatial2D,
        LayoutKind::MasterStack,
    ] {
        let first = world
            .arrange_workspace(
                workspace,
                LayoutEngine::new(kind),
                Rect::new(0, 0, 1600, 900),
            )
            .clone();
        let second = world
            .arrange_workspace(
                workspace,
                LayoutEngine::new(kind),
                Rect::new(0, 0, 1600, 900),
            )
            .clone();
        assert_eq!(first, second, "layout {kind:?} changed without input");
        assert_eq!(first.placements.len(), 4);
    }
}

#[test]
fn ipc_frames_keep_request_ids_and_structured_errors_across_fragmentation() {
    let request = encode(&Request::new(
        41,
        Command::SetLayout {
            layout: LayoutKind::Spatial2D,
        },
    ))
    .unwrap();
    let response = encode(&Response::error(41, "unsupported", "test error")).unwrap();

    let mut request_decoder = FrameDecoder::new();
    let midpoint = request.len() / 2;
    assert!(
        request_decoder
            .push::<Request>(&request[..midpoint])
            .unwrap()
            .is_empty()
    );
    let requests = request_decoder
        .push::<Request>(&request[midpoint..])
        .unwrap();
    assert_eq!(requests[0].request_id, 41);

    let mut response_decoder = FrameDecoder::new();
    let responses = response_decoder.push::<Response>(&response).unwrap();
    assert_eq!(responses[0].request_id, 41);
    assert!(matches!(
        responses[0].result,
        ResultBody::Error(ref error) if error.code == "unsupported"
    ));
}

#[test]
fn ipc_version_is_preserved_for_boundary_validation() {
    let mut request = Request::new(9, Command::Ping);
    request.version = tensor_compositor::ipc::IPC_PROTOCOL_VERSION + 1;
    let encoded = encode(&request).unwrap();
    let decoded = FrameDecoder::new().push::<Request>(&encoded).unwrap();
    assert_eq!(
        decoded[0].version,
        tensor_compositor::ipc::IPC_PROTOCOL_VERSION + 1
    );
}

#[test]
fn ipc_malformed_payload_is_rejected_without_becoming_a_command() {
    let payload = b"not-json";
    let mut frame = (payload.len() as u32).to_le_bytes().to_vec();
    frame.extend_from_slice(payload);

    let error = FrameDecoder::new().push::<Request>(&frame).unwrap_err();
    assert!(matches!(error, CodecError::Deserialize(_)));
}
