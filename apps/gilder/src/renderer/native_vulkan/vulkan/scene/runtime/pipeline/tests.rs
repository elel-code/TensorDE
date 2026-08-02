use super::super::descriptor_layout::scene_pipeline_descriptor_layout;
use super::graphics::{scene_color_target, scene_cull_mode};
use super::*;
use crate::engine::scene::{
    SceneBinaryDocument, SceneRenderPassKind, SceneRenderPassRecord, SceneRenderTargetKind,
    SceneRenderingDeviceGraphPlan, SceneRenderingDeviceMeshDraw, SceneRenderingDevicePassNode,
    SceneRenderingDeviceTargetAllocation, SceneShaderContractRecord, SceneTargetExtentDomain,
};
use vulkan_renderer::{BlendFactor, BlendOperation, ColorWrites, CullMode, Extent2D};

#[test]
fn pipeline_indices_follow_drawn_pass_shader_and_blend_order() {
    let storage = SceneStorage::from_document(SceneBinaryDocument {
        strings: vec![
            "we/genericimage4".to_owned(),
            "effects/opacity__SLOTS_1".to_owned(),
            "generic-pipeline".to_owned(),
            "opacity-pipeline".to_owned(),
        ],
        shader_contracts: vec![
            SceneShaderContractRecord {
                shader_key: SceneStringId(0),
                pipeline_key: SceneStringId(2),
                texture_slot_mask: 0b1,
                input_attachment_slot_mask: 0,
                constant_start: 0,
                constant_count: 0,
                resource_heap_count: 2,
                sampler_heap_count: 1,
            },
            SceneShaderContractRecord {
                shader_key: SceneStringId(1),
                pipeline_key: SceneStringId(3),
                texture_slot_mask: 0b1,
                input_attachment_slot_mask: 0,
                constant_start: 0,
                constant_count: 0,
                resource_heap_count: 1,
                sampler_heap_count: 1,
            },
        ],
        render_passes: vec![
            render_pass(0, SceneStringId(0), ScenePipelineBlend::Normal),
            render_pass(1, SceneStringId(1), ScenePipelineBlend::Additive),
        ],
        ..SceneBinaryDocument::default()
    })
    .expect("storage");
    let graph = graph_with_passes_and_primitives(
        vec![pass_node(0, 0, 1), pass_node(1, 1, 1)],
        [
            SceneRenderingDeviceDrawPrimitive::ObjectMesh,
            SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
        ],
    );

    let layout = scene_pipeline_descriptor_layout(&storage, &graph).expect("layout");
    let indices =
        scene_pipeline_indices_for_draws(&storage, &graph, TextureFormat::Bgra8Unorm, &[], false)
            .expect("indices");

    assert_eq!(layout.sampled_slots, vec![0]);
    assert!(layout.material_uniform_enabled);
    assert_eq!(indices, vec![0, 1]);
}

#[test]
fn descriptor_layout_keeps_sampled_and_input_attachment_slots_disjoint_in_storage() {
    let storage = SceneStorage::from_document(SceneBinaryDocument {
        strings: vec!["effects/opacity__SLOTS_1".to_owned(), "pipeline".to_owned()],
        shader_contracts: vec![SceneShaderContractRecord {
            shader_key: SceneStringId(0),
            pipeline_key: SceneStringId(1),
            texture_slot_mask: 1 << 0,
            input_attachment_slot_mask: 1 << 2,
            constant_start: 0,
            constant_count: 0,
            resource_heap_count: 2,
            sampler_heap_count: 1,
        }],
        render_passes: vec![render_pass(0, SceneStringId(0), ScenePipelineBlend::Normal)],
        ..SceneBinaryDocument::default()
    })
    .expect("storage");
    let graph = graph_with_passes(vec![pass_node(0, 0, 1)]);

    let layout = scene_pipeline_descriptor_layout(&storage, &graph).expect("layout");

    assert_eq!(layout.sampled_slots, vec![0]);
    assert_eq!(layout.input_attachment_slots, vec![2]);
    assert_eq!(layout.sampled_resource_offset(), 2);
    assert_eq!(layout.input_attachment_resource_offset(), 3);
    assert_eq!(layout.per_draw_resource_count(), 4);
    assert_eq!(layout.sampler_count_per_draw(), 1);
}

