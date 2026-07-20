use super::*;
use crate::engine::render_graph::{
    PipelineBlendMode, RenderPassRole, RenderTargetRole, TextureBindingRole,
};
use crate::engine::scene::ScenePipelineBlend;

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
            r#"{"general":{"orthogonalprojection":{"width":1920,"height":1080},"cameraparallax":true,"cameraparallaxamount":0.5,"cameraparallaxdelay":0.1,"cameraparallaxmouseinfluence":0.5},"objects":[{"id":7,"name":"layer","image":"models/layer.json","origin":"1 2 0","parallaxDepth":"-0.2 -0.1","animationlayers":[{"animation":475,"index":2,"additive":true,"autosort":true}]}]}"#,
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
    assert!(ir.scene.camera_parallax_enabled);
    assert_eq!(ir.scene.camera_parallax_amount, 0.5);
    assert_eq!(ir.objects.len(), 1);
    assert_eq!(ir.objects[0].parallax_depth, [-0.2, -0.1]);
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
fn literal_disabled_effects_are_pruned_from_rendering_but_retained_semantically() {
    let root = write_effect_visibility_fixture(
        "literal-disabled",
        serde_json::json!([
            {"file":"effects/waterwaves/effect.json","id":10,"visible":true},
            {"file":"effects/waterwaves/effect.json","id":11,"visible":false},
            {"file":"effects/waterwaves/effect.json","id":12}
        ]),
        None,
        false,
    );

    let ir = ingest_wallpaper_engine_project(&root).expect("literal effect visibility IR");

    assert_eq!(
        ir.object_effects
            .iter()
            .map(|effect| effect.visible)
            .collect::<Vec<_>>(),
        [true, false, true]
    );
    assert_eq!(ir.render_graphs[0].passes.len(), 3);
    assert_eq!(
        ir.render_graphs[0]
            .passes
            .iter()
            .map(|pass| pass.role)
            .collect::<Vec<_>>(),
        [
            RenderPassRole::BaseMaterial,
            RenderPassRole::EffectMaterial,
            RenderPassRole::SceneComposite,
        ]
    );
    assert_eq!(
        ir.render_graphs[0].passes[1].shader.as_deref(),
        Some("we/effect-waterwaves-direct__STAGES_2")
    );
    assert_eq!(
        ir.render_graphs[0].passes[1].effect_visibility,
        crate::engine::render_graph::RenderPassEffectVisibility::NONE
    );
    let document =
        crate::convert::we_ingest::lower::lower_ir_to_scene_binary(&ir).expect("lower static IR");
    assert_eq!(document.object_effects.len(), 3);
    assert_eq!(
        document.render_passes[1].effect_visibility_policy,
        crate::engine::scene::SceneRenderEffectVisibilityPolicy::None
    );
    assert_eq!(document.render_passes[1].effect_binding_start, u32::MAX);
    assert_eq!(document.render_passes[1].effect_binding_count, 0);
    crate::engine::scene::SceneStorage::from_document(document)
        .expect("validate static effect visibility storage");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn bound_effect_visibility_keeps_the_complete_runtime_stage_range() {
    let root = write_effect_visibility_fixture(
        "bound-disabled",
        serde_json::json!([
            {"file":"effects/waterwaves/effect.json","id":20,"visible":true},
            {"file":"effects/waterwaves/effect.json","id":21,"visible":{"value":false}},
            {"file":"effects/waterwaves/effect.json","id":22,"visible":true}
        ]),
        None,
        false,
    );

    let ir = ingest_wallpaper_engine_project(&root).expect("bound effect visibility IR");
    let visibility = ir.render_graphs[0].passes[0].effect_visibility;

    assert_eq!(ir.object_effects.len(), 3);
    assert_eq!(
        visibility.policy,
        crate::engine::render_graph::RenderPassEffectVisibilityPolicy::WaterWavesStages
    );
    assert_eq!(visibility.binding_start, 0);
    assert_eq!(visibility.binding_count, 3);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn effect_visibility_script_keeps_literal_effects_runtime_addressable() {
    let root = write_effect_visibility_fixture(
        "script-visible",
        serde_json::json!([
            {"file":"effects/waterwaves/effect.json","id":30,"visible":true},
            {"file":"effects/waterwaves/effect.json","id":31,"visible":false},
            {"file":"effects/waterwaves/effect.json","id":32,"visible":true}
        ]),
        Some(
            "export function update(value) { thisScene.getLayer('body').getEffect('middle').visible = value; return value; }",
        ),
        false,
    );

    let ir = ingest_wallpaper_engine_project(&root).expect("script effect visibility IR");
    let visibility = ir.render_graphs[0].passes[0].effect_visibility;

    assert_eq!(ir.object_effects.len(), 3);
    assert_eq!(
        visibility.policy,
        crate::engine::render_graph::RenderPassEffectVisibilityPolicy::WaterWavesStages
    );
    assert_eq!(visibility.binding_count, 3);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn literal_disabled_effect_only_layer_records_no_render_work() {
    let root = write_effect_visibility_fixture(
        "effect-only-disabled",
        serde_json::json!([
            {"file":"effects/waterwaves/effect.json","id":40,"visible":false}
        ]),
        None,
        true,
    );

    let ir = ingest_wallpaper_engine_project(&root).expect("disabled effect-only IR");

    assert_eq!(ir.object_effects.len(), 1);
    assert!(!ir.object_effects[0].visible);
    assert!(ir.render_graphs[0].passes.is_empty());
    assert!(
        ir.image_targets
            .iter()
            .all(|target| target.name != utility_layer::FULL_FRAMEBUFFER_TARGET)
    );
    let document = crate::convert::we_ingest::lower::lower_ir_to_scene_binary(&ir)
        .expect("lower disabled effect-only IR");
    crate::engine::scene::SceneStorage::from_document(document)
        .expect("validate disabled effect-only storage");

    let _ = fs::remove_dir_all(root);
}

fn write_effect_visibility_fixture(
    name: &str,
    effects: Value,
    script: Option<&str>,
    effect_only: bool,
) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "gilder-we-effect-visibility-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("effects/waterwaves")).expect("effects");
    fs::create_dir_all(root.join("materials/effects")).expect("effect materials");
    fs::create_dir_all(root.join("models")).expect("models");
    fs::create_dir_all(root.join("materials")).expect("materials");
    fs::write(
        root.join("project.json"),
        r#"{"type":"scene","file":"scene.json","title":"Effect visibility"}"#,
    )
    .expect("project");
    fs::write(
        root.join("effects/waterwaves/effect.json"),
        r#"{"passes":[{"material":"materials/effects/waterwaves.json"}]}"#,
    )
    .expect("effect");
    fs::write(
        root.join("materials/effects/waterwaves.json"),
        r#"{"passes":[{"shader":"effects/waterwaves","blending":"normal"}]}"#,
    )
    .expect("effect material");
    fs::write(
        root.join("models/layer.json"),
        r#"{"width":64,"height":64,"material":"materials/layer.json"}"#,
    )
    .expect("model");
    fs::write(
        root.join("materials/layer.json"),
        r#"{"passes":[{"shader":"genericimage4","blending":"translucent","textures":[null]}]}"#,
    )
    .expect("material");

    let mut object = serde_json::json!({
        "id": 7,
        "name": "body",
        "image": if effect_only {
            "models/util/composelayer.json"
        } else {
            "models/layer.json"
        },
        "effects": effects,
    });
    if let Some(script) = script {
        object["visible"] = serde_json::json!({"value": true, "script": script});
    }
    let scene = serde_json::json!({"objects": [object]});
    fs::write(
        root.join("scene.json"),
        serde_json::to_vec(&scene).expect("scene JSON"),
    )
    .expect("scene");
    root
}

#[test]
fn ingests_falling_leaves_as_typed_particle_ir_and_part_record() {
    let root = std::env::temp_dir().join(format!("gilder-we-particle-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("particles")).expect("particles");
    fs::create_dir_all(root.join("materials/particle")).expect("materials");
    fs::write(
        root.join("project.json"),
        r#"{"type":"scene","file":"scene.json","title":"Leaves"}"#,
    )
    .expect("project");
    fs::write(
        root.join("scene.json"),
        r#"{"general":{"orthogonalprojection":{"width":3840,"height":2160}},"objects":[{"id":199,"name":"leaf","particle":"particles/leaf.json","origin":"-1075 -370 0","scale":"3.27 3.27 3.27"}]}"#,
    )
    .expect("scene");
    fs::write(
        root.join("particles/leaf.json"),
        r#"{
            "children":[],
            "controlpoint":[{"id":0,"angles":null}],
            "emitter":[{"id":9,"name":"sphererandom","origin":"350 750 0","directions":"1 1 1","distancemin":0,"distancemax":750,"rate":5}],
            "initializer":[
                {"id":2,"name":"lifetimerandom","min":8,"max":10},
                {"id":3,"name":"sizerandom","min":70,"max":75},
                {"id":4,"name":"velocityrandom","min":"-100 -100 0","max":"-50 -15 0"},
                {"id":5,"name":"colorrandom","min":"255 255 255"},
                {"id":6,"name":"rotationrandom"},
                {"id":7,"name":"turbulentvelocityrandom","offset":3,"scale":0.5,"speedmin":35,"speedmax":100},
                {"id":8,"name":"angularvelocityrandom","min":"0 0 -1","max":"0 0 1"}
            ],
            "material":"materials/particle/leaf.json",
            "maxcount":11,
            "operator":[{"id":10,"name":"movement"},{"id":11,"name":"alphafade","fadeintime":0.1,"fadeouttime":0.9},{"id":12,"name":"angularmovement"}],
            "renderer":[{"id":1,"name":"sprite"}],
            "sequencemultiplier":3,
            "starttime":3
        }"#,
    )
    .expect("particle");
    fs::write(
        root.join("materials/particle/leaf.json"),
        r#"{"passes":[{"shader":"genericparticle","blending":"translucent","textures":[null]}]}"#,
    )
    .expect("material");

    let ir = ingest_wallpaper_engine_project(&root).expect("particle IR");
    assert_eq!(ir.objects[0].kind, SceneAbiObjectKind::ParticleEmitter);
    assert_eq!(ir.particles.len(), 1);
    let profile = ir.particles[0]
        .falling_leaves_profile()
        .expect("falling leaves specialization");
    assert_eq!(profile.rate, 5.0);
    assert_eq!(profile.lifetime_min, 8.0);
    assert_eq!(profile.size_max, 75.0);
    assert_eq!(
        ir.render_graphs[0].passes[0].role,
        crate::engine::render_graph::RenderPassRole::Particle
    );

    let document =
        crate::convert::we_ingest::lower::lower_ir_to_scene_binary(&ir).expect("lower particle IR");
    assert_eq!(document.particles.len(), 1);
    assert_eq!(
        document.particles[0].simulation,
        crate::engine::scene::SceneParticleSimulationKind::FallingLeaves
    );
    assert_eq!(document.particles[0].max_count, 11);
    assert_eq!(document.particles[0].size_min, 35.0);
    assert_eq!(document.particles[0].size_max, 37.5);
    assert_eq!(document.particles[0].color_min.x, 1.0);
    let mut bytes = Vec::new();
    crate::engine::scene::write_scene_binary(&document, &mut bytes).expect("write PART");
    let decoded = crate::engine::scene::read_scene_binary_bytes(&bytes).expect("read PART");
    assert_eq!(decoded.particles, document.particles);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn ingests_ambient_sparkles_as_typed_particle_ir_and_part_record() {
    let root = std::env::temp_dir().join(format!("gilder-we-sparkles-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("particles")).expect("particles");
    fs::create_dir_all(root.join("materials/particle")).expect("materials");
    fs::write(
        root.join("project.json"),
        r#"{"type":"scene","file":"scene.json","title":"Sparkles"}"#,
    )
    .expect("project");
    fs::write(
        root.join("scene.json"),
        r#"{"general":{"orthogonalprojection":{"width":3840,"height":2160}},"objects":[{"id":927,"name":"stars","particle":"particles/stars.json","origin":"1920 1080 0","instanceoverride":{"colorn":"0.83922 1.00000 1.00000"}}]}"#,
    )
    .expect("scene");
    fs::write(
        root.join("particles/stars.json"),
        r#"{
            "children":[],
            "emitter":[{"id":5,"name":"boxrandom","directions":"2 2 0","distancemax":"1000 500 0","rate":200}],
            "initializer":[
                {"id":2,"name":"lifetimerandom","min":4,"max":8},
                {"id":3,"name":"sizerandom","min":5,"max":10},
                {"id":4,"name":"colorrandom","min":"255 255 255"}
            ],
            "material":"materials/particle/halo.json",
            "maxcount":3000,
            "operator":[
                {"id":6,"name":"alphafade","fadeintime":0.1,"fadeouttime":0.9},
                {"id":7,"name":"oscillatealpha","frequencymax":14,"scalemin":0.2},
                {"id":8,"name":"movement","gravity":"0 -2 0"}
            ],
            "renderer":[{"id":1,"name":"sprite"}]
        }"#,
    )
    .expect("particle");
    fs::write(
        root.join("materials/particle/halo.json"),
        r#"{"passes":[{"shader":"genericparticle","blending":"translucent","textures":[null]}]}"#,
    )
    .expect("material");

    let ir = ingest_wallpaper_engine_project(&root).expect("sparkles IR");
    let profile = ir.particles[0]
        .ambient_sparkles_profile()
        .expect("ambient sparkles specialization");
    assert_eq!(profile.rate, 200.0);
    assert_eq!(profile.oscillation_frequency_min, 1.0);
    assert_eq!(profile.oscillation_frequency_max, 14.0);
    assert_eq!(profile.oscillation_phase_min, 0.0);
    assert_eq!(profile.oscillation_phase_max, std::f32::consts::TAU);
    assert_eq!(profile.oscillation_scale_min, 0.2);
    assert_eq!(profile.oscillation_scale_max, 1.0);
    assert_eq!(ir.particles[0].instance_time_scale, 0.01);

    let document =
        crate::convert::we_ingest::lower::lower_ir_to_scene_binary(&ir).expect("lower sparkles");
    assert_eq!(
        document.particles[0].simulation,
        crate::engine::scene::SceneParticleSimulationKind::AmbientSparkles
    );
    assert_eq!(document.particles[0].size_min, 2.5);
    assert_eq!(document.particles[0].size_max, 5.0);
    assert_eq!(document.particles[0].instance_time_scale, 0.01);
    let mut bytes = Vec::new();
    crate::engine::scene::write_scene_binary(&document, &mut bytes).expect("write PART");
    let decoded = crate::engine::scene::read_scene_binary_bytes(&bytes).expect("read PART");
    assert_eq!(decoded.particles, document.particles);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn ingests_floral_oscillation_as_typed_particle_ir_and_part_record() {
    let root = std::env::temp_dir().join(format!(
        "gilder-we-floral-particle-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("particles")).expect("particles");
    fs::create_dir_all(root.join("materials/particle")).expect("materials");
    fs::write(
        root.join("project.json"),
        r#"{"type":"scene","file":"scene.json","title":"Floral"}"#,
    )
    .expect("project");
    fs::write(
        root.join("scene.json"),
        r#"{"general":{"orthogonalprojection":{"width":3840,"height":2160}},"objects":[{"id":711,"name":"flowers","particle":"particles/flowers.json","origin":"1920 1080 0"}]}"#,
    )
    .expect("scene");
    fs::write(
        root.join("particles/flowers.json"),
        r#"{
            "emitter":[{"id":5,"name":"boxrandom","distancemax":"1800 750 0","rate":150}],
            "initializer":[
                {"id":2,"name":"sizerandom","min":150,"max":400},
                {"id":3,"name":"rotationrandom"},
                {"id":4,"name":"lifetimerandom","min":1000000,"max":1000000}
            ],
            "material":"materials/particle/flowers.json",
            "maxcount":500,
            "operator":[
                {"id":6,"name":"oscillateposition","frequencymax":1,"scalemin":1,"scalemax":2},
                {"id":7,"name":"oscillatesize","frequencymax":1,"phasemax":1,"scalemin":1,"scalemax":1.05}
            ],
            "renderer":[{"id":1,"name":"sprite"}]
        }"#,
    )
    .expect("particle");
    fs::write(
        root.join("materials/particle/flowers.json"),
        r#"{"passes":[{"shader":"genericparticle","blending":"translucent","textures":[null]}]}"#,
    )
    .expect("material");

    let ir = ingest_wallpaper_engine_project(&root).expect("floral IR");
    let profile = ir.particles[0]
        .floral_oscillation_profile()
        .expect("floral oscillation specialization");
    assert_eq!(profile.rate, 150.0);
    assert_eq!(profile.position_frequency_min, 0.0);
    assert_eq!(profile.position_frequency_max, 1.0);
    assert_eq!(profile.position_scale_min, 1.0);
    assert_eq!(profile.position_scale_max, 2.0);
    assert_eq!(
        profile.position_mask,
        SceneVec3 {
            x: 1.0,
            y: 1.0,
            z: 0.0
        }
    );
    assert_eq!(profile.size_phase_max, 1.0);
    assert_eq!(profile.size_scale_max, 1.05);

    let document =
        crate::convert::we_ingest::lower::lower_ir_to_scene_binary(&ir).expect("lower floral");
    assert_eq!(
        document.particles[0].simulation,
        crate::engine::scene::SceneParticleSimulationKind::FloralOscillation
    );
    assert_eq!(document.particles[0].position_oscillation_mask.x, 1.0);
    assert_eq!(document.particles[0].size_min, 75.0);
    assert_eq!(document.particles[0].size_max, 200.0);
    let mut bytes = Vec::new();
    crate::engine::scene::write_scene_binary(&document, &mut bytes).expect("write PART");
    let decoded = crate::engine::scene::read_scene_binary_bytes(&bytes).expect("read PART");
    assert_eq!(decoded.particles, document.particles);

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

#[test]
fn lowers_direct_genericimage_puppet_token_one_clipping_graph() {
    let root = std::env::temp_dir().join(format!(
        "gilder-we-direct-puppet-clipping-test-{}",
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
        r#"{"passes":[{"shader":"genericimage4","textures":[null]}]}"#,
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
            WeIrMeshClippingSliceRole::MaskProducer,
            WeIrMeshClippingSliceRole::ClippedTarget,
            WeIrMeshClippingSliceRole::VisibleRemainder,
        ]
    );
    assert!(ir.shader_contracts.iter().any(|contract| {
        contract.shader_key == "we/genericimage4__PUPPETSKINNING_1__CLIPPINGTARGET_1__CLIPPINGUVS_1"
    }));

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

fn test_clipped_mdlv0023() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"MDLV0023\0");
    push_u32(&mut bytes, 0x0180_0009);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 1);
    bytes.extend_from_slice(b"materials/clipped.json\0");
    push_u32(&mut bytes, 0);
    for value in [0.0_f32, 0.0, 0.0, 1.0, 1.0, 0.0] {
        push_f32(&mut bytes, value);
    }
    push_u32(&mut bytes, 0x0180_000f);
    let mut vertices = Vec::new();
    push_mdl_skinned_vertex(&mut vertices, [0.0, 0.0, 0.0], [0.0, 1.0]);
    push_mdl_skinned_vertex(&mut vertices, [1.0, 0.0, 0.0], [1.0, 1.0]);
    push_mdl_skinned_vertex(&mut vertices, [1.0, 1.0, 0.0], [1.0, 0.0]);
    push_u32(&mut bytes, vertices.len() as u32);
    bytes.extend_from_slice(&vertices);
    let indices = [0_u16, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2];
    push_u32(
        &mut bytes,
        (indices.len() * std::mem::size_of::<u16>()) as u32,
    );
    for index in indices {
        bytes.extend_from_slice(&index.to_le_bytes());
    }

    bytes.push(0);
    bytes.push(1);
    push_u32(&mut bytes, 6 * 16);
    for (source_index, index_start) in [(10, 0), (11, 3), (12, 6), (13, 9), (14, 12), (15, 15)] {
        push_u32(&mut bytes, source_index);
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, index_start);
        push_u32(&mut bytes, 3);
    }
    push_u32(&mut bytes, 2);
    bytes.extend_from_slice(&7_u64.to_le_bytes());
    bytes.extend_from_slice(b"masks/eye-clip\0");
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 2);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 1);
    bytes.extend_from_slice(&8_u64.to_le_bytes());
    bytes.extend_from_slice(b"masks/eye-clip\0");
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 4);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 3);

    bytes.extend_from_slice(b"MDLS0004");
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 1);
    bytes.extend_from_slice(b"root\0");
    bytes.extend_from_slice(&0_i32.to_le_bytes());
    push_u32(&mut bytes, u32::MAX);
    push_u32(&mut bytes, 64);
    for index in 0..16 {
        push_f32(
            &mut bytes,
            f32::from(index == 0 || index == 5 || index == 10 || index == 15),
        );
    }
    bytes.extend_from_slice(b"{}\0");
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

fn push_mdl_skinned_vertex(out: &mut Vec<u8>, position: [f32; 3], uv: [f32; 2]) {
    let start = out.len();
    for value in position {
        push_f32(out, value);
    }
    out.resize(start + 40, 0);
    for blend_index in [0_u32; 4] {
        push_u32(out, blend_index);
    }
    for blend_weight in [1.0_f32, 0.0, 0.0, 0.0] {
        push_f32(out, blend_weight);
    }
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
