use super::*;

#[test]
fn lowers_authored_puppet_effect_stream_token_one_clipping_graph() {
    let root = std::env::temp_dir().join(format!(
        "tensor-wallpaper-we-puppet-effect-stream-clipping-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("models")).expect("models");
    fs::create_dir_all(root.join("materials/effects")).expect("materials");
    fs::create_dir_all(root.join("effects/iris")).expect("iris effect");
    fs::create_dir_all(root.join("effects/waterripple")).expect("ripple effect");
    fs::create_dir_all(root.join("masks")).expect("masks");
    fs::write(
        root.join("project.json"),
        r#"{"type":"scene","file":"scene.json","title":"Puppet Effect Stream Clipping"}"#,
    )
    .expect("project");
    fs::write(
        root.join("scene.json"),
        r#"{"objects":[{"id":9,"image":"models/clipped.json","effects":[{"file":"effects/iris/effect.json","id":40},{"file":"effects/waterripple/effect.json","id":41,"visible":{"value":false}}]}]}"#,
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
        r#"{"passes":[{"shader":"genericimage4","textures":[null],"cullmode":"nocull"}]}"#,
    )
    .expect("base material");
    fs::write(
        root.join("effects/iris/effect.json"),
        r#"{"passes":[{"material":"materials/effects/iris.json","bind":[{"index":0,"target":"previous"}]}]}"#,
    )
    .expect("iris effect");
    fs::write(
        root.join("materials/effects/iris.json"),
        r#"{"passes":[{"shader":"effects/iris","blending":"normal"}]}"#,
    )
    .expect("iris material");
    fs::write(
        root.join("effects/waterripple/effect.json"),
        r#"{"passes":[{"material":"materials/effects/waterripple.json","bind":[{"index":0,"target":"previous"}]}]}"#,
    )
    .expect("ripple effect");
    fs::write(
        root.join("materials/effects/waterripple.json"),
        r#"{"passes":[{"shader":"effects/waterripple","blending":"normal"}]}"#,
    )
    .expect("ripple material");
    fs::write(root.join("masks/eye-clip.png"), b"raw-mask-fixture").expect("mask");

    let ir = ingest_wallpaper_engine_project(&root).expect("ir");

    assert!(ir.unsupported.is_empty(), "{:?}", ir.unsupported);
    let graph = &ir.render_graphs[0];
    assert!(graph.unsupported.is_empty(), "{:?}", graph.unsupported);
    assert_eq!(graph.passes.len(), 17);
    assert_eq!(graph.passes[0].role, RenderPassRole::ObjectLocalSource);
    assert_eq!(graph.passes[1].role, RenderPassRole::EffectMaterial);
    assert_eq!(graph.passes[2].role, RenderPassRole::EffectMaterial);
    assert_eq!(graph.passes[3].role, RenderPassRole::MeshVisiblePrefix);
    assert_eq!(graph.passes[9].role, RenderPassRole::MeshVisibleRemainder);
    assert_eq!(graph.passes[10].role, RenderPassRole::MeshVisiblePrefix);
    assert_eq!(graph.passes[16].role, RenderPassRole::MeshVisibleRemainder);
    assert_eq!(graph.passes[0].state.cull_mode, CullMode::None);
    assert!(graph.passes[3..10].iter().all(|pass| {
        matches!(pass.role, RenderPassRole::MeshClippingMask)
            == (pass.state.cull_mode == CullMode::Back)
    }));
    assert!(graph.passes[10..17].iter().all(|pass| {
        matches!(pass.role, RenderPassRole::MeshClippingMask)
            == (pass.state.cull_mode == CullMode::Back)
    }));
    assert!(graph.passes[3..10].iter().all(|pass| {
        pass.effect_visibility
            == crate::engine::render_graph::RenderPassEffectVisibility::any_visible(1, 1)
    }));
    assert!(graph.passes[10..17].iter().all(|pass| {
        pass.effect_visibility
            == crate::engine::render_graph::RenderPassEffectVisibility::none_visible(1, 1)
    }));
    let clipped_targets = graph
        .passes
        .iter()
        .filter(|pass| pass.role == RenderPassRole::MeshClippedTarget)
        .collect::<Vec<_>>();
    assert_eq!(clipped_targets.len(), 4);
    assert!(clipped_targets.iter().all(|pass| {
        pass.shader.as_deref() == Some("we/puppet-effect-composite-clipping")
            && pass.bindings.contains(&TextureBindingRole::GraphTarget {
                slot: 8,
                role: RenderTargetRole::FirstClassEffectTarget,
                name: Some("_rt_FullAlphaMask".to_owned()),
            })
            && !pass
                .bindings
                .iter()
                .any(|binding| matches!(binding, TextureBindingRole::PreviousGraphTarget { .. }))
    }));
    let mask_materials = graph
        .passes
        .iter()
        .filter(|pass| pass.role == RenderPassRole::MeshClippingMask)
        .filter_map(|pass| pass.material_index)
        .collect::<std::collections::BTreeSet<_>>();
    assert!(!mask_materials.is_empty());
    assert!(mask_materials.into_iter().all(|material| {
        let pass_start = ir.materials[material].pass_start as usize;
        ir.material_passes[pass_start].cull_mode == SceneCullMode::Normal
    }));
    assert!(clipped_targets.iter().any(|pass| {
        pass.bindings.contains(&TextureBindingRole::GraphTarget {
            slot: 0,
            role: RenderTargetRole::ImageLocalSub,
            name: None,
        })
    }));
    assert!(clipped_targets.iter().any(|pass| {
        pass.bindings.contains(&TextureBindingRole::GraphTarget {
            slot: 0,
            role: RenderTargetRole::ImageLocalMain,
            name: None,
        })
    }));
    let clipping_contract = ir
        .shader_contracts
        .iter()
        .find(|contract| contract.shader_key == "we/puppet-effect-composite-clipping")
        .expect("puppet effect clipping shader contract");
    assert_eq!(clipping_contract.texture_slot_mask, 0x101);
    assert_eq!(clipping_contract.resource_heap_count, 4);
    assert_eq!(clipping_contract.sampler_heap_count, 2);
    let document = crate::convert::we_ingest::lower::lower_ir_to_scene_binary(&ir)
        .expect("lower puppet effect clipping graph");
    crate::engine::scene::SceneStorage::from_document(document)
        .expect("validate puppet effect clipping storage");

    let _ = fs::remove_dir_all(root);
}
