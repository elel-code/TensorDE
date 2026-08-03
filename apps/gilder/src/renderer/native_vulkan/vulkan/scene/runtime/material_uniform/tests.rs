use super::*;
use crate::engine::scene::{
    SceneBinaryDocument, SceneCullMode, SceneDepthTest, SceneMaterialConstantRecord,
    SceneMaterialHandle, SceneMaterialPassRecord, SceneMaterialRecord, SceneMaterialTextureRecord,
    SceneObjectHandle, SceneObjectKind, SceneObjectRecord, ScenePipelineBlend,
    SceneRenderingDeviceDrawPrimitive, SceneResourceId, SceneResourceKind, SceneResourceRecord,
    SceneStringId, SceneTextureFormat, SceneTextureSamplerAddressMode, SceneTextureSamplerFilter,
    SceneVec3,
};

#[path = "tests/final_effect_contract.rs"]
mod final_effect_contract;

const TEST_OUTPUT_EXTENT: [u32; 2] = [1920, 1080];

fn pack_test_scene_material_uniforms(
    storage: &SceneStorage,
    draws: &[SceneRenderingDeviceMeshDraw],
    scene_time_seconds: f32,
) -> Vec<u8> {
    super::pack_scene_material_uniforms(storage, draws, scene_time_seconds, TEST_OUTPUT_EXTENT)
}

#[test]
fn material_scalar_override_replaces_only_its_typed_constant() {
    let storage = storage_with_constants("effects/opacity__SLOTS_1", &[("alpha", "0.44")]);
    let draw = draw_with_material(SceneMaterialHandle(0));
    let override_value = crate::engine::scene::semantic_world::ResolvedMaterialScalarValue {
        object: crate::engine::scene::SceneObjectHandle(0),
        constant_index: 0,
        value: 0.75,
    };
    let payload = super::pack_scene_material_uniforms_with_frame_inputs(
        &storage,
        &[draw],
        0.0,
        TEST_OUTPUT_EXTENT,
        SceneMaterialFrameInputs {
            average_spectrum32: None,
            stereo_spectrum64: None,
            parallax_position: [0.5; 2],
            audio_material_values: &[],
            material_scalar_values: &[override_value],
        },
    );
    assert_eq!(f32_from_payload(&payload, 0), 0.75);
}

#[test]
fn material_uniform_uses_default_when_draw_has_no_material() {
    let storage = SceneStorage::from_document(SceneBinaryDocument::default()).expect("storage");
    let draw = draw_with_material(SceneMaterialHandle(INVALID_MATERIAL_ID));

    let payload = pack_test_scene_material_uniforms(&storage, &[draw], 0.0);

    assert_eq!(
        payload.len(),
        SCENE_MATERIAL_UNIFORM_FLOATS * size_of::<f32>()
    );
    assert!(payload.iter().all(|byte| *byte == 0));
}

#[test]
fn material_uniform_packs_color_constant_into_first_vec4() {
    let storage = storage_with_constants("we/genericimage4", &[("tint", "[0.25,0.5,0.75,0.9]")]);
    let draw = draw_with_material(SceneMaterialHandle(0));

    let payload = pack_test_scene_material_uniforms(&storage, &[draw], 0.0);

    assert_eq!(f32_from_payload(&payload, 0), 0.25);
    assert_eq!(f32_from_payload(&payload, 4), 0.5);
    assert_eq!(f32_from_payload(&payload, 8), 0.75);
    assert_eq!(f32_from_payload(&payload, 12), 0.9);
}

#[test]
fn standard_material_multiplies_resolved_object_shadow_tint_and_alpha() {
    let storage = storage_with_constants("we/genericimage4", &[("tint", "[0.8,0.6,0.4,0.5]")]);
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.resolved_color = crate::engine::scene::SceneVec3 {
        x: 0.25,
        y: 0.5,
        z: 0.75,
    };
    draw.resolved_alpha = 0.3;

    let payload = pack_test_scene_material_uniforms(&storage, &[draw], 0.0);

    assert_eq!(f32_from_payload(&payload, 0), 0.2);
    assert_eq!(f32_from_payload(&payload, 4), 0.3);
    assert!((f32_from_payload(&payload, 8) - 0.3).abs() < f32::EPSILON);
    assert_eq!(f32_from_payload(&payload, 12), 0.15);
}