#[test]
fn same_effect_shader_uses_distinct_fullscreen_and_object_mesh_pipelines() {
    let storage = single_shader_storage(
        "effects/shimmer__SLOTS_9",
        vec![
            render_pass(0, SceneStringId(0), ScenePipelineBlend::Normal),
            render_pass(1, SceneStringId(0), ScenePipelineBlend::Normal),
        ],
    );
    let mut second_pass = pass_node(1, 1, 1);
    second_pass.effect_visibility_policy =
        crate::engine::scene::SceneRenderEffectVisibilityPolicy::Passthrough;
    let graph = graph_with_passes_and_primitives(
        vec![pass_node(0, 0, 1), second_pass],
        [
            SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
            SceneRenderingDeviceDrawPrimitive::ObjectMesh,
        ],
    );

    let indices =
        scene_pipeline_indices_for_draws(&storage, &graph, TextureFormat::Bgra8Unorm, &[], false)
            .expect("primitive-specific indices");
    let disabled = scene_disabled_pipeline_indices_for_draws(
        &storage,
        &graph,
        TextureFormat::Bgra8Unorm,
        &[],
        false,
    )
    .expect("primitive-specific passthrough indices");
    let keys =
        drawn_pass_pipeline_keys(&storage, &graph, TextureFormat::Bgra8Unorm, &[], &[], false)
            .expect("pipeline keys");

    assert_eq!(indices, vec![0, 1]);
    assert_eq!(disabled, vec![None, Some(2)]);
    assert!(keys.iter().any(|key| {
        key.shader == ScenePipelineShader::EffectPassthrough(SceneStringId(0))
            && key.primitive == SceneRenderingDeviceDrawPrimitive::ObjectMesh
    }));
}

#[test]
fn object_mesh_effect_without_a_typed_vertex_program_fails_strictly() {
    let storage = single_shader_storage(
        "effects/iris__SLOTS_3__MASK_1",
        vec![render_pass(0, SceneStringId(0), ScenePipelineBlend::Normal)],
    );
    let graph = object_mesh_graph_with_passes(vec![pass_node(0, 0, 1)]);

    let error =
        scene_pipeline_indices_for_draws(&storage, &graph, TextureFormat::Bgra8Unorm, &[], false)
            .expect_err("object-mesh iris must not guess a vertex ABI");

    assert!(error.contains("has no ObjectMesh vertex program"));
}

#[test]
fn one_pass_cannot_mix_incompatible_draw_primitives() {
    let storage = single_shader_storage(
        "effects/shimmer__SLOTS_9",
        vec![render_pass(0, SceneStringId(0), ScenePipelineBlend::Normal)],
    );
    let graph = graph_with_passes_and_primitives(
        vec![pass_node(0, 0, 2)],
        [
            SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
            SceneRenderingDeviceDrawPrimitive::ObjectMesh,
        ],
    );

    let error =
        scene_pipeline_indices_for_draws(&storage, &graph, TextureFormat::Bgra8Unorm, &[], false)
            .expect_err("mixed primitive pass must fail");

    assert!(error.contains("mixes incompatible draw primitives"));
}

