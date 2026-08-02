// Utility-primitive and authored-geometry planning contracts.

#[test]
fn object_effect_utility_retains_semantic_transform_and_authored_source_extent() {
    let mut project = SceneBinaryDocument::default().project;
    project.logical_width = 200;
    project.logical_height = 100;
    let source_resource = SceneResourceId(7);
    let document = SceneBinaryDocument {
        project,
        strings: vec!["effects/waterwaves__SLOTS_1".to_owned()],
        resources: vec![SceneResourceRecord {
            id: source_resource,
            kind: SceneResourceKind::TextureTex,
            path: SceneStringId::NONE,
            source: SceneStringId::NONE,
            payload_offset: 0,
            payload_len: 0,
        }],
        textures: vec![SceneTextureRecord {
            resource: source_resource,
            format: SceneTextureFormat::Bc7UnormBlock,
            source_runtime_format: 0,
            payload_format: 0,
            sampler_filter: SceneTextureSamplerFilter::Anisotropic8,
            sampler_address_mode: SceneTextureSamplerAddressMode::Repeat,
            width: 1571,
            height: 2621,
            storage_width: 1572,
            storage_height: 2624,
            mip_start: 0,
            mip_count: 0,
            texv_tag: SceneStringId::NONE,
            texb_tag: SceneStringId::NONE,
            payload_offset: 0,
            payload_len: 0,
            alpha_coverage_rows: [u32::MAX;
                crate::engine::scene::SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE],
        }],
        objects: vec![SceneObjectRecord {
            id: SceneObjectHandle(0),
            we_id: 937,
            name: SceneStringId::NONE,
            kind: SceneObjectKind::Puppet,
            resource: SceneResourceId::NONE,
            material: SceneMaterialHandle(0),
            parent_we_id: INVALID_OBJECT_ID,
            attachment: SceneStringId::NONE,
            origin: SceneVec3 {
                x: 50.0,
                y: 20.0,
                z: 0.0,
            },
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
            texture_count: 1,
            constant_start: 0,
            constant_count: 0,
            pipeline_blend: ScenePipelineBlend::Normal,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_writing: SceneStringId::NONE,
            clear_target: false,
        }],
        material_textures: vec![SceneMaterialTextureRecord {
            slot: 0,
            resource: source_resource,
            path: SceneStringId::NONE,
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
            id: 1,
            role: SceneRenderPassKind::EffectMaterial,
            draw_primitive: SceneRenderPassDrawPrimitive::FullscreenTriangle,
            object: SceneObjectHandle(0),
            material: SceneMaterialHandle(0),
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
            color_write_mask: SceneColorWriteMask::Rgba,
            clear_target: false,
        }],
        ..SceneBinaryDocument::default()
    };
    let storage = SceneStorage::from_document(document).expect("storage");
    let graph = RenderingServer::new(&storage).rendering_device_graph_plan();
    let draw = graph.mesh_draws.first().expect("fullscreen effect draw");

    assert_eq!(
        draw.primitive,
        SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
    );
    assert_eq!(draw.authored_source_extent, [1571.0, 2621.0]);
    assert_eq!(draw.clip_transform[0], [0.01, 0.0, 0.0, -0.5]);
    assert_eq!(draw.clip_transform[1], [0.0, -0.02, 0.0, 0.6]);
}

#[test]
fn textureless_solid_layer_uses_authored_mesh_extent_for_local_effects() {
    let object = SceneObjectHandle(0);
    let document = SceneBinaryDocument {
        objects: vec![SceneObjectRecord {
            id: object,
            we_id: 1416,
            name: SceneStringId::NONE,
            kind: SceneObjectKind::Image,
            resource: SceneResourceId::NONE,
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
            effect_start: 0,
            effect_count: 0,
            render_graph: INVALID_OBJECT_ID,
        }],
        meshes: vec![SceneMeshRecord {
            object,
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            vertex_start: 0,
            vertex_count: 0,
            index_start: 0,
            index_count: 0,
            width: 550.0,
            height: 3300.0,
            bounds_min: SceneVec3::default(),
            bounds_max: SceneVec3::default(),
        }],
        ..SceneBinaryDocument::default()
    };
    let storage = SceneStorage::from_document(document).expect("storage");

    assert_eq!(authored_source_extent(&storage, object), [550.0, 3300.0]);
}

#[test]
fn textureless_composite_layer_allocates_image_local_targets_at_authored_extent() {
    let object = SceneObjectHandle(0);
    let document = SceneBinaryDocument {
        objects: vec![SceneObjectRecord {
            id: object,
            we_id: 1212,
            name: SceneStringId::NONE,
            kind: SceneObjectKind::Image,
            resource: SceneResourceId::NONE,
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
            effect_start: 0,
            effect_count: 0,
            render_graph: 0,
        }],
        meshes: vec![SceneMeshRecord {
            object,
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            vertex_start: 0,
            vertex_count: 0,
            index_start: 0,
            index_count: 0,
            width: 1760.0,
            height: 500.0,
            bounds_min: SceneVec3::default(),
            bounds_max: SceneVec3::default(),
        }],
        render_graphs: vec![SceneRenderGraphRecord {
            object,
            activation_policy: SceneRenderGraphActivationPolicy::Always,
            pass_start: 0,
            pass_count: 0,
            unsupported_start: 0,
            unsupported_count: 0,
        }],
        ..SceneBinaryDocument::default()
    };
    let storage = SceneStorage::from_document(document).expect("storage");
    let compatibility = target_allocation_compatibility(
        &storage,
        TargetAllocationState {
            graph_index: 0,
            target: SceneRenderTargetKind::ImageLocalMain,
            target_name: SceneStringId::NONE,
            first_write_pass_id: 0,
            last_use_pass_id: 0,
            first_write_order: 0,
            last_use_order: 0,
        },
    );

    assert_eq!(compatibility.target_width, 1760);
    assert_eq!(compatibility.target_height, 500);
    assert_eq!(
        compatibility.extent_domain,
        SceneTargetExtentDomain::OwnerAuthored
    );
}

