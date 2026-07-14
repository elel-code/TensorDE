use super::*;

#[test]
fn effect_image_target_role_and_scale_follow_we_fbo_semantics() {
    assert_eq!(
        image_target_role("fbo_velocity"),
        WeIrImageTargetRole::NamedFbo
    );
    assert_eq!(
        image_target_role("_rt_QuarterCompoBuffer1"),
        WeIrImageTargetRole::FirstClassEffectTarget
    );
    assert_eq!(
        image_target_role("_tmp_GilderFramebufferCaustics"),
        WeIrImageTargetRole::Temporary
    );
    assert_eq!(scale_divisor_to_milli(4.0), 4_000);
    assert_eq!(scale_divisor_to_milli(1.0), 1_000);
}

#[test]
fn ingests_minimal_loose_scene_project() {
    let root = std::env::temp_dir().join(format!("gilder-we-ingest-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("models")).expect("models");
    fs::create_dir_all(root.join("materials")).expect("materials");
    fs::write(
        root.join("project.json"),
        r#"{"type":"scene","file":"scene.json","title":"Demo"}"#,
    )
    .expect("project");
    fs::write(
            root.join("scene.json"),
            r#"{"general":{"orthogonalprojection":{"width":1920,"height":1080}},"objects":[{"id":7,"name":"layer","image":"models/layer.json","origin":"1 2 0","animationlayers":[{"animation":475,"index":2,"additive":true,"autosort":true}]}]}"#,
        )
        .expect("scene");
    fs::write(
        root.join("models/layer.json"),
        r#"{"width":64,"height":64,"material":"materials/layer.json"}"#,
    )
    .expect("model");
    fs::write(
            root.join("materials/layer.json"),
            r#"{"passes":[{"shader":"genericimage4","blending":"translucent","textures":[null],"constantshadervalues":{"tint":[0.2,0.4,0.6,1.0]}}]}"#,
        )
        .expect("material");

    let ir = ingest_wallpaper_engine_project(&root).expect("ir");
    assert_eq!(ir.project.title, "Demo");
    assert_eq!(ir.scene.logical_width, 1920);
    assert_eq!(ir.objects.len(), 1);
    assert_eq!(ir.object_animation_layers.len(), 1);
    assert_eq!(ir.object_animation_layers[0].animation_id, 475);
    assert_eq!(ir.object_animation_layers[0].layer_index, 2);
    assert!(ir.object_animation_layers[0].additive);
    assert!(ir.object_animation_layers[0].autosort);
    assert_eq!(ir.materials.len(), 1);
    assert_eq!(ir.meshes.len(), 1);
    assert_eq!(ir.mesh_vertices.len(), 4);
    assert_eq!(ir.mesh_indices, [0, 1, 2, 0, 2, 3]);
    assert_eq!(ir.meshes[0].width, 64.0);
    assert_eq!(ir.meshes[0].height, 64.0);
    assert_eq!(ir.render_graphs.len(), 1);
    assert!(ir.render_graphs[0].passes[0].bindings.contains(
        &crate::engine::render_graph::TextureBindingRole::PassConstant {
            name: "tint".to_owned()
        }
    ));
    assert_eq!(ir.shader_contracts.len(), 1);
    assert_eq!(ir.shader_contracts[0].texture_slot_mask, 1);
    assert_eq!(ir.shader_contracts[0].resource_heap_count, 3);
    assert_eq!(ir.shader_contracts[0].sampler_heap_count, 1);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn ingests_json_puppet_descriptor_into_mdl_ir_records() {
    let root =
        std::env::temp_dir().join(format!("gilder-we-mdl-ingest-test-{}", std::process::id()));
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

fn test_mdlv0023() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"MDLV0023\0");
    push_u32(&mut bytes, 0x0180_0009);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 1);
    bytes.extend_from_slice(b"materials/puppet.json\0");
    push_u32(&mut bytes, 0);
    for value in [0.0_f32, 0.0, 0.0, 1.0, 1.0, 0.0] {
        push_f32(&mut bytes, value);
    }
    push_u32(&mut bytes, 0x0180_000f);
    let mut vertices = Vec::new();
    push_mdl_vertex(&mut vertices, [0.0, 0.0, 0.0], [0.0, 1.0]);
    push_mdl_vertex(&mut vertices, [1.0, 0.0, 0.0], [1.0, 1.0]);
    push_mdl_vertex(&mut vertices, [1.0, 1.0, 0.0], [1.0, 1.0]);
    push_u32(&mut bytes, vertices.len() as u32);
    bytes.extend_from_slice(&vertices);
    push_u32(&mut bytes, 6);
    for index in [0_u16, 1, 2] {
        bytes.extend_from_slice(&index.to_le_bytes());
    }
    bytes.extend_from_slice(b"MDLS0004");
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 1);
    bytes.extend_from_slice(b"eye-bone\0");
    bytes.extend_from_slice(&0_i32.to_le_bytes());
    push_u32(&mut bytes, u32::MAX);
    push_u32(&mut bytes, 64);
    for index in 0..16 {
        let value = if index == 0 || index == 5 || index == 10 || index == 15 {
            1.0
        } else {
            0.0
        };
        push_f32(&mut bytes, value);
    }
    bytes.extend_from_slice(b"{}\0");
    bytes.extend_from_slice(b"MDLA0006");
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 475);
    push_u32(&mut bytes, 0);
    bytes.extend_from_slice(b"blink\0");
    bytes.extend_from_slice(b"loop\0");
    push_f32(&mut bytes, 30.0);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 72);
    push_transform_sample(
        &mut bytes,
        [1.0, 2.0, 3.0],
        [0.0, 0.0, 0.0],
        [1.0, 1.0, 1.0],
    );
    push_transform_sample(
        &mut bytes,
        [4.0, 5.0, 6.0],
        [0.0, 0.0, 1.0],
        [2.0, 2.0, 2.0],
    );
    bytes.extend_from_slice(b"MDAT0001\0");
    push_u32(&mut bytes, 0);
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(b"eye\0");
    for index in 0..16 {
        let value = if index == 0 || index == 5 || index == 10 || index == 15 {
            1.0
        } else {
            0.0
        };
        push_f32(&mut bytes, value);
    }
    bytes
}

fn push_mdl_vertex(out: &mut Vec<u8>, position: [f32; 3], uv: [f32; 2]) {
    for value in position {
        push_f32(out, value);
    }
    out.resize(out.len() + 60, 0);
    push_f32(out, uv[0]);
    push_f32(out, uv[1]);
}

fn push_transform_sample(
    out: &mut Vec<u8>,
    translation: [f32; 3],
    rotation: [f32; 3],
    scale: [f32; 3],
) {
    for value in translation.into_iter().chain(rotation).chain(scale) {
        push_f32(out, value);
    }
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}