#[test]
fn pipeline_indices_include_dynamic_rendering_target_format() {
    let storage = SceneStorage::from_document(SceneBinaryDocument {
        strings: vec!["effects/opacity__SLOTS_1".to_owned(), "pipeline".to_owned()],
        shader_contracts: vec![SceneShaderContractRecord {
            shader_key: SceneStringId(0),
            pipeline_key: SceneStringId(1),
            texture_slot_mask: 1,
            input_attachment_slot_mask: 0,
            constant_start: 0,
            constant_count: 0,
            resource_heap_count: 1,
            sampler_heap_count: 1,
        }],
        render_passes: vec![
            render_pass(0, SceneStringId(0), ScenePipelineBlend::Normal),
            render_pass(1, SceneStringId(0), ScenePipelineBlend::Normal),
        ],
        ..SceneBinaryDocument::default()
    })
    .expect("storage");
    let mut offscreen_pass = pass_node(1, 1, 1);
    offscreen_pass.target = SceneRenderTargetKind::NamedFbo;
    offscreen_pass.target_name = SceneStringId(7);
    let mut graph = graph_with_passes(vec![pass_node(0, 0, 1), offscreen_pass]);
    graph.target_allocations = vec![SceneRenderingDeviceTargetAllocation {
        graph_index: 0,
        target: SceneRenderTargetKind::NamedFbo,
        target_name: SceneStringId(7),
        first_write_pass_id: 1,
        last_use_pass_id: 1,
        physical_slot: 3,
        width: 0,
        height: 0,
        extent_domain: SceneTargetExtentDomain::PhysicalSurface,
    }];
    let target_plans = vec![SceneEffectTargetImagePlan {
        physical_slot: 3,
        graph_index: 0,
        target: SceneRenderTargetKind::NamedFbo,
        target_name: SceneStringId(7),
        format: TextureFormat::Rgba16Float,
        extent: Extent2D::new(960, 540),
        batch_field_count: 1,
        batch_atlas_columns: 1,
        batch_atlas_rows: 1,
        persistent_across_frames: true,
        aliased_logical_target_count: 1,
        input_attachment_required: false,
    }];

    let indices = scene_pipeline_indices_for_draws(
        &storage,
        &graph,
        TextureFormat::Bgra8Unorm,
        &target_plans,
        false,
    )
    .expect("indices");

    assert_eq!(indices, vec![0, 1]);
}

#[test]
fn pipeline_indices_keep_scene_color_msaa_separate_from_single_sample_effect_targets() {
    let mut scene_pass = render_pass(0, SceneStringId(0), ScenePipelineBlend::Normal);
    scene_pass.scene_blend = SceneCompositeBlend::Normal;
    let mut effect_pass = render_pass(1, SceneStringId(0), ScenePipelineBlend::Normal);
    effect_pass.scene_blend = SceneCompositeBlend::Normal;
    let storage = SceneStorage::from_document(SceneBinaryDocument {
        strings: vec!["effects/opacity__SLOTS_1".to_owned(), "pipeline".to_owned()],
        shader_contracts: vec![SceneShaderContractRecord {
            shader_key: SceneStringId(0),
            pipeline_key: SceneStringId(1),
            texture_slot_mask: 1,
            input_attachment_slot_mask: 0,
            constant_start: 0,
            constant_count: 0,
            resource_heap_count: 1,
            sampler_heap_count: 1,
        }],
        render_passes: vec![scene_pass, effect_pass],
        ..SceneBinaryDocument::default()
    })
    .expect("storage");
    let mut offscreen_pass = pass_node(1, 1, 1);
    offscreen_pass.target = SceneRenderTargetKind::NamedFbo;
    offscreen_pass.target_name = SceneStringId(7);
    let mut graph = graph_with_passes(vec![pass_node(0, 0, 1), offscreen_pass]);
    graph.target_allocations = vec![SceneRenderingDeviceTargetAllocation {
        graph_index: 0,
        target: SceneRenderTargetKind::NamedFbo,
        target_name: SceneStringId(7),
        first_write_pass_id: 1,
        last_use_pass_id: 1,
        physical_slot: 3,
        width: 0,
        height: 0,
        extent_domain: SceneTargetExtentDomain::PhysicalSurface,
    }];
    let target_plans = vec![SceneEffectTargetImagePlan {
        physical_slot: 3,
        graph_index: 0,
        target: SceneRenderTargetKind::NamedFbo,
        target_name: SceneStringId(7),
        format: TextureFormat::Bgra8Unorm,
        extent: Extent2D::new(960, 540),
        batch_field_count: 1,
        batch_atlas_columns: 1,
        batch_atlas_rows: 1,
        persistent_across_frames: true,
        aliased_logical_target_count: 1,
        input_attachment_required: false,
    }];

    let single_sample = scene_pipeline_indices_for_draws(
        &storage,
        &graph,
        TextureFormat::Bgra8Unorm,
        &target_plans,
        false,
    )
    .expect("single-sample indices");
    let scene_msaa = scene_pipeline_indices_for_draws(
        &storage,
        &graph,
        TextureFormat::Bgra8Unorm,
        &target_plans,
        true,
    )
    .expect("scene MSAA indices");

    assert_eq!(single_sample, vec![0, 0]);
    assert_eq!(scene_msaa, vec![0, 1]);
}