#[test]
fn scene_color_blend_packs_resolved_visual_and_authored_mode() {
    let mut document = storage_with_constants("we/genericimage4", &[])
        .document()
        .clone();
    document
        .strings
        .push("we/genericimage4-scene-color-blend".to_owned());
    let blend_shader = SceneStringId((document.strings.len() - 1) as u32);
    document.objects.push(SceneObjectRecord {
        id: SceneObjectHandle(0),
        we_id: 7,
        name: SceneStringId::NONE,
        kind: SceneObjectKind::Image,
        resource: SceneResourceId::NONE,
        material: SceneMaterialHandle(0),
        parent_we_id: crate::engine::scene::INVALID_OBJECT_ID,
        attachment: SceneStringId::NONE,
        origin: SceneVec3::default(),
        angles: SceneVec3::default(),
        scale: SceneVec3::ONE,
        camera_zoom: 1.0,
        color: SceneVec3::ONE,
        alpha: 0.55,
        visible: true,
        color_blend_mode: 11,
        sort_order: 0,
        effect_start: u32::MAX,
        effect_count: 0,
        render_graph: u32::MAX,
    });
    let storage = SceneStorage::from_document(document).expect("scene color blend storage");
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.shader_key = blend_shader;
    draw.resolved_color = SceneVec3 {
        x: 0.25,
        y: 0.5,
        z: 0.75,
    };
    draw.resolved_alpha = 0.55;

    let payload = pack_test_scene_material_uniforms(&storage, &[draw], 0.0);

    assert_eq!(f32_from_payload(&payload, 0), 0.25);
    assert_eq!(f32_from_payload(&payload, 4), 0.5);
    assert_eq!(f32_from_payload(&payload, 8), 0.75);
    assert_eq!(f32_from_payload(&payload, 12), 0.55);
    assert_eq!(f32_from_payload(&payload, 16), 11.0);
}

#[test]
fn synthetic_composite_uses_actual_pass_shader_uniform_layout() {
    let mut document = storage_with_constants("we/composelayer", &[])
        .document()
        .clone();
    document.strings.push("we/objectcomposite".to_owned());
    let composite_shader = SceneStringId((document.strings.len() - 1) as u32);
    let storage = SceneStorage::from_document(document).expect("composite storage");
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.shader_key = composite_shader;
    draw.primitive = SceneRenderingDeviceDrawPrimitive::FullscreenTriangle;
    draw.apply_resolved_visual = true;

    let payload = pack_test_scene_material_uniforms(&storage, &[draw], 0.0);

    for lane in 0..4 {
        assert_eq!(f32_from_payload(&payload, lane * 4), 1.0);
    }
}

#[test]
fn offscreen_object_source_defers_resolved_visual_to_object_composite() {
    let storage = storage_with_constants("we/genericimage4", &[("tint", "[0.8,0.6,0.4,0.5]")]);
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.resolved_color = crate::engine::scene::SceneVec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    draw.resolved_alpha = 0.3;
    draw.apply_resolved_visual = false;

    let payload = pack_test_scene_material_uniforms(&storage, &[draw], 0.0);

    assert_eq!(f32_from_payload(&payload, 0), 0.8);
    assert_eq!(f32_from_payload(&payload, 4), 0.6);
    assert_eq!(f32_from_payload(&payload, 8), 0.4);
    assert_eq!(f32_from_payload(&payload, 12), 0.5);
}

#[test]
fn object_composite_applies_only_resolved_visual_not_base_material_twice() {
    let storage = storage_with_constants("we/genericimage4", &[("tint", "[0.8,0.6,0.4,0.5]")]);
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.primitive = crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle;
    draw.resolved_color = crate::engine::scene::SceneVec3 {
        x: 0.1,
        y: 0.2,
        z: 0.3,
    };
    draw.resolved_alpha = 0.3;

    let payload = pack_test_scene_material_uniforms(&storage, &[draw], 0.0);

    assert_eq!(f32_from_payload(&payload, 0), 0.1);
    assert_eq!(f32_from_payload(&payload, 4), 0.2);
    assert_eq!(f32_from_payload(&payload, 8), 0.3);
    assert_eq!(f32_from_payload(&payload, 12), 0.3);
}

