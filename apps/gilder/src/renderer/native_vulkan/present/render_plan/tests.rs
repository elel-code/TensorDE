    use super::*;
    use crate::core::path::PackagePath;
    use crate::core::scene::{SceneEffectUvTransform, SceneMeshVertex};
    use crate::core::{FitMode, SceneBlendMode, SceneNodeKind, ScenePathFillRule, SceneSystems};
    use crate::renderer::native_vulkan::{NativeVulkanClearColor, NativeVulkanRenderItem};
    use crate::renderer::{
        SceneDisplayPlan, SceneRenderImageEffectPass, SceneRenderLayer, SceneRenderTextureSlot,
    };
    use std::path::PathBuf;

    #[test]
    fn scene_color_display_overrides_default_clear_color() {
        let fallback = NativeVulkanClearColor {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        let item = NativeVulkanRenderItem::Scene {
            scene: Box::new(crate::renderer::native_vulkan::NativeVulkanSceneRenderItem {
            output_name: "HDMI-A-1".to_owned(),
            scene_source: Some(PathBuf::from("/tmp/scene.json")),
            display: Some(SceneDisplayPlan::Color {
                color: "#102030".to_owned(),
            }),
            display_image: None,
            display_color: Some("#102030".to_owned()),
            manifest_max_fps: Some(60),
            layer_count: 0,
            layers: Vec::new(),
            scene_systems: SceneSystems::default(),
            audio_cue_count: 0,
            bound_properties: Vec::new(),
            timeline_animation_count: 0,
            timeline_animated_layer_count: 0,
            puppet_animation_layer_count: 0,
            property_binding_count: 0,
            cursor_parallax_input_ready: false,
            dynamic_topology_required: false,
            scene_engine: None,
            scene_scenescript_binding_count: 0,
            scene_material_graph_count: 0,
            scene_material_graph_resource_count: 0,
            scene_effect_graph_count: 0,
            scene_mesh_count: 0,
            scene_mesh_vertex_count: 0,
            scene_mesh_index_count: 0,
            scene_audio_response_binding_count: 0,
            unsupported_scene_features: Vec::new(),
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            target_max_fps: Some(60),
                renderer_status: "deterministic-scene-snapshot-ready-for-vulkan-passes",
            }),
        };

        let color = native_vulkan_render_item_clear_color(&item, fallback);

        assert!((color.r - 16.0 / 255.0).abs() < f32::EPSILON);
        assert!((color.g - 32.0 / 255.0).abs() < f32::EPSILON);
        assert!((color.b - 48.0 / 255.0).abs() < f32::EPSILON);
        assert_eq!(color.a, 1.0);
    }

    #[test]
    fn effect_uv_transform_selection_matches_mask_slot_before_fallback() {
        let passes = vec![
            image_effect_pass_with_transform(2, [0.5, 0.5], [0.5, 0.0]),
            image_effect_pass_with_transform(1, [1.0, 1.0], [0.25, 0.0]),
        ];

        let selected = native_vulkan_scene_effect_uv_transform_for_render_passes(&passes, Some(1))
            .expect("mask slot transform");
        assert_eq!(selected.mask_slot, 1);
        assert_eq!(selected.offset, [0.25, 0.0]);

        let fallback = native_vulkan_scene_effect_uv_transform_for_render_passes(&passes, None)
            .expect("fallback transform");
        assert_eq!(fallback.mask_slot, 2);
    }

    #[test]
    fn scene_draw_plan_keeps_opacity_mask_duplicate_as_independent_draw() {
        let composite_key = Some(SceneLayerCompositeKey {
            parent_source_id: Some("937".to_owned()),
            puppet_attachment: "eye".to_owned(),
            original_path: "models/eye.json".to_owned(),
            base_source: PackagePath::new("assets/eye.gtex").unwrap(),
        });
        let mut base = scene_layer("eye-base", SceneNodeKind::Image);
        base.source = Some(PathBuf::from("/tmp/eye.gtex"));
        base.composite_key = composite_key.clone();
        base.texture_slots = vec![SceneRenderTextureSlot {
            slot: 0,
            source: PathBuf::from("/tmp/eye.gtex"),
            width: Some(663),
            height: Some(230),
        }];
        base.mesh = Some(Arc::new(SceneMesh {
            vertices: vec![
                SceneMeshVertex {
                    x: -10.0,
                    y: -20.0,
                    u: 0.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: 10.0,
                    y: -20.0,
                    u: 1.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: -10.0,
                    y: 20.0,
                    u: 0.0,
                    v: 1.0,
                    opacity: 1.0,
                },
            ],
            indices: vec![0, 1, 2],
            skin: None,
            puppet_clips: Vec::new(),
            puppet_clipping_records: Vec::new(),
            puppet_clipping_active_sources: Vec::new(),
        }));
        let mut carrier = scene_layer("eye-opacity", SceneNodeKind::Image);
        carrier.source = Some(PathBuf::from("/tmp/eye.gtex"));
        carrier.composite_key = composite_key.clone();
        carrier.alpha_texture_slot = Some(1);
        carrier.alpha_texture_mode = SceneRenderAlphaTextureMode::Multiply;
        carrier.texture_slots = vec![
            SceneRenderTextureSlot {
                slot: 0,
                source: PathBuf::from("/tmp/eye.gtex"),
                width: Some(663),
                height: Some(230),
            },
            SceneRenderTextureSlot {
                slot: 1,
                source: PathBuf::from("/tmp/eye-mask.gtex"),
                width: Some(331),
                height: Some(115),
            },
        ];
        carrier.image_effect_passes = vec![SceneRenderImageEffectPass {
            effect_file: "effects/opacity/effect.json".to_owned(),
            runtime: Some("native-opacity-mask".to_owned()),
            pass_index: 0,
            command: None,
            source: None,
            target: None,
            binds: Default::default(),
            fbos: Default::default(),
            shader: Some("effects/opacity".to_owned()),
            blending: Some("normal".to_owned()),
            depthtest: None,
            depthwrite: None,
            cullmode: None,
            alphawriting: None,
            texture_slots: vec![SceneRenderTextureSlot {
                slot: 1,
                source: PathBuf::from("/tmp/eye-mask.gtex"),
                width: Some(331),
                height: Some(115),
            }],
            effect_uv_transform: None,
            combos: Default::default(),
            constant_shader_values: Default::default(),
        }];
        carrier.mesh = base.mesh.clone();

        let plan = native_vulkan_scene_draw_plan_from_layers(
            0,
            None,
            FitMode::Cover,
            false,
            true,
            &[base, carrier],
        );

        assert_eq!(plan.draw_ops.len(), 2);
        assert_eq!(plan.draw_ops[0].layer_id, "eye-base");
        assert_eq!(plan.draw_ops[0].alpha_texture_slot, None);
        assert_eq!(
            plan.draw_ops[0].alpha_texture_mode,
            SceneRenderAlphaTextureMode::Multiply
        );
        assert!(plan.draw_ops[0].effect_uv_space.is_none());
        assert_eq!(plan.draw_ops[0].composite_key, composite_key);
        assert_eq!(plan.draw_ops[0].texture_slots.len(), 1);
        assert_eq!(plan.draw_ops[1].layer_id, "eye-opacity");
        assert_eq!(plan.draw_ops[1].alpha_texture_slot, Some(1));
        assert_eq!(
            plan.draw_ops[1].alpha_texture_mode,
            SceneRenderAlphaTextureMode::Multiply
        );
        assert_eq!(
            plan.draw_ops[1].effect_uv_space.map(|space| space.mapping),
            Some(NativeVulkanSceneEffectUvMapping::MaterialUvTransformed {
                scale_u: 1.0,
                scale_v: 1.0,
                offset_u: 0.0,
                offset_v: 0.0
            })
        );
        assert_eq!(plan.draw_ops[1].composite_key, composite_key);
        assert_eq!(plan.draw_ops[1].texture_slots.len(), 2);
        assert_eq!(plan.draw_ops[1].image_effect_passes.len(), 1);
        assert_eq!(
            plan.draw_ops[1].image_effect_passes[0].effect_file,
            "effects/opacity/effect.json"
        );
        assert_eq!(
            plan.draw_ops[1].texture_slots[1].source,
            PathBuf::from("/tmp/eye-mask.gtex")
        );
    }

    fn scene_layer(id: &str, kind: SceneNodeKind) -> SceneRenderLayer {
        SceneRenderLayer {
            id: id.to_owned(),
            kind,
            source: None,
            texture_slots: Vec::new(),
            alpha_texture_slot: None,
            alpha_texture_mode: Default::default(),
            image_effect_passes: Vec::new(),
            composite_key: None,
            texture_region: None,
            effect_motion: Default::default(),
            blend_mode: SceneBlendMode::Alpha,
            audio: Vec::new(),
            color: None,
            stroke_color: None,
            stroke_width: None,
            corner_radius: None,
            width: Some(100.0),
            height: Some(50.0),
            mesh: None,
            text: None,
            font_size: None,
            font_family: None,
            font_source: None,
            font_weight: None,
            text_align: None,
            path_data: None,
            path_fill_rule: ScenePathFillRule::Nonzero,
            fit: FitMode::Cover,
            opacity: 1.0,
            transform: SceneTransform::default(),
        }
    }

    fn image_effect_pass_with_transform(
        mask_slot: u32,
        scale: [f64; 2],
        offset: [f64; 2],
    ) -> SceneRenderImageEffectPass {
        SceneRenderImageEffectPass {
            effect_file: "effects/opacity/effect.json".to_owned(),
            runtime: Some("native-opacity-mask".to_owned()),
            pass_index: 0,
            command: None,
            source: None,
            target: None,
            binds: Default::default(),
            fbos: Default::default(),
            shader: Some("effects/opacity".to_owned()),
            blending: Some("normal".to_owned()),
            depthtest: None,
            depthwrite: None,
            cullmode: None,
            alphawriting: None,
            texture_slots: Vec::new(),
            effect_uv_transform: Some(SceneEffectUvTransform {
                mapping: Default::default(),
                source_slot: 0,
                mask_slot,
                scale,
                offset,
                input_extent: None,
                mask_extent: None,
                mask_backing_extent: None,
            }),
            combos: Default::default(),
            constant_shader_values: Default::default(),
        }
    }