#[test]
fn final_target_pipeline_keys_include_scene_composite_blend() {
    let mut alpha = render_pass(0, SceneStringId(0), ScenePipelineBlend::Normal);
    alpha.scene_blend = SceneCompositeBlend::Alpha;
    let mut multiply = render_pass(1, SceneStringId(0), ScenePipelineBlend::Normal);
    multiply.scene_blend = SceneCompositeBlend::Multiply;
    let storage = SceneStorage::from_document(SceneBinaryDocument {
        strings: vec!["we/genericimage4".to_owned(), "pipeline".to_owned()],
        shader_contracts: vec![SceneShaderContractRecord {
            shader_key: SceneStringId(0),
            pipeline_key: SceneStringId(1),
            texture_slot_mask: 1,
            input_attachment_slot_mask: 0,
            constant_start: 0,
            constant_count: 0,
            resource_heap_count: 2,
            sampler_heap_count: 1,
        }],
        render_passes: vec![alpha, multiply],
        ..SceneBinaryDocument::default()
    })
    .expect("storage");
    let graph = object_mesh_graph_with_passes(vec![pass_node(0, 0, 1), pass_node(1, 1, 1)]);

    let indices =
        scene_pipeline_indices_for_draws(&storage, &graph, TextureFormat::Bgra8Unorm, &[], false)
            .expect("indices");

    assert_eq!(indices, vec![0, 1]);
    assert_eq!(
        scene_gpu_blend(
            &storage,
            &storage.document().render_passes[0],
            SceneRenderTargetKind::SceneColor
        ),
        SceneGpuBlend::Alpha
    );
    assert_eq!(
        scene_gpu_blend(
            &storage,
            &storage.document().render_passes[1],
            SceneRenderTargetKind::SceneColor
        ),
        SceneGpuBlend::Multiply
    );
    assert_eq!(
        scene_gpu_blend(
            &storage,
            &storage.document().render_passes[1],
            SceneRenderTargetKind::NamedFbo
        ),
        SceneGpuBlend::Replace
    );
}

#[test]
fn foliage_screen_variant_uses_standard_premultiplied_screen_blend() {
    let storage = SceneStorage::from_document(SceneBinaryDocument {
        strings: vec![
            "we/image-foliage-ripple-screen-composite".to_owned(),
            "we/image-foliage-ripple-screen-composite__GILDER_FOLIAGE_POWER_TWO_1".to_owned(),
        ],
        ..SceneBinaryDocument::default()
    })
    .expect("storage");
    let mut pass = render_pass(0, SceneStringId(0), ScenePipelineBlend::Translucent);
    pass.scene_blend = SceneCompositeBlend::Screen;

    assert_eq!(
        scene_gpu_blend(&storage, &pass, SceneRenderTargetKind::SceneColor),
        SceneGpuBlend::ScreenPremultiplied
    );
    pass.shader_key = SceneStringId(1);
    assert_eq!(
        scene_gpu_blend(&storage, &pass, SceneRenderTargetKind::SceneColor),
        SceneGpuBlend::ScreenPremultiplied
    );
    let attachment = scene_color_target(
        SceneGpuBlend::ScreenPremultiplied,
        SceneColorWriteMask::Rgba,
        TextureFormat::Rgba8Unorm,
    );
    let blend = attachment.blend.expect("screen blend");
    assert_eq!(blend.color.operation, BlendOperation::Add);
    assert_eq!(blend.color.src_factor, BlendFactor::One);
    assert_eq!(blend.color.dst_factor, BlendFactor::OneMinusSourceColor);
    assert_eq!(blend.alpha.src_factor, BlendFactor::One);
    assert_eq!(blend.alpha.dst_factor, BlendFactor::OneMinusSourceAlpha);
}

