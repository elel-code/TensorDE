use tensor_util::Rect;

use crate::render::frame::{BackdropPass, BackdropRegionSpan};

use super::super::{DrawPushData, PreparedDraw};
use super::*;

fn draw(view_id: u64) -> PreparedSceneDraw {
    PreparedSceneDraw::Client(PreparedDraw {
        view_id: Some(ViewId::new(view_id)),
        push: DrawPushData {
            descriptor_index: 0,
            corner_radius: 0,
            opacity: 1.0,
            sampler_index: 0,
            destination: [0.0; 4],
            uv_origin_axis_x: [0.0; 4],
            uv_axis_y_surface_size: [0.0; 4],
        },
        color: None,
        scissor: vulkan_renderer::Rect2D::default(),
    })
}

fn backdrop(view_id: u64) -> BackdropPass {
    BackdropPass {
        view_id: ViewId::new(view_id),
        region: Rect::new(10, 10, 40, 30),
        sample_region: Rect::new(6, 6, 48, 38),
        radius: 4,
        composite_regions: BackdropRegionSpan { start: 0, len: 1 },
    }
}

#[test]
fn scene_slices_place_each_filter_before_its_view() {
    let draws = [draw(1), draw(1), draw(2), draw(3), draw(3)];
    let mut scratch = BackdropSceneScratch::new();

    scratch
        .prepare(&draws, &[backdrop(2), backdrop(3)])
        .unwrap();

    assert_eq!(
        scratch.slices(),
        [
            BackdropSceneSlice {
                draws_before: 0..2,
                backdrop_index: 0,
            },
            BackdropSceneSlice {
                draws_before: 2..3,
                backdrop_index: 1,
            },
        ]
    );
    assert_eq!(scratch.tail(), 3..5);
}

#[test]
fn missing_or_reordered_effect_view_fails_closed() {
    let draws = [draw(1), draw(2)];
    let mut scratch = BackdropSceneScratch::new();

    assert_eq!(
        scratch.prepare(&draws, &[backdrop(2), backdrop(1)]),
        Err(BackdropScenePlanError::MissingView(ViewId::new(1)))
    );
}

#[test]
fn filter_push_uses_fixed_abi_and_active_region_uv_scale() {
    let push = backdrop_filter_push(
        17,
        3,
        backdrop(1),
        Extent2D::new(48, 38),
        Extent2D::new(96, 76),
        true,
    );

    assert_eq!(mem::size_of::<BackdropFilterPushData>(), 32);
    assert_eq!(push.descriptor_index, 17);
    assert_eq!(push.sampler_index, 3);
    assert_eq!(push.radius, 4);
    assert_eq!(push.horizontal, 1);
    assert_eq!(push.inverse_extent, [1.0 / 96.0, 1.0 / 76.0]);
    assert_eq!(push.uv_scale, [0.5, 0.5]);
}

#[test]
fn composite_samples_the_effect_subregion_from_the_retained_lane() {
    let push = backdrop_composite_push(
        backdrop(1),
        Rect::new(10, 10, 40, 30),
        19,
        5,
        Extent2D::new(96, 76),
        Rect::new(0, 0, 200, 100),
    );

    assert_eq!(push.descriptor_index, 19);
    assert_eq!(push.sampler_index, 5);
    assert_eq!(push.destination, [-0.9, -0.8, 0.4, 0.6]);
    assert_eq!(
        push.uv_origin_axis_x,
        [4.0 / 96.0, 4.0 / 76.0, 40.0 / 96.0, 0.0]
    );
    assert_eq!(push.uv_axis_y_surface_size, [0.0, 30.0 / 76.0, 40.0, 30.0]);
}
