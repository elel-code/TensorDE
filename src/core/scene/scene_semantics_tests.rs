
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_full_scene_document_with_resources_and_native_lowering() {
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
            "native_lowering": {
                "target_runtime": "native-vulkan-full-scene",
                "current_runtime": "native-vulkan-scene-runtime",
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
            document.native_lowering.progress_estimate_percent,
            Some(100)
        );
        assert!(document.native_lowering.full_scene_complete);
        assert_eq!(
            document.native_lowering.unsupported_boundaries,
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
        assert_eq!(pass.runtime.as_deref(), Some("native-iris-mask"));
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

    #[test]
    fn puppet_animation_lock_transforms_samples_opacity_without_moving_bones() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "resource-puppet", "type": "image", "source": "assets/puppet.gtex" }
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
                                            { "translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0], "scale": [1.0, 1.0, 1.0], "opacity": 1.0 },
                                            { "translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0], "scale": [1.0, 1.0, 1.0], "opacity": 1.0 }
                                        ]
                                    },
                                    {
                                        "frames": [
                                            { "translation": [10.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0], "scale": [1.0, 1.0, 1.0], "opacity": 1.0 },
                                            { "translation": [10.0, 0.0, 0.0], "rotation": [0.0, 0.0, 1.5707963267948966], "scale": [1.0, 1.0, 1.0], "opacity": 0.25 }
                                        ]
                                    }
                                ]
                            }
                        ]
                    },
                    "puppet_animation_layers": [
                        { "clip_id": 7, "rate": 1.0, "blend": 1.0, "lock_transforms": true }
                    ]
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let mut later = Vec::new();
        document.snapshot_sampled_image_layers_at_with_resolvers(
            1000,
            |_| None,
            |_| None,
            &mut later,
        );

        let mesh = later[0].mesh.as_ref().expect("locked puppet mesh");
        assert!((mesh.vertices[0].x - 20.0).abs() < 0.000_001);
        assert!(mesh.vertices[0].y.abs() < 0.000_001);
        assert!((mesh.vertices[0].opacity - 0.25).abs() < 0.000_001);
    }

    #[test]
    fn puppet_attachment_children_use_sampled_attachment_pose() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                {
                    "id": "resource-puppet",
                    "type": "image",
                    "source": "assets/puppet.gtex"
                },
                {
                    "id": "resource-eye",
                    "type": "image",
                    "source": "assets/eye.gtex"
                }
            ],
            "nodes": [
                {
                    "id": "node-puppet",
                    "type": "image",
                    "resource": "resource-puppet",
                    "width": 32,
                    "height": 32,
                    "transform": { "x": 100.0, "y": 200.0 },
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
                            ],
                            "attachments": [
                                {
                                    "name": "eye",
                                    "bone_index": 1,
                                    "local_position": [10.0, 0.0, 0.0],
                                    "bind_position": [999.0, 999.0, 0.0]
                                }
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
                        ]
                    },
                    "puppet_animation_layers": [
                        { "clip_id": 7, "rate": 1.0, "blend": 1.0 }
                    ],
                    "children": [
                        {
                            "id": "node-eye",
                            "type": "image",
                            "resource": "resource-eye",
                            "width": 8,
                            "height": 4,
                            "transform": { "x": 0.0, "y": 0.0 },
                            "puppet_attachment": "eye"
                        }
                    ]
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let first = document.snapshot_at_with_property_resolver(0, |_| None);
        let first_eye = first
            .layers
            .iter()
            .find(|layer| layer.id == "node-eye")
            .expect("first eye layer");
        assert!((first_eye.transform.x - 120.0).abs() < 0.000_001);
        assert!((first_eye.transform.y - 200.0).abs() < 0.000_001);
        assert!(first_eye.transform.rotation_deg.abs() < 0.000_001);

        let later = document.snapshot_at_with_property_resolver(1000, |_| None);
        let later_eye = later
            .layers
            .iter()
            .find(|layer| layer.id == "node-eye")
            .expect("later eye layer");
        assert!((later_eye.transform.x - 110.0).abs() < 0.000_001);
        assert!((later_eye.transform.y - 210.0).abs() < 0.000_001);
        assert!((later_eye.transform.rotation_deg - 90.0).abs() < 0.000_001);

        let mut sampled = Vec::new();
        document.snapshot_sampled_image_layers_at_with_resolvers(
            1000,
            |_| None,
            |_| None,
            &mut sampled,
        );
        assert!((sampled[1].transform.x - 110.0).abs() < 0.000_001);
        assert!((sampled[1].transform.y - 210.0).abs() < 0.000_001);
        assert!((sampled[1].transform.rotation_deg - 90.0).abs() < 0.000_001);
    }

    #[test]
    fn spritesheet_properties_drive_time_sampled_texture_region() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                {
                    "id": "resource-atlas",
                    "type": "image",
                    "source": "assets/atlas.gtex"
                }
            ],
            "nodes": [
                {
                    "id": "node-atlas",
                    "type": "image",
                    "resource": "resource-atlas",
                    "properties": {
                        "spritesheet": {
                            "type": "atlas-grid",
                            "atlas_width": 300,
                            "atlas_height": 400,
                            "frame_width": 100,
                            "frame_height": 100,
                            "columns": 3,
                            "rows": 4,
                            "frame_count": 12,
                            "fps": 12,
                            "loop": true
                        }
                    }
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let first = document.snapshot_at_with_property_resolver(0, |_| None);
        assert_eq!(
            first.layers[0].texture_region,
            Some(SceneTextureRegion {
                u_min: 0.0,
                v_min: 0.0,
                u_max: 1.0 / 3.0,
                v_max: 0.25,
                frame_index: 0,
                frame_count: 12,
                columns: 3,
                rows: 4,
                fps: Some(12.0),
                loop_playback: true,
            })
        );

        let sixth = document.snapshot_at_with_property_resolver(417, |_| None);
        assert_eq!(
            sixth.layers[0].texture_region,
            Some(SceneTextureRegion {
                u_min: 2.0 / 3.0,
                v_min: 0.25,
                u_max: 1.0,
                v_max: 0.5,
                frame_index: 5,
                frame_count: 12,
                columns: 3,
                rows: 4,
                fps: Some(12.0),
                loop_playback: true,
            })
        );
    }

    #[test]
    fn waterwaves_effect_stays_gpu_image_space_without_native_motion() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                {
                    "id": "resource-image",
                    "type": "image",
                    "source": "assets/image.gtex"
                }
            ],
            "nodes": [
                {
                    "id": "node-water",
                    "type": "image",
                    "resource": "resource-image",
                    "width": 100,
                    "height": 100,
                    "effects": [
                        {
                            "file": "effects/waterwaves/effect.json",
                            "passes": [
                                {
                                    "constant_shader_values": {
                                        "speed": 1.0,
                                        "strength": 0.25,
                                        "direction": 0.0,
                                        "scale": 8.0
                                    }
                                }
                            ]
                        }
                    ]
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let mut layers = Vec::new();
        document.snapshot_sampled_image_layers_at_with_resolvers(
            1000,
            |_| None,
            |_| None,
            &mut layers,
        );
        assert_eq!(layers.len(), 1);
        assert!(!layers[0].effect_motion.is_active());
    }

    #[test]
    fn waterwave_effect_preserves_parameters_in_image_effect_passes_not_native_motion() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                {
                    "id": "resource-image",
                    "type": "image",
                    "source": "assets/image.gtex"
                }
            ],
            "nodes": [
                {
                    "id": "node-water",
                    "type": "image",
                    "resource": "resource-image",
                    "width": 100,
                    "height": 100,
                    "effects": [
                        {
                            "file": "effects/waterwaves/effect.json",
                            "passes": [
                                {
                                    "constant_shader_values": {
                                        "speed": 2.0,
                                        "speed2": 3.0,
                                        "strength": 0.1,
                                        "direction": 0.0,
                                        "direction2": 1.57079632679,
                                        "scale": 12.0,
                                        "scale2": 6.0,
                                        "offset2": 0.25
                                    }
                                }
                            ]
                        }
                    ]
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let mut layers = Vec::new();
        document.snapshot_sampled_image_layers_at_with_resolvers(
            1000,
            |_| None,
            |_| None,
            &mut layers,
        );
        let motion = layers[0].effect_motion;
        assert_eq!(motion.wave_count, 0);
        assert_eq!(motion.wave2_count, 0);
        assert_eq!(
            layers[0].image_effect_passes[0].constant_shader_values["speed2"],
            json!(3.0)
        );
        assert_eq!(
            layers[0].image_effect_passes[0].constant_shader_values["direction2"],
            json!(1.57079632679)
        );
    }

    #[test]
    fn foliage_sway_effect_stays_gpu_image_space_without_native_motion() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                {
                    "id": "resource-image",
                    "type": "image",
                    "source": "assets/image.gtex"
                }
            ],
            "nodes": [
                {
                    "id": "node-skirt",
                    "type": "image",
                    "resource": "resource-image",
                    "width": 100,
                    "height": 100,
                    "effects": [
                        {
                            "file": "effects/workshop/2790231929/foliagesway/effect.json",
                            "passes": [
                                {
                                    "constant_shader_values": {
                                        "phase": 2.0,
                                        "power": 2.0,
                                        "ratio": 2.0,
                                        "scale": 0.05,
                                        "scrolldirection": 0.0,
                                        "speeduv": 5.0,
                                        "strength": 0.5
                                    }
                                }
                            ]
                        }
                    ]
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let mut layers = Vec::new();
        document.snapshot_sampled_image_layers_at_with_resolvers(
            1000,
            |_| None,
            |_| None,
            &mut layers,
        );
        let motion = layers[0].effect_motion;
        assert_eq!(motion.sway_count, 0);
        assert!(!motion.is_active());
    }

    #[test]
    fn waterwave_effect_keeps_layer_origin_and_skips_native_vertex_deformation() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                {
                    "id": "resource-image",
                    "type": "image",
                    "source": "assets/image.gtex"
                }
            ],
            "nodes": [
                {
                    "id": "node-water",
                    "type": "image",
                    "resource": "resource-image",
                    "width": 100,
                    "height": 100,
                    "transform": { "x": 25, "y": 50 },
                    "effects": [
                        {
                            "file": "effects/waterwaves/effect.json",
                            "passes": [
                                {
                                    "constant_shader_values": {
                                        "speed": 3.0,
                                        "strength": 1.0,
                                        "direction": 1.0,
                                        "scale": 8.0
                                    }
                                }
                            ]
                        }
                    ]
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let mut layers = Vec::new();
        document.snapshot_sampled_image_layers_at_with_resolvers(
            1000,
            |_| None,
            |_| None,
            &mut layers,
        );
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].transform.x, 25.0);
        assert_eq!(layers[0].transform.y, 50.0);
        assert!(!layers[0].effect_motion.is_active());
    }

    #[test]
    fn watercaustics_effect_stays_on_base_layer_for_gpu_graph() {
        let document: SceneDocument = serde_json::from_value(json!({
            "nodes": [
                {
                    "id": "node-water-layer",
                    "type": "rectangle",
                    "width": 400,
                    "height": 200,
                    "opacity": 0.8,
                    "effects": [
                        {
                            "file": "effects/watercaustics/effect.json",
                            "runtime": "native-water-caustics",
                            "id": 641,
                            "passes": [
                                {
                                    "constant_shader_values": {
                                        "ui_editor_properties_brightness": 2.48,
                                        "ui_editor_properties_speed": 0.3,
                                        "ui_editor_properties_distortion": 1.0,
                                        "ui_editor_properties_color_start": "0 0.7 1"
                                    }
                                }
                            ]
                        }
                    ]
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(1000, |_| None);
        let caustic_layers = snapshot
            .layers
            .iter()
            .filter(|layer| layer.id.contains("water-caustics"))
            .collect::<Vec<_>>();
        assert!(caustic_layers.is_empty());
        assert_eq!(snapshot.layers.len(), 1);
        assert_eq!(snapshot.layers[0].id, "node-water-layer");
        assert_eq!(snapshot.layers[0].image_effect_passes.len(), 1);
    }

    #[test]
    fn parallax_properties_offset_node_transforms_by_depth() {
        let document: SceneDocument = serde_json::from_value(json!({
            "render": {
                "parallax": { "amount": 10 }
            },
            "nodes": [
                {
                    "id": "near",
                    "type": "rectangle",
                    "color": "#ffffff",
                    "transform": { "x": 3, "y": 4 },
                    "parallax_depth": 0.5
                },
                {
                    "id": "flat",
                    "type": "rectangle",
                    "color": "#ffffff",
                    "transform": { "x": 1, "y": 2 }
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(0, |property| match property {
            "scene.parallax.x" => Some(2.0),
            "scene.parallax.y" => Some(-1.0),
            _ => None,
        });
        assert_eq!(snapshot.layers[0].transform.x, 13.0);
        assert_eq!(snapshot.layers[0].transform.y, -1.0);
        assert_eq!(snapshot.layers[0].parallax_depth, Some(0.5));
        assert_eq!(snapshot.layers[1].transform.x, 1.0);
        assert_eq!(snapshot.layers[1].transform.y, 2.0);
    }

    #[test]
    fn parent_rotation_offsets_child_transform_coordinates() {
        let document: SceneDocument = serde_json::from_value(json!({
            "nodes": [
                {
                    "id": "rotating-parent",
                    "type": "group",
                    "transform": {
                        "x": 10,
                        "y": 20,
                        "scale_x": 2,
                        "scale_y": 3,
                        "rotation_deg": 90
                    },
                    "children": [
                        {
                            "id": "child-panel",
                            "type": "rectangle",
                            "color": "#ffffff",
                            "transform": {
                                "x": 5,
                                "y": 2,
                                "rotation_deg": 15
                            }
                        }
                    ]
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);

        assert_eq!(snapshot.layers.len(), 1);
        assert_eq!(snapshot.layers[0].id, "child-panel");
        assert!((snapshot.layers[0].transform.x - 4.0).abs() < 0.000001);
        assert!((snapshot.layers[0].transform.y - 30.0).abs() < 0.000001);
        assert!((snapshot.layers[0].transform.rotation_deg - 105.0).abs() < f64::EPSILON);
        assert!((snapshot.layers[0].transform.scale_x - 2.0).abs() < f64::EPSILON);
        assert!((snapshot.layers[0].transform.scale_y - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn particle_emitter_expands_to_native_rectangle_layers() {
        let document: SceneDocument = serde_json::from_value(json!({
            "nodes": [
                {
                    "id": "spark-emitter",
                    "type": "particle-emitter",
                    "opacity": 0.5,
                    "transform": { "x": 50, "y": 25 },
                    "properties": {
                        "particle": {
                            "count": 3,
                            "seed": 11,
                            "lifetime_ms": 1000,
                            "size": 12,
                            "speed": 0,
                            "spawn_width": 0,
                            "spawn_height": 0,
                            "fade": false,
                            "color": "#ffaa00"
                        }
                    }
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(250, |_| None);

        assert_eq!(snapshot.layers.len(), 3);
        assert_eq!(snapshot.layers[0].id, "spark-emitter::particle-0");
        assert_eq!(snapshot.layers[0].kind, SceneNodeKind::Rectangle);
        assert_eq!(snapshot.layers[0].color.as_deref(), Some("#ffaa00"));
        assert_eq!(snapshot.layers[0].width, Some(12.0));
        assert_eq!(snapshot.layers[0].height, Some(12.0));
        assert_eq!(snapshot.layers[0].opacity, 0.5);
        assert_eq!(snapshot.layers[0].transform.x, 50.0);
        assert_eq!(snapshot.layers[0].transform.y, 25.0);
        assert!(
            snapshot
                .layers
                .iter()
                .all(|layer| layer.kind != SceneNodeKind::ParticleEmitter)
        );
    }

    #[test]
    fn particle_emitter_with_resource_expands_to_sampled_image_layers() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                {
                    "id": "resource-spark",
                    "type": "image",
                    "source": "assets/scene-resources/spark.gtex"
                }
            ],
            "nodes": [
                {
                    "id": "spark-emitter",
                    "type": "particle-emitter",
                    "resource": "resource-spark",
                    "properties": {
                        "particle": {
                            "count": 2,
                            "seed": 11,
                            "lifetime_ms": 1000,
                            "size": 12,
                            "speed": 0,
                            "spawn_width": 0,
                            "spawn_height": 0,
                            "fade": false
                        },
                        "spritesheet": {
                            "type": "atlas-grid",
                            "atlas_width": 64,
                            "atlas_height": 32,
                            "frame_width": 32,
                            "frame_height": 32,
                            "columns": 2,
                            "rows": 1,
                            "frame_count": 2,
                            "fps": 2,
                            "loop": true
                        }
                    }
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(500, |_| None);

        assert_eq!(snapshot.layers.len(), 2);
        assert_eq!(snapshot.layers[0].kind, SceneNodeKind::Image);
        assert_eq!(
            snapshot.layers[0].source,
            Some(PackagePath::new("assets/scene-resources/spark.gtex").unwrap())
        );
        assert_eq!(
            snapshot.layers[0].texture_region,
            Some(SceneTextureRegion {
                u_min: 0.5,
                v_min: 0.0,
                u_max: 1.0,
                v_max: 1.0,
                frame_index: 1,
                frame_count: 2,
                columns: 2,
                rows: 1,
                fps: Some(2.0),
                loop_playback: true,
            })
        );
        assert!(snapshot.layers.iter().all(|layer| layer.source.is_some()));
    }

    #[test]
    fn particle_emitter_inherits_rotated_parent_transform() {
        let document: SceneDocument = serde_json::from_value(json!({
            "nodes": [
                {
                    "id": "rotating-parent",
                    "type": "group",
                    "transform": { "x": 10, "y": 20, "rotation_deg": 90 },
                    "children": [
                        {
                            "id": "spark-emitter",
                            "type": "particle-emitter",
                            "transform": { "x": 5, "y": 0 },
                            "properties": {
                                "particle": {
                                    "count": 1,
                                    "seed": 11,
                                    "lifetime_ms": 1000,
                                    "size": 12,
                                    "speed": 0,
                                    "spawn_width": 0,
                                    "spawn_height": 0,
                                    "fade": false
                                }
                            }
                        }
                    ]
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(250, |_| None);

        assert_eq!(snapshot.layers.len(), 1);
        assert_eq!(snapshot.layers[0].id, "spark-emitter::particle-0");
        assert!((snapshot.layers[0].transform.x - 10.0).abs() < 0.000001);
        assert!((snapshot.layers[0].transform.y - 25.0).abs() < 0.000001);
    }
}
