use vulkanalia::vk::Handle;

use super::*;

fn client_image(first: bool) -> ClientImageInfo {
    ClientImageInfo {
        image: vk::Image::null(),
        view_info: vk::ImageViewCreateInfo::default(),
        foreign_owned: true,
        needs_initial_acquire: first,
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