#[test]
fn screen_group_composite_applies_base_and_resolved_visual_once() {
    let mut document = storage_with_constants("we/genericimage4", &[("tint", "[0.8,0.6,0.4,0.5]")])
        .document()
        .clone();
    document
        .strings
        .push("we/objectcomposite-screen-group".to_owned());
    let composite_shader = SceneStringId((document.strings.len() - 1) as u32);
    let storage = SceneStorage::from_document(document).expect("screen group composite storage");
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.shader_key = composite_shader;
    draw.primitive = SceneRenderingDeviceDrawPrimitive::FullscreenTriangle;
    draw.resolved_color = crate::engine::scene::SceneVec3 {
        x: 0.25,
        y: 0.5,
        z: 0.75,
    };
    draw.resolved_alpha = 0.3;

    let payload = pack_test_scene_material_uniforms(&storage, &[draw], 0.0);

    assert_eq!(f32_from_payload(&payload, 0), 0.2);
    assert_eq!(f32_from_payload(&payload, 4), 0.3);
    assert!((f32_from_payload(&payload, 8) - 0.3).abs() < f32::EPSILON);
    assert_eq!(f32_from_payload(&payload, 12), 0.15);
}

#[test]
fn waterwaves_uniform_uses_named_lanes_and_scene_time() {
    let storage = storage_with_constants(
        "effects/waterwaves__SLOTS_3__DUALWAVES_1",
        &[
            ("speed", "2.0"),
            ("scale", "5.0"),
            ("strength", "0.03"),
            ("direction", "-0.9"),
            ("speed2", "3.0"),
            ("scale2", "7.0"),
            ("direction2", "0.5"),
            ("offset2", "0.25"),
            ("exponent", "1.75"),
            ("exponent2", "2.25"),
        ],
    );
    let payload = pack_test_scene_material_uniforms(
        &storage,
        &[draw_with_material(SceneMaterialHandle(0))],
        4.25,
    );

    assert_eq!(f32_from_payload(&payload, 0), 4.25);
    assert_eq!(f32_from_payload(&payload, 4), 2.0);
    assert_eq!(f32_from_payload(&payload, 8), 5.0);
    assert_eq!(f32_from_payload(&payload, 12), 0.03);
    assert_eq!(f32_from_payload(&payload, 16), -0.9);
    assert_eq!(f32_from_payload(&payload, 20), 3.0);
    assert_eq!(f32_from_payload(&payload, 24), 7.0);
    assert_eq!(f32_from_payload(&payload, 28), 0.5);
    assert_eq!(f32_from_payload(&payload, 32), 0.25);
    assert_eq!(f32_from_payload(&payload, 36), 1.0);
    assert_eq!(f32_from_payload(&payload, 40), 1.75);
    assert_eq!(f32_from_payload(&payload, 44), 2.25);
}

#[test]
fn cloudmotion_uniform_uses_the_current_source_extent_for_aspect() {
    let storage = storage_with_constants(
        "effects/cloudmotion__SLOTS_5",
        &[
            ("speed", "0.015"),
            ("amount", "0.1"),
            ("direction", "2.1208904"),
            ("scale", "1.4"),
            ("scalex", "0.1"),
        ],
    );
    let draw = draw_with_material(SceneMaterialHandle(0));

    let framebuffer = super::pack_scene_material_uniforms(&storage, &[draw], 1.1326716, [3856, 2199]);
    assert_eq!(
        f32_from_payload(&framebuffer, 6 * size_of::<f32>()),
        3856.0 / 2199.0
    );

    let mut object_source = draw;
    object_source.authored_source_extent = [905.0, 200.0];
    let object =
        super::pack_scene_material_uniforms(&storage, &[object_source], 1.1326716, [3856, 2199]);
    assert_eq!(
        f32_from_payload(&object, 6 * size_of::<f32>()),
        905.0 / 200.0
    );
}

