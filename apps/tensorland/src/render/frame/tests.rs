use tensor_host::{DrmFormat, Fourcc, Modifier};
use vulkan_renderer::DescriptorHeapAllocator;

use super::*;
use crate::{
    ecs::{SurfaceBufferId, SurfaceId, ViewId, WorkspaceId},
    layout::LayoutPlacement,
    scene::{
        BackdropBlur, ContentRevision, ContentSpan, EffectStyle, SceneNode, SurfaceContent,
        SurfaceLayer, SurfaceSampleTransform,
    },
};

const OUTPUT: RenderOutputId = RenderOutputId {
    device_id: 1,
    connector_id: 2,
};
const SECOND_OUTPUT: RenderOutputId = RenderOutputId {
    device_id: 1,
    connector_id: 3,
};
const VIEWPORT: Rect = Rect::new(0, 0, 1920, 1080);

fn target(output: RenderOutputId) -> NativeOutputTarget {
    NativeOutputTarget {
        output,
        viewport: VIEWPORT,
        format: OutputFormat {
            format: DrmFormat {
                code: Fourcc::XRGB8888,
                modifier: Modifier::from_raw(9),
            },
            plane_count: 1,
        },
        scale: OutputScale::ONE,
    }
}

fn scene(view_id: u64) -> SceneSnapshot {
    scene_in(view_id, VIEWPORT)
}

fn scene_in(view_id: u64, viewport: Rect) -> SceneSnapshot {
    let contents = vec![SurfaceContent {
        surface_id: SurfaceId::new(view_id),
        buffer_id: SurfaceBufferId::new(view_id),
        revision: ContentRevision::new(1),
        layer: SurfaceLayer::View,
        alpha: Default::default(),
        color: Default::default(),
        local_geometry: Rect::new(0, 0, 640, 480),
        sample_transform: SurfaceSampleTransform::IDENTITY,
    }];
    SceneSnapshot::with_content(
        WorkspaceId::new(0),
        viewport,
        vec![
            SceneNode::new(
                ViewId::new(view_id),
                view_id,
                LayoutPlacement {
                    geometry: Rect::new(0, 0, 640, 480),
                    visible: Some(Rect::new(0, 0, 640, 480)),
                },
                EffectStyle::default(),
            )
            .with_content(ContentSpan::new(0, 1).unwrap()),
        ],
        contents,
    )
}

fn backdrop_scene(view_id: u64) -> SceneSnapshot {
    let scene = scene(view_id);
    let node = scene
        .nodes()
        .first()
        .cloned()
        .expect("fixture contains one scene node");
    SceneSnapshot::with_content(
        scene.workspace_id,
        scene.viewport,
        vec![
            SceneNode::new(
                node.view_id,
                node.stacking_order,
                node.placement,
                EffectStyle {
                    backdrop_blur: Some(BackdropBlur { radius: 12 }),
                    ..node.effects
                },
            )
            .with_content(ContentSpan::new(0, 1).unwrap()),
        ],
        scene.contents().to_vec(),
    )
}

#[test]
fn first_frame_and_scene_change_produce_damage() {
    let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
    scheduler.register_output(target(OUTPUT)).unwrap();
    let first = scheduler.submit(OUTPUT, scene(1), 0).unwrap();
    assert_eq!(first.serial, 1);
    assert_eq!(first.damage.regions(), &[VIEWPORT]);
    scheduler.retire_completed(first.timeline_value);

    let second = scheduler
        .submit(OUTPUT, scene(2), first.timeline_value)
        .unwrap();
    assert!(!second.damage.is_empty());
    assert_eq!(second.serial, 2);
}

#[test]
fn shared_renderer_timeline_value_controls_frame_retirement() {
    let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
    scheduler.register_output(target(OUTPUT)).unwrap();

    let frame = scheduler
        .prepare_with_cursors_for_timeline(OUTPUT, scene(1), CursorOverlays::default(), None, 0, 41)
        .unwrap();
    assert_eq!(frame.timeline_value, 41);
    scheduler.commit(&frame).unwrap();

    scheduler.retire_completed(40);
    assert!(scheduler.output_waiting_for_gpu(OUTPUT));
    scheduler.retire_completed(41);
    assert!(!scheduler.output_waiting_for_gpu(OUTPUT));
}