#[test]
fn typed_multiply_variant_uses_standard_premultiplied_multiply_blend() {
    let storage = SceneStorage::from_document(SceneBinaryDocument {
        strings: vec![
            "we/image-waterwaves-multiply-composite".to_owned(),
            "we/image-waterwaves-multiply-direct__STAGES_2".to_owned(),
        ],
        ..SceneBinaryDocument::default()
    })
    .expect("storage");
    let mut pass = render_pass(0, SceneStringId(0), ScenePipelineBlend::Translucent);
    pass.scene_blend = SceneCompositeBlend::Multiply;

    assert_eq!(
        scene_gpu_blend(&storage, &pass, SceneRenderTargetKind::SceneColor),
        SceneGpuBlend::MultiplyPremultiplied
    );
    pass.shader_key = SceneStringId(1);
    assert_eq!(
        scene_gpu_blend(&storage, &pass, SceneRenderTargetKind::SceneColor),
        SceneGpuBlend::MultiplyPremultiplied
    );
    let attachment = scene_color_target(
        SceneGpuBlend::MultiplyPremultiplied,
        SceneColorWriteMask::Rgba,
        TextureFormat::Rgba8Unorm,
    );
    let blend = attachment.blend.expect("multiply blend");
    assert_eq!(blend.color.operation, BlendOperation::Add);
    assert_eq!(blend.color.src_factor, BlendFactor::DestinationColor);
    assert_eq!(blend.color.dst_factor, BlendFactor::OneMinusSourceAlpha);
    assert_eq!(blend.alpha.src_factor, BlendFactor::One);
    assert_eq!(blend.alpha.dst_factor, BlendFactor::OneMinusSourceAlpha);
}

#[test]
fn typed_static_black_multiply_uses_equivalent_standard_alpha_blend() {
    let storage = SceneStorage::from_document(SceneBinaryDocument {
        strings: vec![
            "we/image-effect-composite__STATIC_BLACK_1".to_owned(),
            "we/image-effect-composite".to_owned(),
            "we/image-effect-composite__static_black_1".to_owned(),
        ],
        ..SceneBinaryDocument::default()
    })
    .expect("storage");
    let mut pass = render_pass(0, SceneStringId(0), ScenePipelineBlend::Translucent);
    pass.scene_blend = SceneCompositeBlend::Multiply;

    assert_eq!(
        scene_gpu_blend(&storage, &pass, SceneRenderTargetKind::SceneColor),
        SceneGpuBlend::Alpha
    );
    let attachment = scene_color_target(
        SceneGpuBlend::Alpha,
        SceneColorWriteMask::Rgba,
        TextureFormat::Rgba8Unorm,
    );
    let blend = attachment.blend.expect("alpha blend");
    assert_eq!(blend.color.operation, BlendOperation::Add);
    assert_eq!(blend.color.src_factor, BlendFactor::SourceAlpha);
    assert_eq!(blend.color.dst_factor, BlendFactor::OneMinusSourceAlpha);

    pass.shader_key = SceneStringId(1);
    assert_eq!(
        scene_gpu_blend(&storage, &pass, SceneRenderTargetKind::SceneColor),
        SceneGpuBlend::Multiply
    );

    pass.shader_key = SceneStringId(2);
    assert_eq!(
        scene_gpu_blend(&storage, &pass, SceneRenderTargetKind::SceneColor),
        SceneGpuBlend::Multiply
    );
}

