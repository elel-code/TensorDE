    #[test]
    fn render_sync_reports_package_cache_limit_and_evictions() {
        let test_dir = TestDir::new("gilder-render-sync-package-cache-limit");
        let package_a = test_dir.path.join("a.gwpdir");
        let package_b = test_dir.path.join("b.gwpdir");
        write_minimal_static_variant_gwpdir(&package_a);
        write_minimal_static_variant_gwpdir(&package_b);
        let mut config = GilderConfig::default();
        config.cache.package_cache_max_entries = 1;
        config.outputs.insert(
            "eDP-1".to_owned(),
            OutputConfig {
                wallpaper: Some(package_a.display().to_string()),
                ..OutputConfig::default()
            },
        );
        config.outputs.insert(
            "HDMI-A-1".to_owned(),
            OutputConfig {
                wallpaper: Some(package_b.display().to_string()),
                ..OutputConfig::default()
            },
        );
        let desktop = DesktopSnapshot {
            outputs: vec![
                DesktopOutput::virtual_output("eDP-1"),
                DesktopOutput::virtual_output("HDMI-A-1"),
            ],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            test_dir.path.join("cache"),
        );

        assert!(sync.errors.is_empty());
        assert_eq!(sync.plans.len(), 2);
        assert_eq!(sync.cache.package_cache_entries, 1);
        assert_eq!(sync.cache.package_cache_max_entries, 1);
        assert_eq!(sync.cache.package_cache_misses, 2);
        assert_eq!(sync.cache.package_cache_evictions, 1);
    }

    fn adaptive_cpu_pressure_snapshot() -> crate::adaptive::AdaptiveSnapshot {
        crate::adaptive::AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![crate::adaptive::AdaptiveTrigger {
                metric: crate::adaptive::AdaptiveMetric::CpuPressureSomeAvg10,
                value_x100: 9_000,
                threshold_x100: 7_500,
            }],
            ..crate::adaptive::AdaptiveSnapshot::default()
        }
    }

    fn write_minimal_video_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::create_dir_all(path.join("previews")).unwrap();
        fs::write(path.join("assets/loop.webm"), b"not a real video").unwrap();
        fs::write(path.join("assets/loop-mobile.webm"), b"not a real video").unwrap();
        fs::write(path.join("previews/poster.jpg"), b"not a real image").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.video-demo",
            "version": "1.0.0",
            "title": "Video Demo",
            "kind": "video",
            "preview": {
                "poster": "previews/poster.jpg"
            },
            "entry": {
                "type": "video",
                "source": "assets/loop.webm",
                "poster": "previews/poster.jpg",
                "loop": false,
                "muted": false,
                "fit": "contain",
                "max_fps": 60,
                "start_offset_ms": 1200
            },
            "variants": [
                {
                    "id": "mobile",
                    "source": "assets/loop-mobile.webm",
                    "width": 1080,
                    "height": 1920
                }
            ]
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_static_variant_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(path.join("assets/wallpaper.svg"), b"<svg/>").unwrap();
        fs::write(path.join("assets/wide.svg"), b"<svg/>").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.static-variant",
            "version": "1.0.0",
            "title": "Static Variant Demo",
            "kind": "static-image",
            "entry": {
                "type": "static-image",
                "source": "assets/wallpaper.svg",
                "fit": "cover"
            },
            "variants": [
                {
                    "id": "wide",
                    "source": "assets/wide.svg",
                    "width": 2560,
                    "height": 1080
                }
            ]
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_slideshow_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(path.join("assets/a.svg"), b"<svg/>").unwrap();
        fs::write(path.join("assets/b.svg"), b"<svg/>").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.slideshow-demo",
            "version": "1.0.0",
            "title": "Slideshow Demo",
            "kind": "slideshow",
            "entry": {
                "type": "slideshow",
                "sources": ["assets/a.svg", "assets/b.svg"],
                "interval_ms": 1500,
                "transition": "crossfade",
                "fit": "contain"
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_playlist_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(path.join("assets/battery.svg"), b"<svg/>").unwrap();
        fs::write(path.join("assets/loop.webm"), b"not a real video").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.playlist-demo",
            "version": "1.0.0",
            "title": "Playlist Demo",
            "kind": "playlist",
            "entry": {
                "type": "playlist",
                "items": [
                    {
                        "id": "battery-static",
                        "conditions": {
                            "power": "battery"
                        },
                        "entry": {
                            "type": "static-image",
                            "source": "assets/battery.svg",
                            "fit": "cover"
                        }
                    },
                    {
                        "id": "default-video",
                        "entry": {
                            "type": "video",
                            "source": "assets/loop.webm",
                            "loop": true,
                            "muted": true,
                            "fit": "cover",
                            "max_fps": 60
                        }
                    }
                ]
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_playlist_no_match_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(path.join("assets/loop.webm"), b"not a real video").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.playlist-no-match",
            "version": "1.0.0",
            "title": "Playlist No Match",
            "kind": "playlist",
            "entry": {
                "type": "playlist",
                "items": [
                    {
                        "id": "dp-only-video",
                        "conditions": {
                            "outputs": ["DP-1"]
                        },
                        "entry": {
                            "type": "video",
                            "source": "assets/loop.webm",
                            "loop": true,
                            "muted": true,
                            "fit": "cover"
                        }
                    }
                ]
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_static_auto_variant_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(path.join("assets/wallpaper.svg"), b"<svg/>").unwrap();
        fs::write(path.join("assets/small.svg"), b"<svg/>").unwrap();
        fs::write(path.join("assets/hd.svg"), b"<svg/>").unwrap();
        fs::write(path.join("assets/uhd.svg"), b"<svg/>").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.static-auto-variant",
            "version": "1.0.0",
            "title": "Static Auto Variant Demo",
            "kind": "static-image",
            "entry": {
                "type": "static-image",
                "source": "assets/wallpaper.svg",
                "fit": "cover"
            },
            "variants": [
                {
                    "id": "small",
                    "source": "assets/small.svg",
                    "width": 1280,
                    "height": 720
                },
                {
                    "id": "hd",
                    "source": "assets/hd.svg",
                    "width": 1920,
                    "height": 1080
                },
                {
                    "id": "uhd",
                    "source": "assets/uhd.svg",
                    "width": 3840,
                    "height": 2160
                }
            ]
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_static_large_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(path.join("assets/wallpaper.png"), b"original-large-image").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.static-large",
            "version": "1.0.0",
            "title": "Static Large Demo",
            "kind": "static-image",
            "entry": {
                "type": "static-image",
                "source": "assets/wallpaper.png",
                "fit": "cover",
                "width": 7680,
                "height": 4320
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_web_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets/web")).unwrap();
        fs::create_dir_all(path.join("previews")).unwrap();
        fs::write(
            path.join("assets/web/index.html"),
            b"<main>web wallpaper</main>",
        )
        .unwrap();
        fs::write(
            path.join("assets/web/gilder-bridge.js"),
            b"window.gilder = {};",
        )
        .unwrap();
        fs::write(path.join("previews/poster.svg"), b"<svg/>").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.web-demo",
            "version": "1.0.0",
            "title": "Web Demo",
            "kind": "web",
            "preview": {
                "poster": "previews/poster.svg"
            },
            "entry": {
                "type": "web",
                "root": "assets/web",
                "index": "index.html",
                "fallback": "previews/poster.svg",
                "max_fps": 30
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_shader_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("shaders")).unwrap();
        fs::create_dir_all(path.join("previews")).unwrap();
        fs::write(
            path.join("shaders/main.frag"),
            br##"
uniform float u_time;
uniform vec2 u_resolution;
uniform float u_intensity;
void main() {}
"##,
        )
        .unwrap();
        fs::write(path.join("previews/poster.svg"), b"<svg/>").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.shader-demo",
            "version": "1.0.0",
            "title": "Shader Demo",
            "kind": "shader",
            "preview": {
                "poster": "previews/poster.svg"
            },
            "entry": {
                "type": "shader",
                "source": "shaders/main.frag",
                "fallback": "previews/poster.svg",
                "language": "glsl",
                "max_fps": 60,
                "uniforms": [
                    { "name": "u_time", "source": "time" },
                    { "name": "u_resolution", "source": "resolution" },
                    { "name": "u_intensity", "source": "property", "property": "intensity" }
                ]
            },
            "properties": {
                "intensity": {
                    "type": "range",
                    "min": 0.0,
                    "max": 1.0,
                    "default": 0.5
                }
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_scene_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::create_dir_all(path.join("previews")).unwrap();
        let scene_binary = minimal_scene_engine_binary();
        let mut scene_file = fs::File::create(path.join("assets/scene.gscene")).unwrap();
        crate::engine::scene::write_scene_binary(&scene_binary, &mut scene_file).unwrap();
        fs::write(path.join("previews/poster.svg"), b"<svg/>").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.scene-demo",
            "version": "1.0.0",
            "title": "Scene Demo",
            "kind": "scene",
            "preview": {
                "poster": "previews/poster.svg"
            },
            "entry": {
                "type": "scene",
                "source": "assets/scene.gscene",
                "max_fps": 60
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn minimal_scene_engine_binary() -> crate::engine::scene::SceneBinaryDocument {
        use crate::engine::scene::{
            SCENE_DEFAULT_FEATURE_FLAGS, SceneBinaryDocument, SceneMaterialHandle,
            SceneMaterialRecord, SceneMeshRecord, SceneMeshVertexRecord, SceneObjectHandle,
            SceneObjectKind, SceneObjectRecord, SceneProjectRecord, SceneRenderGraphRecord,
            SceneRenderPassKind, SceneRenderPassRecord, SceneRenderTargetKind, SceneResourceId,
            SceneResourceKind, SceneResourceRecord, SceneShaderContractRecord, SceneStringId,
            SceneVec3,
        };

        SceneBinaryDocument {
            feature_flags: SCENE_DEFAULT_FEATURE_FLAGS,
            strings: vec![
                "Scene Demo".to_owned(),
                "scene".to_owned(),
                "scene.json".to_owned(),
                "materials/layer.json".to_owned(),
                "genericimage4".to_owned(),
                "genericimage4|blend=normal".to_owned(),
                "loose-file".to_owned(),
            ],
            project: SceneProjectRecord {
                title: SceneStringId(0),
                wallpaper_type: SceneStringId(1),
                scene_file: SceneStringId(2),
                preview: SceneStringId::NONE,
                properties_json: SceneStringId::NONE,
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
            },
            resources: vec![SceneResourceRecord {
                id: SceneResourceId(0),
                kind: SceneResourceKind::MaterialJson,
                path: SceneStringId(3),
                source: SceneStringId(6),
                payload_offset: 0,
                payload_len: 2,
            }],
            resource_payload: b"{}".to_vec(),
            objects: vec![SceneObjectRecord {
                id: SceneObjectHandle(0),
                we_id: 7,
                name: SceneStringId::NONE,
                kind: SceneObjectKind::Image,
                resource: SceneResourceId::NONE,
                material: SceneMaterialHandle(0),
                parent_we_id: crate::engine::scene::INVALID_OBJECT_ID,
                attachment: SceneStringId::NONE,
                origin: SceneVec3::default(),
                angles: SceneVec3::default(),
                scale: SceneVec3 {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                },
                camera_zoom: 1.0,
                color: SceneVec3 {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                },
                alpha: 1.0,
                visible: true,
                color_blend_mode: 0,
                sort_order: 0,
                effect_start: u32::MAX,
                effect_count: 0,
                render_graph: 0,
            }],
            materials: vec![SceneMaterialRecord {
                id: SceneMaterialHandle(0),
                resource: SceneResourceId(0),
                pass_start: 0,
                pass_count: 0,
            }],
            meshes: vec![SceneMeshRecord {
                object: SceneObjectHandle(0),
                material: SceneMaterialHandle(0),
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
                SceneMeshVertexRecord {
                    position: SceneVec3 {
                        x: -32.0,
                        y: -16.0,
                        z: 0.0,
                    },
                    uv: [0.0, 1.0],
                    blend_indices: [0; 4],
                    blend_weights: [0.0; 4],
                },
                SceneMeshVertexRecord {
                    position: SceneVec3 {
                        x: 32.0,
                        y: -16.0,
                        z: 0.0,
                    },
                    uv: [1.0, 1.0],
                    blend_indices: [0; 4],
                    blend_weights: [0.0; 4],
                },
                SceneMeshVertexRecord {
                    position: SceneVec3 {
                        x: 32.0,
                        y: 16.0,
                        z: 0.0,
                    },
                    uv: [1.0, 0.0],
                    blend_indices: [0; 4],
                    blend_weights: [0.0; 4],
                },
                SceneMeshVertexRecord {
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
            render_graphs: vec![SceneRenderGraphRecord {
                object: SceneObjectHandle(0),
                activation_policy:
                    crate::engine::scene::SceneRenderGraphActivationPolicy::Always,
                pass_start: 0,
                pass_count: 1,
                unsupported_start: 0,
                unsupported_count: 0,
            }],
            render_passes: vec![SceneRenderPassRecord {
                id: 0,
                role: SceneRenderPassKind::BaseMaterial,
                draw_primitive: crate::engine::scene::SceneRenderPassDrawPrimitive::ObjectMesh,
                object: SceneObjectHandle(0),
                material: SceneMaterialHandle(crate::engine::scene::INVALID_MATERIAL_ID),
                pass_index: 0,
                shader_key: SceneStringId(4),
                target: SceneRenderTargetKind::SceneColor,
                target_name: SceneStringId::NONE,
                binding_start: 0,
                binding_count: 0,
                effect_binding_start: u32::MAX,
                effect_binding_count: 0,
                effect_visibility_policy:
                    crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
                pipeline_blend: crate::engine::scene::ScenePipelineBlend::Normal,
                scene_blend: crate::engine::scene::SceneCompositeBlend::Alpha,
                depth_test: crate::engine::scene::SceneDepthTest::Disabled,
                depth_write: false,
                cull_mode: crate::engine::scene::SceneCullMode::None,
                color_write_mask: crate::engine::scene::SceneColorWriteMask::Rgba,
                clear_target: false,
            }],
            shader_contracts: vec![SceneShaderContractRecord {
                shader_key: SceneStringId(4),
                pipeline_key: SceneStringId(5),
                texture_slot_mask: 0,
                input_attachment_slot_mask: 0,
                constant_start: 0,
                constant_count: 0,
                resource_heap_count: 1,
                sampler_heap_count: 0,
            }],
            ..SceneBinaryDocument::default()
        }
    }

    fn remove_entry_poster(path: &Path) {
        let manifest_path = path.join(crate::core::MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest
            .get_mut("entry")
            .and_then(|entry| entry.as_object_mut())
            .unwrap()
            .remove("poster");
        fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    }

    fn remove_entry_fallback(path: &Path) {
        let manifest_path = path.join(crate::core::MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest
            .get_mut("entry")
            .and_then(|entry| entry.as_object_mut())
            .unwrap()
            .remove("fallback");
        fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    }

    fn set_runtime_pause_when_unfocused(path: &Path) {
        let manifest_path = path.join(crate::core::MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest["runtime"] = json!({
            "pause_when_unfocused": true
        });
        fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    }

    fn set_runtime_continue_when_fullscreen(path: &Path) {
        let manifest_path = path.join(crate::core::MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest["runtime"] = json!({
            "pause_when_fullscreen": false
        });
        fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    }

    fn set_runtime_allow_audio(path: &Path) {
        let manifest_path = path.join(crate::core::MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest["runtime"] = json!({
            "allow_audio": true
        });
        fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    }

    fn active_performance_decision() -> PerformanceDecision {
        PerformanceDecision {
            mode: RenderMode::Active,
            max_fps: Some(60),
            reason: DecisionReason::Interactive,
        }
    }

    fn write_executable_script(path: &Path, contents: &str) {
        fs::write(path, contents).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(path).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions).unwrap();
        }
    }

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(prefix: &str) -> Self {
            let pid = std::process::id();
            let sequence = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("{prefix}-{pid}-{sequence}-{nanos}"));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
