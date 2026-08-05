use super::*;
use crate::engine::render_graph::{
    CullMode, PipelineBlendMode, RenderPassRole, RenderTargetRole, TextureBindingRole,
};
use crate::engine::scene::{SceneCullMode, ScenePipelineBlend};

mod particle_children;
mod particle_profiles;
mod particle_texture_storage;
mod puppet_effect_clipping;
mod puppet_models;
mod user_property;

include!("tests/mdl_fixtures.rs");

#[test]
fn effect_image_target_role_and_scale_follow_we_fbo_semantics() {
    assert_eq!(
        image_target_role("fbo_velocity"),
        WeIrImageTargetRole::NamedFbo
    );
    assert_eq!(
        image_target_role("blur_start_4"),
        WeIrImageTargetRole::NamedFbo
    );
    assert_eq!(
        image_target_role("_rt_QuarterCompoBuffer1"),
        WeIrImageTargetRole::FirstClassEffectTarget
    );
    assert_eq!(
        image_target_role("_tmp_TensorWallpaperFramebufferCaustics"),
        WeIrImageTargetRole::Temporary
    );
    assert_eq!(scale_divisor_to_milli(4.0), 4_000);
    assert_eq!(scale_divisor_to_milli(1.0), 1_000);
}

#[test]
fn ingests_minimal_loose_scene_project() {
    let root = std::env::temp_dir().join(format!(
        "tensor-wallpaper-we-ingest-test-{}",
        std::process::id()
    ));
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
fn sound_layer_retains_typed_audio_resource_identity_for_script_host() {
    let root = std::env::temp_dir().join(format!(
        "tensor-wallpaper-we-sound-layer-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("sounds")).expect("sounds");
    fs::write(
        root.join("project.json"),
        r#"{"type":"scene","file":"scene.json","title":"Sound"}"#,
    )
    .expect("project");
    fs::write(
        root.join("scene.json"),
        r#"{"objects":[{"id":7,"name":"song","sound":["sounds/song.mp3"],"startsilent":true}]}"#,
    )
    .expect("scene");
    fs::write(root.join("sounds/song.mp3"), b"authored-audio").expect("audio");

    let ir = ingest_wallpaper_engine_project(&root).expect("sound layer IR");
    let resource = ir.objects[0].resource.expect("sound resource");
    assert_eq!(
        ir.resources[resource as usize].kind,
        SceneResourceKind::Audio
    );
    assert_eq!(ir.resources[resource as usize].path, "sounds/song.mp3");

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
    assert_eq!(ir.render_graphs[0].passes.len(), 2);
    assert_eq!(
        ir.render_graphs[0]
            .passes
            .iter()
            .map(|pass| pass.role)
            .collect::<Vec<_>>(),
        [
            RenderPassRole::ObjectLocalSource,
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
fn bound_effect_visibility_keeps_literal_siblings_static() {
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
    assert_eq!(ir.object_effects.len(), 3);
    let dynamic_passes = ir.render_graphs[0]
        .passes
        .iter()
        .filter(|pass| {
            pass.effect_visibility.policy
                != crate::engine::render_graph::RenderPassEffectVisibilityPolicy::None
        })
        .collect::<Vec<_>>();
    assert!(!dynamic_passes.is_empty());
    assert!(dynamic_passes.iter().all(|pass| {
        pass.effect_visibility.binding_start == 1 && pass.effect_visibility.binding_count == 1
    }));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn effect_visibility_script_keeps_only_the_literal_target_runtime_addressable() {
    let root = write_effect_visibility_fixture(
        "script-visible",
        serde_json::json!([
            {"file":"effects/waterwaves/effect.json","id":30,"name":"first","visible":true},
            {"file":"effects/waterwaves/effect.json","id":31,"name":"middle","visible":false},
            {"file":"effects/waterwaves/effect.json","id":32,"name":"last","visible":true}
        ]),
        Some(
            "export function update(value) { thisScene.getLayer('body').getEffect('middle').visible = value; return value; }",
        ),
        false,
    );

    let ir = ingest_wallpaper_engine_project(&root).expect("script effect visibility IR");
    let dynamic_passes = ir.render_graphs[0]
        .passes
        .iter()
        .filter(|pass| {
            pass.effect_visibility.policy
                != crate::engine::render_graph::RenderPassEffectVisibilityPolicy::None
        })
        .collect::<Vec<_>>();

    assert_eq!(ir.object_effects.len(), 3);
    assert!(!dynamic_passes.is_empty());
    assert!(dynamic_passes.iter().all(|pass| {
        pass.effect_visibility.binding_start == 1 && pass.effect_visibility.binding_count == 1
    }));

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
        "tensor-wallpaper-we-effect-visibility-{name}-{}",
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
