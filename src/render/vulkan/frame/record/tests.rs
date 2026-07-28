use vulkanalia::vk::Handle;

use tensor_host::{DrmFormat, Fourcc, Modifier};
use tensor_util::{OutputScale, Size};

use crate::render::vulkan::import::ClientImageUpload;
use crate::{
    ecs::{SurfaceBufferId, SurfaceId, ViewId, WorkspaceId},
    layout::LayoutPlacement,
    render::{FrameScheduler, NativeOutputTarget, OutputFormat, RenderOutputId},
    scene::{
        ContentRevision, ContentSpan, EffectStyle, SceneNode, SceneSnapshot, SurfaceContent,
        SurfaceLayer, SurfaceSampleTransform, SurfaceSourceRect, SurfaceTransform,
    },
};

use super::*;

fn client_image(first: bool) -> ClientImageInfo {
    ClientImageInfo {
        image: vk::Image::null(),
        view_info: vk::ImageViewCreateInfo::default(),
        foreign_owned: true,
        needs_initial_acquire: first,
        upload: None,
    }
}

fn shm_client_image(first: bool, upload: bool) -> ClientImageInfo {
    ClientImageInfo {
        image: vk::Image::from_raw(7),
        view_info: vk::ImageViewCreateInfo::default(),
        foreign_owned: false,
        needs_initial_acquire: first,
        upload: upload.then_some(ClientImageUpload {
            buffer: vk::Buffer::from_raw(9),
            extent: vk::Extent3D {
                width: 80,
                height: 32,
                depth: 1,
            },
        }),
    }
}

fn prepared_client(descriptor_index: u32) -> PreparedDraw {
    PreparedDraw {
        push: DrawPushData {
            descriptor_index,
            corner_radius: 0,
            opacity: 1.0,
            padding: 0.0,
            destination: [0.0; 4],
            uv_origin_axis_x: [0.0; 4],
            uv_axis_y_surface_size: [0.0; 4],
        },
        scissor: vk::Rect2D::default(),
    }
}

fn prepared_ring() -> PreparedFocusRingDraw {
    PreparedFocusRingDraw {
        push: FocusRingPushData {
            destination: [0.0; 4],
            color: [0.0; 4],
            inner_rect: [0.0; 4],
            shape: [0.0; 4],
        },
        scissor: vk::Rect2D::default(),
    }
}

#[test]
fn descriptor_push_index_includes_the_frame_heap_offset() {
    assert_eq!(
        descriptor_index(
            HeapAllocation {
                offset: 256,
                size: 256,
            },
            32,
            128,
            3,
        )
        .unwrap(),
        7
    );
}

#[test]
fn shm_upload_transitions_without_foreign_queue_ownership() {
    let subresource = color_subresource();
    let image = shm_client_image(true, true);
    let upload = client_upload_acquire(image, subresource);
    assert_eq!(upload.old_layout, vk::ImageLayout::UNDEFINED);
    assert_eq!(upload.new_layout, vk::ImageLayout::TRANSFER_DST_OPTIMAL);
    assert_eq!(upload.src_queue_family_index, vk::QUEUE_FAMILY_IGNORED);
    assert_eq!(upload.dst_queue_family_index, vk::QUEUE_FAMILY_IGNORED);

    let sample = client_acquire(image, subresource, 3);
    assert_eq!(sample.old_layout, vk::ImageLayout::TRANSFER_DST_OPTIMAL);
    assert_eq!(sample.new_layout, vk::ImageLayout::GENERAL);
    let release = client_release(image, subresource, 3);
    assert_eq!(release.src_queue_family_index, vk::QUEUE_FAMILY_IGNORED);
    assert_eq!(release.dst_queue_family_index, vk::QUEUE_FAMILY_IGNORED);
}

#[test]
fn reused_shm_image_preserves_general_layout_without_an_upload() {
    let acquire = client_acquire(shm_client_image(false, false), color_subresource(), 3);
    assert_eq!(acquire.old_layout, vk::ImageLayout::GENERAL);
    assert_eq!(acquire.new_layout, vk::ImageLayout::GENERAL);
}

#[test]
fn top_left_physical_rect_maps_to_the_top_left_of_vulkan_ndc() {
    assert_eq!(
        destination_to_ndc(Rect::new(100, 200, 50, 25), Rect::new(100, 200, 100, 50)),
        [-1.0, -1.0, 1.0, 1.0]
    );
    assert_eq!(
        destination_to_ndc(Rect::new(150, 225, 50, 25), Rect::new(100, 200, 100, 50)),
        [0.0, 0.0, 1.0, 1.0]
    );
}

#[test]
fn descriptor_push_index_rejects_out_of_slice_draws() {
    assert!(matches!(
        descriptor_index(
            HeapAllocation {
                offset: 256,
                size: 64,
            },
            32,
            128,
            2,
        ),
        Err(FrameRecordError::DescriptorOutsideAllocation { .. })
    ));
}

#[test]
fn draw_push_data_stays_within_the_descriptor_heap_push_budget() {
    assert_eq!(DRAW_PUSH_DATA_SIZE, 64);
}

