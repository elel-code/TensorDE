use super::*;
use crate::core::SceneBlendMode;
use crate::engine::scene::SceneResourceKind;

#[test]
fn lower_scene_blend_preserves_all_typed_composite_modes() {
    let cases = [
        (SceneBlendMode::Alpha, SceneCompositeBlend::Alpha),
        (SceneBlendMode::Normal, SceneCompositeBlend::Normal),
        (SceneBlendMode::Additive, SceneCompositeBlend::Additive),
        (SceneBlendMode::Multiply, SceneCompositeBlend::Multiply),
        (SceneBlendMode::Screen, SceneCompositeBlend::Screen),
        (SceneBlendMode::Max, SceneCompositeBlend::Max),
        (SceneBlendMode::Modulate, SceneCompositeBlend::Modulate),
        (SceneBlendMode::HslColor, SceneCompositeBlend::HslColor),
        (
            SceneBlendMode::AlphaToCoverage,
            SceneCompositeBlend::AlphaToCoverage,
        ),
    ];

    for (source, expected) in cases {
        assert_eq!(lower_scene_blend(source), expected);
    }
}

#[test]
fn lower_target_bindings_preserve_we_texture_slots() {
    let mut strings = StringInterner::default();
    let previous = lower_binding(
        &TextureBindingRole::PreviousGraphTarget { slot: 2 },
        Some((RenderTargetRole::ImageLocalSub, None)),
        3,
        7,
        &mut strings,
    )
    .expect("previous target");
    let named = lower_binding(
        &TextureBindingRole::NamedFboBind {
            slot: 7,
            name: "fbo_velocity".to_owned(),
        },
        None,
        3,
        7,
        &mut strings,
    )
    .expect("named target");

    assert_eq!(previous.kind, SceneRenderBindingKind::PreviousGraphTarget);
    assert_eq!(previous.slot, 2);
    assert_eq!(previous.target, SceneRenderTargetKind::ImageLocalSub);
    assert_eq!(previous.name, SceneStringId::NONE);
    assert_eq!(named.kind, SceneRenderBindingKind::NamedFboBind);
    assert_eq!(named.slot, 7);
    assert_eq!(strings.strings[named.name.0 as usize], "fbo_velocity");
}

#[test]
fn lower_previous_target_requires_an_earlier_pass() {
    let error = lower_binding(
        &TextureBindingRole::PreviousGraphTarget { slot: 0 },
        None,
        5,
        11,
        &mut StringInterner::default(),
    )
    .expect_err("first pass cannot sample a previous target");

    assert!(matches!(
        error,
        WeLowerError::MissingPreviousGraphTarget {
            graph_index: 5,
            pass_id: 11,
            slot: 0,
        }
    ));
}

