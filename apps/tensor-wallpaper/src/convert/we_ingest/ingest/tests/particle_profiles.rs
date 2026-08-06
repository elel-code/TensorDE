use super::super::particle::{particle_instance_color, particle_instance_time_scale};
use super::*;

#[test]
fn ingests_falling_leaves_as_typed_particle_ir_and_part_record() {
    let root = std::env::temp_dir().join(format!(
        "tensor-wallpaper-we-particle-test-{}",
        std::process::id()
    ));
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
            "animationmode":"randomframe",
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
    assert_eq!(
        ir.particles[0].animation_mode,
        crate::engine::scene::SceneParticleAnimationMode::RandomFrame
    );
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
    assert_eq!(
        document.particles[0].animation_mode,
        crate::engine::scene::SceneParticleAnimationMode::RandomFrame
    );
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
    let root = std::env::temp_dir().join(format!(
        "tensor-wallpaper-we-sparkles-test-{}",
        std::process::id()
    ));
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
    assert_eq!(ir.particles[0].instance_time_scale, 0.83922);
    assert_eq!(
        ir.particles[0].instance_color,
        Some(SceneVec3 {
            x: 0.83922,
            y: 1.0,
            z: 1.0,
        })
    );

    let document =
        crate::convert::we_ingest::lower::lower_ir_to_scene_binary(&ir).expect("lower sparkles");
    assert_eq!(
        document.particles[0].simulation,
        crate::engine::scene::SceneParticleSimulationKind::AmbientSparkles
    );
    assert_eq!(document.particles[0].size_min, 2.5);
    assert_eq!(document.particles[0].size_max, 5.0);
    assert_eq!(document.particles[0].instance_time_scale, 0.83922);
    assert_eq!(document.particles[0].instance_color_enabled, 1);
    assert_eq!(
        document.particles[0].color_min,
        SceneVec3 {
            x: 0.83922,
            y: 1.0,
            z: 1.0,
        }
    );
    let mut bytes = Vec::new();
    crate::engine::scene::write_scene_binary(&document, &mut bytes).expect("write PART");
    let decoded = crate::engine::scene::read_scene_binary_bytes(&bytes).expect("read PART");
    assert_eq!(decoded.particles, document.particles);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn particle_instance_color_preserves_direct_and_legacy_we_semantics() {
    let absent = serde_json::json!({});
    assert_eq!(particle_instance_color(&absent), None);
    assert_eq!(particle_instance_time_scale(None), 1.0);

    let white = serde_json::json!({"instanceoverride":{"colorn":"1 1 1"}});
    assert_eq!(particle_instance_color(&white), Some(SceneVec3::ONE));
    assert_eq!(
        particle_instance_time_scale(particle_instance_color(&white)),
        1.0
    );

    let tinted = serde_json::json!({
        "instanceoverride":{"colorn":"0.5 0.25 0.75"}
    });
    assert_eq!(
        particle_instance_color(&tinted),
        Some(SceneVec3 {
            x: 0.5,
            y: 0.25,
            z: 0.75,
        })
    );
    assert_eq!(
        particle_instance_time_scale(particle_instance_color(&tinted)),
        0.5
    );

    let legacy = serde_json::json!({"instanceoverride":{"color":"128 64 255"}});
    assert_eq!(
        particle_instance_color(&legacy),
        Some(SceneVec3 {
            x: 0.50196,
            y: 0.25098,
            z: 1.0,
        })
    );
    assert_eq!(
        particle_instance_time_scale(particle_instance_color(&legacy)),
        0.50196
    );
}

#[test]
fn ingests_floral_oscillation_as_typed_particle_ir_and_part_record() {
    let root = std::env::temp_dir().join(format!(
        "tensor-wallpaper-we-floral-particle-test-{}",
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
fn ingests_composable_sprite_modules_without_fixture_profile_matching() {
    let root = std::env::temp_dir().join(format!(
        "tensor-wallpaper-we-module-sprite-particle-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("particles")).expect("particles");
    fs::create_dir_all(root.join("materials/particle")).expect("materials");
    fs::write(
        root.join("project.json"),
        r#"{"type":"scene","file":"scene.json","title":"Module sprite"}"#,
    )
    .expect("project");
    fs::write(
        root.join("scene.json"),
        r#"{"general":{"orthogonalprojection":{"width":3840,"height":2160}},"objects":[{"id":940,"name":"snow","particle":"particles/snow.json","origin":"1920 1080 0"}]}"#,
    )
    .expect("scene");
    fs::write(
        root.join("particles/snow.json"),
        r#"{
            "children":null,
            "emitter":[{"id":6,"name":"sphererandom","origin":"0 650 0","directions":"2 0.8 0","distancemin":10,"distancemax":1200,"rate":15}],
            "initializer":[
                {"id":2,"name":"lifetimerandom","min":15,"max":23},
                {"id":3,"name":"sizerandom","min":2,"max":30},
                {"id":4,"name":"velocityrandom","min":"-10 -50 0","max":"-37 -90 0"},
                {"id":5,"name":"colorrandom","min":"255 255 255","max":"95 98 100"},
                {"id":6,"name":"rotationrandom"},
                {"id":11,"name":"alpharandom","max":0.3},
                {"id":12,"name":"turbulentvelocityrandom","right":"0 100 1"}
            ],
            "material":"materials/particle/snow.json",
            "maxcount":300,
            "operator":[
                {"id":7,"name":"movement"},
                {"id":8,"name":"oscillateposition","frequencymin":0.8,"frequencymax":1.0,"phasemin":0,"phasemax":1,"scalemin":20,"scalemax":35,"mask":"1 0.5 0"},
                {"id":9,"name":"alphafade","fadeintime":0.1}
            ],
            "renderer":[{"id":1,"name":"sprite"}],
            "starttime":15
        }"#,
    )
    .expect("particle");
    fs::write(
        root.join("materials/particle/snow.json"),
        r#"{"passes":[{"shader":"genericparticle","blending":"additive","textures":[null]}]}"#,
    )
    .expect("material");

    let ir = ingest_wallpaper_engine_project(&root).expect("module sprite IR");
    assert_eq!(
        ir.render_graphs[0].passes[0].state.pipeline_blend,
        crate::engine::render_graph::PipelineBlendMode::Translucent
    );
    let profile = ir.particles[0]
        .module_sprite_profile()
        .expect("composable module sprite");
    assert_eq!(
        profile.emitter_shape,
        crate::engine::scene::SceneParticleEmitterShape::SphereRandom
    );
    assert!(
        profile
            .module_mask
            .contains(crate::engine::scene::SceneParticleModuleMask::ALPHA_RANDOM)
    );
    assert_eq!((profile.alpha_min, profile.alpha_max), (0.0, 0.3));
    assert_eq!(profile.position_mask.y, 0.5);
    use crate::engine::scene::SceneParticleInitializerKind as InitializerKind;
    assert_eq!(profile.initializer_order.count(), 7);
    assert_eq!(
        (0..profile.initializer_order.count() as usize)
            .map(|index| profile.initializer_order.kind_at(index).expect("kind"))
            .collect::<Vec<_>>(),
        vec![
            InitializerKind::LifetimeRandom,
            InitializerKind::SizeRandom,
            InitializerKind::VelocityRandom,
            InitializerKind::ColorRandom,
            InitializerKind::RotationRandom,
            InitializerKind::AlphaRandom,
            InitializerKind::TurbulentVelocityRandom,
        ]
    );

    let document = crate::convert::we_ingest::lower::lower_ir_to_scene_binary(&ir)
        .expect("lower module sprite");
    let particle = document.particles[0];
    assert_eq!(
        particle.simulation,
        crate::engine::scene::SceneParticleSimulationKind::ModuleSprite
    );
    assert_eq!(particle.start_time, 15.0);
    assert_eq!(particle.initializer_order, profile.initializer_order);
    assert_eq!((particle.alpha_min, particle.alpha_max), (0.0, 0.3));
    assert_eq!(
        (particle.turbulence_speed_min, particle.turbulence_speed_max),
        (100.0, 250.0)
    );
    assert_eq!(
        particle.turbulent_velocity_right,
        SceneVec3 {
            x: 0.0,
            y: 100.0,
            z: 1.0
        }
    );
    let mut bytes = Vec::new();
    crate::engine::scene::write_scene_binary(&document, &mut bytes).expect("write module PART");
    let decoded = crate::engine::scene::read_scene_binary_bytes(&bytes).expect("read module PART");
    assert_eq!(decoded.particles, document.particles);

    let _ = fs::remove_dir_all(root);
}
