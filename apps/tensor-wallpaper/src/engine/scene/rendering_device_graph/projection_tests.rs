use super::*;
use crate::engine::scene::{RenderingServer, SceneBinaryDocument, SceneStorage};

#[test]
fn authored_texture_object_mesh_uses_target_local_projection_before_scene_composite() {
    for [width, height] in [[415, 405], [496, 680]] {
        let storage = projection_storage(width, height, true);
        let graph = RenderingServer::new(&storage).rendering_device_graph_plan();
        let [base, ripple, composite] = graph.mesh_draws.as_slice() else {
            panic!("expected two local producers and one scene composite draw");
        };

        assert_eq!(
            base.projection_domain,
            SceneRenderingDeviceProjectionDomain::AuthoredTexture { width, height }
        );
        assert_eq!(
            base.clip_transform,
            authored_texture_clip_transform(width, height)
        );
        assert_eq!(
            base.effect_model_view_projection_matrix,
            authored_texture_clip_transform(width, height)
        );
        assert_eq!(ripple.projection_domain, base.projection_domain);
        assert_eq!(ripple.clip_transform, base.clip_transform);
        assert_eq!(base.uv_inset_texels, 0.0);
        assert_eq!(ripple.uv_inset_texels, 0.0);
        assert_eq!(
            composite.projection_domain,
            SceneRenderingDeviceProjectionDomain::Scene
        );
        assert_eq!(
            composite.uv_inset_texels,
            SCENE_OBJECT_COMPOSITE_UV_INSET_TEXELS
        );
        assert_eq!(
            composite.primitive,
            SceneRenderingDeviceDrawPrimitive::ObjectMesh
        );
        assert_eq!(composite.clip_transform[0], [2.0 / 1_000.0, 0.0, 0.0, -1.0]);
        assert_eq!(composite.clip_transform[1], [0.0, -2.0 / 500.0, 0.0, 1.0]);
        assert_ne!(base.clip_transform, composite.clip_transform);
    }
}

#[test]
fn authored_texture_projection_maps_centered_quad_like_we_zero_based_quad() {
    let width = 2_560.0;
    let height = 1_152.0;
    let transform = authored_texture_clip_transform(width as u32, height as u32);
    let tensor_wallpaper_vertices = [
        ([-width * 0.5, -height * 0.5], [0.0, 1.0]),
        ([width * 0.5, -height * 0.5], [1.0, 1.0]),
        ([width * 0.5, height * 0.5], [1.0, 0.0]),
        ([-width * 0.5, height * 0.5], [0.0, 0.0]),
    ];
    let we_vertices = [
        ([0.0, 0.0], [0.0, 1.0]),
        ([width, 0.0], [1.0, 1.0]),
        ([width, height], [1.0, 0.0]),
        ([0.0, height], [0.0, 0.0]),
    ];

    for ((tensor_wallpaper_position, tensor_wallpaper_uv), (we_position, we_uv)) in
        tensor_wallpaper_vertices.into_iter().zip(we_vertices)
    {
        let tensor_wallpaper_clip = [
            transform[0][0] * tensor_wallpaper_position[0] + transform[0][3],
            transform[1][1] * tensor_wallpaper_position[1] + transform[1][3],
        ];
        let we_d3d_clip = [
            2.0 * we_position[0] / width - 1.0,
            2.0 * we_position[1] / height - 1.0,
        ];
        let we_vulkan_clip = [we_d3d_clip[0], -we_d3d_clip[1]];

        assert_eq!(tensor_wallpaper_clip, we_vulkan_clip);
        assert_eq!(tensor_wallpaper_uv, we_uv);
    }
}

#[test]
fn scene_color_object_composite_mesh_keeps_scene_projection() {
    let storage = projection_storage(415, 405, true);
    let graph = RenderingServer::new(&storage).rendering_device_graph_plan();
    let composite = &graph.mesh_draws[2];

    assert_eq!(
        composite.projection_domain,
        SceneRenderingDeviceProjectionDomain::Scene
    );
    assert_eq!(
        composite.effect_model_view_projection_matrix,
        composite.clip_transform
    );
}