#[test]
fn shimmer_uniform_packs_time_offset_then_effect_color_in_the_same_vec4() {
    let storage = storage_with_constants(
        "effects/shimmer__SLOTS_9",
        &[
            ("ui_editor_properties_timescale", "0.125"),
            ("ui_editor_properties_color", "[0.25,0.5,0.75]"),
            ("ui_editor_properties_brightness", "2.0"),
        ],
    );
    let payload = pack_test_scene_material_uniforms(
        &storage,
        &[draw_with_material(SceneMaterialHandle(0))],
        3.0,
    );

    assert_eq!(f32_from_payload(&payload, 8 * 4), 0.125);
    assert_eq!(f32_from_payload(&payload, 9 * 4), 0.25);
    assert_eq!(f32_from_payload(&payload, 10 * 4), 0.5);
    assert_eq!(f32_from_payload(&payload, 11 * 4), 0.75);
    assert_eq!(f32_from_payload(&payload, 6 * 4), 2.0);
}

#[test]
fn waterwaves_uv_field_batch_metadata_does_not_overwrite_stage_parameters() {
    let storage = storage_with_constants(
        "we/waterwaves-uv-field",
        &[
            ("waterwaves.stage_count", "1"),
            ("waterwaves.0.offset2", "0.125"),
            ("waterwaves.0.dualwaves", "1"),
            ("waterwaves.0.exponent", "1.75"),
            ("waterwaves.0.exponent2", "2.25"),
        ],
    );
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.effect_batch_atlas_tile = 6;
    draw.effect_batch_atlas_grid = [4, 3];

    let payload = pack_test_scene_material_uniforms(&storage, &[draw], 4.25);

    assert_eq!(f32_from_payload(&payload, 16), 21.25);
    assert!((f32_from_payload(&payload, 24) - 0.01).abs() < 1.0e-7);
    assert_eq!(f32_from_payload(&payload, 36), 1.0);
    assert_eq!(f32_from_payload(&payload, 40), 21.875);
    assert_eq!(f32_from_payload(&payload, 44), 200.0);
    assert_eq!(f32_from_payload(&payload, 48), 0.0);
    assert_eq!(f32_from_payload(&payload, 52), 1.0);
    assert_eq!(f32_from_payload(&payload, 56), 1.75);
    assert_eq!(f32_from_payload(&payload, 60), 2.25);
}

#[test]
fn waterwaves_visibility_mask_neutralizes_only_the_hidden_stage() {
    let storage = storage_with_constants(
        "we/waterwaves-uv-field",
        &[
            ("waterwaves.stage_count", "2"),
            ("waterwaves.0.strength", "0.5"),
            ("waterwaves.1.strength", "0.75"),
        ],
    );
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.effect_binding_start = 4;
    draw.effect_binding_count = 2;
    draw.effect_visibility_policy =
        crate::engine::scene::SceneRenderEffectVisibilityPolicy::WaterWavesStages;
    draw.resolved_effect_visibility_mask = 0b01;

    let payload = pack_test_scene_material_uniforms(&storage, &[draw], 0.0);

    assert_eq!(f32_from_payload(&payload, 6 * 4), 0.25);
    assert_eq!(f32_from_payload(&payload, 22 * 4), 0.0);
}

#[test]
fn direct_waterwaves_disabled_chain_has_zero_displacement() {
    let storage = storage_with_constants(
        "we/effect-waterwaves-direct__STAGES_3",
        &[
            ("waterwaves.stage_count", "3"),
            ("waterwaves.0.strength", "0.5"),
            ("waterwaves.1.strength", "0.75"),
            ("waterwaves.2.strength", "0.25"),
        ],
    );
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.effect_binding_start = 4;
    draw.effect_binding_count = 3;
    draw.effect_visibility_policy =
        crate::engine::scene::SceneRenderEffectVisibilityPolicy::WaterWavesStages;
    draw.resolved_effect_visibility_mask = 0;

    let payload = pack_test_scene_material_uniforms(&storage, &[draw], 0.0);

    for strength_lane in [10, 26, 42] {
        assert_eq!(f32_from_payload(&payload, strength_lane * 4), 0.0);
    }
}