#[test]
fn cropped_sampling_transform_reaches_push_constants_without_record_time_math() {
    let viewport = Rect::new(0, 0, 100, 80);
    let target = NativeOutputTarget {
        output: RenderOutputId {
            device_id: 1,
            connector_id: 2,
        },
        viewport,
        format: OutputFormat {
            format: DrmFormat::new(Fourcc::XRGB8888, Modifier::from_raw(9)),
            plane_count: 1,
        },
        scale: OutputScale::ONE,
    };
    let sample_transform = SurfaceSampleTransform::for_surface(
        Size::new(64, 32),
        2,
        SurfaceTransform::Rotate90,
        Some(SurfaceSourceRect::from_raw_fixed(
            2 * 256,
            4 * 256,
            10 * 256,
            20 * 256,
        )),
    );
    let content = SurfaceContent {
        surface_id: SurfaceId::new(1),
        buffer_id: SurfaceBufferId::new(1),
        revision: ContentRevision::new(1),
        layer: SurfaceLayer::View,
        alpha: crate::scene::SurfaceAlpha::from_raw(0x8000_0000),
        local_geometry: Rect::new(0, 0, 40, 20),
        sample_transform,
    };
    let scene = SceneSnapshot::with_content(
        WorkspaceId::new(1),
        viewport,
        vec![
            SceneNode::new(
                ViewId::new(1),
                1,
                LayoutPlacement {
                    geometry: Rect::new(0, 0, 40, 20),
                    visible: Some(Rect::new(0, 0, 40, 20)),
                },
                EffectStyle::default(),
            )
            .with_content(ContentSpan::new(0, 1).unwrap()),
        ],
        vec![content],
    );
    let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
    scheduler.register_output(target).unwrap();
    let frame = scheduler.prepare(target.output, scene, 0).unwrap();

    let prepared = prepare_draws(&frame, 32, 0).unwrap();
    assert_eq!(prepared.len(), 1);
    assert_eq!(
        prepared[0].push.opacity,
        crate::scene::SurfaceAlpha::from_raw(0x8000_0000).as_f32()
    );
    assert_eq!(
        prepared[0].push.uv_origin_axis_x,
        [0.875, 0.125, 0.0, 0.625]
    );
    assert_eq!(
        prepared[0].push.uv_axis_y_surface_size,
        [-0.625, 0.0, 40.0, 20.0]
    );
}

#[test]
fn descriptor_push_index_is_relative_to_a_non_stride_aligned_reserved_range() {
    assert_eq!(
        descriptor_index(
            HeapAllocation {
                offset: 192,
                size: 256,
            },
            128,
            64,
            1,
        )
        .unwrap(),
        2
    );
}

#[test]
fn first_foreign_client_acquire_preserves_imported_contents() {
    let barrier = client_acquire(client_image(true), color_subresource(), 7);
    assert_eq!(barrier.old_layout, vk::ImageLayout::UNDEFINED);
    assert_eq!(barrier.src_queue_family_index, vk::QUEUE_FAMILY_FOREIGN_EXT);
    assert_eq!(barrier.dst_queue_family_index, 7);
}

#[test]
fn reused_foreign_client_acquire_uses_released_layout() {
    let barrier = client_acquire(client_image(false), color_subresource(), 7);
    assert_eq!(barrier.old_layout, vk::ImageLayout::GENERAL);
    assert_eq!(barrier.src_queue_family_index, vk::QUEUE_FAMILY_FOREIGN_EXT);
    assert_eq!(barrier.dst_queue_family_index, 7);
}

#[test]
fn focus_outline_colors_remain_linear_through_push_conversion() {
    assert_eq!(
        linear_rgba(crate::scene::LinearRgba16::new(0, u16::MAX, 32_768, 16_384)),
        [0.0, 1.0, 32_768.0 / 65_535.0, 16_384.0 / 65_535.0]
    );
}

#[test]
fn scene_commands_preserve_ring_popup_and_stacking_order() {
    let commands = [
        SceneDrawCommand::FocusRing(0),
        SceneDrawCommand::Client(1),
        SceneDrawCommand::Client(0),
    ];

    let prepared = prepare_scene_draws(
        &commands,
        &[prepared_client(3), prepared_client(7)],
        &[prepared_ring()],
    )
    .unwrap();

    assert!(matches!(prepared[0], PreparedSceneDraw::FocusRing(_)));
    let PreparedSceneDraw::Client(popup) = prepared[1] else {
        panic!("second command must remain the popup client draw");
    };
    assert_eq!(popup.push.descriptor_index, 7);
    let PreparedSceneDraw::Client(later_view) = prepared[2] else {
        panic!("third command must remain the later stacked client draw");
    };
    assert_eq!(later_view.push.descriptor_index, 3);
}

#[test]
fn scene_commands_reject_missing_prepared_draws() {
    assert!(matches!(
        prepare_scene_draws(&[SceneDrawCommand::FocusRing(0)], &[], &[]),
        Err(FrameRecordError::MissingSceneDraw {
            kind: "focus-ring",
            index: 0,
        })
    ));
}
