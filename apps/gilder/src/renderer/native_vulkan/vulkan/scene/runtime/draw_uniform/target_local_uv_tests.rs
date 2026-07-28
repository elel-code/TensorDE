use std::mem::size_of;

use super::pack_scene_draw_uniforms;
use crate::engine::scene::{
    INVALID_OBJECT_ID, SceneBinaryDocument, SceneColorWriteMask, SceneCompositeBlend,
    SceneCullMode, SceneDepthTest, SceneMaterialHandle, SceneMaterialPassRecord,
    SceneMaterialRecord, SceneObjectHandle, SceneObjectKind, SceneObjectRecord, ScenePipelineBlend,
    SceneRenderEffectVisibilityPolicy, SceneRenderGraphActivationPolicy, SceneRenderGraphRecord,
    SceneRenderPassDrawPrimitive, SceneRenderPassKind, SceneRenderPassRecord,
    SceneRenderTargetKind, SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceMeshDraw,
    SceneResourceId, SceneStorage, SceneStringId, SceneVec3,
};

#[test]
fn audio_image_local_pass_uses_target_local_uvs() {
    let storage = image_local_audio_storage();
    let draw = SceneRenderingDeviceMeshDraw {
        primitive: SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
        shader_key: SceneStringId::NONE,
        mesh_index: 0,
        resolved_object_index: 0,
        clip_transform: [
            [0.000313, 0.0, 0.0, 0.5315],
            [0.0, -0.000557, 0.0, 0.6945],
            [0.0, 0.0, 0.6, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
        authored_source_extent: [1000.0, 1000.0],
        skinning_palette_start: INVALID_OBJECT_ID,
        skinning_palette_count: 0,
        resolved_color: SceneVec3::ONE,
        resolved_alpha: 1.0,
        apply_resolved_visual: true,
        effect_batch_atlas_tile: INVALID_OBJECT_ID,
        effect_batch_atlas_grid: [0; 2],
        effect_binding_start: INVALID_OBJECT_ID,
        effect_binding_count: 0,
        effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
        resolved_effect_visibility_mask: 0,
        object: SceneObjectHandle(0),
        material: SceneMaterialHandle(0),
        vertex_start: 0,
        vertex_count: 4,
        index_start: 0,
        index_count: 6,
        instance_count: 1,
    };

    let payload = pack_scene_draw_uniforms(&storage, &[draw], 0.0, [3840, 2160]);

    for (lane, expected) in [(0, 1.0), (1, 0.0), (2, 0.0), (4, 0.0), (5, 1.0), (6, 0.0)] {
        let offset = lane * size_of::<f32>();
        let actual = f32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap());
        assert!(
            (actual - expected).abs() <= 1.0e-5,
            "expected {expected}, got {actual}"
        );
    }
}

fn image_local_audio_storage() -> SceneStorage {
    SceneStorage::from_document(SceneBinaryDocument {
        strings: vec![
            "effects/simple_audio_bars__SLOTS_1__SHAPE_7".to_owned(),
            "we/image-effect-source".to_owned(),
        ],
        objects: vec![SceneObjectRecord {
            id: SceneObjectHandle(0),
            we_id: 1,
            name: SceneStringId::NONE,
            kind: SceneObjectKind::Image,
            resource: SceneResourceId::NONE,
            material: SceneMaterialHandle(0),
            parent_we_id: INVALID_OBJECT_ID,
            attachment: SceneStringId::NONE,
            origin: SceneVec3::default(),
            angles: SceneVec3::default(),
            scale: SceneVec3::ONE,
            color: SceneVec3::ONE,
            alpha: 1.0,
            visible: true,
            color_blend_mode: 0,
            sort_order: 0,
            effect_start: INVALID_OBJECT_ID,
            effect_count: 0,
            render_graph: 0,
        }],
        materials: vec![SceneMaterialRecord {
            id: SceneMaterialHandle(0),
            resource: SceneResourceId::NONE,
            pass_start: 0,
            pass_count: 1,
        }],
        material_passes: vec![SceneMaterialPassRecord {
            material: SceneMaterialHandle(0),
            shader_key: SceneStringId(0),
            target: SceneStringId::NONE,
            texture_start: 0,
            texture_count: 0,
            constant_start: 0,
            constant_count: 0,
            pipeline_blend: ScenePipelineBlend::Normal,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_writing: SceneStringId::NONE,
            clear_target: false,
        }],
        render_graphs: vec![SceneRenderGraphRecord {
            object: SceneObjectHandle(0),
            activation_policy: SceneRenderGraphActivationPolicy::Always,
            pass_start: 0,
            pass_count: 1,
            unsupported_start: 0,
            unsupported_count: 0,
        }],
        render_passes: vec![SceneRenderPassRecord {
            id: 0,
            role: SceneRenderPassKind::EffectMaterial,
            draw_primitive: SceneRenderPassDrawPrimitive::FullscreenTriangle,
            object: SceneObjectHandle(0),
            material: SceneMaterialHandle(0),
            pass_index: 0,
            shader_key: SceneStringId(1),
            target: SceneRenderTargetKind::FirstClassEffectTarget,
            target_name: SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            effect_binding_start: INVALID_OBJECT_ID,
            effect_binding_count: 0,
            effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
            pipeline_blend: ScenePipelineBlend::Normal,
            scene_blend: SceneCompositeBlend::Alpha,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            color_write_mask: SceneColorWriteMask::Rgba,
            clear_target: false,
        }],
        ..SceneBinaryDocument::default()
    })
    .expect("image-local audio storage")
}