#[test]
fn owner_authored_effect_fbo_scales_from_its_graph_owner_not_the_surface() {
    let object = SceneObjectHandle(0);
    let target_name = SceneStringId(0);
    let document = SceneBinaryDocument {
        strings: vec!["_rt_QuarterCompoBuffer1".to_owned(), "rgba8".to_owned()],
        objects: vec![SceneObjectRecord {
            id: object,
            we_id: 384,
            name: SceneStringId::NONE,
            kind: SceneObjectKind::Image,
            resource: SceneResourceId::NONE,
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
            effect_start: 0,
            effect_count: 0,
            render_graph: 0,
        }],
        meshes: vec![SceneMeshRecord {
            object,
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            vertex_start: 0,
            vertex_count: 0,
            index_start: 0,
            index_count: 0,
            width: 3840.0,
            height: 2160.0,
            bounds_min: SceneVec3::default(),
            bounds_max: SceneVec3::default(),
        }],
        render_graphs: vec![SceneRenderGraphRecord {
            object,
            activation_policy: SceneRenderGraphActivationPolicy::Always,
            pass_start: 0,
            pass_count: 0,
            unsupported_start: 0,
            unsupported_count: 0,
        }],
        image_targets: vec![SceneImageTargetRecord {
            name: target_name,
            role: SceneRenderTargetKind::FirstClassEffectTarget,
            format: SceneStringId(1),
            extent_domain: SceneTargetExtentDomain::OwnerAuthored,
            width_divisor_milli: 4_000,
            height_divisor_milli: 4_000,
        }],
        ..SceneBinaryDocument::default()
    };
    let storage = SceneStorage::from_document(document).expect("storage");
    let compatibility = target_allocation_compatibility(
        &storage,
        TargetAllocationState {
            graph_index: 0,
            target: SceneRenderTargetKind::FirstClassEffectTarget,
            target_name,
            first_write_pass_id: 0,
            last_use_pass_id: 0,
            first_write_order: 0,
            last_use_order: 0,
        },
    );

    assert_eq!(compatibility.target_width, 960);
    assert_eq!(compatibility.target_height, 540);
    assert_eq!(
        compatibility.extent_domain,
        SceneTargetExtentDomain::OwnerAuthored
    );
}

#[test]
fn explicit_object_and_object_composite_meshes_draw_authored_geometry() {
    let mut pass = named_fbo_pass(5, 0, SceneStringId(1), 0, 0);
    pass.object = SceneObjectHandle(7);
    assert!(!pass_draws_object_mesh(&pass));

    pass.draw_primitive = SceneRenderPassDrawPrimitive::FullscreenTriangle;
    assert!(!pass_draws_object_mesh(&pass));

    pass.role = SceneRenderPassKind::BaseMaterial;
    assert!(!pass_draws_object_mesh(&pass));

    pass.draw_primitive = SceneRenderPassDrawPrimitive::ObjectMesh;
    assert!(pass_draws_object_mesh(&pass));

    pass.draw_primitive = SceneRenderPassDrawPrimitive::ObjectCompositeMesh;
    assert!(pass_draws_object_mesh(&pass));
}

fn named_fbo_pass(
    id: u32,
    pass_index: u32,
    target_name: SceneStringId,
    binding_start: u32,
    binding_count: u32,
) -> SceneRenderPassRecord {
    SceneRenderPassRecord {
        id,
        role: SceneRenderPassKind::EffectMaterial,
        draw_primitive: SceneRenderPassDrawPrimitive::None,
        object: SceneObjectHandle(INVALID_OBJECT_ID),
        material: SceneMaterialHandle(INVALID_MATERIAL_ID),
        pass_index,
        shader_key: SceneStringId::NONE,
        target: SceneRenderTargetKind::NamedFbo,
        target_name,
        binding_start,
        binding_count,
        effect_binding_start: u32::MAX,
        effect_binding_count: 0,
        effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
        pipeline_blend: ScenePipelineBlend::Normal,
        scene_blend: SceneCompositeBlend::Alpha,
        depth_test: SceneDepthTest::Disabled,
        depth_write: false,
        cull_mode: SceneCullMode::None,
        color_write_mask: SceneColorWriteMask::Rgba,
        clear_target: false,
    }
}

fn scene_color_pass_reading_fbo(
    id: u32,
    binding_start: u32,
    binding_count: u32,
) -> SceneRenderPassRecord {
    SceneRenderPassRecord {
        target: SceneRenderTargetKind::SceneColor,
        target_name: SceneStringId::NONE,
        ..named_fbo_pass(id, 2, SceneStringId::NONE, binding_start, binding_count)
    }
}

fn named_fbo_binding(name: SceneStringId, slot: u32) -> SceneRenderBindingRecord {
    SceneRenderBindingRecord {
        kind: SceneRenderBindingKind::NamedFboBind,
        slot,
        target: SceneRenderTargetKind::NamedFbo,
        name,
    }
}
