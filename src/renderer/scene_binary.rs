use crate::core::scene::binary::SceneBinaryError;
use crate::renderer::RendererPlanError;

mod dynamic_state;
mod effect_program;
mod engine_plan;
mod facts;
mod mdlv;
mod mesh;
mod reader;
mod render_layers;
mod schema;
mod texture;
mod topology;
mod wallpaper_plan;

pub(super) use engine_plan::scene_engine_plan_from_gscn_path_with_properties;
pub(super) use wallpaper_plan::{
    scene_wallpaper_plan_from_gscn_path, scene_wallpaper_plan_from_gscn_path_with_properties,
};

fn binary_plan_error(err: SceneBinaryError) -> RendererPlanError {
    RendererPlanError::PackageLoad(format!("failed to read binary scene: {err}"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::json;

    use super::*;
    use crate::core::scene::SceneDocument;
    use crate::core::scene::binary::encode_scene_binary_document;
    use crate::core::{FitMode, SceneBlendMode, SceneNodeKind, SceneSize};
    use crate::engine::scene_engine::{
        SceneEffectCommand, SceneEffectConstantValue, SceneEffectFboFormat, SceneEffectImageRef,
        SceneEffectPassBlend, SceneGraphTarget, SceneObjectId, SceneResource, SceneTextureFormat,
    };

    #[test]
    fn gscn_direct_ingest_defaults_to_we_projection_stretch_fit() {
        let document: SceneDocument = serde_json::from_value(json!({
            "size": { "width": 3840, "height": 2160 },
            "nodes": [
                {
                    "id": "background",
                    "type": "rectangle",
                    "width": 3840.0,
                    "height": 2160.0
                }
            ]
        }))
        .expect("scene document");
        let bytes = encode_scene_binary_document(0, &document).expect("binary scene");
        let root = unique_test_dir("gilder-binary-scene-fit");
        let assets = root.join("assets");
        fs::create_dir_all(&assets).expect("assets dir");
        let scene_path = assets.join("scene.gscn");
        fs::write(&scene_path, bytes).expect("write gscn");

        let plan = scene_wallpaper_plan_from_gscn_path(
            "HDMI-A-1".to_owned(),
            scene_path.clone(),
            None,
            0,
            None,
        )
        .expect("binary scene plan");
        let cover_plan = scene_wallpaper_plan_from_gscn_path(
            "HDMI-A-1".to_owned(),
            scene_path,
            None,
            0,
            Some(FitMode::Cover),
        )
        .expect("binary scene plan with override");
        fs::remove_dir_all(root).expect("remove test dir");

        assert_eq!(
            plan.scene_size,
            Some(SceneSize {
                width: 3840,
                height: 2160
            })
        );
        assert_eq!(plan.scene_fit, FitMode::Stretch);
        assert_eq!(cover_plan.scene_fit, FitMode::Cover);
    }

    #[test]
    fn gscn_direct_ingest_preserves_hsl_color_blend_from_binary_payload() {
        let document: SceneDocument = serde_json::from_value(json!({
            "size": { "width": 3840, "height": 2160 },
            "nodes": [
                {
                    "id": "hsl-color-bar",
                    "type": "rectangle",
                    "width": 550.0,
                    "height": 3300.0,
                    "color": "#003ca4",
                    "properties": {
                        "wallpaper_engine_blend": { "colorBlendMode": 28 }
                    }
                }
            ]
        }))
        .expect("scene document");
        let bytes = encode_scene_binary_document(0, &document).expect("binary scene");
        let root = unique_test_dir("gilder-binary-hsl-color-blend");
        let assets = root.join("assets");
        fs::create_dir_all(&assets).expect("assets dir");
        let scene_path = assets.join("scene.gscn");
        fs::write(&scene_path, bytes).expect("write gscn");

        let plan = scene_wallpaper_plan_from_gscn_path(
            "HDMI-A-1".to_owned(),
            scene_path.clone(),
            None,
            0,
            None,
        )
        .expect("binary scene plan");
        fs::remove_dir_all(root).expect("remove test dir");

        assert_eq!(plan.layers.len(), 1);
        assert_eq!(plan.layers[0].blend_mode, SceneBlendMode::HslColor);
    }

    #[test]
    fn gscn_direct_ingest_emits_meshless_retained_particle_marker_from_binary_payload() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "spark", "type": "image", "source": "assets/spark.gtex", "width": 16, "height": 16 }
            ],
            "nodes": [
                {
                    "id": "parent",
                    "type": "group",
                    "opacity": 0.5,
                    "transform": { "x": 100.0, "y": 50.0 },
                    "children": [
                        {
                            "id": "spark-emitter",
                            "type": "particle-emitter",
                            "resource": "spark",
                            "opacity": 0.8,
                            "transform": { "x": 10.0, "y": 20.0 },
                            "properties": {
                                "particle": {
                                    "count": 3,
                                    "seed": 1,
                                    "lifetime_ms": 1000,
                                    "loop": true,
                                    "spawn_width": 0.0,
                                    "spawn_height": 0.0,
                                    "width": 6.0,
                                    "height": 8.0,
                                    "speed": 0.0,
                                    "spread_deg": 0.0,
                                    "gravity_x": 0.0,
                                    "gravity_y": 0.0,
                                    "fade": false,
                                    "color": "#aabbcc"
                                }
                            }
                        }
                    ]
                }
            ]
        }))
        .expect("scene document");
        let bytes = encode_scene_binary_document(0, &document).expect("binary scene");
        let root = unique_test_dir("gilder-binary-particle-plan");
        let assets = root.join("assets");
        fs::create_dir_all(&assets).expect("assets dir");
        let scene_path = assets.join("scene.gscn");
        fs::write(&scene_path, bytes).expect("write gscn");

        let plan =
            scene_wallpaper_plan_from_gscn_path("HDMI-A-1".to_owned(), scene_path, None, 250, None)
                .expect("binary scene plan");
        fs::remove_dir_all(root).expect("remove test dir");

        assert_eq!(plan.layers.len(), 1);
        let layer = &plan.layers[0];
        assert_eq!(layer.id, "spark-emitter");
        assert_eq!(layer.kind, SceneNodeKind::Image);
        assert_eq!(layer.texture_slots.len(), 0);
        assert_eq!(layer.color.as_deref(), Some("#aabbcc"));
        assert_eq!(layer.width, Some(6.0));
        assert_eq!(layer.height, Some(8.0));
        assert!((layer.opacity - 0.4).abs() < 1e-6);
        assert!((layer.transform.x - 110.0).abs() < f64::EPSILON);
        assert!((layer.transform.y - 70.0).abs() < f64::EPSILON);
        assert!(
            layer.mesh.is_none(),
            "textured retained particle marker should not carry CPU mesh vertices"
        );
    }

    #[test]
    fn gscn_direct_ingest_preserves_effect_graph_pass_fields_from_binary_payload() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "base", "type": "image", "source": "assets/base.gtex", "width": 320, "height": 180 },
                { "id": "normal", "type": "image", "source": "assets/normal.gtex", "width": 64, "height": 64 }
            ],
            "nodes": [
                {
                    "id": "water-carrier",
                    "type": "image",
                    "resource": "base",
                    "width": 320.0,
                    "height": 180.0,
                    "effects": [
                        {
                            "file": "effects/custom/effect.json",
                            "fbos": [
                                { "name": "_rt_Custom", "format": "rgba8888", "scale": 0.5, "unique": true }
                            ],
                            "passes": [
                                {
                                    "command": "draw",
                                    "source": "previous",
                                    "target": "_rt_Custom",
                                    "binds": { "0": "previous", "2": "_rt_CustomNormal" },
                                    "shader": "effects/custom",
                                    "blending": "normal",
                                    "texture_resources": ["base", null, "normal"],
                                    "combos": { "MASK": 0 },
                                    "constant_shader_values": { "strength": 0.5 }
                                }
                            ]
                        }
                    ]
                }
            ]
        }))
        .expect("scene document");
        let bytes = encode_scene_binary_document(0, &document).expect("binary scene");
        let root = unique_test_dir("gilder-binary-effect-graph-plan");
        let assets = root.join("assets");
        fs::create_dir_all(&assets).expect("assets dir");
        let scene_path = assets.join("scene.gscn");
        fs::write(&scene_path, bytes).expect("write gscn");

        let plan = scene_wallpaper_plan_from_gscn_path(
            "HDMI-A-1".to_owned(),
            scene_path.clone(),
            None,
            0,
            None,
        )
        .expect("binary scene plan");
        fs::remove_dir_all(root).expect("remove test dir");

        assert_eq!(plan.layers.len(), 1);
        let pass = &plan.layers[0].image_effect_passes[0];
        assert_eq!(pass.command.as_deref(), Some("draw"));
        assert_eq!(pass.source.as_deref(), Some("previous"));
        assert_eq!(pass.target.as_deref(), Some("_rt_Custom"));
        assert_eq!(pass.binds.get(&0).map(String::as_str), Some("previous"));
        assert_eq!(
            pass.binds.get(&2).map(String::as_str),
            Some("_rt_CustomNormal")
        );
        assert_eq!(pass.fbos.len(), 1);
        assert_eq!(pass.fbos[0].name, "_rt_Custom");
        assert_eq!(pass.fbos[0].format.as_deref(), Some("rgba8888"));
        assert!((pass.fbos[0].scale - 0.5).abs() < f64::EPSILON);
        assert!(pass.fbos[0].unique);
        assert_eq!(pass.combos.get("MASK"), Some(&0));
        assert_eq!(
            pass.constant_shader_values
                .get("strength")
                .and_then(|value| value.as_f64()),
            Some(0.5)
        );
    }

    #[test]
    fn gscn_engine_plan_preserves_typed_effect_program_fbos_copy_and_swaps() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "base", "type": "image", "source": "assets/base.gtex", "width": 320, "height": 180 },
                { "id": "normal", "type": "image", "source": "assets/normal.gtex", "width": 64, "height": 64 }
            ],
            "nodes": [
                {
                    "id": "water-carrier",
                    "type": "image",
                    "resource": "base",
                    "width": 320.0,
                    "height": 180.0,
                    "effects": [
                        {
                            "file": "effects/custom/effect.json",
                            "fbos": [
                                { "name": "_rt_Custom", "format": "rgba8888", "scale": 0.5, "unique": true },
                                { "name": "_rt_CustomPrev", "format": "rgba8888", "scale": 0.5, "unique": true }
                            ],
                            "passes": [
                                {
                                    "command": "draw",
                                    "source": "previous",
                                    "target": "_rt_Custom",
                                    "binds": { "0": "previous", "2": "_rt_CustomNormal" },
                                    "shader": "effects/custom",
                                    "blending": "normal",
                                    "texture_resources": ["base", null, "normal"],
                                    "combos": { "MASK": 0 },
                                    "constant_shader_values": { "strength": 0.5 }
                                },
                                {
                                    "command": "copy",
                                    "source": "_rt_Custom",
                                    "target": "_rt_CustomPrev"
                                },
                                {
                                    "command": "swap",
                                    "source": "_rt_Custom",
                                    "target": "_rt_CustomPrev"
                                }
                            ]
                        }
                    ]
                }
            ]
        }))
        .expect("scene document");
        let bytes = encode_scene_binary_document(0, &document).expect("binary scene");
        let root = unique_test_dir("gilder-binary-engine-effect-program");
        let assets = root.join("assets");
        fs::create_dir_all(&assets).expect("assets dir");
        let scene_path = assets.join("scene.gscn");
        fs::write(&scene_path, bytes).expect("write gscn");

        let plan = scene_engine_plan_from_gscn_path_with_properties(scene_path, 0, None)
            .expect("scene engine plan");
        fs::remove_dir_all(root).expect("remove test dir");

        assert_eq!(plan.effects.len(), 1);
        assert_eq!(plan.effects[0].object, SceneObjectId(0));
        let program = &plan.effects[0].program;
        assert_eq!(program.effect_file, "effects/custom/effect.json");
        assert_eq!(program.fbos.len(), 2);
        assert_eq!(program.fbos[0].name, "_rt_Custom");
        assert_eq!(program.fbos[0].target, SceneGraphTarget::NamedFbo(0));
        assert_eq!(
            program.fbos[0].format,
            Some(SceneEffectFboFormat::Rgba8Unorm)
        );
        assert!((program.fbos[0].scale - 0.5).abs() < f32::EPSILON);
        assert!(program.fbos[0].unique);
        assert_eq!(program.fbos[1].name, "_rt_CustomPrev");
        assert_eq!(program.fbos[1].target, SceneGraphTarget::NamedFbo(1));
        assert_eq!(program.material_pass_count(), 1);
        assert_eq!(program.copy_command_count(), 1);
        assert_eq!(program.swap_command_count(), 1);
        let SceneEffectCommand::MaterialPass(pass) = &program.commands[0] else {
            panic!("expected material pass");
        };
        assert_eq!(pass.pass_index, 0);
        assert_eq!(pass.shader.as_deref(), Some("effects/custom"));
        assert_eq!(pass.source, Some(SceneEffectImageRef::PreviousFramebuffer));
        assert_eq!(
            pass.target,
            Some(SceneEffectImageRef::NamedFbo("_rt_Custom".to_owned()))
        );
        assert_eq!(pass.blend, SceneEffectPassBlend::NormalReplace);
        assert_eq!(
            pass.binds.get(&0),
            Some(&SceneEffectImageRef::PreviousFramebuffer)
        );
        assert_eq!(
            pass.binds.get(&2),
            Some(&SceneEffectImageRef::NamedFbo(
                "_rt_CustomNormal".to_owned()
            ))
        );
        assert_eq!(pass.texture_resources.len(), 2);
        assert_eq!(pass.texture_resources[0].slot, 0);
        assert_eq!(pass.texture_resources[1].slot, 2);
        assert_eq!(pass.combos.get("MASK"), Some(&0));
        assert_eq!(
            pass.constants.get("strength"),
            Some(&SceneEffectConstantValue::Float(0.5))
        );
        let SceneEffectCommand::Copy(copy) = &program.commands[1] else {
            panic!("expected copy command");
        };
        assert_eq!(copy.pass_index, 1);
        assert_eq!(
            copy.source,
            SceneEffectImageRef::NamedFbo("_rt_Custom".to_owned())
        );
        assert_eq!(
            copy.target,
            SceneEffectImageRef::NamedFbo("_rt_CustomPrev".to_owned())
        );
        let SceneEffectCommand::Swap(swap) = &program.commands[2] else {
            panic!("expected swap command");
        };
        assert_eq!(swap.pass_index, 2);
        assert_eq!(
            swap.a,
            SceneEffectImageRef::NamedFbo("_rt_Custom".to_owned())
        );
        assert_eq!(
            swap.b,
            SceneEffectImageRef::NamedFbo("_rt_CustomPrev".to_owned())
        );
    }

    #[test]
    fn gscn_binary_runtime_topology_keeps_initially_hidden_layers() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "hero", "type": "image", "source": "assets/hero.gtex", "width": 16, "height": 16 }
            ],
            "nodes": [
                {
                    "id": "hidden-hero",
                    "type": "image",
                    "resource": "hero",
                    "visible": false,
                    "width": 16.0,
                    "height": 16.0,
                    "transform": { "x": 10.0, "y": 20.0 }
                }
            ]
        }))
        .expect("scene document");
        let bytes = encode_scene_binary_document(0, &document).expect("binary scene");
        let root = unique_test_dir("gilder-binary-retained-hidden");
        let assets = root.join("assets");
        fs::create_dir_all(&assets).expect("assets dir");
        let scene_path = assets.join("scene.gscn");
        fs::write(&scene_path, bytes).expect("write gscn");

        let plan = scene_wallpaper_plan_from_gscn_path(
            "HDMI-A-1".to_owned(),
            scene_path.clone(),
            None,
            0,
            None,
        )
        .expect("binary scene plan");

        assert_eq!(plan.layers.len(), 1);
        let layer = &plan.layers[0];
        assert_eq!(layer.id, "hidden-hero");
        assert_eq!(layer.kind, SceneNodeKind::Image);
        assert_eq!(layer.opacity, 0.0);
        assert_eq!(layer.width, Some(16.0));
        assert_eq!(layer.height, Some(16.0));

        fs::remove_dir_all(root).expect("remove test dir");
    }

    #[test]
    fn gscn_engine_plan_preserves_native_gtex_metadata() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "eye", "type": "image", "source": "assets/eye.gtex", "width": 32, "height": 16 }
            ],
            "nodes": [
                {
                    "id": "eye-node",
                    "type": "image",
                    "resource": "eye",
                    "width": 32,
                    "height": 16
                }
            ]
        }))
        .expect("scene document");
        let bytes = encode_scene_binary_document(0, &document).expect("binary scene");
        let root = unique_test_dir("gilder-binary-gtex-metadata");
        let assets = root.join("assets");
        fs::create_dir_all(&assets).expect("assets dir");
        write_test_gtex_header(&assets.join("eye.gtex"), 663, 230, 7, 1, 155_520);
        let scene_path = assets.join("scene.gscn");
        fs::write(&scene_path, bytes).expect("write gscn");

        let plan = scene_engine_plan_from_gscn_path_with_properties(scene_path, 0, None)
            .expect("scene engine plan");

        fs::remove_dir_all(root).expect("remove test dir");

        let SceneResource::Texture {
            width,
            height,
            format,
            mip_count,
            payload_bytes,
            ..
        } = &plan.resources[0]
        else {
            panic!("expected texture resource");
        };
        assert_eq!(*width, Some(663));
        assert_eq!(*height, Some(230));
        assert_eq!(*format, Some(SceneTextureFormat::Bc7UnormBlock));
        assert_eq!(*mip_count, Some(1));
        assert_eq!(*payload_bytes, Some(155_520));
    }

    #[test]
    fn gscn_direct_ingest_preserves_puppet_clipping_records() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "eye", "type": "image", "source": "assets/eye.gtex", "width": 32, "height": 16 }
            ],
            "nodes": [
                {
                    "id": "eye-node",
                    "type": "image",
                    "resource": "eye",
                    "width": 32,
                    "height": 16,
                    "mesh": {
                        "vertices": [
                            { "x": 0.0, "y": 0.0, "u": 0.0, "v": 0.0 },
                            { "x": 1.0, "y": 0.0, "u": 1.0, "v": 0.0 },
                            { "x": 0.0, "y": 1.0, "u": 0.0, "v": 1.0 }
                        ],
                        "indices": [0, 1, 2],
                        "skin": {
                            "bones": [
                                { "bind": { "translation": [0.0, 0.0, 0.0] } },
                                { "parent": 0, "bind": { "translation": [1.0, 0.0, 0.0] } }
                            ],
                            "vertices": [
                                { "bone_indices": [1, 0, 0, 0], "weights": [1.0, 0.0, 0.0, 0.0] },
                                { "bone_indices": [1, 0, 0, 0], "weights": [1.0, 0.0, 0.0, 0.0] },
                                { "bone_indices": [0, 0, 0, 0], "weights": [1.0, 0.0, 0.0, 0.0] }
                            ]
                        },
                        "puppet_clipping_records": [
                            {
                                "source_name": "eye-right",
                                "mask": "masks/clipping_mask_eye",
                                "duration_frames": 1680,
                                "flags": 1,
                                "bones": [1],
                                "frame_keys": [0, 1, 2]
                            }
                        ],
                        "puppet_clipping_active_sources": [
                            {
                                "source_name": "eye-right",
                                "source_id": 1234605616436508552u64,
                                "scalar_bits": 1065353216,
                                "source_scale": 6,
                                "flags": 2,
                                "transform_index": 4,
                                "parameter0": -1.0,
                                "parameter1": 0.5
                            }
                        ]
                    }
                }
            ]
        }))
        .expect("scene document");
        let bytes = encode_scene_binary_document(0, &document).expect("binary scene");
        let root = unique_test_dir("gilder-binary-puppet-clipping");
        let assets = root.join("assets");
        fs::create_dir_all(&assets).expect("assets dir");
        let scene_path = assets.join("scene.gscn");
        fs::write(&scene_path, bytes).expect("write gscn");

        let plan = scene_wallpaper_plan_from_gscn_path(
            "HDMI-A-1".to_owned(),
            scene_path.clone(),
            None,
            0,
            None,
        )
        .expect("binary scene plan");

        let mesh = plan.layers[0].mesh.as_ref().expect("mesh");
        assert_eq!(mesh.puppet_clipping_records.len(), 1);
        assert_eq!(
            mesh.puppet_clipping_records[0].source_name.as_deref(),
            Some("eye-right")
        );
        assert_eq!(
            mesh.puppet_clipping_records[0].mask,
            "masks/clipping_mask_eye"
        );
        assert_eq!(mesh.puppet_clipping_records[0].duration_frames, 1680);
        assert_eq!(mesh.puppet_clipping_records[0].flags, 1);
        assert_eq!(mesh.puppet_clipping_records[0].bones, vec![1]);
        assert_eq!(mesh.puppet_clipping_records[0].frame_keys, vec![0, 1, 2]);
        assert_eq!(mesh.puppet_clipping_active_sources.len(), 1);
        assert_eq!(
            mesh.puppet_clipping_active_sources[0].source_name,
            "eye-right"
        );
        assert_eq!(
            mesh.puppet_clipping_active_sources[0].source_id,
            0x1122_3344_5566_7788
        );

        let engine_plan = scene_engine_plan_from_gscn_path_with_properties(scene_path, 0, None)
            .expect("scene engine plan");
        fs::remove_dir_all(root).expect("remove test dir");
        let puppet = engine_plan
            .resources
            .iter()
            .find_map(|resource| match resource {
                SceneResource::PuppetRig { clipping, .. } => Some(clipping),
                _ => None,
            })
            .expect("puppet rig");
        assert_eq!(puppet.active_sources.len(), 1);
        assert_eq!(puppet.records[0].active_source_index, Some(0));
    }

    #[test]
    fn gscn_engine_plan_extracts_rt_method8_raw_mdlv_geometry_from_puppet_resource() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "eye", "type": "image", "source": "assets/eye.gtex", "width": 32, "height": 16 },
                {
                    "id": "eye-puppet-mdl",
                    "type": "model",
                    "source": "assets/eye_puppet.mdl",
                    "original_source": "models/eye_puppet.mdl",
                    "role": "we-puppet-mdl"
                }
            ],
            "nodes": [
                {
                    "id": "eye-node",
                    "type": "image",
                    "resource": "eye",
                    "width": 32,
                    "height": 16,
                    "provenance": {
                        "source_format": "wallpaper-engine-scene",
                        "model": {
                            "source": "models/eye.json",
                            "puppet": "models/eye_puppet.mdl"
                        }
                    },
                    "mesh": {
                        "vertices": [
                            { "x": 0.0, "y": 0.0, "u": 0.0, "v": 0.0 },
                            { "x": 1.0, "y": 0.0, "u": 1.0, "v": 0.0 },
                            { "x": 0.0, "y": 1.0, "u": 0.0, "v": 1.0 }
                        ],
                        "indices": [0, 1, 2],
                        "skin": {
                            "bones": [
                                { "bind": { "translation": [0.0, 0.0, 0.0] } }
                            ],
                            "vertices": [
                                { "bone_indices": [0, 0, 0, 0], "weights": [1.0, 0.0, 0.0, 0.0] },
                                { "bone_indices": [0, 0, 0, 0], "weights": [1.0, 0.0, 0.0, 0.0] },
                                { "bone_indices": [0, 0, 0, 0], "weights": [1.0, 0.0, 0.0, 0.0] }
                            ]
                        },
                        "puppet_clipping_records": [
                            {
                                "source_name": "eye-right",
                                "mask": "masks/clipping_mask_eye",
                                "duration_frames": 1680,
                                "flags": 1,
                                "bones": [0],
                                "frame_keys": [0]
                            }
                        ]
                    }
                }
            ]
        }))
        .expect("scene document");
        let bytes = encode_scene_binary_document(0, &document).expect("binary scene");
        let root = unique_test_dir("gilder-binary-rt-method8-mdlv");
        let assets = root.join("assets");
        fs::create_dir_all(&assets).expect("assets dir");
        write_test_gtex_header(&assets.join("eye.gtex"), 32, 16, 7, 1, 128);
        let (vertex_payload, index_payload) = test_mdlv0023_entry_payloads();
        fs::write(
            assets.join("eye_puppet.mdl"),
            test_mdlv0023_bytes(&vertex_payload, &index_payload),
        )
        .expect("write mdlv");
        let scene_path = assets.join("scene.gscn");
        fs::write(&scene_path, bytes).expect("write gscn");

        let plan = scene_engine_plan_from_gscn_path_with_properties(scene_path, 0, None)
            .expect("scene engine plan");
        fs::remove_dir_all(root).expect("remove test dir");

        let geometry = plan
            .resources
            .iter()
            .find_map(|resource| match resource {
                SceneResource::LayerAlphaMaskRtMethod8MdlvGeometry { geometry } => Some(geometry),
                _ => None,
            })
            .expect("rt method [8] MDLV geometry resource");
        assert_eq!(geometry.object, SceneObjectId(0));
        assert_eq!(geometry.entry_owner_index, 0);
        assert_eq!(geometry.layout_key, 0x0180_000f);
        assert_eq!(geometry.vertex_stride_bytes, 80);
        assert_eq!(geometry.vertex_count, 3);
        assert_eq!(geometry.index_count, 3);
        assert_eq!(geometry.vertex_payload, vertex_payload);
        assert_eq!(geometry.index_payload, index_payload);
        assert_eq!(geometry.source_records.len(), 2);
        assert_eq!(geometry.source_records[0].source_index, 0);
        assert_eq!(geometry.source_records[0].local_offset, 10);
        assert_eq!(geometry.source_records[0].index_span_offset, 0);
        assert_eq!(geometry.source_records[0].index_span_count, 2);
        assert_eq!(geometry.source_records[1].source_index, 1);
        assert_eq!(geometry.source_records[1].index_span_offset, 2);
        assert_eq!(geometry.source_records[1].index_span_count, 1);
        assert_eq!(geometry.subdraws.len(), 2);
        assert_eq!(geometry.subdraws[0].source_qword, 0x690);
        assert_eq!(
            geometry.subdraws[0].mask_resource,
            "masks/clipping_mask_eye"
        );
        assert_eq!(geometry.subdraws[0].raw_flags, 0);
        assert_eq!(geometry.subdraws[0].first_indices, vec![0, 1]);
        assert_eq!(geometry.subdraws[0].second_indices, Vec::<u32>::new());
        assert_eq!(geometry.subdraws[0].link, u32::MAX);
        assert_eq!(geometry.subdraws[1].source_qword, 0x691);
        assert_eq!(geometry.subdraws[1].raw_flags, 1);
        assert_eq!(geometry.subdraws[1].first_indices, vec![1]);
        assert_eq!(geometry.subdraws[1].second_indices, vec![0]);
        assert_eq!(geometry.subdraws[1].link, 0);
    }

    #[test]
    fn gscn_direct_ingest_resolves_puppet_clipping_mask_resource_paths() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "eye", "type": "image", "source": "assets/eye.gtex", "width": 32, "height": 16 }
            ],
            "nodes": [
                {
                    "id": "eye-node",
                    "type": "image",
                    "resource": "eye",
                    "width": 32,
                    "height": 16,
                    "mesh": {
                        "vertices": [
                            { "x": 0.0, "y": 0.0, "u": 0.0, "v": 0.0 },
                            { "x": 1.0, "y": 0.0, "u": 1.0, "v": 0.0 },
                            { "x": 0.0, "y": 1.0, "u": 0.0, "v": 1.0 }
                        ],
                        "indices": [0, 1, 2],
                        "skin": {
                            "bones": [
                                { "bind": { "translation": [0.0, 0.0, 0.0] } },
                                { "parent": 0, "bind": { "translation": [1.0, 0.0, 0.0] } }
                            ],
                            "vertices": [
                                { "bone_indices": [1, 0, 0, 0], "weights": [1.0, 0.0, 0.0, 0.0] },
                                { "bone_indices": [1, 0, 0, 0], "weights": [1.0, 0.0, 0.0, 0.0] },
                                { "bone_indices": [0, 0, 0, 0], "weights": [1.0, 0.0, 0.0, 0.0] }
                            ]
                        },
                        "puppet_clipping_records": [
                            {
                                "mask": "masks/clipping_mask_eye",
                                "mask_resource": "assets/clipping-mask.gtex",
                                "duration_frames": 1680,
                                "bones": [1],
                                "frame_keys": [0, 1, 2]
                            }
                        ]
                    }
                }
            ]
        }))
        .expect("scene document");
        let bytes = encode_scene_binary_document(0, &document).expect("binary scene");
        let root = unique_test_dir("gilder-binary-puppet-clipping-resource");
        let assets = root.join("assets");
        fs::create_dir_all(&assets).expect("assets dir");
        let scene_path = assets.join("scene.gscn");
        fs::write(&scene_path, bytes).expect("write gscn");

        let plan =
            scene_wallpaper_plan_from_gscn_path("HDMI-A-1".to_owned(), scene_path, None, 0, None)
                .expect("binary scene plan");
        let expected_mask_resource = root
            .join("assets/clipping-mask.gtex")
            .to_string_lossy()
            .into_owned();
        fs::remove_dir_all(root).expect("remove test dir");

        let mesh = plan.layers[0].mesh.as_ref().expect("mesh");
        assert_eq!(mesh.puppet_clipping_records.len(), 1);
        assert_eq!(
            mesh.puppet_clipping_records[0].mask,
            "assets/clipping-mask.gtex"
        );
        assert_eq!(
            mesh.puppet_clipping_records[0].mask_resource.as_deref(),
            Some(expected_mask_resource.as_str())
        );
    }

    fn unique_test_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
    }

    fn write_test_gtex_header(
        path: &Path,
        width: u32,
        height: u32,
        format: u32,
        mip_count: u32,
        payload_bytes: u64,
    ) {
        let mut bytes = [0u8; 32];
        bytes[0..8].copy_from_slice(b"GDTEX002");
        bytes[8..12].copy_from_slice(&width.to_le_bytes());
        bytes[12..16].copy_from_slice(&height.to_le_bytes());
        bytes[16..20].copy_from_slice(&format.to_le_bytes());
        bytes[20..24].copy_from_slice(&mip_count.to_le_bytes());
        bytes[24..32].copy_from_slice(&payload_bytes.to_le_bytes());
        fs::write(path, bytes).expect("write test gtex");
    }

    fn test_mdlv0023_entry_payloads() -> (Vec<u8>, Vec<u8>) {
        let vertex_payload = (0..240).map(|value| value as u8).collect::<Vec<_>>();
        let index_payload = vec![0, 0, 1, 0, 2, 0];
        (vertex_payload, index_payload)
    }

    fn test_mdlv0023_bytes(vertex_payload: &[u8], index_payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MDLV0023\0");
        bytes.extend_from_slice(&0x0180_0009u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(b"materials/eye.json\0");
        bytes.extend_from_slice(&0x4u32.to_le_bytes());
        for _ in 0..6 {
            bytes.extend_from_slice(&0.0f32.to_le_bytes());
        }
        bytes.extend_from_slice(&0x0180_000fu32.to_le_bytes());
        bytes.extend_from_slice(&(vertex_payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(vertex_payload);
        bytes.extend_from_slice(&(index_payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(index_payload);
        bytes.push(0);
        bytes.push(1);
        bytes.extend_from_slice(&32u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&10u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&20u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&0x690u64.to_le_bytes());
        bytes.extend_from_slice(b"masks/clipping_mask_eye\0");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0x691u64.to_le_bytes());
        bytes.extend_from_slice(b"masks/clipping_mask_inner\0");
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes
    }
}
