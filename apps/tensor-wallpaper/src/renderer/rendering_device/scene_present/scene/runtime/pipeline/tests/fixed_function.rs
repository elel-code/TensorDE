use super::*;

#[test]
fn rgb_only_alpha_over_preserves_target_alpha_and_has_a_distinct_pipeline_key() {
    let mut rgba = render_pass(0, SceneStringId(0), ScenePipelineBlend::Translucent);
    rgba.color_write_mask = SceneColorWriteMask::Rgba;
    let mut rgb = render_pass(1, SceneStringId(0), ScenePipelineBlend::Translucent);
    rgb.color_write_mask = SceneColorWriteMask::Rgb;
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
        render_passes: vec![rgba, rgb],
        ..SceneBinaryDocument::default()
    })
    .expect("storage");
    let graph = object_mesh_graph_with_passes(vec![pass_node(0, 0, 1), pass_node(1, 1, 1)]);

    let indices =
        scene_pipeline_indices_for_draws(&storage, &graph, TextureFormat::Bgra8Unorm, &[], false)
            .expect("indices");
    let attachment = scene_color_target(
        SceneGpuBlend::Alpha,
        SceneColorWriteMask::Rgb,
        TextureFormat::Rgba8Unorm,
    );
    let blend = attachment.blend.expect("alpha blend");

    assert_eq!(indices, vec![0, 1]);
    assert_eq!(attachment.write_mask, ColorWrites::RGB);
    assert_eq!(blend.color.src_factor, BlendFactor::SourceAlpha);
    assert_eq!(blend.color.dst_factor, BlendFactor::OneMinusSourceAlpha);
}
#[test]
fn authored_normal_cull_uses_back_faces_and_has_a_distinct_pipeline_key() {
    let mut no_cull = render_pass(0, SceneStringId(0), ScenePipelineBlend::Translucent);
    no_cull.cull_mode = crate::engine::scene::SceneCullMode::None;
    let mut normal_cull = render_pass(1, SceneStringId(0), ScenePipelineBlend::Translucent);
    normal_cull.cull_mode = crate::engine::scene::SceneCullMode::Normal;
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
        render_passes: vec![no_cull, normal_cull],
        ..SceneBinaryDocument::default()
    })
    .expect("storage");
    let graph = object_mesh_graph_with_passes(vec![pass_node(0, 0, 1), pass_node(1, 1, 1)]);

    let indices =
        scene_pipeline_indices_for_draws(&storage, &graph, TextureFormat::Bgra8Unorm, &[], false)
            .expect("indices");

    assert_eq!(indices, vec![0, 1]);
    assert_eq!(
        scene_cull_mode(crate::engine::scene::SceneCullMode::None),
        CullMode::None
    );
    assert_eq!(
        scene_cull_mode(crate::engine::scene::SceneCullMode::Normal),
        CullMode::Back
    );
    assert_eq!(scene_front_face(), FrontFace::Clockwise);
}

#[test]
fn advanced_blend_marks_only_effect_output_as_premultiplied() {
    let mut effect = render_pass(0, SceneStringId(0), ScenePipelineBlend::Normal);
    effect.scene_blend = SceneCompositeBlend::Multiply;
    assert!(advanced_source_is_premultiplied(&effect));

    effect.role = SceneRenderPassKind::BaseMaterial;
    assert!(!advanced_source_is_premultiplied(&effect));

    effect.role = SceneRenderPassKind::EffectMaterial;
    effect.scene_blend = SceneCompositeBlend::Alpha;
    assert!(!advanced_source_is_premultiplied(&effect));
}

#[test]
fn gpu_blend_attachments_match_we_composite_equations() {
    let multiply = scene_color_target(
        SceneGpuBlend::Multiply,
        SceneColorWriteMask::Rgba,
        TextureFormat::Rgba8Unorm,
    )
    .blend
    .expect("advanced multiply blend");
    assert_eq!(multiply.color.operation, BlendOperation::Multiply);
    assert_eq!(multiply.alpha.operation, BlendOperation::Multiply);

    let modulate = scene_color_target(
        SceneGpuBlend::Modulate,
        SceneColorWriteMask::Rgba,
        TextureFormat::Rgba8Unorm,
    )
    .blend
    .expect("modulate blend");
    assert_eq!(modulate.color.src_factor, BlendFactor::DestinationColor);
    assert_eq!(modulate.color.dst_factor, BlendFactor::One);
    assert_eq!(modulate.color.operation, BlendOperation::Add);
    assert_eq!(modulate.alpha.src_factor, BlendFactor::Zero);
    assert_eq!(modulate.alpha.dst_factor, BlendFactor::One);
}