#[test]
fn effect_material_writing_owner_authored_target_uses_target_local_projection() {
    let storage = projection_storage(1_579, 956, false);
    let graph = RenderingServer::new(&storage).rendering_device_graph_plan();
    let producer = &graph.mesh_draws[0];

    assert_eq!(
        producer.projection_domain,
        SceneRenderingDeviceProjectionDomain::AuthoredTexture {
            width: 1_579,
            height: 956,
        }
    );
    assert_eq!(
        producer.clip_transform,
        authored_texture_clip_transform(1_579, 956)
    );
    assert_eq!(
        producer.effect_model_view_projection_matrix,
        authored_texture_clip_transform(1_579, 956)
    );
    assert_eq!(producer.clip_transform[0][3], 0.0);
    assert_eq!(producer.clip_transform[1][3], 0.0);
}

#[test]
fn image_layer_composite_base_target_uses_authored_texture_projection() {
    let storage = projection_storage_with_first_class_source(1_188, 403);
    let graph = RenderingServer::new(&storage).rendering_device_graph_plan();
    let producer = &graph.mesh_draws[0];

    assert_eq!(
        producer.projection_domain,
        SceneRenderingDeviceProjectionDomain::AuthoredTexture {
            width: 1_188,
            height: 403,
        }
    );
    assert_eq!(
        producer.clip_transform,
        authored_texture_clip_transform(1_188, 403)
    );
    let expected = [
        [1.188, 0.0, 0.0, -1.0],
        [0.0, 0.806, 0.0, -1.0],
        [0.0, 0.0, 0.00025, 0.5],
        [0.0, 0.0, 0.0, 1.0],
    ];
    for (actual, expected) in producer
        .effect_texture_projection_matrix
        .into_iter()
        .flatten()
        .zip(expected.into_iter().flatten())
    {
        assert!((actual - expected).abs() <= 1.0e-6);
    }
}

