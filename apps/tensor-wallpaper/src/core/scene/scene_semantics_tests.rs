
#[cfg(test)]
#[allow(clippy::approx_constant)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_full_scene_document_with_resources_and_shader_lowering() {
        let document: SceneDocument = serde_json::from_value(json!({
            "version": 1,
            "source": {
                "format": "wallpaper-engine-scene",
                "metadata": "metadata/source-scene.json",
                "entry": "scene.json"
            },
            "resources": [
                {
                    "id": "resource-background",
                    "type": "image",
                    "source": "assets/scene-resources/background.png",
                    "original_source": "background.png"
                }
            ],
            "nodes": [
                {
                    "id": "node-background",
                    "type": "image",
                    "resource": "resource-background"
                }
            ],
            "shader_lowering": {
                "target_runtime": "rendering-device-full-scene",
                "current_runtime": "rendering-device-scene-runtime",
                "progress_estimate_percent": 100,
                "full_scene_complete": true,
                "unsupported_boundaries": ["cursor-parallax-input-source"]
            }
        }))
        .unwrap();

        document.validate().unwrap();
        assert_eq!(
            document.referenced_paths(),
            vec![
                PackagePath::new("metadata/source-scene.json").unwrap(),
                PackagePath::new("assets/scene-resources/background.png").unwrap(),
            ]
        );
        assert_eq!(
            document.shader_lowering.progress_estimate_percent,
            Some(100)
        );
        assert!(document.shader_lowering.full_scene_complete);
        assert_eq!(
            document.shader_lowering.unsupported_boundaries,
            vec!["cursor-parallax-input-source".to_owned()]
        );
    }

    #[test]
    fn rejects_nodes_that_reference_unknown_resources() {
        let document: SceneDocument = serde_json::from_value(json!({
            "nodes": [
                {
                    "id": "node-background",
                    "type": "image",
                    "resource": "missing-resource"
                }
            ]
        }))
        .unwrap();

        assert!(document.validate().is_err());
    }

    #[test]
    fn puppet_clipping_active_source_requires_runtime_source_id() {
        let err = serde_json::from_value::<SceneMeshPuppetClippingActiveSource>(json!({
            "source_name": "eye-right",
            "scalar_bits": 1065353216,
            "source_scale": 6,
            "flags": 2,
            "transform_index": 4,
            "parameter0": -1.0,
            "parameter1": 0.5
        }))
        .expect_err("active source without record+0x00 source id must be rejected");

        assert!(err.to_string().contains("source_id"));
    }

    #[test]
    fn render_clear_color_becomes_first_snapshot_layer() {
        let document: SceneDocument = serde_json::from_value(json!({
            "size": { "width": 320, "height": 180 },
            "render": {
                "clear_color": "#102030",
                "clear_enabled": true
            },
            "nodes": [
                {
                    "id": "node-panel",
                    "type": "rectangle",
                    "color": "#ffffff",
                    "width": 50,
                    "height": 25
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
        assert_eq!(snapshot.layers.len(), 2);
        assert_eq!(snapshot.layers[0].id, "scene-render-clear-color");
        assert_eq!(snapshot.layers[0].kind, SceneNodeKind::Color);
        assert_eq!(snapshot.layers[0].color.as_deref(), Some("#102030"));
        assert_eq!(snapshot.layers[0].width, Some(320.0));
        assert_eq!(snapshot.layers[0].height, Some(180.0));
        assert_eq!(snapshot.layers[1].id, "node-panel");
    }

    #[test]
    fn wallpaper_engine_color_blend_mode_screen_reaches_snapshot_layer() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                {
                    "id": "resource-caustic",
                    "type": "image",
                    "source": "assets/caustic.gtex"
                }
            ],
            "nodes": [
                {
                    "id": "node-caustic",
                    "type": "image",
                    "resource": "resource-caustic",
                    "properties": {
                        "wallpaper_engine_blend": {
                            "colorBlendMode": 7
                        }
                    }
                }
            ]
        }))
        .unwrap();

        let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);

        assert_eq!(snapshot.layers.len(), 1);
        assert_eq!(snapshot.layers[0].id, "node-caustic");
        // WE colorBlendMode 7 is genuine Screen (1-(1-A)(1-B)) per decompiled
        // common_blending.h; previously mis-mapped to Max.
        assert_eq!(snapshot.layers[0].blend_mode, SceneBlendMode::Screen);
    }

    #[test]
    fn wallpaper_engine_color_blend_modes_from_real_scenes_reach_snapshot_layers() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "resource-a", "type": "image", "source": "assets/a.gtex" },
                { "id": "resource-b", "type": "image", "source": "assets/b.gtex" },
                { "id": "resource-c", "type": "image", "source": "assets/c.gtex" }
            ],
            "nodes": [
                {
                    "id": "node-shadow",
                    "type": "image",
                    "resource": "resource-a",
                    "properties": { "wallpaper_engine_blend": { "colorBlendMode": 2 } }
                },
                {
                    "id": "node-blue-solid",
                    "type": "rectangle",
                    "color": "#003ca4",
                    "width": 32,
                    "height": 16,
                    "properties": { "wallpaper_engine_blend": { "colorBlendMode": 28 } }
                },
                {
                    "id": "node-water",
                    "type": "image",
                    "resource": "resource-c",
                    "properties": { "wallpaper_engine_blend": { "colorBlendMode": 32 } }
                }
            ]
        }))
        .unwrap();

        let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);

        assert_eq!(snapshot.layers[0].blend_mode, SceneBlendMode::Multiply);
        assert_eq!(snapshot.layers[1].blend_mode, SceneBlendMode::HslColor);
        // WE colorBlendMode 32 = A*(1+B*a) (multiplicative brighten), now mapped to
        // Modulate; previously mis-mapped to Screen which caused the visible rectangle.
        assert_eq!(snapshot.layers[2].blend_mode, SceneBlendMode::Modulate);
    }

    #[test]
    fn wallpaper_engine_material_normal_blend_reaches_snapshot_layers() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "resource-eye", "type": "image", "source": "assets/eye.gtex" }
            ],
            "nodes": [
                {
                    "id": "node-eye",
                    "type": "image",
                    "resource": "resource-eye",
                    "properties": {
                        "material": {
                            "passes": [
                                { "shader": "effects/iris", "blending": "normal" }
                            ]
                        }
                    }
                }
            ]
        }))
        .unwrap();

        let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);

        assert_eq!(snapshot.layers.len(), 1);
        assert_eq!(snapshot.layers[0].blend_mode, SceneBlendMode::Normal);
    }

    #[test]
    fn wallpaper_engine_material_alphatocoverage_blend_reaches_snapshot_layers() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "resource-cutout", "type": "image", "source": "assets/cutout.gtex" }
            ],
            "nodes": [
                {
                    "id": "node-cutout",
                    "type": "image",
                    "resource": "resource-cutout",
                    "properties": {
                        "material": {
                            "passes": [
                                { "shader": "genericimage4", "blending": "alphatocoverage" }
                            ]
                        }
                    }
                }
            ]
        }))
        .unwrap();

        let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);

        assert_eq!(snapshot.layers.len(), 1);
        assert_eq!(
            snapshot.layers[0].blend_mode,
            SceneBlendMode::AlphaToCoverage
        );
    }

    #[test]
    fn iris_effect_mask_stays_effect_metadata_without_alpha_slot() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "resource-eye", "type": "image", "source": "assets/eye.gtex", "width": 663, "height": 230 },
                { "id": "resource-iris-mask", "type": "image", "source": "assets/iris-mask.gtex", "width": 331, "height": 115 }
            ],
            "nodes": [
                {
                    "id": "node-eye",
                    "type": "image",
                    "resource": "resource-eye",
                    "width": 663,
                    "height": 230,
                    "effects": [
                        {
                            "file": "effects/iris/effect.json",
                            "runtime": "wallpaper-engine-effect",
                            "passes": [
                                {
                                    "shader": "effects/iris",
                                    "blending": "normal",
                                    "texture_resources": [null, "resource-iris-mask"]
                                }
                            ]
                        }
                    ]
                }
            ]
        }))
        .unwrap();

        let mut layers = Vec::new();
        document.snapshot_sampled_image_layers_at_with_resolvers(
            0,
            |_| None,
            |_| None,
            &mut layers,
        );

        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].alpha_texture_slot, None);
        assert_eq!(
            layers[0].alpha_texture_mode,
            SceneAlphaTextureMode::Multiply
        );
        assert_eq!(layers[0].texture_slots.len(), 1);
        assert_eq!(layers[0].texture_slots[0].slot, 0);
        assert_eq!(
            layers[0].texture_slots[0].source.as_str(),
            "assets/eye.gtex"
        );
        assert_eq!(layers[0].image_effect_passes.len(), 1);
        let pass = &layers[0].image_effect_passes[0];
        assert_eq!(pass.effect_file, "effects/iris/effect.json");
        assert_eq!(pass.runtime.as_deref(), Some("builtin-iris-mask"));
        assert_eq!(pass.pass_index, 0);
        assert_eq!(pass.shader.as_deref(), Some("effects/iris"));
        assert_eq!(pass.blending.as_deref(), Some("normal"));
        assert_eq!(pass.texture_slots.len(), 1);
        assert_eq!(pass.texture_slots[0].slot, 1);
        assert_eq!(
            pass.texture_slots[0].source.as_str(),
            "assets/iris-mask.gtex"
        );
    }

    #[test]
    fn text_binding_resolver_overrides_static_snapshot_text() {
        let document: SceneDocument = serde_json::from_value(json!({
            "nodes": [
                {
                    "id": "node-clock",
                    "type": "text",
                    "text": "12:34",
                    "properties": {
                        "text_binding": {
                            "property": "scene.clock.local.strftime:%H:%M"
                        }
                    }
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let static_snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
        assert_eq!(static_snapshot.layers[0].text.as_deref(), Some("12:34"));

        let dynamic_snapshot = document.snapshot_at_with_resolvers(
            0,
            |_| None,
            |property| (property == "scene.clock.local.strftime:%H:%M").then(|| "23:45".to_owned()),
        );
        assert_eq!(dynamic_snapshot.layers[0].text.as_deref(), Some("23:45"));
    }

    #[test]
    fn visibility_condition_uses_runtime_choice_property() {
        let document: SceneDocument = serde_json::from_value(json!({
            "nodes": [
                {
                    "id": "default-theme",
                    "type": "rectangle",
                    "width": 10.0,
                    "height": 10.0,
                    "color": "#00b7ff",
                    "properties": {
                        "visibility_condition": {
                            "runtime": "wallpaper-engine-user-condition",
                            "property": "theme",
                            "condition": "1",
                            "default_visible": true,
                            "authored_value": true
                        }
                    }
                },
                {
                    "id": "solid-theme",
                    "type": "rectangle",
                    "width": 10.0,
                    "height": 10.0,
                    "color": "#ffffff",
                    "properties": {
                        "visibility_condition": {
                            "runtime": "wallpaper-engine-user-condition",
                            "property": "theme",
                            "condition": "2",
                            "default_visible": false,
                            "authored_value": false
                        }
                    }
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let default_snapshot = document.snapshot_at_with_resolvers(0, |_| None, |_| None);
        assert_eq!(default_snapshot.layers.len(), 1);
        assert_eq!(default_snapshot.layers[0].id, "default-theme");

        let switched_snapshot = document.snapshot_at_with_resolvers(
            0,
            |property| (property == "theme").then_some(2.0),
            |property| (property == "theme").then_some("2".to_owned()),
        );
        assert_eq!(switched_snapshot.layers.len(), 1);
        assert_eq!(switched_snapshot.layers[0].id, "solid-theme");
    }

    #[test]
    fn user_color_binding_overrides_static_snapshot_color() {
        let document: SceneDocument = serde_json::from_value(json!({
            "nodes": [
                {
                    "id": "node-panel",
                    "type": "rectangle",
                    "width": 10.0,
                    "height": 10.0,
                    "color": "#ffffff",
                    "properties": {
                        "color_binding": {
                            "runtime": "wallpaper-engine-user-color",
                            "property": "panel_color",
                            "default": "#003ca4"
                        }
                    }
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let default_snapshot = document.snapshot_at_with_resolvers(0, |_| None, |_| None);
        assert_eq!(default_snapshot.layers[0].color.as_deref(), Some("#003ca4"));

        let switched_snapshot = document.snapshot_at_with_resolvers(
            0,
            |_| None,
            |property| (property == "panel_color").then_some("0 0.59216 0.73725".to_owned()),
        );
        assert_eq!(
            switched_snapshot.layers[0].color.as_deref(),
            Some("#0097bc")
        );
    }

    #[test]
    fn sampled_image_snapshot_resolves_static_and_bound_tint() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "resource-shadow", "type": "image", "source": "assets/shadow.gtex" },
                { "id": "resource-tinted", "type": "image", "source": "assets/tinted.gtex" }
            ],
            "nodes": [
                {
                    "id": "node-shadow",
                    "type": "image",
                    "resource": "resource-shadow",
                    "color": "#000000"
                },
                {
                    "id": "node-tinted",
                    "type": "image",
                    "resource": "resource-tinted",
                    "color": "#ffffff",
                    "properties": {
                        "color_binding": {
                            "runtime": "wallpaper-engine-user-color",
                            "property": "tint_color",
                            "default": "#003ca4"
                        }
                    }
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let mut default_snapshot = Vec::new();
        document.snapshot_sampled_image_layers_at_with_resolvers(
            0,
            |_| None,
            |_| None,
            &mut default_snapshot,
        );
        assert_eq!(default_snapshot[0].tint, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(
            default_snapshot[1].tint,
            [0.0, 60.0_f32 / 255.0, 164.0_f32 / 255.0, 1.0]
        );

        let mut switched_snapshot = Vec::new();
        document.snapshot_sampled_image_layers_at_with_resolvers(
            0,
            |_| None,
            |property| (property == "tint_color").then_some("0 0 0".to_owned()),
            &mut switched_snapshot,
        );
        assert_eq!(switched_snapshot[1].tint, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn audio_cue_active_conditions_filter_snapshot_audio() {
        let document: SceneDocument = serde_json::from_value(json!({
            "nodes": [
                {
                    "id": "node-audio",
                    "type": "audio",
                    "audio": [
                        {
                            "source": "voice.mp3",
                            "start_silent": true,
                            "active_conditions": [
                                { "property": "scene.controller.idle.active" },
                                { "property": "voice_enabled", "equals": 1.0 }
                            ]
                        }
                    ]
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let inactive = document.snapshot_at_with_property_resolver(0, |property| match property {
            "scene.controller.idle.active" => Some(1.0),
            "voice_enabled" => Some(0.0),
            _ => None,
        });
        assert!(inactive.layers[0].audio.is_empty());

        let active = document.snapshot_at_with_property_resolver(0, |property| match property {
            "scene.controller.idle.active" => Some(1.0),
            "voice_enabled" => Some(1.0),
            _ => None,
        });
        assert_eq!(active.layers[0].audio.len(), 1);
        assert_eq!(active.layers[0].audio[0].start_silent, Some(false));
    }

    #[test]
    fn disabled_render_clear_color_does_not_emit_snapshot_layer() {
        let document: SceneDocument = serde_json::from_value(json!({
            "render": {
                "clear_color": "#102030",
                "clear_enabled": false
            },
            "nodes": [
                {
                    "id": "node-panel",
                    "type": "rectangle",
                    "color": "#ffffff"
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
        assert_eq!(snapshot.layers.len(), 1);
        assert_eq!(snapshot.layers[0].id, "node-panel");
    }

    #[test]
    fn timeline_and_property_bindings_drive_scene_geometry_fields() {
        let document: SceneDocument = serde_json::from_value(json!({
            "nodes": [
                {
                    "id": "node-panel",
                    "type": "rectangle",
                    "color": "#ffffff",
                    "width": 100,
                    "height": 50,
                    "corner_radius": 4
                }
            ],
            "timelines": [
                {
                    "id": "panel-size",
                    "target_node": "node-panel",
                    "channels": [
                        {
                            "property": "width",
                            "keyframes": [
                                { "time_ms": 0, "value": 100 },
                                { "time_ms": 1000, "value": 200 }
                            ]
                        },
                        {
                            "property": "height",
                            "keyframes": [
                                { "time_ms": 0, "value": 50 },
                                { "time_ms": 1000, "value": 150 }
                            ]
                        }
                    ]
                }
            ],
            "property_bindings": [
                {
                    "property": "panel_radius",
                    "target_node": "node-panel",
                    "target": "corner-radius"
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(500, |property| {
            if property == "panel_radius" {
                Some(12.0)
            } else {
                None
            }
        });
        assert_eq!(snapshot.layers[0].width, Some(150.0));
        assert_eq!(snapshot.layers[0].height, Some(100.0));
        assert_eq!(snapshot.layers[0].corner_radius, Some(12.0));
    }

    #[test]
    fn looping_timeline_channels_apply_time_offset_for_animation_phase() {
        let document: SceneDocument = serde_json::from_value(json!({
            "nodes": [
                {
                    "id": "node-panel",
                    "type": "rectangle",
                    "color": "#ffffff"
                }
            ],
            "timelines": [
                {
                    "id": "panel-slide",
                    "target_node": "node-panel",
                    "channels": [
                        {
                            "property": "x",
                            "loop": true,
                            "time_offset_ms": 500,
                            "keyframes": [
                                { "time_ms": 0, "value": 0 },
                                { "time_ms": 1000, "value": 100 }
                            ]
                        }
                    ]
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
        assert_eq!(snapshot.layers[0].transform.x, 50.0);
    }

    #[test]
    fn puppet_animation_layers_sample_skinned_mesh_vertices_over_time() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                {
                    "id": "resource-puppet",
                    "type": "image",
                    "source": "assets/puppet.gtex"
                }
            ],
            "nodes": [
                {
                    "id": "node-puppet",
                    "type": "image",
                    "resource": "resource-puppet",
                    "width": 32,
                    "height": 32,
                    "mesh": {
                        "vertices": [
                            { "x": 20.0, "y": 0.0, "u": 0.0, "v": 0.0 },
                            { "x": 20.0, "y": 1.0, "u": 0.0, "v": 1.0 },
                            { "x": 21.0, "y": 0.0, "u": 1.0, "v": 0.0 }
                        ],
                        "indices": [0, 1, 2],
                        "skin": {
                            "bones": [
                                {
                                    "bind": {
                                        "translation": [0.0, 0.0, 0.0],
                                        "rotation": [0.0, 0.0, 0.0],
                                        "scale": [1.0, 1.0, 1.0]
                                    }
                                },
                                {
                                    "parent": 0,
                                    "bind": {
                                        "translation": [10.0, 0.0, 0.0],
                                        "rotation": [0.0, 0.0, 0.0],
                                        "scale": [1.0, 1.0, 1.0]
                                    }
                                }
                            ],
                            "vertices": [
                                { "bone_indices": [1, 0, 0, 0], "weights": [1.0, 0.0, 0.0, 0.0] },
                                { "bone_indices": [1, 0, 0, 0], "weights": [1.0, 0.0, 0.0, 0.0] },
                                { "bone_indices": [1, 0, 0, 0], "weights": [1.0, 0.0, 0.0, 0.0] }
                            ]
                        },
                        "puppet_clips": [
                            {
                                "id": 7,
                                "fps": 1.0,
                                "frame_count": 1,
                                "looping": false,
                                "bones": [
                                    {
                                        "frames": [
                                            { "translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0], "scale": [1.0, 1.0, 1.0] },
                                            { "translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0], "scale": [1.0, 1.0, 1.0] }
                                        ]
                                    },
                                    {
                                        "frames": [
                                            { "translation": [10.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0], "scale": [1.0, 1.0, 1.0] },
                                            { "translation": [10.0, 0.0, 0.0], "rotation": [0.0, 0.0, 1.5707963267948966], "scale": [1.0, 1.0, 1.0] }
                                        ]
                                    }
                                ]
                            }
                        ],
                        "puppet_clipping_records": [
                            {
                                "mask": "masks/clipping_mask_eye",
                                "mask_resource": "assets/clipping-mask.gtex",
                                "bones": [1],
                                "frame_keys": [0]
                            }
                        ]
                    },
                    "puppet_animation_layers": [
                        { "clip_id": 7, "rate": 1.0, "blend": 1.0 }
                    ]
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let mut first = Vec::new();
        document.snapshot_sampled_image_layers_at_with_resolvers(0, |_| None, |_| None, &mut first);
        let first_mesh = first[0].mesh.as_ref().expect("first sampled mesh");
        assert_eq!(first_mesh.indices, vec![0, 1, 2]);
        assert!((first_mesh.vertices[0].x - 20.0).abs() < 0.000_001);
        assert!(first_mesh.vertices[0].y.abs() < 0.000_001);

        let mut later = Vec::new();
        document.snapshot_sampled_image_layers_at_with_resolvers(
            1000,
            |_| None,
            |_| None,
            &mut later,
        );
        let later_mesh = later[0].mesh.as_ref().expect("later sampled mesh");
        assert_eq!(later_mesh.indices, vec![0, 1, 2]);
        assert!((later_mesh.vertices[0].x - 10.0).abs() < 0.000_001);
        assert!((later_mesh.vertices[0].y - 10.0).abs() < 0.000_001);
        assert_eq!(later_mesh.vertices[0].u, first_mesh.vertices[0].u);
        assert_eq!(later_mesh.vertices[0].v, first_mesh.vertices[0].v);
        assert!(later_mesh.skin.is_some());
        assert_eq!(later_mesh.puppet_clipping_records.len(), 1);
        assert_eq!(
            later_mesh.puppet_clipping_records[0]
                .mask_resource
                .as_deref(),
            Some("assets/clipping-mask.gtex")
        );
        assert_eq!(later_mesh.puppet_clipping_records[0].bones, vec![1]);
    }

    include!("scene_semantics_tests/puppet_and_particles.rs");
}
