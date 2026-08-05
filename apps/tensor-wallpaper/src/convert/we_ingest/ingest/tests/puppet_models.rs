use super::*;

#[test]
fn ingests_json_puppet_descriptor_into_mdl_ir_records() {
    let root = std::env::temp_dir().join(format!(
        "tensor-wallpaper-we-mdl-ingest-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("models")).expect("models");
    fs::create_dir_all(root.join("materials")).expect("materials");
    fs::write(
        root.join("project.json"),
        r#"{"type":"scene","file":"scene.json","title":"MDL Demo"}"#,
    )
    .expect("project");
    fs::write(
            root.join("scene.json"),
            r#"{"objects":[{"id":9,"name":"puppet","image":"models/puppet.json","color":"0.1 0.2 0.3","alpha":0.4}]}"#,
        )
        .expect("scene");
    fs::write(
        root.join("models/puppet.json"),
        r#"{"material":"materials/puppet.json","puppet":"models/puppet.mdl"}"#,
    )
    .expect("model");
    fs::write(root.join("models/puppet.mdl"), test_mdlv0023()).expect("mdl");
    fs::write(
        root.join("materials/puppet.json"),
        r#"{"passes":[{"shader":"genericimage4","textures":[null]}]}"#,
    )
    .expect("material");

    let ir = ingest_wallpaper_engine_project(&root).expect("ir");

    assert_eq!(ir.objects[0].kind, SceneAbiObjectKind::Puppet);
    assert_eq!(ir.objects[0].material, Some(0));
    assert_eq!(
        ir.objects[0].color,
        SceneVec3 {
            x: 0.1,
            y: 0.2,
            z: 0.3
        }
    );
    assert_eq!(ir.objects[0].alpha, 0.4);
    assert_eq!(ir.materials.len(), 1);
    assert_eq!(ir.meshes.len(), 1);
    assert_eq!(ir.meshes[0].vertex_count, 3);
    assert_eq!(ir.meshes[0].index_count, 3);
    assert_eq!(ir.mesh_indices, [0, 1, 2]);
    assert_eq!(ir.mesh_vertices[2].position.x, 1.0);
    assert_eq!(ir.mesh_vertices[2].uv, [1.0, 1.0]);
    assert_eq!(ir.puppets.len(), 1);
    assert_eq!(ir.puppets[0].mesh_count, 1);
    assert_eq!(ir.puppets[0].attachment_count, 1);
    assert_eq!(ir.puppet_attachments[0].bone_index, 0);
    assert_eq!(ir.puppet_attachments[0].name, "eye");
    assert_eq!(ir.puppet_animation_clips.len(), 1);
    assert_eq!(ir.puppet_animation_clips[0].clip_id, 475);
    assert_eq!(ir.puppet_animation_tracks.len(), 1);
    assert_eq!(ir.puppet_animation_tracks[0].bone_index, 0);
    assert_eq!(ir.puppet_animation_transform_samples.len(), 2);
    assert_eq!(ir.puppet_animation_transform_samples[1].translation.x, 4.0);
    assert_eq!(ir.render_graphs.len(), 1);
    assert_eq!(ir.shader_contracts.len(), 1);
    assert!(ir.unsupported.is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lowers_direct_genericimage_puppet_token_one_clipping_graph() {
    let root = std::env::temp_dir().join(format!(
        "tensor-wallpaper-we-direct-puppet-clipping-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("models")).expect("models");
    fs::create_dir_all(root.join("materials")).expect("materials");
    fs::create_dir_all(root.join("masks")).expect("masks");
    fs::write(
        root.join("project.json"),
        r#"{"type":"scene","file":"scene.json","title":"Direct Puppet Clipping"}"#,
    )
    .expect("project");
    fs::write(
        root.join("scene.json"),
        r#"{"objects":[{"id":9,"image":"models/clipped.json"}]}"#,
    )
    .expect("scene");
    fs::write(
        root.join("models/clipped.json"),
        r#"{"material":"materials/clipped.json","puppet":"models/clipped.mdl"}"#,
    )
    .expect("model");
    fs::write(root.join("models/clipped.mdl"), test_clipped_mdlv0023()).expect("mdl");
    fs::write(
        root.join("materials/clipped.json"),
        r#"{"passes":[{"shader":"genericimage4","textures":[null],"cullmode":"normal"}]}"#,
    )
    .expect("material");
    fs::write(root.join("masks/eye-clip.png"), b"raw-mask-fixture").expect("mask");

    let ir = ingest_wallpaper_engine_project(&root).expect("ir");

    assert!(ir.unsupported.is_empty(), "{:?}", ir.unsupported);
    assert_eq!(ir.render_graphs.len(), 1);
    let graph = &ir.render_graphs[0];
    assert!(graph.unsupported.is_empty(), "{:?}", graph.unsupported);
    assert_eq!(
        graph
            .passes
            .iter()
            .map(|pass| pass.role)
            .collect::<Vec<_>>(),
        [
            RenderPassRole::MeshVisiblePrefix,
            RenderPassRole::MeshClippingMask,
            RenderPassRole::MeshClippedTarget,
            RenderPassRole::MeshVisibleRemainder,
            RenderPassRole::MeshClippingMask,
            RenderPassRole::MeshClippedTarget,
            RenderPassRole::MeshVisibleRemainder,
        ]
    );
    assert_eq!(
        graph.passes[1].shader.as_deref(),
        Some("we/clippingmaskimage4__PUPPETSKINNING_1")
    );
    assert_eq!(
        graph.passes[2].shader.as_deref(),
        Some("we/genericimage4__PUPPETSKINNING_1__CLIPPINGTARGET_1__CLIPPINGUVS_1")
    );
    assert_eq!(
        graph.passes[1].target_name.as_deref(),
        Some("_rt_FullAlphaMask")
    );
    assert_eq!(
        graph.passes[1].state.pipeline_blend,
        PipelineBlendMode::Translucent
    );
    let mask_producers = graph
        .passes
        .iter()
        .filter(|pass| pass.role == RenderPassRole::MeshClippingMask)
        .collect::<Vec<_>>();
    assert_eq!(mask_producers.len(), 2);
    assert!(mask_producers.iter().all(|pass| pass.state.clear_target));
    assert!(
        graph
            .passes
            .iter()
            .all(|pass| pass.state.cull_mode == CullMode::Back)
    );
    assert_eq!(
        graph.passes[2].state.pipeline_blend,
        PipelineBlendMode::Translucent
    );
    assert_eq!(
        graph.passes[2].state.color_write_mask,
        crate::engine::render_graph::ColorWriteMask::Rgb
    );
    let mask_material = graph.passes[1]
        .material_index
        .and_then(|index| ir.materials.get(index))
        .expect("mask material");
    assert_eq!(
        ir.material_passes[mask_material.pass_start as usize].pipeline_blend,
        ScenePipelineBlend::Translucent
    );
    assert_eq!(
        ir.material_passes[mask_material.pass_start as usize].cull_mode,
        SceneCullMode::Normal
    );
    assert!(
        graph.passes[2]
            .bindings
            .contains(&TextureBindingRole::GraphTarget {
                slot: 8,
                role: RenderTargetRole::FirstClassEffectTarget,
                name: Some("_rt_FullAlphaMask".to_owned()),
            })
    );
    assert_eq!(graph.target_specs.len(), 1);
    assert_eq!(graph.target_specs[0].name, "_rt_FullAlphaMask");
    assert_eq!(graph.target_specs[0].format, "r8");
    assert_eq!(graph.target_specs[0].width_divisor_milli, 2_000);
    assert_eq!(graph.target_specs[0].height_divisor_milli, 2_000);
    assert_eq!(
        ir.mesh_clipping_slices
            .iter()
            .map(|slice| slice.role)
            .collect::<Vec<_>>(),
        [
            WeIrMeshClippingSliceRole::VisiblePrefix,
            WeIrMeshClippingSliceRole::MaskProducer,
            WeIrMeshClippingSliceRole::ClippedTarget,
            WeIrMeshClippingSliceRole::VisibleRemainder,
            WeIrMeshClippingSliceRole::MaskProducer,
            WeIrMeshClippingSliceRole::ClippedTarget,
            WeIrMeshClippingSliceRole::VisibleRemainder,
        ]
    );
    assert_eq!(graph.passes[3].pass_index, 0);
    assert_eq!(graph.passes[6].pass_index, 1);
    assert_eq!(
        ir.mesh_clipping_slices
            .iter()
            .map(|slice| slice.index_count)
            .collect::<Vec<_>>(),
        [6, 3, 3, 6, 3, 3, 6]
    );
    let clipping_contract = ir
        .shader_contracts
        .iter()
        .find(|contract| {
            contract.shader_key
                == "we/genericimage4__PUPPETSKINNING_1__CLIPPINGTARGET_1__CLIPPINGUVS_1"
        })
        .expect("generated clipping-target shader contract");
    assert_eq!(clipping_contract.texture_slot_mask, 0x101);
    assert_eq!(clipping_contract.resource_heap_count, 4);
    assert_eq!(clipping_contract.sampler_heap_count, 2);

    fs::write(
        root.join("materials/clipped.json"),
        r#"{"passes":[{"shader":"genericimage2","textures":[null]}]}"#,
    )
    .expect("unsupported material");
    let unsupported_ir = ingest_wallpaper_engine_project(&root).expect("unsupported ir");
    assert!(unsupported_ir.unsupported.is_empty());
    assert_eq!(unsupported_ir.render_graphs[0].passes.len(), 1);
    assert_eq!(unsupported_ir.render_graphs[0].unsupported.len(), 1);
    assert!(
        unsupported_ir.render_graphs[0].unsupported[0]
            .feature
            .starts_with("mdlv0023-token-one-clipping-unsupported-shader:")
    );

    let _ = fs::remove_dir_all(root);
}