#[test]
fn effect_texture_projection_uses_we_y_conjugation_and_default_camera_depth() {
    let mut document = SceneBinaryDocument::default();
    document.project.logical_width = 3_840;
    document.project.logical_height = 2_160;
    let storage = SceneStorage::from_document(document).expect("projection storage");
    let frame = ResolvedSemanticFrame::from_resolved_parts(
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    let world = [
        0.99827063,
        -0.008665401,
        0.0,
        0.0,
        0.008665401,
        0.99827063,
        0.0,
        0.0,
        0.0,
        0.0,
        0.99830824,
        0.0,
        2_182.4326,
        1_094.1395,
        0.0,
        1.0,
    ];

    let projection = effect_texture_projection_matrix(&storage, &frame, world, [993.0, 844.0]);
    let expected = [
        [0.258_146_05, 0.001_904_58, 0.0, 0.136_683_7],
        [-0.003_983_68, 0.390_064_98, 0.0, 0.013_092_16],
        [0.0, 0.0, 0.000_249_577_06, 0.5],
        [0.0, 0.0, 0.0, 1.0],
    ];
    for (actual, expected) in projection
        .into_iter()
        .flatten()
        .zip(expected.into_iter().flatten())
    {
        assert!(
            (actual - expected).abs() <= 2.0e-6,
            "{actual} != {expected}"
        );
    }
}

#[test]
fn image_layer_base_target_applies_object_visual_at_its_external_output_boundary() {
    let mut document = projection_storage_with_first_class_source(668, 339)
        .document()
        .clone();
    document.objects[0].alpha = 0.37;
    let storage = SceneStorage::from_document(document).expect("image-layer alpha storage");
    let graph = RenderingServer::new(&storage).rendering_device_graph_plan();
    let producer = &graph.mesh_draws[0];

    assert_eq!(producer.resolved_alpha, 0.37);
    assert!(producer.apply_resolved_visual);
}

#[test]
fn authored_texture_uv_support_quad_covers_the_local_target_without_object_mesh_geometry() {
    let storage = projection_storage_with_local_primitive(
        1_367,
        1_676,
        SceneRenderPassDrawPrimitive::ObjectUvSupportQuad,
    );
    let graph = RenderingServer::new(&storage).rendering_device_graph_plan();
    let [source, effect, _composite] = graph.mesh_draws.as_slice() else {
        panic!("expected two local UV quads and one terminal object mesh");
    };

    for draw in [source, effect] {
        assert_eq!(
            draw.primitive,
            SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad
        );
        assert_eq!(
            draw.projection_domain,
            SceneRenderingDeviceProjectionDomain::AuthoredTexture {
                width: 1_367,
                height: 1_676,
            }
        );
        assert_eq!(draw.vertex_count, 6);
        assert_eq!(draw.index_count, 6);
        assert_eq!(draw.mesh_index, INVALID_OBJECT_ID);
    }
}

fn projection_storage(width: u32, height: u32, object_local_source: bool) -> SceneStorage {
    projection_storage_with_local_primitive(
        width,
        height,
        if object_local_source {
            SceneRenderPassDrawPrimitive::ObjectMesh
        } else {
            SceneRenderPassDrawPrimitive::None
        },
    )
}

fn projection_storage_with_local_primitive(
    width: u32,
    height: u32,
    local_primitive: SceneRenderPassDrawPrimitive,
) -> SceneStorage {
    projection_storage_with_source_target(width, height, local_primitive, false)
}

fn projection_storage_with_first_class_source(width: u32, height: u32) -> SceneStorage {
    projection_storage_with_source_target(
        width,
        height,
        SceneRenderPassDrawPrimitive::ObjectMesh,
        true,
    )
}

fn projection_storage_with_source_target(
    width: u32,
    height: u32,
    local_primitive: SceneRenderPassDrawPrimitive,
    first_class_source: bool,
) -> SceneStorage {
    let object = SceneObjectHandle(0);
    let resource = SceneResourceId(0);
    let object_local_source = local_primitive != SceneRenderPassDrawPrimitive::None;
    let target_name = SceneStringId(0);
    let mut project = SceneBinaryDocument::default().project;
    project.logical_width = 1_000;
    project.logical_height = 500;
    SceneStorage::from_document(SceneBinaryDocument {
        project,
        strings: first_class_source
            .then(|| vec!["_rt_imageLayerComposite_47_a".to_owned()])
            .unwrap_or_default(),
        resources: vec![SceneResourceRecord {
            id: resource,
            kind: SceneResourceKind::TextureTex,
            path: SceneStringId::NONE,
            source: SceneStringId::NONE,
            payload_offset: 0,
            payload_len: 0,
        }],
        textures: vec![SceneTextureRecord {
            resource,
            format: SceneTextureFormat::Bc7UnormBlock,
            source_runtime_format: 0,
            payload_format: 0,
            sampler_filter: SceneTextureSamplerFilter::Anisotropic8,
            sampler_address_mode: SceneTextureSamplerAddressMode::Repeat,
            width,
            height,
            storage_width: width,
            storage_height: height,
            mip_start: 0,
            mip_count: 0,
            texv_tag: SceneStringId::NONE,
            texb_tag: SceneStringId::NONE,
            sequence_tag: SceneStringId::NONE,
            sequence_cell_width: 0,
            sequence_cell_height: 0,
            sequence_frame_start: 0,
            sequence_frame_count: 0,
            payload_offset: 0,
            payload_len: 0,
            alpha_coverage_rows: [u32::MAX; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE],
        }],
        objects: vec![SceneObjectRecord {
            id: object,
            we_id: 7,
            name: SceneStringId::NONE,
            kind: SceneObjectKind::Image,
            resource,
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            parent_we_id: INVALID_OBJECT_ID,
            attachment: SceneStringId::NONE,
            origin: SceneVec3::default(),
            angles: SceneVec3::default(),
            scale: SceneVec3::ONE,
            camera_zoom: 1.0,
            color: SceneVec3::ONE,
            alpha: 1.0,
            visible: true,
            color_blend_mode: 0,
            sort_order: 0,
            effect_start: u32::MAX,
            effect_count: 0,
            render_graph: 0,
        }],
        meshes: vec![SceneMeshRecord {
            object,
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            vertex_start: 0,
            vertex_count: 4,
            index_start: 0,
            index_count: 6,
            width: width as f32,
            height: height as f32,
            bounds_min: SceneVec3::default(),
            bounds_max: SceneVec3 {
                x: width as f32,
                y: height as f32,
                z: 0.0,
            },
        }],
        mesh_vertices: centered_quad_vertices(width as f32, height as f32),
        mesh_indices: vec![0, 1, 2, 0, 2, 3],
        render_graphs: vec![SceneRenderGraphRecord {
            object,
            activation_policy: SceneRenderGraphActivationPolicy::Always,
            source_extent_domain: SceneRenderSourceExtentDomain::OwnerAuthored,
            pass_start: 0,
            pass_count: 3,
            unsupported_start: 0,
            unsupported_count: 0,
        }],
        render_passes: vec![
            if first_class_source {
                let mut pass = local_source_pass(
                    0,
                    object,
                    SceneRenderTargetKind::FirstClassEffectTarget,
                    false,
                    local_primitive,
                );
                pass.target_name = target_name;
                pass
            } else {
                local_source_pass(
                    0,
                    object,
                    SceneRenderTargetKind::ImageLocalMain,
                    object_local_source,
                    local_primitive,
                )
            },
            local_source_pass(
                1,
                object,
                SceneRenderTargetKind::ImageLocalSub,
                object_local_source,
                local_primitive,
            ),
            object_mesh_pass(2, object, SceneRenderTargetKind::SceneColor, false),
        ],
        image_targets: first_class_source
            .then(|| {
                vec![SceneImageTargetRecord {
                    name: target_name,
                    role: SceneRenderTargetKind::FirstClassEffectTarget,
                    format: SceneStringId::NONE,
                    extent_domain: SceneTargetExtentDomain::GraphSource,
                    width_divisor_milli: 1_000,
                    height_divisor_milli: 1_000,
                }]
            })
            .unwrap_or_default(),
        ..SceneBinaryDocument::default()
    })
    .expect("projection storage")
}

fn local_source_pass(
    id: u32,
    object: SceneObjectHandle,
    target: SceneRenderTargetKind,
    object_local_source: bool,
    local_primitive: SceneRenderPassDrawPrimitive,
) -> SceneRenderPassRecord {
    let mut pass = object_mesh_pass(id, object, target, object_local_source);
    if object_local_source {
        pass.draw_primitive = local_primitive;
    }
    pass
}

fn centered_quad_vertices(width: f32, height: f32) -> Vec<SceneMeshVertexRecord> {
    let half_width = width * 0.5;
    let half_height = height * 0.5;
    [
        ([-half_width, -half_height], [0.0, 1.0]),
        ([half_width, -half_height], [1.0, 1.0]),
        ([half_width, half_height], [1.0, 0.0]),
        ([-half_width, half_height], [0.0, 0.0]),
    ]
    .into_iter()
    .map(|(position, uv)| SceneMeshVertexRecord {
        position: SceneVec3 {
            x: position[0],
            y: position[1],
            z: 0.0,
        },
        uv,
        blend_indices: [0; 4],
        blend_weights: [0.0; 4],
    })
    .collect()
}

fn object_mesh_pass(
    id: u32,
    object: SceneObjectHandle,
    target: SceneRenderTargetKind,
    object_local_source: bool,
) -> SceneRenderPassRecord {
    SceneRenderPassRecord {
        id,
        role: if object_local_source {
            SceneRenderPassKind::ObjectLocalSource
        } else if target == SceneRenderTargetKind::SceneColor {
            SceneRenderPassKind::SceneComposite
        } else {
            SceneRenderPassKind::BaseMaterial
        },
        draw_primitive: if target == SceneRenderTargetKind::SceneColor {
            SceneRenderPassDrawPrimitive::ObjectCompositeMesh
        } else {
            SceneRenderPassDrawPrimitive::ObjectMesh
        },
        object,
        material: SceneMaterialHandle(INVALID_MATERIAL_ID),
        pass_index: 0,
        shader_key: SceneStringId::NONE,
        target,
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
        color_write_mask: SceneColorWriteMask::Rgba,
        clear_target: target != SceneRenderTargetKind::SceneColor,
    }
}
