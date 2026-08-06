use std::fs;
use std::path::{Path, PathBuf};

use super::super::*;
use crate::engine::scene::{
    INVALID_PARTICLE_INDEX, SceneParticleChildType, SceneParticleSimulationKind,
};

#[test]
fn recursively_ingests_builtin_default_children_with_precise_pass_indices() {
    let root = particle_fixture_root("recursive");
    write_base_project(&root, 0.6);
    write_particle(&root, "particles/root.json", "particles/child.json", None);
    write_leaf_particle(&root, "particles/child.json");

    let ir = ingest_wallpaper_engine_project(&root).expect("recursive particle IR");
    assert_eq!(
        ir.particles.len(),
        3,
        "duplicate references remain independent"
    );
    assert_eq!(
        ir.particles[0].parent_particle_index,
        INVALID_PARTICLE_INDEX
    );
    assert_eq!(ir.particles[1].parent_particle_index, 0);
    assert_eq!(ir.particles[2].parent_particle_index, 0);
    assert!(
        ir.particles
            .iter()
            .all(|particle| particle.child_type == WeIrParticleChildType::BuiltinDefault)
    );
    assert_eq!(ir.particles[1].child_max_count, 10);
    assert_eq!(ir.particles[1].child_probability, 1.0);
    assert_eq!(ir.render_graphs[0].passes.len(), 3);
    assert_eq!(
        ir.render_graphs[0]
            .passes
            .iter()
            .map(|pass| pass.id)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert!(ir.render_graphs[0].passes.iter().all(|pass| {
        pass.state.color_write_mask == crate::engine::render_graph::ColorWriteMask::Rgb
    }));

    let document = crate::convert::we_ingest::lower::lower_ir_to_scene_binary(&ir)
        .expect("lower recursive particles");
    assert_eq!(
        document.particles[1].child_type,
        SceneParticleChildType::BuiltinDefault
    );
    assert_eq!(document.particles[1].instance_count_scale, 0.6);
    assert_eq!(document.particles[1].rate, 1.2);
    assert_eq!(
        document.particles[1].simulation,
        SceneParticleSimulationKind::ModuleSprite
    );
    let mut bytes = Vec::new();
    crate::engine::scene::write_scene_binary(&document, &mut bytes).expect("write current scene");
    let decoded =
        crate::engine::scene::read_scene_binary_bytes(&bytes).expect("read current scene");
    assert_eq!(decoded.particles, document.particles);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn turbulent_velocity_retains_builtin_defaults_and_direction_basis() {
    let automatic_projection = super::super::parse_scene_root_ir(&serde_json::json!({
        "general": {"orthogonalprojection": {"auto": true}}
    }));
    assert!(automatic_projection.orthogonal_projection_auto);

    let definition = serde_json::json!({
        "initializer": [
            {"id": 1, "name": "turbulentvelocityrandom"},
            {"id": 2, "name": "turbulentvelocityrandom", "offset": -0.5, "scale": 0.1},
            {"id": 3, "name": "turbulentvelocityrandom", "right": "0 100 1"}
        ]
    });
    let initializers = super::super::particle::parse_initializers(&definition, false);
    let WeIrParticleInitializer::TurbulentVelocityRandom {
        offset,
        scale,
        speed_min,
        speed_max,
        phase_min,
        phase_max,
        time_scale,
        right,
        forward,
        ..
    } = initializers[0]
    else {
        panic!("built-in turbulent velocity initializer");
    };
    assert_eq!((offset, scale), (0.0, 1.0));
    assert_eq!((speed_min, speed_max), (0.5, 1.0));
    assert_eq!((phase_min, phase_max, time_scale), (0.0, 0.0, 1.0));
    assert_eq!(
        right,
        SceneVec3 {
            x: 0.0,
            y: 0.0,
            z: 1.0
        }
    );
    assert_eq!(
        forward,
        SceneVec3 {
            x: 0.0,
            y: 1.0,
            z: 0.0
        }
    );

    let WeIrParticleInitializer::TurbulentVelocityRandom { offset, scale, .. } = initializers[1]
    else {
        panic!("offset/scale turbulent velocity initializer");
    };
    assert_eq!((offset, scale), (-0.5, 0.1));

    let WeIrParticleInitializer::TurbulentVelocityRandom { right, .. } = initializers[2] else {
        panic!("right-basis turbulent velocity initializer");
    };
    assert_eq!(
        right,
        SceneVec3 {
            x: 0.0,
            y: 100.0,
            z: 1.0
        }
    );

    let projection_initializers = super::super::particle::parse_initializers(&definition, true);
    let WeIrParticleInitializer::TurbulentVelocityRandom {
        speed_min,
        speed_max,
        ..
    } = projection_initializers[0]
    else {
        panic!("projected turbulent velocity initializer");
    };
    assert_eq!((speed_min, speed_max), (100.0, 250.0));
}

#[test]
fn turbulence_operator_retains_builtin_projection_defaults_and_authored_phase() {
    let defaults = serde_json::json!({
        "operator": [{"id": 1, "name": "turbulence"}]
    });
    let planar = super::super::particle::parse_operators(&defaults, false);
    let WeIrParticleOperator::Turbulence {
        phase_min,
        phase_max,
        scale,
        speed_min,
        speed_max,
        time_scale,
        ..
    } = planar[0]
    else {
        panic!("built-in turbulence operator");
    };
    assert_eq!(
        (
            phase_min, phase_max, scale, speed_min, speed_max, time_scale
        ),
        (0.0, 0.0, 1.0, 1.0, 2.0, 1.0)
    );

    let projected = super::super::particle::parse_operators(&defaults, true);
    let WeIrParticleOperator::Turbulence {
        scale,
        speed_min,
        speed_max,
        time_scale,
        ..
    } = projected[0]
    else {
        panic!("projected built-in turbulence operator");
    };
    assert_eq!(
        (scale, speed_min, speed_max, time_scale),
        (0.01, 500.0, 1000.0, 20.0)
    );

    let authored = serde_json::json!({
        "operator": [{
            "id": 2,
            "name": "turbulence",
            "mask": "1 0 0",
            "phasemin": 2.0,
            "phasemax": 5.0,
            "scale": 0.002,
            "speedmin": 100.0,
            "speedmax": 150.0,
            "timescale": 7.0
        }]
    });
    let operators = super::super::particle::parse_operators(&authored, true);
    let WeIrParticleOperator::Turbulence {
        mask,
        phase_min,
        phase_max,
        scale,
        speed_min,
        speed_max,
        time_scale,
        ..
    } = operators[0]
    else {
        panic!("authored built-in turbulence operator");
    };
    assert_eq!(
        mask,
        SceneVec3 {
            x: 1.0,
            y: 0.0,
            z: 0.0
        }
    );
    assert_eq!(
        (
            phase_min, phase_max, scale, speed_min, speed_max, time_scale
        ),
        (2.0, 5.0, 0.002, 100.0, 150.0, 7.0)
    );
}

#[test]
fn turbulent_birth_angle_uses_gradient_noise_kernel() {
    let vertex = include_str!("../../../../../shaders/scene/genericparticle.vert.slang");
    assert!(vertex.contains("case 0u: return 0x5b89a097u;"));
    assert!(vertex.contains("default: return 0xb49c3dd7u;"));
    assert!(vertex.contains("float gradient_noise_1d(float value)"));
    assert!(vertex.contains("return (left + right) * 0.395;"));
    assert!(vertex.contains("float velocityWave = gradient_noise_1d("));
    assert!(vertex.contains("vec3 axis = g_Particle.g_TurbulentVelocityRightOperatorSpeedMin.xyz"));
    assert!(
        vertex
            .contains("vec3 direction = g_Particle.g_TurbulentVelocityForwardOperatorSpeedMax.xyz")
    );
    assert!(vertex.contains("cross(axis, direction) * velocitySine"));

    assert_eq!(gradient_noise_1d_prefix(0.0).to_bits(), 0.0f32.to_bits());
    assert!((gradient_noise_1d_prefix(0.5) - 0.437_431_63).abs() < 1.0e-7);
    assert!((gradient_noise_1d_prefix(1.25) - 0.097_989_06).abs() < 1.0e-7);
}

#[test]
fn sprite_trail_uses_builtin_default_maximum_and_full_3d_expansion() {
    let definition = serde_json::json!({
        "renderer": [{
            "id": 4,
            "name": "spritetrail",
            "length": 0.007
        }]
    });
    let renderers = super::super::particle::parse_renderers(&definition).expect("renderers");
    assert_eq!(
        renderers,
        [WeIrParticleRenderer::SpriteTrail {
            id: 4,
            flags: 0,
            blending: crate::engine::scene::ScenePipelineBlend::Translucent,
            length: 0.007,
            min_length: 0.0,
            max_length: 10.0,
        }]
    );

    let vertex = include_str!("../../../../../shaders/scene/genericparticle.vert.slang");
    let trail_start = vertex
        .find("if (spriteTrail) {")
        .expect("SpriteTrail branch");
    let trail_end = vertex[trail_start..]
        .find("    } else {")
        .map(|offset| trail_start + offset)
        .expect("SpriteTrail branch end");
    let trail = &vertex[trail_start..trail_end];
    assert!(trail.contains("vec3 trailVelocity = velocity;"));
    assert!(trail.contains("normalize(cross(eyeDirection, trailVelocity))"));
    assert!(trail.contains("float trailLength = clamp("));
    assert!(trail.contains("g_Particle.g_TrailEyeMax.w"));
    assert!(trail.contains("vec3 up = tangent * trailLength;"));
    assert!(!trail.contains("sin(rotation)"));
    assert!(!trail.contains("cos(rotation)"));
}

#[test]
fn turbulence_operator_uses_canonical_simplex_noise3_kernel() {
    let vertex = include_str!("../../../../../shaders/scene/genericparticle.vert.slang");
    assert!(vertex.contains("float simplex_noise_3d(vec3 position)"));
    assert!(vertex.contains("const float F3 = 0.3333333333333333;"));
    assert!(vertex.contains("const float G3 = 0.1666666666666667;"));
    assert!(vertex.contains("float attenuation = 0.6 - dot(distance, distance);"));
    assert!(vertex.contains("return 32.0 * ("));

    let permutation = noise_permutation_from_slang(vertex);
    assert_eq!(permutation.len(), 256);
    assert_eq!(&permutation[..8], &[151, 160, 137, 91, 90, 15, 131, 13]);
    assert_eq!(simplex_noise3([0.0, 0.0, 0.0], &permutation), 0.0);
    assert!((simplex_noise3([0.1, 0.2, 0.3], &permutation) - 0.635_890_36).abs() < 1.0e-6);
    assert!((simplex_noise3([1.25, -0.75, 2.5], &permutation) + 0.285_170_82).abs() < 1.0e-6);
    let negative = simplex_noise3([-0.2, 0.4, -0.6], &permutation);
    assert!((negative + 0.679_332_8).abs() < 1.0e-6, "{negative}");
}

fn noise_permutation_from_slang(source: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(256);
    for line in source.lines() {
        let Some(hex) = line.split("return 0x").nth(1) else {
            continue;
        };
        let Some(word) = hex.split('u').next() else {
            continue;
        };
        let value = u32::from_str_radix(word, 16).expect("packed permutation word");
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn simplex_noise3(position: [f32; 3], permutation: &[u8]) -> f32 {
    const GRADIENTS: [[f32; 3]; 12] = [
        [1.0, 1.0, 0.0],
        [-1.0, 1.0, 0.0],
        [1.0, -1.0, 0.0],
        [-1.0, -1.0, 0.0],
        [1.0, 0.0, 1.0],
        [-1.0, 0.0, 1.0],
        [1.0, 0.0, -1.0],
        [-1.0, 0.0, -1.0],
        [0.0, 1.0, 1.0],
        [0.0, -1.0, 1.0],
        [0.0, 1.0, -1.0],
        [0.0, -1.0, -1.0],
    ];
    let skew = (position[0] + position[1] + position[2]) / 3.0;
    let cell = [
        (position[0] + skew).floor() as i32,
        (position[1] + skew).floor() as i32,
        (position[2] + skew).floor() as i32,
    ];
    let unskew = (cell[0] + cell[1] + cell[2]) as f32 / 6.0;
    let d0 = [
        position[0] - (cell[0] as f32 - unskew),
        position[1] - (cell[1] as f32 - unskew),
        position[2] - (cell[2] as f32 - unskew),
    ];
    let (c1, c2) = if d0[0] >= d0[1] {
        if d0[1] >= d0[2] {
            ([1, 0, 0], [1, 1, 0])
        } else if d0[0] >= d0[2] {
            ([1, 0, 0], [1, 0, 1])
        } else {
            ([0, 0, 1], [1, 0, 1])
        }
    } else if d0[1] < d0[2] {
        ([0, 0, 1], [0, 1, 1])
    } else if d0[0] < d0[2] {
        ([0, 1, 0], [0, 1, 1])
    } else {
        ([0, 1, 0], [1, 1, 0])
    };
    let mut sum = 0.0;
    for (corner, distance) in [
        ([0, 0, 0], d0),
        (c1, add_corner(d0, c1, 1.0 / 6.0)),
        (c2, add_corner(d0, c2, 1.0 / 3.0)),
        ([1, 1, 1], [d0[0] - 0.5, d0[1] - 0.5, d0[2] - 0.5]),
    ] {
        let attenuation = 0.6 - dot3(distance, distance);
        if attenuation <= 0.0 {
            continue;
        }
        let z = permutation[((cell[2] + corner[2]) & 255) as usize] as i32;
        let y = permutation[((cell[1] + corner[1] + z) & 255) as usize] as i32;
        let gradient =
            GRADIENTS[permutation[((cell[0] + corner[0] + y) & 255) as usize] as usize % 12];
        sum += attenuation.powi(4) * dot3(gradient, distance);
    }
    32.0 * sum
}

fn add_corner(distance: [f32; 3], corner: [i32; 3], unskew: f32) -> [f32; 3] {
    [
        distance[0] - corner[0] as f32 + unskew,
        distance[1] - corner[1] as f32 + unskew,
        distance[2] - corner[2] as f32 + unskew,
    ]
}

fn dot3(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn gradient_noise_1d_prefix(value: f32) -> f32 {
    const PREFIX: [u8; 3] = [151, 160, 137];
    let lattice = value.floor() as usize;
    let distance = value - lattice as f32;
    let left = gradient_noise_gradient(PREFIX[lattice], distance);
    let right = gradient_noise_gradient(PREFIX[lattice + 1], distance - 1.0);
    (left + right) * 0.395
}

fn gradient_noise_gradient(permutation: u8, distance: f32) -> f32 {
    let mut gradient = f32::from(1 + (permutation & 7));
    if permutation & 8 != 0 {
        gradient = -gradient;
    }
    let mut falloff = 1.0 - distance * distance;
    falloff *= falloff;
    falloff *= falloff;
    gradient * distance * falloff
}

#[test]
fn rejects_particle_child_cycles_and_unknown_explicit_types() {
    let cycle_root = particle_fixture_root("cycle");
    write_base_project(&cycle_root, 1.0);
    write_particle(
        &cycle_root,
        "particles/root.json",
        "particles/root.json",
        None,
    );
    let cycle = ingest_wallpaper_engine_project(&cycle_root)
        .expect_err("particle child cycle must be rejected")
        .to_string();
    assert!(cycle.contains("particle child cycle"), "{cycle}");
    let _ = fs::remove_dir_all(cycle_root);

    let type_root = particle_fixture_root("invalid-type");
    write_base_project(&type_root, 1.0);
    write_particle(
        &type_root,
        "particles/root.json",
        "particles/child.json",
        Some("eventbirth"),
    );
    write_leaf_particle(&type_root, "particles/child.json");
    let invalid = ingest_wallpaper_engine_project(&type_root)
        .expect_err("unknown built-in child type must be rejected")
        .to_string();
    assert!(invalid.contains("invalid child type"), "{invalid}");
    let _ = fs::remove_dir_all(type_root);
}

#[test]
fn particle_animation_mode_is_typed_and_strict() {
    use crate::engine::scene::SceneParticleAnimationMode;

    let default = serde_json::json!({});
    let sequence = serde_json::json!({"animationmode": "sequence"});
    let random = serde_json::json!({"animationmode": "randomframe"});
    assert_eq!(
        super::super::particle::particle_animation_mode(&default, "default.json").unwrap(),
        SceneParticleAnimationMode::InterpolatedSequence
    );
    assert_eq!(
        super::super::particle::particle_animation_mode(&sequence, "sequence.json").unwrap(),
        SceneParticleAnimationMode::InterpolatedSequence
    );
    assert_eq!(
        super::super::particle::particle_animation_mode(&random, "random.json").unwrap(),
        SceneParticleAnimationMode::RandomFrame
    );
    for invalid in [
        serde_json::json!({"animationmode": "RandomFrame"}),
        serde_json::json!({"animationmode": 1}),
        serde_json::json!({"animationmode": null}),
    ] {
        assert!(super::super::particle::particle_animation_mode(&invalid, "invalid.json").is_err());
    }
}

#[test]
fn particle_shader_keeps_random_frames_stable_and_interpolates_sequences() {
    let vertex = include_str!("../../../../../shaders/scene/genericparticle.vert.slang");
    let fragment = include_str!("../../../../../shaders/scene/genericparticle.frag.slang");

    assert!(vertex.contains("bool randomFrame ="));
    assert!(vertex.contains("nextFrame = randomFrame"));
    assert!(vertex.contains("coordinates.blend = randomFrame ? 0.0 : fract(framePosition)"));
    assert!(fragment.contains("texture(g_Texture0, v_TexCoordNext)"));
    assert!(fragment.contains("mix(texel, nextTexel, v_TextureSequenceBlend)"));
}

fn particle_fixture_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "tensor-wallpaper-particle-child-{name}-{}",
        std::process::id()
    ))
}

fn write_base_project(root: &Path, count: f32) {
    let _ = fs::remove_dir_all(root);
    fs::create_dir_all(root.join("particles")).expect("particle dir");
    fs::create_dir_all(root.join("materials")).expect("material dir");
    fs::write(
        root.join("project.json"),
        r#"{"type":"scene","file":"scene.json","title":"Particle children"}"#,
    )
    .expect("project");
    fs::write(
        root.join("scene.json"),
        format!(
            r#"{{"objects":[{{"id":1,"name":"particles","particle":"particles/root.json","instanceoverride":{{"count":{count}}}}}]}}"#
        ),
    )
    .expect("scene");
    fs::write(
        root.join("materials/particle.json"),
        r#"{"passes":[{"shader":"genericparticle","textures":[null]}]}"#,
    )
    .expect("material");
}

fn write_particle(root: &Path, path: &str, child: &str, child_type: Option<&str>) {
    let type_field = child_type.map_or(String::new(), |kind| format!(r#", "type":"{kind}""#));
    fs::write(
        root.join(path),
        format!(
            r#"{{"material":"materials/particle.json","maxcount":3,"starttime":60,"children":[{{"id":1,"name":"{child}"{type_field}}},{{"id":2,"name":"{child}"}}],"emitter":[{{"id":1,"name":"boxrandom","rate":2}}],"initializer":[{{"id":2,"name":"lifetimerandom","min":1,"max":2}},{{"id":3,"name":"sizerandom","min":1,"max":2}}],"operator":[],"renderer":[{{"id":4,"name":"sprite"}}]}}"#
        ),
    )
    .expect("parent particle");
}

fn write_leaf_particle(root: &Path, path: &str) {
    fs::write(
        root.join(path),
        r#"{"material":"materials/particle.json","maxcount":3,"starttime":60,"emitter":[{"id":1,"name":"boxrandom","rate":2}],"initializer":[{"id":2,"name":"lifetimerandom","min":1,"max":2},{"id":3,"name":"sizerandom","min":1,"max":2}],"operator":[],"renderer":[{"id":4,"name":"sprite"}]}"#,
    )
    .expect("leaf particle");
}