#[test]
fn rounded_hsl_quad_declares_disjoint_advanced_blend_coverage() {
    let storage = SceneStorage::from_document(SceneBinaryDocument {
        strings: vec!["we/flat-rounded-mask-composite".to_owned()],
        ..SceneBinaryDocument::default()
    })
    .expect("storage");
    let mut pass = render_pass(0, SceneStringId(0), ScenePipelineBlend::Translucent);
    pass.scene_blend = SceneCompositeBlend::HslColor;

    assert_eq!(
        advanced_blend_overlap(&storage, &pass),
        BlendOverlap::Disjoint
    );
}

#[test]
fn material_normal_replaces_while_translucent_alpha_blends() {
    assert_eq!(
        pipeline_gpu_blend(ScenePipelineBlend::Normal),
        SceneGpuBlend::Replace
    );
    assert_eq!(
        pipeline_gpu_blend(ScenePipelineBlend::Disabled),
        SceneGpuBlend::Replace
    );
    assert_eq!(
        pipeline_gpu_blend(ScenePipelineBlend::Translucent),
        SceneGpuBlend::Alpha
    );

    let replace = scene_color_target(
        SceneGpuBlend::Replace,
        SceneColorWriteMask::Rgba,
        TextureFormat::Rgba8Unorm,
    );
    let translucent = scene_color_target(
        SceneGpuBlend::Alpha,
        SceneColorWriteMask::Rgba,
        TextureFormat::Rgba8Unorm,
    );
    assert!(replace.blend.is_none());
    let translucent = translucent.blend.expect("alpha blend");
    assert_eq!(translucent.color.src_factor, BlendFactor::SourceAlpha);
    assert_eq!(
        translucent.color.dst_factor,
        BlendFactor::OneMinusSourceAlpha
    );
}

#[path = "tests/fixed_function.rs"]
mod fixed_function;
fn graph_with_passes(
    pass_nodes: Vec<SceneRenderingDevicePassNode>,
) -> SceneRenderingDeviceGraphPlan {
    graph_with_passes_and_primitives(
        pass_nodes,
        [
            SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
            SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
        ],
    )
}

fn single_shader_storage(shader: &str, render_passes: Vec<SceneRenderPassRecord>) -> SceneStorage {
    SceneStorage::from_document(SceneBinaryDocument {
        strings: vec![shader.to_owned(), "pipeline".to_owned()],
        shader_contracts: vec![SceneShaderContractRecord {
            shader_key: SceneStringId(0),
            pipeline_key: SceneStringId(1),
            texture_slot_mask: 1,
            input_attachment_slot_mask: 0,
            constant_start: 0,
            constant_count: 0,
            resource_heap_count: 1,
            sampler_heap_count: 1,
        }],
        render_passes,
        ..SceneBinaryDocument::default()
    })
    .expect("storage")
}

fn object_mesh_graph_with_passes(
    pass_nodes: Vec<SceneRenderingDevicePassNode>,
) -> SceneRenderingDeviceGraphPlan {
    graph_with_passes_and_primitives(
        pass_nodes,
        [
            SceneRenderingDeviceDrawPrimitive::ObjectMesh,
            SceneRenderingDeviceDrawPrimitive::ObjectMesh,
        ],
    )
}