#[test]
fn lower_ir_uses_payload_chunk_and_string_handles() {
    let ir = WeSceneIr {
        project_root: ".".into(),
        project: WeProjectIr {
            title: "demo".to_owned(),
            wallpaper_type: "scene".to_owned(),
            scene_file: "scene.json".to_owned(),
            preview: String::new(),
            properties_json: "{}".to_owned(),
        },
        scene: WeSceneRootIr {
            logical_width: 1920,
            logical_height: 1080,
            clear_color: [0.0, 0.0, 0.0, 1.0],
            ambient_color: [0.3, 0.3, 0.3, 1.0],
            skylight_color: [0.3, 0.3, 0.3, 1.0],
            camera_eye: SceneVec3::default(),
            camera_center: SceneVec3 {
                x: 0.0,
                y: 0.0,
                z: -1.0,
            },
            camera_up: SceneVec3 {
                x: 0.0,
                y: 1.0,
                z: 0.0,
            },
            camera_parallax_enabled: true,
            camera_parallax_amount: 0.5,
            camera_parallax_delay: 0.1,
            camera_parallax_mouse_influence: 0.5,
        },
        resources: vec![WeIrResource {
            handle: 0,
            kind: SceneResourceKind::SceneJson,
            path: "scene.json".to_owned(),
            source: WeIrResourceSource::LooseFile,
            payload: b"{}".to_vec(),
        }],
        textures: Vec::new(),
        objects: Vec::new(),
        object_effects: Vec::new(),
        object_animation_layers: Vec::new(),
        object_transform_tracks: Vec::new(),
        object_transform_channels: Vec::new(),
        object_transform_keyframes: Vec::new(),
        script_programs: Vec::new(),
        user_property_bindings: Vec::new(),
        puppet_animation_clips: Vec::new(),
        puppet_animation_tracks: Vec::new(),
        puppet_animation_transform_samples: Vec::new(),
        puppet_animation_opacity_samples: Vec::new(),
        particles: Vec::new(),
        materials: Vec::new(),
        material_passes: Vec::new(),
        material_textures: Vec::new(),
        material_constants: Vec::new(),
        meshes: vec![WeIrMesh {
            object: 0,
            material: None,
            vertex_start: 0,
            vertex_count: 4,
            index_start: 0,
            index_count: 6,
            width: 64.0,
            height: 32.0,
            bounds_min: SceneVec3 {
                x: -32.0,
                y: -16.0,
                z: 0.0,
            },
            bounds_max: SceneVec3 {
                x: 32.0,
                y: 16.0,
                z: 0.0,
            },
        }],
        mesh_vertices: vec![
            WeIrMeshVertex {
                position: SceneVec3 {
                    x: -32.0,
                    y: -16.0,
                    z: 0.0,
                },
                uv: [0.0, 1.0],
                blend_indices: [0; 4],
                blend_weights: [0.0; 4],
            },
            WeIrMeshVertex {
                position: SceneVec3 {
                    x: 32.0,
                    y: -16.0,
                    z: 0.0,
                },
                uv: [1.0, 1.0],
                blend_indices: [0; 4],
                blend_weights: [0.0; 4],
            },
            WeIrMeshVertex {
                position: SceneVec3 {
                    x: 32.0,
                    y: 16.0,
                    z: 0.0,
                },
                uv: [1.0, 0.0],
                blend_indices: [0; 4],
                blend_weights: [0.0; 4],
            },
            WeIrMeshVertex {
                position: SceneVec3 {
                    x: -32.0,
                    y: 16.0,
                    z: 0.0,
                },
                uv: [0.0, 0.0],
                blend_indices: [0; 4],
                blend_weights: [0.0; 4],
            },
        ],
        mesh_indices: vec![0, 1, 2, 0, 2, 3],
        mesh_source_records: Vec::new(),
        mesh_clipping_subdraws: Vec::new(),
        mesh_clipping_source_ordinals: Vec::new(),
        mesh_clipping_slices: Vec::new(),
        puppets: Vec::new(),
        puppet_bones: Vec::new(),
        puppet_attachments: Vec::new(),
        effects: Vec::new(),
        effect_passes: Vec::new(),
        effect_bindings: Vec::new(),
        effect_combos: Vec::new(),
        shader_combo_definitions: Vec::new(),
        effect_fbos: Vec::new(),
        render_graphs: Vec::new(),
        image_targets: Vec::new(),
        shader_contracts: Vec::new(),
        unsupported: Vec::new(),
    };

    let binary = lower_ir_to_scene_binary(&ir).expect("lower");
    assert_eq!(binary.resource_payload, b"{}".to_vec());
    assert_eq!(binary.resources[0].payload_len, 2);
    assert!(binary.strings.iter().any(|value| value == "scene.json"));
    assert_eq!(binary.meshes.len(), 1);
    assert_eq!(binary.mesh_vertices.len(), 4);
    assert_eq!(binary.mesh_indices, [0, 1, 2, 0, 2, 3]);
    assert_eq!(binary.meshes[0].width, 64.0);
    assert_eq!(
        binary.camera_parallax,
        SceneCameraParallaxRecord {
            enabled: true,
            amount: 0.5,
            delay: 0.1,
            mouse_influence: 0.5,
        }
    );
}
