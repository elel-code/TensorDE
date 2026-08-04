use super::*;

use crate::engine::scene::{
    INVALID_MATERIAL_ID, INVALID_OBJECT_ID, SceneBinaryDocument, SceneCompositeBlend,
    SceneCullMode, SceneDepthTest, SceneMaterialHandle, SceneObjectHandle, ScenePipelineBlend,
    SceneRenderEffectVisibilityPolicy, SceneRenderPassKind, SceneRenderPassRecord,
    SceneRenderTargetKind, SceneRenderingDevicePassNode, SceneStringId,
};

#[test]
fn pixel_bounds_scissor_rounds_outward_and_clamps_to_target() {
    let scissor = PixelBounds {
        min_x: -8.5,
        min_y: 20.25,
        max_x: 90.1,
        max_y: 120.75,
    }
    .scissor([100, 100])
    .expect("visible bounds");

    assert_eq!(scissor.offset, [0, 18]);
    assert_eq!(scissor.extent, [93, 82]);
}

#[test]
fn non_finite_bounds_force_full_target_fallback() {
    assert!(
        PixelBounds {
            min_x: f32::NAN,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        }
        .scissor([100, 100])
        .is_none()
    );
}

#[test]
fn complete_axis_aligned_quad_covers_output_pixel_centers() {
    assert!(rectangle_mesh_covers_pixel_centers(
        [
            [-10.0, -10.0],
            [110.0, -10.0],
            [-10.0, 110.0],
            [110.0, 110.0]
        ],
        &[0, 1, 3, 0, 3, 2],
        [100, 100],
    ));
}

#[test]
fn incomplete_or_rotated_quad_does_not_prove_full_output_coverage() {
    assert!(!rectangle_mesh_covers_pixel_centers(
        [[0.0, 0.0], [100.0, 0.0], [0.0, 100.0], [100.0, 100.0]],
        &[0, 1, 2, 0, 1, 3],
        [100, 100],
    ));
    assert!(!rectangle_mesh_covers_pixel_centers(
        [[50.0, -50.0], [150.0, 50.0], [-50.0, 50.0], [50.0, 150.0]],
        &[0, 1, 3, 0, 3, 2],
        [100, 100],
    ));
}

#[test]
fn composite_consumer_detection_normalizes_shader_variants() {
    let storage = SceneStorage::from_document(SceneBinaryDocument {
        strings: vec!["we/objectcomposite__TEST_1".to_owned()],
        render_passes: vec![SceneRenderPassRecord {
            id: 0,
            role: SceneRenderPassKind::SceneComposite,
            draw_primitive: crate::engine::scene::SceneRenderPassDrawPrimitive::FullscreenTriangle,
            object: SceneObjectHandle(INVALID_OBJECT_ID),
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            pass_index: 0,
            shader_key: SceneStringId(0),
            target: SceneRenderTargetKind::SceneColor,
            target_name: SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            effect_binding_start: u32::MAX,
            effect_binding_count: 0,
            effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
            pipeline_blend: ScenePipelineBlend::Normal,
            scene_blend: SceneCompositeBlend::Alpha,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            color_write_mask: crate::engine::scene::SceneColorWriteMask::Rgba,
            clear_target: false,
        }],
        ..SceneBinaryDocument::default()
    })
    .expect("storage");
    let pass = SceneRenderingDevicePassNode {
        graph_index: 4,
        graph_activation_policy: crate::engine::scene::SceneRenderGraphActivationPolicy::Always,
        pass_record_index: 0,
        pass_id: 0,
        role: SceneRenderPassKind::SceneComposite,
        target: SceneRenderTargetKind::SceneColor,
        target_name: SceneStringId::NONE,
        binding_start: 0,
        binding_count: 0,
        effect_binding_start: u32::MAX,
        effect_binding_count: 0,
        effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
        mesh_draw_start: 0,
        mesh_draw_count: 1,
    };

    assert!(pass_shader_is(&storage, &pass, "we/objectcomposite"));
    assert!(!pass_shader_is(
        &storage,
        &pass,
        "we/flat-rounded-mask-composite"
    ));
}

#[test]
fn rounded_hsl_source_uses_the_same_conservative_support_quad_bounds() {
    assert!(flat_rounded_support_quad_shader(
        "we/flat-rounded-hsl-source"
    ));
    assert!(!flat_rounded_support_quad_shader("we/passthrough"));
}