#[test]
fn frame_policy_retires_the_same_allocator_owned_by_the_resource_heap() {
    let allocator = DescriptorHeapAllocator::new(4096, 96, 32).unwrap();
    let mut scheduler =
        FrameScheduler::with_descriptor_allocator(allocator.clone(), 32, 32).unwrap();
    scheduler.register_output(target(OUTPUT)).unwrap();

    let frame = scheduler
        .prepare_with_cursors_for_timeline(OUTPUT, scene(1), CursorOverlays::default(), None, 0, 41)
        .unwrap();
    assert_eq!(scheduler.descriptor_allocation(&frame).unwrap().offset(), 0);
    scheduler.commit(&frame).unwrap();

    assert_eq!(allocator.pending_retirements(), 1);
    assert_eq!(allocator.reclaim(40), 0);
    assert_eq!(allocator.reclaim(41), 1);
}

#[test]
fn fractional_target_scales_draws_and_damage_to_physical_pixels() {
    let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
    let scaled_target = NativeOutputTarget {
        scale: OutputScale::from_f64(1.25).unwrap(),
        ..target(OUTPUT)
    };
    scheduler.register_output(scaled_target).unwrap();
    let logical_viewport = Rect::new(0, 0, 1536, 864);
    let frame = scheduler
        .submit(OUTPUT, scene_in(1, logical_viewport), 0)
        .unwrap();

    assert_eq!(frame.damage.regions(), [VIEWPORT]);
    assert_eq!(
        frame.draw_plan.draws()[0].destination,
        Rect::new(0, 0, 800, 600)
    );
    assert_eq!(frame.draw_plan.draws()[0].clip, Rect::new(0, 0, 800, 600));
}

#[test]
fn in_flight_output_cannot_reuse_descriptors() {
    let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
    scheduler.register_output(target(OUTPUT)).unwrap();
    let first = scheduler.submit(OUTPUT, scene(1), 0).unwrap();
    assert!(matches!(
        scheduler.submit(OUTPUT, scene(2), 0),
        Err(FrameError::OutputBusy { .. })
    ));
    scheduler.retire_completed(first.timeline_value);
    assert!(
        scheduler
            .submit(OUTPUT, scene(2), first.timeline_value)
            .is_ok()
    );
}

#[test]
fn descriptor_exhaustion_is_reported_until_timeline_retires() {
    let mut scheduler = FrameScheduler::new(96, 32, 0, 32).unwrap();
    scheduler.register_output(target(OUTPUT)).unwrap();
    scheduler.register_output(target(SECOND_OUTPUT)).unwrap();
    let first = scheduler.submit(OUTPUT, scene(1), 0).unwrap();
    assert!(matches!(
        scheduler.submit(SECOND_OUTPUT, scene(2), 0),
        Err(FrameError::DescriptorHeapExhausted { .. })
    ));
    scheduler.retire_completed(first.timeline_value);
    let second = scheduler
        .submit(SECOND_OUTPUT, scene(2), first.timeline_value)
        .unwrap();
    assert_eq!(second.descriptors.offset, 0);
}

#[test]
fn invalid_and_unknown_outputs_fail_at_boundary() {
    let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
    assert!(matches!(
        scheduler.register_output(NativeOutputTarget {
            viewport: Rect::new(0, 0, 0, 100),
            ..target(OUTPUT)
        }),
        Err(FrameError::InvalidViewport(_))
    ));
    assert!(matches!(
        scheduler.submit(OUTPUT, scene(1), 0),
        Err(FrameError::UnknownOutput(_))
    ));
}

#[test]
fn device_loss_stops_future_frame_submission() {
    let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
    scheduler.register_output(target(OUTPUT)).unwrap();
    scheduler.mark_device_lost();
    assert_eq!(
        scheduler.submit(OUTPUT, scene(1), 0),
        Err(FrameError::DeviceLost)
    );
}

#[test]
fn descriptor_heap_respects_reserved_range_and_stride() {
    let mut scheduler = FrameScheduler::new(4096, 32, 96, 64).unwrap();
    scheduler.register_output(target(OUTPUT)).unwrap();
    let frame = scheduler.submit(OUTPUT, scene(1), 0).unwrap();
    // The shared direct heap reserves its implementation range as a suffix,
    // so application descriptors begin at byte zero rather than recreating
    // Tensor's former prefix-reservation layout.
    assert_eq!(frame.descriptors.offset, 0);
    assert!(frame.descriptors.offset.is_multiple_of(64));
    assert!(frame.descriptors.size.is_multiple_of(64));
}

