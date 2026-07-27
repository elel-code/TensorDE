use calloop::EventLoop;
use tensor_host::{DrmFormat, Fourcc, Modifier, PhysicalMode, SubpixelLayout};
use wayland_server::Display;

use super::*;

fn descriptor(connector_id: u32, name: &str, width: i32) -> OutputDescriptor {
    let mode = PhysicalMode::new(width, 1080, 60_000);
    OutputDescriptor {
        id: BackendOutputId::new(1, connector_id),
        name: name.to_owned(),
        physical_size: (600, 340),
        subpixel: SubpixelLayout::HorizontalRgb,
        modes: vec![mode],
        mode,
        crtc: connector_id,
        native_format: crate::render::OutputFormat {
            format: DrmFormat {
                code: Fourcc::XRGB8888,
                modifier: Modifier::from_raw(9),
            },
            plane_count: 1,
        },
        scale: tensor_util::OutputScale::ONE,
        position: None,
    }
}

fn output_location(state: &RuntimeState, name: &str) -> i32 {
    state
        .space
        .outputs()
        .find(|output| output.name() == name)
        .unwrap()
        .current_location()
        .0
}

fn output_geometry(
    state: &RuntimeState,
    name: &str,
) -> smithay::utils::Rectangle<i32, smithay::utils::Logical> {
    let output = state
        .space
        .outputs()
        .find(|output| output.name() == name)
        .unwrap();
    state.space.output_geometry(output).unwrap()
}

#[test]
fn output_events_keep_tensor_window_space_stable_across_hotplug() {
    let display = Display::<RuntimeState>::new().unwrap();
    let mut state = RuntimeState::with_appearance(
        display,
        EventLoop::<RuntimeState>::try_new().unwrap().handle(),
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    );

    state
        .apply_backend_output_events([
            BackendOutputEvent::Connected(descriptor(2, "DP-2", 2560)),
            BackendOutputEvent::Connected(descriptor(1, "DP-1", 1920)),
        ])
        .unwrap();
    assert_eq!(state.output_count(), 2);
    assert_eq!(output_location(&state, "DP-1"), 0);
    assert_eq!(output_location(&state, "DP-2"), 1920);

    state
        .apply_backend_output_events([BackendOutputEvent::Changed(descriptor(1, "DP-1", 1280))])
        .unwrap();
    assert_eq!(output_location(&state, "DP-2"), 1280);

    state
        .apply_backend_output_events([BackendOutputEvent::Disconnected(BackendOutputId::new(1, 1))])
        .unwrap();
    assert_eq!(state.output_count(), 1);
    assert_eq!(output_location(&state, "DP-2"), 0);
}

#[test]
fn fractional_output_scale_controls_logical_reflow() {
    let display = Display::<RuntimeState>::new().unwrap();
    let mut state = RuntimeState::with_appearance(
        display,
        EventLoop::<RuntimeState>::try_new().unwrap().handle(),
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    );
    let mut first = descriptor(1, "DP-1", 1920);
    first.scale = tensor_util::OutputScale::from_f64(1.25).unwrap();
    let second = descriptor(2, "DP-2", 1920);

    state
        .apply_backend_output_events([
            BackendOutputEvent::Connected(first),
            BackendOutputEvent::Connected(second),
        ])
        .unwrap();

    assert_eq!(output_geometry(&state, "DP-1").size, (1536, 864).into());
    assert_eq!(output_location(&state, "DP-2"), 1536);
    let first = state
        .space
        .outputs()
        .find(|output| output.name() == "DP-1")
        .unwrap();
    assert_eq!(first.current_scale().as_f64(), 1.25);
    assert_eq!(
        first
            .current_scale()
            .units()
            .div_ceil(tensor_util::OutputScale::DENOMINATOR),
        2
    );
}

#[test]
fn every_connected_output_starts_a_redraw_cycle() {
    let display = Display::<RuntimeState>::new().unwrap();
    let mut state = RuntimeState::with_appearance(
        display,
        EventLoop::<RuntimeState>::try_new().unwrap().handle(),
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    );

    state
        .apply_backend_output_events([
            BackendOutputEvent::Connected(descriptor(827, "HDMI-A-1", 2560)),
            BackendOutputEvent::Connected(descriptor(830, "eDP-1", 2560)),
        ])
        .unwrap();

    // Without a Vulkan/KMS backend the first-frame submit leaves each CRTC
    // queued so a later renderer/backend attach can complete the ring.
    assert_eq!(state.redraw_states.len(), 2);
    assert!(
        state
            .redraw_states
            .values()
            .all(|state| state.needs_gpu_retry() || matches!(state, OutputRedrawState::Idle))
    );
    assert!(
        state
            .redraw_states
            .contains_key(&BackendOutputId::new(1, 827))
    );
    assert!(
        state
            .redraw_states
            .contains_key(&BackendOutputId::new(1, 830))
    );
}