fn graph_with_passes_and_primitives(
    pass_nodes: Vec<SceneRenderingDevicePassNode>,
    primitives: [SceneRenderingDeviceDrawPrimitive; 2],
) -> SceneRenderingDeviceGraphPlan {
    SceneRenderingDeviceGraphPlan {
        pass_nodes,
        mesh_draws: primitives
            .into_iter()
            .map(|primitive| {
                let mut draw = draw();
                draw.primitive = primitive;
                draw
            })
            .collect(),
        target_allocations: Vec::new(),
        effect_batches: Vec::new(),
        effect_batch_instances: Vec::new(),
        sampled_bindings: Vec::new(),
        material_sampled_bindings: Vec::new(),
        puppet_bone_palettes: Vec::new(),
        puppet_bone_matrices: Vec::new(),
        particle_gpu_emitters: Vec::new(),
        resolved_object_count: 0,
        resolved_visible_object_count: 0,
        resolved_attachment_link_count: 0,
        resolved_visible_effect_instance_count: 0,
        resolved_visible_effect_pass_count: 0,
        resolved_visible_effect_fbo_count: 0,
        descriptor_heap_required: true,
        descriptor_heap_resource_count: 0,
        descriptor_heap_sampled_image_count: 0,
        descriptor_heap_uniform_buffer_count: 0,
        descriptor_heap_storage_buffer_count: 0,
        descriptor_heap_sampler_count: 0,
        graph_physical_target_count: 0,
        graph_aliased_target_count: 0,
        fifo_latest_ready_present_required: true,
    }
}

fn pass_node(
    pass_record_index: u32,
    mesh_draw_start: u32,
    mesh_draw_count: u32,
) -> SceneRenderingDevicePassNode {
    SceneRenderingDevicePassNode {
        graph_index: 0,
        graph_activation_policy: crate::engine::scene::SceneRenderGraphActivationPolicy::Always,
        pass_record_index,
        pass_id: pass_record_index,
        role: SceneRenderPassKind::EffectMaterial,
        target: SceneRenderTargetKind::SceneColor,
        target_name: SceneStringId::NONE,
        binding_start: 0,
        binding_count: 0,
        effect_binding_start: u32::MAX,
        effect_binding_count: 0,
        effect_visibility_policy: crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
        mesh_draw_start,
        mesh_draw_count,
    }
}

fn render_pass(
    id: u32,
    shader_key: SceneStringId,
    pipeline_blend: ScenePipelineBlend,
) -> SceneRenderPassRecord {
    SceneRenderPassRecord {
        id,
        role: SceneRenderPassKind::EffectMaterial,
        draw_primitive: crate::engine::scene::SceneRenderPassDrawPrimitive::FullscreenTriangle,
        object: crate::engine::scene::SceneObjectHandle(crate::engine::scene::INVALID_OBJECT_ID),
        material: crate::engine::scene::SceneMaterialHandle(
            crate::engine::scene::INVALID_MATERIAL_ID,
        ),
        pass_index: id,
        shader_key,
        target: SceneRenderTargetKind::SceneColor,
        target_name: SceneStringId::NONE,
        binding_start: 0,
        binding_count: 0,
        effect_binding_start: u32::MAX,
        effect_binding_count: 0,
        effect_visibility_policy: crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
        pipeline_blend,
        scene_blend: crate::engine::scene::SceneCompositeBlend::Alpha,
        depth_test: crate::engine::scene::SceneDepthTest::Disabled,
        depth_write: false,
        cull_mode: crate::engine::scene::SceneCullMode::None,
        color_write_mask: SceneColorWriteMask::Rgba,
        clear_target: false,
    }
}

fn draw() -> SceneRenderingDeviceMeshDraw {
    SceneRenderingDeviceMeshDraw {
        primitive: crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
        projection_domain: crate::engine::scene::SceneRenderingDeviceProjectionDomain::Scene,
        shader_key: crate::engine::scene::SceneStringId::NONE,
        mesh_index: crate::engine::scene::INVALID_OBJECT_ID,
        resolved_object_index: crate::engine::scene::INVALID_OBJECT_ID,
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
        object: crate::engine::scene::SceneObjectHandle(crate::engine::scene::INVALID_OBJECT_ID),
        material: crate::engine::scene::SceneMaterialHandle(
            crate::engine::scene::INVALID_MATERIAL_ID,
        ),
        vertex_start: 0,
        vertex_count: 3,
        index_start: 0,
        index_count: 3,
        instance_count: 1,
    }
}