#[test]
fn multi_pass_reserves_two_ping_pong_image_descriptors() {
    let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
    scheduler.register_output(target(OUTPUT)).unwrap();

    let frame = scheduler.prepare(OUTPUT, backdrop_scene(1), 0).unwrap();

    // Output + one client image + two reusable intermediate lanes.
    assert_eq!(frame.descriptors.size, 4 * 32);
    assert_eq!(frame.pass_plan.intermediate_descriptor_count(), 2);
}

#[test]
fn invalid_descriptor_heap_layout_fails_before_output_registration() {
    assert!(matches!(
        FrameScheduler::new(4096, 0, 0, 32),
        Err(FrameError::InvalidDescriptorAlignment { .. })
    ));
    assert!(matches!(
        FrameScheduler::new(4096, 64, 4096, 64),
        Err(FrameError::DescriptorHeapTooSmall { .. })
    ));
    // The shared allocator accepts an exact descriptor ABI that rounds to
    // its declared alignment; it does not impose a power-of-two stride.
    assert!(FrameScheduler::new(4096, 64, 0, 48).is_ok());
}

#[test]
fn native_target_requires_explicit_modifier_and_planes() {
    let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
    let implicit = NativeOutputTarget {
        format: OutputFormat {
            format: DrmFormat {
                code: Fourcc::XRGB8888,
                modifier: Modifier::INVALID,
            },
            plane_count: 1,
        },
        ..target(OUTPUT)
    };
    assert!(matches!(
        scheduler.register_output(implicit),
        Err(FrameError::ImplicitOutputModifier(_))
    ));
    let no_planes = NativeOutputTarget {
        format: OutputFormat {
            plane_count: 0,
            ..target(OUTPUT).format
        },
        ..target(OUTPUT)
    };
    assert!(matches!(
        scheduler.register_output(no_planes),
        Err(FrameError::InvalidOutputPlaneCount(_))
    ));
}

#[test]
fn output_slots_cycle_with_the_native_triple_buffer_contract() {
    let mut scheduler = FrameScheduler::new(16 * 1024, 32, 0, 32).unwrap();
    scheduler.register_output(target(OUTPUT)).unwrap();
    let first = scheduler.submit(OUTPUT, scene(1), 0).unwrap();
    assert_eq!(first.output_slot, 0);
    scheduler.retire_completed(first.timeline_value);
    let second = scheduler
        .submit(OUTPUT, scene(2), first.timeline_value)
        .unwrap();
    assert_eq!(second.output_slot, 1);
    scheduler.retire_completed(second.timeline_value);
    let third = scheduler
        .submit(OUTPUT, scene(3), second.timeline_value)
        .unwrap();
    assert_eq!(third.output_slot, 2);
    scheduler.retire_completed(third.timeline_value);
    let fourth = scheduler
        .submit(OUTPUT, scene(4), third.timeline_value)
        .unwrap();
    assert_eq!(fourth.output_slot, 0);
}

#[test]
fn render_damage_tracks_the_exact_triple_buffer_slot_history() {
    let mut scheduler = FrameScheduler::new(16 * 1024, 32, 0, 32).unwrap();
    scheduler.register_output(target(OUTPUT)).unwrap();

    let first = scheduler.submit(OUTPUT, scene(1), 0).unwrap();
    assert_eq!(first.render_damage.regions(), &[VIEWPORT]);
    assert_eq!(first.pass_plan.output_load(), OutputLoad::Clear);
    scheduler.retire_completed(first.timeline_value);

    let second = scheduler
        .submit(OUTPUT, scene(1), first.timeline_value)
        .unwrap();
    assert!(second.damage.is_empty());
    assert_eq!(second.render_damage.regions(), &[VIEWPORT]);
    assert_eq!(second.pass_plan.output_load(), OutputLoad::Clear);
    scheduler.retire_completed(second.timeline_value);

    let third = scheduler
        .submit(OUTPUT, scene(1), second.timeline_value)
        .unwrap();
    assert!(third.damage.is_empty());
    assert_eq!(third.render_damage.regions(), &[VIEWPORT]);
    scheduler.retire_completed(third.timeline_value);

    let fourth = scheduler
        .submit(OUTPUT, scene(1), third.timeline_value)
        .unwrap();
    assert_eq!(fourth.output_slot, 0);
    assert!(fourth.damage.is_empty());
    assert!(fourth.render_damage.is_empty());
    assert_eq!(fourth.pass_plan.output_load(), OutputLoad::Preserve);
    assert_eq!(fourth.pass_plan.path(), &CompositionPath::DirectSinglePass);
}

