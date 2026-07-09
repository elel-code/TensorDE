
    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn unwraps_h265_poc_lsb_across_continuous_stream() {
        let mut access_units = Vec::new();
        access_units.push(h265_test_access_unit(0, 0, true, &[]));
        for index in 1..=15 {
            access_units.push(h265_test_access_unit(index, index, false, &[]));
        }
        access_units.push(h265_test_access_unit(16, 0, false, &[-1]));
        access_units.push(h265_test_access_unit(17, 1, false, &[-1]));

        let plan = native_vulkan_h265_decode_reference_plan(&access_units, 18, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(
            plan.iter()
                .map(|entry| entry.current_poc)
                .collect::<Vec<_>>(),
            (0..=17).map(Some).collect::<Vec<_>>()
        );
        assert_eq!(plan[16].references[0].poc, 15);
        assert_eq!(plan[17].references[0].poc, 16);
    }

    #[test]
    fn contract_covers_full_wallpaper_type_matrix() {
        let contract = backend_contract();

        assert_eq!(contract.backend_name, "native-vulkan");
        assert_eq!(
            contract.wallpaper_types,
            &[
                NativeVulkanWallpaperType::StaticImage,
                NativeVulkanWallpaperType::Video,
                NativeVulkanWallpaperType::Web,
                NativeVulkanWallpaperType::Scene,
                NativeVulkanWallpaperType::Shader,
                NativeVulkanWallpaperType::Playlist,
            ]
        );
        assert!(contract.video_interop.avoids_default_rgba_upload);
        assert_eq!(
            contract.video_pipeline.reference,
            "FFmpeg packet/frame/clock model"
        );
        assert_eq!(contract.video_pipeline.stages.len(), 10);
        assert!(
            contract
                .video_pipeline
                .stages
                .iter()
                .any(|stage| stage.owner == "ffmpeg-decoder-boundary")
        );
        assert_eq!(contract.wallpaper_type_support.len(), 6);
    }

    #[test]
    fn wallpaper_type_support_marks_current_items_and_future_contracts() {
        let support = wallpaper_type_support_matrix();

        assert_eq!(support.len(), WALLPAPER_TYPE_CONTRACT.len());
        assert!(
            support
                .iter()
                .find(|entry| entry.wallpaper_type == NativeVulkanWallpaperType::StaticImage)
                .is_some_and(|entry| entry.current_vulkan_item)
        );
        assert!(
            support
                .iter()
                .find(|entry| entry.wallpaper_type == NativeVulkanWallpaperType::Video)
                .is_some_and(|entry| entry.current_vulkan_item)
        );
        assert!(
            support
                .iter()
                .find(|entry| entry.wallpaper_type == NativeVulkanWallpaperType::Web)
                .is_some_and(|entry| !entry.current_vulkan_item)
        );
        assert!(
            support
                .iter()
                .find(|entry| entry.wallpaper_type == NativeVulkanWallpaperType::Shader)
                .is_some_and(|entry| !entry.current_vulkan_item)
        );
    }

    #[test]
    fn maps_sync_plan_to_vulkan_items() {
        let sync_plan = StaticRenderSyncPlan {
            plans: vec![StaticWallpaperPlan {
                output_name: "HDMI-A-1".to_owned(),
                source: PathBuf::from("/tmp/static.png"),
                fit: FitMode::Cover,
                background: Some("#000000".to_owned()),
            }],
            video_plans: vec![VideoWallpaperPlan {
                output_name: "HDMI-A-1".to_owned(),
                source: PathBuf::from("/tmp/video.mp4"),
                poster: None,
                fit: FitMode::Contain,
                loop_playback: true,
                muted: true,
                manifest_max_fps: Some(240),
                target_max_fps: Some(240),
                decoder_policy: crate::config::VideoDecoderPolicy::HardwarePreferred,
                start_offset_ms: 0,
            }],
            slideshow_plans: Vec::new(),
            scene_plans: vec![SceneWallpaperPlan {
                output_name: "HDMI-A-1".to_owned(),
                source: Some(PathBuf::from("/tmp/scene.json")),
                manifest_max_fps: Some(60),
                target_max_fps: Some(30),
                snapshot_time_ms: 1234,
                scene_size: None,
                scene_fit: FitMode::Cover,
                scene_systems: Default::default(),
                audio_cue_count: 0,
                bound_properties: vec!["scene_opacity".to_owned()],
                timeline_animation_count: 2,
                timeline_animated_layer_count: 1,
                puppet_animation_layer_count: 0,
                property_binding_count: 1,
                cursor_parallax_input_ready: true,
                scene_input_properties: Default::default(),
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
                display: Some(SceneDisplayPlan::Color {
                    color: "#102030".to_owned(),
                }),
                layers: vec![SceneRenderLayer {
                    id: "panel".to_owned(),
                    kind: crate::core::SceneNodeKind::Rectangle,
                    source: None,
                    texture_slots: Vec::new(),
                    alpha_texture_slot: None,
                    alpha_texture_mode: Default::default(),
                    image_effect_passes: Vec::new(),
                    composite_key: None,
                    texture_region: None,
                    effect_motion: Default::default(),
                    blend_mode: Default::default(),
                    audio: Vec::new(),
                    color: Some("#102030".to_owned()),
                    stroke_color: Some("#ffffff".to_owned()),
                    stroke_width: Some(2.0),
                    corner_radius: Some(8.0),
                    width: Some(320.0),
                    height: Some(180.0),
                    mesh: None,
                    text: None,
                    font_size: None,
                    font_family: None,
                    font_source: None,
                    font_weight: None,
                    text_align: None,
                    path_data: None,
                    path_fill_rule: crate::core::ScenePathFillRule::default(),
                    fit: FitMode::Cover,
                    opacity: 0.75,
                    transform: crate::core::SceneTransform {
                        x: 12.0,
                        y: 24.0,
                        ..Default::default()
                    },
                }],
            }],
            removals: Vec::new(),
            errors: Vec::new(),
            decisions: Vec::new(),
            playlist_clock_dependency: Default::default(),
            cache: Default::default(),
        };

        let items = render_items_from_sync_plan(&sync_plan);

        assert_eq!(items.len(), 3);
        assert_eq!(items[0].wallpaper_type(), NativeVulkanWallpaperType::Scene);
        let NativeVulkanRenderItem::Scene {
            display_image: static_display_image,
            layers: static_layers,
            renderer_status: static_renderer_status,
            ..
        } = &items[0]
        else {
            unreachable!("static wallpaper should lower to a scene image layer");
        };
        assert_eq!(
            static_display_image,
            &Some(PathBuf::from("/tmp/static.png"))
        );
        assert_eq!(static_layers.len(), 1);
        assert_eq!(static_layers[0].kind, crate::core::SceneNodeKind::Image);
        assert_eq!(
            *static_renderer_status,
            "static-image-lowered-to-scene-sampled-image-layer"
        );
        assert!(matches!(items[1], NativeVulkanRenderItem::Video { .. }));
        assert_eq!(items[1].wallpaper_type(), NativeVulkanWallpaperType::Video);
        let NativeVulkanRenderItem::Video {
            target_max_fps,
            decoder_policy,
            start_offset_ms,
            renderer_status,
            ..
        } = &items[1]
        else {
            unreachable!("item already matched as video");
        };
        assert_eq!(*target_max_fps, Some(240));
        assert_eq!(
            *decoder_policy,
            crate::config::VideoDecoderPolicy::HardwarePreferred
        );
        assert_eq!(*start_offset_ms, 0);
        assert_eq!(*renderer_status, "vulkan-lifecycle-video-placeholder");
        assert_eq!(items[2].wallpaper_type(), NativeVulkanWallpaperType::Scene);
        let NativeVulkanRenderItem::Scene {
            scene_source,
            display,
            display_image,
            display_color,
            manifest_max_fps,
            layer_count,
            layers,
            bound_properties,
            timeline_animation_count,
            timeline_animated_layer_count,
            property_binding_count,
            cursor_parallax_input_ready,
            snapshot_time_ms,
            target_max_fps,
            renderer_status,
            ..
        } = &items[2]
        else {
            unreachable!("item already matched as scene");
        };
        assert_eq!(scene_source, &Some(PathBuf::from("/tmp/scene.json")));
        assert_eq!(display_image, &None);
        assert_eq!(display_color.as_deref(), Some("#102030"));
        assert!(matches!(
            display,
            Some(SceneDisplayPlan::Color { color }) if color == "#102030"
        ));
        assert_eq!(*manifest_max_fps, Some(60));
        assert_eq!(*target_max_fps, Some(30));
        assert_eq!(*layer_count, 1);
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].id, "panel");
        assert_eq!(layers[0].kind, crate::core::SceneNodeKind::Rectangle);
        assert_eq!(layers[0].opacity, 0.75);
        assert_eq!(layers[0].transform.x, 12.0);
        assert_eq!(layers[0].transform.y, 24.0);
        assert_eq!(bound_properties, &vec!["scene_opacity".to_owned()]);
        assert_eq!(*timeline_animation_count, 2);
        assert_eq!(*timeline_animated_layer_count, 1);
        assert_eq!(*property_binding_count, 1);
        assert!(*cursor_parallax_input_ready);
        assert_eq!(*snapshot_time_ms, 1234);
        assert_eq!(
            *renderer_status,
            "deterministic-scene-snapshot-ready-for-vulkan-passes"
        );
    }

    #[test]
    fn contract_names_required_vulkan_extensions() {
        let contract = backend_contract();

        assert!(
            contract
                .required_instance_extensions
                .contains(&"VK_KHR_wayland_surface")
        );
        assert!(
            contract
                .required_device_extensions
                .contains(&"VK_KHR_swapchain")
        );
        assert!(
            contract
                .required_device_extensions
                .contains(&"VK_EXT_external_memory_dma_buf")
        );
        assert!(
            contract
                .required_device_extensions
                .contains(&"VK_EXT_image_drm_format_modifier")
        );
        assert!(
            contract
                .video_interop
                .vulkan_binding_policy
                .contains("vulkanalia")
        );
        assert!(
            contract
                .video_interop
                .vulkanalia_primary_policy
                .contains("vulkanalia owns")
        );
        assert!(
            contract
                .video_interop
                .vulkanalia_primary_policy
                .contains("AVVkFrame descriptor sampling")
        );
        assert!(
            contract
                .video_interop
                .vulkan_1_4_value
                .contains("dynamic-rendering-local-read")
        );
        assert!(
            contract
                .video_interop
                .vulkan_binding_policy
                .contains("zero-copy evidence")
        );
        assert!(
            contract
                .video_interop
                .removed_ash_baseline
                .contains("Vulkan Video")
        );
        assert!(
            contract
                .video_interop
                .removed_ash_baseline
                .contains("external-memory")
        );
        assert_eq!(contract.vulkan_backend.binding, "vulkanalia");
        assert!(contract.vulkan_backend.api_baseline.contains("Vulkan 1.4"));
    }
