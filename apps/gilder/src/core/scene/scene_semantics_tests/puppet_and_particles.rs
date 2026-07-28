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