#[test]
fn foliage_ripple_visibility_mask_neutralizes_each_owned_stage() {
    let storage = storage_with_constants(
        "we/image-foliage-ripple-composite",
        &[("foliage.strength", "0.5"), ("ripple.strength", "0.75")],
    );
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.effect_binding_start = 8;
    draw.effect_binding_count = 2;
    draw.effect_visibility_policy =
        crate::engine::scene::SceneRenderEffectVisibilityPolicy::MaterialStages;
    draw.resolved_effect_visibility_mask = 0b01;

    let payload = pack_test_scene_material_uniforms(&storage, &[draw], 0.0);

    assert_eq!(f32_from_payload(&payload, 6 * 4), 0.5);
    assert_eq!(f32_from_payload(&payload, 21 * 4), 0.0);
}

#[test]
fn ripple_flow_visibility_neutralizes_both_typed_passes_independently() {
    let ripple_storage =
        storage_with_constants("we/image-ripple-source", &[("ripplestrength", "0.6")]);
    let flow_storage = storage_with_constants(
        "we/image-ripple-flow-composite",
        &[("flow.strength", "2.5")],
    );
    let mut hidden = draw_with_material(SceneMaterialHandle(0));
    hidden.effect_binding_start = 5;
    hidden.effect_binding_count = 1;
    hidden.effect_visibility_policy =
        crate::engine::scene::SceneRenderEffectVisibilityPolicy::MaterialStages;
    hidden.resolved_effect_visibility_mask = 0;

    let ripple_payload = pack_test_scene_material_uniforms(&ripple_storage, &[hidden], 0.0);
    let flow_payload = pack_test_scene_material_uniforms(&flow_storage, &[hidden], 0.0);

    assert_eq!(f32_from_payload(&ripple_payload, 5 * 4), 0.0);
    assert_eq!(f32_from_payload(&flow_payload, 7 * 4), 0.0);
}

#[test]
fn waterwaves_composite_receives_its_atlas_tile_rectangle() {
    let storage = storage_with_constants("we/genericimage4", &[]);
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.effect_batch_atlas_tile = 6;
    draw.effect_batch_atlas_grid = [4, 3];

    let payload = pack_test_scene_material_uniforms(&storage, &[draw], 0.0);

    assert_eq!(f32_from_payload(&payload, 48), 0.25);
    assert_eq!(f32_from_payload(&payload, 52), 1.0 / 3.0);
    assert_eq!(f32_from_payload(&payload, 56), 0.5);
    assert_eq!(f32_from_payload(&payload, 60), 1.0 / 3.0);
}

#[test]
fn scroll_uniform_packs_time_signed_speed_and_repeat_inputs() {
    let storage = storage_with_constants(
        "effects/scroll__SLOTS_1",
        &[
            ("speedx", "-0.4"),
            ("speedy", "0.25"),
            ("repeat", "\"2 3\""),
        ],
    );
    let payload = pack_test_scene_material_uniforms(
        &storage,
        &[draw_with_material(SceneMaterialHandle(0))],
        6.5,
    );

    assert_eq!(f32_from_payload(&payload, 0), 6.5);
    assert_eq!(f32_from_payload(&payload, 4), -0.4);
    assert_eq!(f32_from_payload(&payload, 8), 0.25);
    assert_eq!(f32_from_payload(&payload, 16), 2.0);
    assert_eq!(f32_from_payload(&payload, 20), 3.0);
}

#[test]
fn skew_uniform_packs_authored_edge_offsets() {
    let storage = storage_with_constants(
        "effects/skew__SLOTS_1",
        &[
            ("top", "0.1"),
            ("bottom", "-0.39"),
            ("left", "0.2"),
            ("right", "-0.3"),
        ],
    );
    let payload = pack_test_scene_material_uniforms(
        &storage,
        &[draw_with_material(SceneMaterialHandle(0))],
        0.0,
    );

    for (lane, expected) in [0.1, -0.39, 0.2, -0.3].into_iter().enumerate() {
        assert_eq!(f32_from_payload(&payload, lane * 4), expected);
    }
}