#[test]
fn secondary_output_scene_is_blank_with_its_own_viewport() {
    let display = Display::<RuntimeState>::new().unwrap();
    let mut state = RuntimeState::with_appearance(
        display,
        EventLoop::<RuntimeState>::try_new().unwrap().handle(),
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    );
    state
        .apply_backend_output_events([
            BackendOutputEvent::Connected(descriptor(1, "DP-1", 1920)),
            BackendOutputEvent::Connected(descriptor(2, "DP-2", 1280)),
        ])
        .unwrap();

    let primary_output = state
        .space
        .outputs()
        .find(|output| output.name() == "DP-1")
        .cloned()
        .unwrap();
    let secondary_output = state
        .space
        .outputs()
        .find(|output| output.name() == "DP-2")
        .cloned()
        .unwrap();
    let primary = state.scene_for_output(&primary_output, tensor_util::Rect::new(0, 0, 1920, 1080));
    let secondary = state.scene_for_output(
        &secondary_output,
        tensor_util::Rect::new(1920, 0, 1280, 1080),
    );
    assert_eq!(primary.viewport, tensor_util::Rect::new(0, 0, 1920, 1080));
    assert_eq!(
        secondary.viewport,
        tensor_util::Rect::new(1920, 0, 1280, 1080)
    );
    assert!(secondary.nodes().is_empty());
}

#[test]
fn workspace_redraw_targets_only_the_primary_output() {
    let display = Display::<RuntimeState>::new().unwrap();
    let mut state = RuntimeState::with_appearance(
        display,
        EventLoop::<RuntimeState>::try_new().unwrap().handle(),
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    );
    state
        .apply_backend_output_events([
            BackendOutputEvent::Connected(descriptor(1, "DP-1", 1920)),
            BackendOutputEvent::Connected(descriptor(2, "DP-2", 1280)),
        ])
        .unwrap();

    let primary = BackendOutputId::new(1, 1);
    let secondary = BackendOutputId::new(1, 2);
    // Simulate a settled dual-head session: both heads idle after first frame.
    state.redraw_states.insert(primary, OutputRedrawState::Idle);
    state
        .redraw_states
        .insert(secondary, OutputRedrawState::Idle);

    state.request_redraw_workspace();

    assert_eq!(
        state.redraw_states.get(&primary).copied(),
        Some(OutputRedrawState::Queued)
    );
    assert_eq!(
        state.redraw_states.get(&secondary).copied(),
        Some(OutputRedrawState::Idle)
    );
}

#[test]
fn pointer_redraw_targets_only_the_output_under_the_cursor() {
    let display = Display::<RuntimeState>::new().unwrap();
    let mut state = RuntimeState::with_appearance(
        display,
        EventLoop::<RuntimeState>::try_new().unwrap().handle(),
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    );
    state
        .apply_backend_output_events([
            BackendOutputEvent::Connected(descriptor(1, "DP-1", 1920)),
            BackendOutputEvent::Connected(descriptor(2, "DP-2", 1280)),
        ])
        .unwrap();

    let primary = BackendOutputId::new(1, 1);
    let secondary = BackendOutputId::new(1, 2);
    state.redraw_states.insert(primary, OutputRedrawState::Idle);
    state
        .redraw_states
        .insert(secondary, OutputRedrawState::Idle);

    // Secondary head is laid out at x=1920 after reflow.
    state.request_redraw_at((2000.0, 10.0).into());

    assert_eq!(
        state.redraw_states.get(&primary).copied(),
        Some(OutputRedrawState::Idle)
    );
    assert_eq!(
        state.redraw_states.get(&secondary).copied(),
        Some(OutputRedrawState::Queued)
    );
}

#[test]
fn queue_marks_idle_and_waiting_outputs_dirty() {
    assert_eq!(OutputRedrawState::Idle.queue(), OutputRedrawState::Queued);
    assert_eq!(OutputRedrawState::Queued.queue(), OutputRedrawState::Queued);
    assert_eq!(
        OutputRedrawState::WaitingForVBlank {
            redraw_needed: false
        }
        .queue(),
        OutputRedrawState::WaitingForVBlank {
            redraw_needed: true
        }
    );
    assert!(
        OutputRedrawState::WaitingForVBlank {
            redraw_needed: true
        }
        .queue()
        .needs_gpu_retry()
    );
}