#[test]
fn next_slot_is_hidden_while_gpu_work_is_in_flight() {
    let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
    scheduler.register_output(target(OUTPUT)).unwrap();
    assert_eq!(scheduler.next_output_slot(OUTPUT), Some(0));

    let frame = scheduler.submit(OUTPUT, scene(1), 0).unwrap();
    assert!(scheduler.output_waiting_for_gpu(OUTPUT));
    assert_eq!(scheduler.next_output_slot(OUTPUT), None);
    scheduler.retire_completed(frame.timeline_value);
    assert!(!scheduler.output_waiting_for_gpu(OUTPUT));
    assert_eq!(scheduler.next_output_slot(OUTPUT), Some(1));
}

#[test]
fn idle_output_can_rotate_around_kms_owned_slots() {
    let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
    scheduler.register_output(target(OUTPUT)).unwrap();

    assert_eq!(scheduler.advance_output_slot(OUTPUT), Some(1));
    assert_eq!(scheduler.advance_output_slot(OUTPUT), Some(2));
    assert_eq!(scheduler.advance_output_slot(OUTPUT), Some(0));

    let frame = scheduler.submit(OUTPUT, scene(1), 0).unwrap();
    assert_eq!(scheduler.advance_output_slot(OUTPUT), None);
    scheduler.retire_completed(frame.timeline_value);
    assert_eq!(scheduler.advance_output_slot(OUTPUT), Some(2));
}

#[test]
fn descriptor_heap_suffix_reservation_keeps_application_offsets_dense() {
    let mut scheduler = FrameScheduler::new(4096, 32, 96, 64).unwrap();
    scheduler.register_output(target(OUTPUT)).unwrap();
    let frame = scheduler.prepare(OUTPUT, scene(1), 0).unwrap();
    assert_eq!(frame.descriptors.offset, 0);
    assert_eq!(frame.descriptors.size, 128);
}

#[test]
fn target_change_preserves_in_flight_lifetime_and_resets_damage_history() {
    let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
    scheduler.register_output(target(OUTPUT)).unwrap();
    let first = scheduler.submit(OUTPUT, scene(1), 0).unwrap();
    let resized = NativeOutputTarget {
        viewport: Rect::new(0, 0, 2560, 1440),
        ..target(OUTPUT)
    };

    scheduler.register_output(resized).unwrap();
    assert!(matches!(
        scheduler.submit(OUTPUT, scene(1), 0),
        Err(FrameError::OutputBusy { .. })
    ));
    scheduler.retire_completed(first.timeline_value);
    let second = scheduler
        .submit(OUTPUT, scene(1), first.timeline_value)
        .unwrap();

    assert_eq!(second.serial, 2);
    assert_eq!(second.target, resized);
    assert_eq!(second.damage.regions(), &[VIEWPORT]);
}

#[test]
fn aborted_prepare_releases_heap_and_preserves_output_sequence() {
    let mut scheduler = FrameScheduler::new(128, 32, 0, 32).unwrap();
    scheduler.register_output(target(OUTPUT)).unwrap();
    let first = scheduler.prepare(OUTPUT, scene(1), 0).unwrap();

    scheduler.abort(&first).unwrap();
    let retry = scheduler.prepare(OUTPUT, scene(1), 0).unwrap();

    assert_eq!(retry.serial, first.serial);
    assert_eq!(retry.output_slot, first.output_slot);
    assert_eq!(retry.descriptors, first.descriptors);
    assert!(retry.timeline_value > first.timeline_value);
    scheduler.commit(&retry).unwrap();
}

#[test]
fn prepared_frame_blocks_target_replacement_until_resolved() {
    let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
    scheduler.register_output(target(OUTPUT)).unwrap();
    let frame = scheduler.prepare(OUTPUT, scene(1), 0).unwrap();
    let resized = NativeOutputTarget {
        viewport: Rect::new(0, 0, 2560, 1440),
        ..target(OUTPUT)
    };

    assert!(matches!(
        scheduler.register_output(resized),
        Err(FrameError::OutputBusy { .. })
    ));
    scheduler.abort(&frame).unwrap();
    assert!(scheduler.register_output(resized).is_ok());
}