#[path = "tests/workshop_effects.rs"]
mod workshop_effects;
fn storage_with_constants(shader: &str, constants: &[(&str, &str)]) -> SceneStorage {
    let mut strings = vec![shader.to_owned()];
    let mut material_constants = Vec::with_capacity(constants.len());
    for (name, value) in constants {
        let name_id = SceneStringId(strings.len() as u32);
        strings.push((*name).to_owned());
        let value_id = SceneStringId(strings.len() as u32);
        strings.push((*value).to_owned());
        material_constants.push(SceneMaterialConstantRecord {
            name: name_id,
            value_json: value_id,
        });
    }
    SceneStorage::from_document(SceneBinaryDocument {
        strings,
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
            constant_count: material_constants.len() as u32,
            pipeline_blend: ScenePipelineBlend::Normal,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_writing: SceneStringId::NONE,
            clear_target: false,
        }],
        material_constants,
        ..SceneBinaryDocument::default()
    })
    .expect("storage")
}

fn storage_with_padded_mask(
    shader: &str,
    storage_width: u32,
    storage_height: u32,
    width: u32,
    height: u32,
) -> SceneStorage {
    let storage = storage_with_constants(shader, &[]);
    let mut document = storage.document().clone();
    let resource = SceneResourceId(7);
    document.resources.push(SceneResourceRecord {
        id: resource,
        kind: SceneResourceKind::TextureTex,
        path: SceneStringId::NONE,
        source: SceneStringId::NONE,
        payload_offset: 0,
        payload_len: 0,
    });
    document.textures.push(SceneTextureRecord {
        resource,
        format: SceneTextureFormat::Bc4UnormBlock,
        source_runtime_format: 9,
        payload_format: 0,
        sampler_filter: SceneTextureSamplerFilter::Anisotropic8,
        sampler_address_mode: SceneTextureSamplerAddressMode::Repeat,
        width,
        height,
        storage_width,
        storage_height,
        mip_start: 0,
        mip_count: 0,
        texv_tag: SceneStringId::NONE,
        texb_tag: SceneStringId::NONE,
        payload_offset: 0,
        payload_len: 0,
        alpha_coverage_rows: [u32::MAX;
            crate::engine::scene::SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE],
    });
    document.material_textures.push(SceneMaterialTextureRecord {
        slot: 1,
        resource,
        path: SceneStringId::NONE,
    });
    document.material_passes[0].texture_count = 1;
    SceneStorage::from_document(document).expect("storage with padded mask")
}

fn draw_with_material(material: SceneMaterialHandle) -> SceneRenderingDeviceMeshDraw {
    SceneRenderingDeviceMeshDraw {
        primitive: SceneRenderingDeviceDrawPrimitive::ObjectMesh,
        projection_domain: crate::engine::scene::SceneRenderingDeviceProjectionDomain::Scene,
        shader_key: SceneStringId::NONE,
        mesh_index: 0,
        resolved_object_index: 0,
        render_world_matrix: [[0.0; 4]; 4],
        clip_transform: [[0.0; 4]; 4],
        effect_model_view_projection_matrix: [[0.0; 4]; 4],
        authored_source_extent: [0.0; 2],
        uv_inset_texels: 0.0,
        skinning_palette_start: crate::engine::scene::INVALID_OBJECT_ID,
        skinning_palette_count: 0,
        resolved_color: crate::engine::scene::SceneVec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
        resolved_alpha: 1.0,
        apply_resolved_visual: true,
        effect_batch_atlas_tile: u32::MAX,
        effect_batch_atlas_grid: [0; 2],
        effect_binding_start: u32::MAX,
        effect_binding_count: 0,
        effect_visibility_policy: crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
        resolved_effect_visibility_mask: 0,
        object: crate::engine::scene::SceneObjectHandle(0),
        material,
        vertex_start: 0,
        vertex_count: 4,
        index_start: 0,
        index_count: 6,
        instance_count: 1,
    }
}

fn draw_with_material_visibility(
    material: SceneMaterialHandle,
    binding_count: u32,
    visibility_mask: u32,
) -> SceneRenderingDeviceMeshDraw {
    let mut draw = draw_with_material(material);
    draw.effect_binding_start = 0;
    draw.effect_binding_count = binding_count;
    draw.effect_visibility_policy =
        crate::engine::scene::SceneRenderEffectVisibilityPolicy::MaterialStages;
    draw.resolved_effect_visibility_mask = visibility_mask;
    draw
}

fn f32_from_payload(payload: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap())
}
