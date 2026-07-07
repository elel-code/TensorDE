use super::*;
use crate::core::scene::binary::{SceneBinaryChunkKind, decode_scene_binary_container};
use std::time::{SystemTime, UNIX_EPOCH};

fn write_test_png(path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let rgba = [
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
    ];
    let file = fs::File::create(path).unwrap();
    let writer = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, 2, 2);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&rgba).unwrap();
}

#[test]
fn converts_static_image_project() {
    let source = TestDir::new("we-static-source");
    let output = TestDir::new("we-static-output");
    output.remove();
    source.write_file("wallpaper.png", "not real png");
    source.write_file(
        PROJECT_FILE,
        r##"{
              "type": "image",
              "title": "Static Example",
              "file": "wallpaper.png"
            }"##,
    );

    let summary = convert_project(source.path(), output.path()).unwrap();
    assert_eq!(summary.source_type, "image");
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
            .unwrap();
    assert_eq!(manifest["kind"], "static-image");
    assert_eq!(manifest["entry"]["source"], "assets/wallpaper.png");
}

#[test]
fn converts_static_image_audio_to_binary_scene() {
    let source = TestDir::new("we-static-audio-source");
    let output = TestDir::new("we-static-audio-output");
    output.remove();
    write_test_png(&source.path().join("wallpaper.png"));
    source.write_file("music.ogg", "not real ogg");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "image",
              "title": "Static With Audio",
              "file": "wallpaper.png",
              "audio": "music.ogg"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
            .unwrap();
    assert_eq!(manifest["kind"], "scene");
    assert_eq!(manifest["entry"]["source"], "assets/scene.gscn");
    assert!(output.path().join("assets/scene.gscn").exists());
    assert!(!legacy_scene_json_path(output.path()).exists());
    assert_eq!(
        binary_chunk_count(
            &output.path().join("assets/scene.gscn"),
            SceneBinaryChunkKind::NodeTable
        ),
        1
    );
    assert_eq!(
        binary_chunk_count(
            &output.path().join("assets/scene.gscn"),
            SceneBinaryChunkKind::ResourceTable
        ),
        2
    );
}

#[test]
fn converts_scene_project_to_binary_scene() {
    let source = TestDir::new("we-scene-source");
    let output = TestDir::new("we-scene-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{"objects":[{"type":"image","path":"background.png"}]}"#,
    );
    source.write_file("background.png", "not real png");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Scene Example",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
            .unwrap();
    assert_eq!(manifest["kind"], "scene");
    assert_eq!(manifest["entry"]["source"], "assets/scene.gscn");
    assert!(output.path().join("metadata/source-scene.json").exists());
    assert!(output.path().join("assets/scene.gscn").exists());
    assert!(!legacy_scene_json_path(output.path()).exists());
    assert_eq!(
        binary_chunk_count(
            &output.path().join("assets/scene.gscn"),
            SceneBinaryChunkKind::NodeTable
        ),
        1
    );
    assert_eq!(
        binary_chunk_count(
            &output.path().join("assets/scene.gscn"),
            SceneBinaryChunkKind::ResourceTable
        ),
        1
    );
}

#[test]
fn parses_eye_puppet_clipping_records_before_mdls() {
    let fixture = Path::new("reverse-engineered/extracted/3742497499/models/眼睛_puppet.mdl");
    if !fixture.is_file() {
        return;
    }
    let bytes = fs::read(fixture).unwrap();
    let frame_size = SceneWeModelFrameSize {
        width: 663,
        height: 230,
    };
    let map = scene_parse_puppet_attachment_map(&bytes, frame_size).unwrap();
    let mesh = map.mesh.expect("eye puppet mesh");

    assert_eq!(mesh.clipping_records.len(), 4);
    assert_eq!(
        mesh.clipping_records[0].mask,
        "masks/clipping_mask_42ff01af"
    );
    assert_eq!(mesh.clipping_records[0].duration_frames, 1680);
    assert_eq!(
        mesh.clipping_records[0].frame_keys,
        (0..10).collect::<Vec<_>>()
    );
    assert!(mesh.clipping_records[0].bones.contains(&42));
    assert!(mesh.clipping_records[0].bones.contains(&43));
    assert!(mesh.clipping_active_sources.is_empty());
}

#[test]
fn parses_mdmp_active_sources_from_mdl_owner_flags() {
    let owner_flags = (1 << 10) | (1 << 11) | (1 << 12) | (1 << 13);
    let mut bytes = Vec::new();
    test_push_cstr(&mut bytes, "MDLV0023");
    test_push_u32(&mut bytes, 0x0180_0009);
    test_push_u32(&mut bytes, 1);
    test_push_u32(&mut bytes, 1);
    test_push_mdlv0023_owner(&mut bytes, "materials/test.json", owner_flags);

    let mdmp_start = bytes.len();
    bytes.extend_from_slice(b"MDMP0001\0");
    let mdmp_end_field = bytes.len();
    test_push_u32(&mut bytes, 0);
    test_push_u16(&mut bytes, 1);
    test_push_u32(&mut bytes, 0.5f32.to_bits());
    test_push_u32(&mut bytes, 9);
    bytes.extend_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
    test_push_cstr(&mut bytes, "eye-right");
    test_push_block(&mut bytes, &[1; 12]);
    test_push_block(&mut bytes, &[2; 12]);
    test_push_block(&mut bytes, &[3; 12]);
    test_push_block(&mut bytes, &[4; 4]);
    test_push_u32(&mut bytes, 4);
    test_push_u32(&mut bytes, 2);
    test_push_f32(&mut bytes, -0.25);
    test_push_f32(&mut bytes, 0.75);
    let mdmp_end = u32::try_from(bytes.len()).unwrap();
    bytes[mdmp_end_field..mdmp_end_field + 4].copy_from_slice(&mdmp_end.to_le_bytes());

    let parsed_owner_flags = scene_parse_puppet_entry_owner_flags(&bytes, mdmp_start).unwrap();
    assert_eq!(parsed_owner_flags, vec![owner_flags]);

    let active_sources =
        scene_parse_puppet_mdmp_active_sources(&bytes, &parsed_owner_flags).unwrap();
    assert_eq!(active_sources.len(), 1);
    assert_eq!(active_sources[0].source_name, "eye-right");
    assert_eq!(active_sources[0].source_id, 0x1122_3344_5566_7788);
    assert_eq!(active_sources[0].scalar_bits, 0.5f32.to_bits());
    assert_eq!(active_sources[0].source_scale, 2);
    assert_eq!(active_sources[0].flags, 2);
    assert_eq!(active_sources[0].transform_index, 4);
    assert_eq!(active_sources[0].parameter0, -0.25);
    assert_eq!(active_sources[0].parameter1, 0.75);
}

#[test]
fn parses_mdmp_owner_scalar_and_scale_even_when_owner_has_no_active_sources() {
    let mut bytes = Vec::new();
    test_push_cstr(&mut bytes, "MDLV0023");
    test_push_u32(&mut bytes, 0x0180_0009);
    test_push_u32(&mut bytes, 1);
    test_push_u32(&mut bytes, 2);
    test_push_mdlv0023_owner(&mut bytes, "materials/empty.json", 0);
    test_push_mdlv0023_owner(&mut bytes, "materials/active.json", 1 << 13);

    let mdmp_start = bytes.len();
    bytes.extend_from_slice(b"MDMP0001\0");
    let mdmp_end_field = bytes.len();
    test_push_u32(&mut bytes, 0);
    test_push_u16(&mut bytes, 0);
    test_push_u32(&mut bytes, 1.25f32.to_bits());
    test_push_u32(&mut bytes, 7);
    test_push_u16(&mut bytes, 1);
    test_push_u32(&mut bytes, 0.75f32.to_bits());
    test_push_u32(&mut bytes, 3);
    bytes.extend_from_slice(&0x8877_6655_4433_2211u64.to_le_bytes());
    test_push_cstr(&mut bytes, "second-owner-source");
    test_push_block(&mut bytes, &[1; 18]);
    test_push_u32(&mut bytes, 9);
    test_push_u32(&mut bytes, 2);
    test_push_f32(&mut bytes, 1.5);
    test_push_f32(&mut bytes, -0.5);
    let mdmp_end = u32::try_from(bytes.len()).unwrap();
    bytes[mdmp_end_field..mdmp_end_field + 4].copy_from_slice(&mdmp_end.to_le_bytes());

    let parsed_owner_flags = scene_parse_puppet_entry_owner_flags(&bytes, mdmp_start).unwrap();
    assert_eq!(parsed_owner_flags, vec![0, 1 << 13]);

    let active_sources =
        scene_parse_puppet_mdmp_active_sources(&bytes, &parsed_owner_flags).unwrap();
    assert_eq!(active_sources.len(), 1);
    assert_eq!(active_sources[0].source_name, "second-owner-source");
    assert_eq!(active_sources[0].source_id, 0x8877_6655_4433_2211);
    assert_eq!(active_sources[0].scalar_bits, 0.75f32.to_bits());
    assert_eq!(active_sources[0].source_scale, 3);
    assert_eq!(active_sources[0].transform_index, 9);
    assert_eq!(active_sources[0].flags, 2);
    assert_eq!(active_sources[0].parameter0, 1.5);
    assert_eq!(active_sources[0].parameter1, -0.5);
}

#[test]
fn mdmp_base_block_len_overrides_owner_source_scale_without_rejecting() {
    let owner_flags = (1 << 10) | (1 << 13);
    let mut bytes = Vec::new();
    test_push_cstr(&mut bytes, "MDLV0023");
    test_push_u32(&mut bytes, 0x0180_0009);
    test_push_u32(&mut bytes, 1);
    test_push_u32(&mut bytes, 1);
    test_push_mdlv0023_owner(&mut bytes, "materials/test.json", owner_flags);

    let mdmp_start = bytes.len();
    bytes.extend_from_slice(b"MDMP0001\0");
    let mdmp_end_field = bytes.len();
    test_push_u32(&mut bytes, 0);
    test_push_u16(&mut bytes, 1);
    test_push_u32(&mut bytes, 1.0f32.to_bits());
    test_push_u32(&mut bytes, 1);
    bytes.extend_from_slice(&0x0102_0304_0506_0708u64.to_le_bytes());
    test_push_cstr(&mut bytes, "scale-from-base-block");
    test_push_block(&mut bytes, &[1; 18]);
    test_push_block(&mut bytes, &[2; 18]);
    test_push_u32(&mut bytes, 3);
    test_push_u32(&mut bytes, 2);
    test_push_f32(&mut bytes, 0.25);
    test_push_f32(&mut bytes, -0.75);
    let mdmp_end = u32::try_from(bytes.len()).unwrap();
    bytes[mdmp_end_field..mdmp_end_field + 4].copy_from_slice(&mdmp_end.to_le_bytes());

    let parsed_owner_flags = scene_parse_puppet_entry_owner_flags(&bytes, mdmp_start).unwrap();
    let active_sources =
        scene_parse_puppet_mdmp_active_sources(&bytes, &parsed_owner_flags).unwrap();

    assert_eq!(active_sources.len(), 1);
    assert_eq!(active_sources[0].source_scale, 3);
    assert_eq!(active_sources[0].transform_index, 3);
    assert_eq!(active_sources[0].flags, 2);
    assert_eq!(active_sources[0].parameter0, 0.25);
    assert_eq!(active_sources[0].parameter1, -0.75);
}

#[test]
fn material_runtime_passes_preserve_alpha_writing_state() {
    let material = serde_json::json!({
        "passes": [
            {
                "shader": "genericimage4",
                "blending": "normal",
                "alphawriting": "enabled",
                "depthtest": "disabled",
                "depthwrite": "disabled",
                "cullmode": "nocull"
            }
        ]
    });

    let passes = scene_material_runtime_passes(&material);

    assert_eq!(passes.len(), 1);
    assert_eq!(passes[0]["alphawriting"].as_str(), Some("enabled"));
}

fn test_push_mdlv0023_owner(bytes: &mut Vec<u8>, material: &str, owner_flags: u32) {
    test_push_cstr(bytes, material);
    test_push_u32(bytes, owner_flags);
    for _ in 0..6 {
        test_push_f32(bytes, 0.0);
    }
    test_push_u32(bytes, 9);
    test_push_block(bytes, &[]);
    test_push_block(bytes, &[]);
    bytes.push(0);
    bytes.push(0);
    test_push_u32(bytes, 0);
}

fn test_push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn test_push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn test_push_f32(bytes: &mut Vec<u8>, value: f32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn test_push_cstr(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend_from_slice(value.as_bytes());
    bytes.push(0);
}

fn test_push_block(bytes: &mut Vec<u8>, value: &[u8]) {
    test_push_u32(bytes, u32::try_from(value.len()).unwrap());
    bytes.extend_from_slice(value);
}

#[test]
fn composelayer_framebuffer_effects_are_marked_without_child_propagation() {
    let source_model = scene_builtin_util_model("models/util/composelayer.json").unwrap();
    let mut effects = serde_json::json!([
        {
            "file": "effects/watercaustics/effect.json",
            "id": 641,
            "runtime": "native-water-caustics",
            "passes": [{
                "combos": { "BLENDMODE": 6 },
                "textures": [null, null, "pattern/voronoi_local"]
            }]
        },
        {
            "file": "effects/shake/effect.json",
            "id": 413,
            "runtime": "native-effect-motion",
            "passes": [{}]
        }
    ])
    .as_array()
    .unwrap()
    .clone();
    let mut context = SceneDocumentBuildContext::default();

    scene_prepare_utility_framebuffer_effects(Some(&source_model), &mut effects, &mut context);

    assert_eq!(effects.len(), 1);
    assert_eq!(
        effects[0].get("file").and_then(Value::as_str),
        Some("effects/watercaustics/effect.json")
    );
    assert_eq!(
        effects[0]["passes"][0]["combos"]["GILDER_FRAMEBUFFER_OVERLAY"],
        serde_json::json!(1)
    );
    assert!(
        context
            .converted_features
            .contains(&"wallpaper-engine-composelayer-framebuffer-effect".to_owned())
    );
}

#[test]
fn composelayer_framebuffer_source_model_uses_image_runtime() {
    let mut source_model = scene_builtin_util_model("models/util/composelayer.json").unwrap();
    let node = serde_json::json!({
        "effects": [
            {
                "file": "effects/watercaustics/effect.json",
                "id": 641,
                "runtime": "native-water-caustics",
                "passes": [{ "textures": [null, null, "pattern/voronoi_local"] }]
            },
            {
                "file": "effects/shake/effect.json",
                "id": 413,
                "runtime": "native-effect-motion",
                "passes": [{}]
            }
        ]
    });
    let node = node.as_object().unwrap().clone();

    assert_eq!(
        scene_builtin_util_node_kind(&node, &source_model),
        Some("rectangle")
    );
    source_model.render_resource = Some("white-placeholder".to_owned());
    source_model.render_kind = Some("image");
    assert_eq!(
        scene_builtin_util_node_kind(&node, &source_model),
        Some("image")
    );
}

#[test]
fn composelayer_effect_only_sources_use_image_runtime() {
    let mut source_model = scene_builtin_util_model("models/util/composelayer.json").unwrap();
    let node = serde_json::json!({
        "effects": [
            {
                "file": "effects/workshop/2123274886/tech_circle/effect.json",
                "passes": [{}]
            }
        ]
    });
    let node = node.as_object().unwrap().clone();

    assert_eq!(
        scene_builtin_util_node_kind(&node, &source_model),
        Some("rectangle")
    );
    source_model.render_resource = Some("white-placeholder".to_owned());
    source_model.render_kind = Some("image");
    assert_eq!(
        scene_builtin_util_node_kind(&node, &source_model),
        Some("image")
    );
}

#[test]
fn rounded_mask_effect_lowers_rectangle_corner_radius() {
    let node = serde_json::json!({
        "width": 550.0,
        "height": 3300.0
    })
    .as_object()
    .unwrap()
    .clone();
    let effects = serde_json::json!([
        {
            "file": "effects/workshop/3083593512/rounded_mask/effect.json",
            "passes": [{
                "constantshadervalues": {
                    "Radius": 0.5,
                    "Size": "0.9 0.9"
                }
            }]
        }
    ])
    .as_array()
    .unwrap()
    .clone();

    let radius = scene_corner_radius_from_rounded_mask_effect(&node, &effects).unwrap();

    assert!((radius - 123.75).abs() < 1.0e-6);
}

#[test]
fn text_outline_lowers_to_stroke_paint() {
    let object = serde_json::json!({
        "type": "text",
        "outline": true,
        "outlinecolor": "1.00000 1.00000 1.00000",
        "outlinethickness": 7.0
    })
    .as_object()
    .unwrap()
    .clone();

    let outline = scene_text_outline_from_object(&object).unwrap();

    assert_eq!(outline.0, "#ffffff");
    assert_eq!(outline.1, 7.0);
}

#[test]
fn font_text_lowers_to_generated_image_texture() {
    let fixture = Path::new("reverse-engineered/extracted/3742497499/fonts/Tourner (562).ttf");
    if !fixture.is_file() {
        return;
    }
    let source = TestDir::new("we-text-raster-source");
    let output = TestDir::new("we-text-raster-output");
    output.remove();
    fs::create_dir_all(source.path().join("fonts")).unwrap();
    fs::copy(fixture, source.path().join("fonts/Tourner (562).ttf")).unwrap();
    source.write_file(
        "scene.json",
        r#"{
            "general": { "width": 128, "height": 64 },
            "objects": [{
                "type": "text",
                "name": "scrolling label",
                "text": { "value": "DREAM" },
                "font": "fonts/Tourner (562).ttf",
                "pointsize": 24,
                "size": "128 64",
                "horizontalalign": "center",
                "verticalalign": "center",
                "color": "1 1 1",
                "outline": true,
                "outlinecolor": "0 0 0",
                "outlinethickness": 2,
                "effects": [{
                    "file": "effects/scroll/effect.json",
                    "passes": [{ "constantshadervalues": { "speedx": 0.1, "speedy": 0 } }]
                }]
            }]
        }"#,
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Text Raster",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let report: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report["converted_features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| feature.as_str() == Some("wallpaper-engine-font-text-raster"))
    );
    let generated = report["generated_assets"].as_array().unwrap();
    let text_texture = generated
        .iter()
        .filter_map(Value::as_str)
        .find(|asset| asset.contains("font-text-raster") && asset.ends_with(".gtex"))
        .expect("generated font text texture");
    assert!(output.path().join(text_texture).is_file());
    assert!(
        binary_chunk_count(
            &output.path().join("assets/scene.gscn"),
            SceneBinaryChunkKind::ResourceTable
        ) >= 1
    );
}

#[test]
fn font_text_alpha_crop_trims_transparent_layout_box() {
    let mut image = SceneWeTexImage {
        width: 6,
        height: 4,
        backing_width: 6,
        backing_height: 4,
        rgba: vec![0; 6 * 4 * 4],
        r8: None,
    };
    for (x, y) in [(2usize, 1usize), (4, 2)] {
        image.rgba[(y * 6 + x) * 4 + 3] = 255;
    }

    let crop = scene_crop_text_image_to_alpha_bounds(&mut image).expect("crop");

    assert_eq!(
        crop,
        SceneTextRasterCrop {
            original_width: 6,
            original_height: 4,
            x: 2,
            y: 1,
            width: 3,
            height: 2,
        }
    );
    assert_eq!((image.width, image.height), (3, 2));
    assert_eq!(image.rgba.len(), 3 * 2 * 4);
    assert_eq!(image.rgba[3], 255);
    assert_eq!(image.rgba[(2 * 3 - 1) * 4 + 3], 255);
}

#[test]
fn font_text_uv_effects_preserve_layout_box() {
    let scroll_text = serde_json::json!({
        "effects": [{
            "file": "effects/scroll/effect.json",
            "visible": true
        }]
    })
    .as_object()
    .unwrap()
    .clone();
    let colorkey_text = serde_json::json!({
        "effects": [{
            "file": "effects/colorkey/effect.json",
            "visible": true
        }]
    })
    .as_object()
    .unwrap()
    .clone();

    assert!(!scene_text_raster_can_alpha_crop(&scroll_text));
    assert!(scene_text_raster_can_alpha_crop(&colorkey_text));
}

#[test]
fn font_text_uv_effects_fit_glyphs_to_authored_layout_period() {
    let font_path =
        Path::new("reverse-engineered/extracted/3742497499/fonts/SourceHanSans-Heavy.otf");
    let font = scene_load_text_raster_font(font_path).unwrap();
    let object = serde_json::json!({
        "pointsize": 96.0,
        "size": "2457.00000 616.00000",
        "padding": "0.00000 0.00000",
        "spacing": "16.00000 16.00000",
        "effects": [{
            "file": "effects/scroll/effect.json",
            "visible": true
        }]
    })
    .as_object()
    .unwrap()
    .clone();

    let font_size = scene_text_raster_effective_font_size(
        &font,
        &object,
        "DREAMLIKE",
        scene_font_size_from_object(&object),
        2457,
        616,
        (0.0, 0.0),
        (16.0, 16.0),
        None,
    );
    assert!(
        font_size > 96.0 * 2.0,
        "scroll text should use the authored effect-texture period, not the small pointsize"
    );

    let image = scene_rasterize_text_image(
        &font,
        "DREAMLIKE",
        font_size,
        2457,
        616,
        SceneTextRasterLayout {
            x: 0.0,
            y: 0.0,
            width: 2457.0,
            height: 616.0,
        },
        SceneTextRasterHorizontalAlign::Middle,
        SceneTextRasterVerticalAlign::Middle,
        (0.0, 0.0),
        (16.0, 16.0),
        [255, 255, 255, 255],
        None,
    )
    .unwrap();
    let (min_x, min_y, max_x, max_y) =
        scene_text_image_alpha_bounds(&image.rgba, image.width, image.height).unwrap();
    assert!(max_x - min_x > 2457 * 3 / 4);
    assert!(max_y - min_y > 616 / 3);

    let plain = serde_json::json!({ "pointsize": 96.0 })
        .as_object()
        .unwrap()
        .clone();
    assert_eq!(
        scene_text_raster_effective_font_size(
            &font,
            &plain,
            "DREAMLIKE",
            scene_font_size_from_object(&plain),
            2457,
            616,
            (0.0, 0.0),
            (16.0, 16.0),
            None,
        ),
        96.0
    );
}

#[test]
fn user_bound_logo_label_text_fits_authored_box() {
    let font_path =
        Path::new("reverse-engineered/extracted/3742497499/fonts/SourceHanSans-Heavy.otf");
    let font = scene_load_text_raster_font(font_path).unwrap();
    let object = serde_json::json!({
        "pointsize": 18.0,
        "size": "746.00000 113.00000",
        "maxwidth": 500.0,
        "padding": "32.00000 32.00000",
        "horizontalalign": "right",
        "verticalalign": "center",
        "text": {
            "user": "newproperty25",
            "value": "A FLOATING DREAM."
        }
    })
    .as_object()
    .unwrap()
    .clone();
    let layout = scene_text_raster_layout_from_object(&object, 746, 113);

    let font_size = scene_text_raster_effective_font_size(
        &font,
        &object,
        "A FLOATING DREAM.",
        scene_font_size_from_object(&object),
        scene_f32_to_u32_layout_extent(layout.width),
        scene_f32_to_u32_layout_extent(layout.height),
        (32.0, 32.0),
        (0.0, 0.0),
        None,
    );

    assert!(
        font_size > 18.0 * 1.5,
        "logo label should not rasterize to a barely visible 18px glyph box"
    );
    assert!(font_size < 96.0);

    let mut image = scene_rasterize_text_image(
        &font,
        "A FLOATING DREAM.",
        font_size,
        746,
        113,
        layout,
        SceneTextRasterHorizontalAlign::End,
        SceneTextRasterVerticalAlign::Middle,
        (32.0, 32.0),
        (0.0, 0.0),
        [255, 255, 255, 255],
        None,
    )
    .unwrap();
    let crop = scene_crop_text_image_to_alpha_bounds(&mut image).unwrap();
    assert!(
        crop.width > 640,
        "logo label should fill the authored WE text box horizontally"
    );
}

#[test]
fn user_bound_logo_label_text_ignores_maxwidth_without_limitwidth() {
    let font_path =
        Path::new("reverse-engineered/extracted/3742497499/fonts/SourceHanSans-Heavy.otf");
    let font = scene_load_text_raster_font(font_path).unwrap();
    let object = serde_json::json!({
        "pointsize": 18.0,
        "size": "746.00000 113.00000",
        "maxwidth": 500.0,
        "padding": "32.00000 32.00000",
        "horizontalalign": "right",
        "verticalalign": "center",
        "text": {
            "user": "newproperty25",
            "value": "A FLOATING DREAM."
        }
    })
    .as_object()
    .unwrap()
    .clone();
    let layout = scene_text_raster_layout_from_object(&object, 746, 113);
    assert_eq!((layout.x, layout.width), (0.0, 746.0));

    let font_size = scene_text_raster_effective_font_size(
        &font,
        &object,
        "A FLOATING DREAM.",
        scene_font_size_from_object(&object),
        scene_f32_to_u32_layout_extent(layout.width),
        scene_f32_to_u32_layout_extent(layout.height),
        (32.0, 32.0),
        (0.0, 0.0),
        None,
    );
    let mut image = scene_rasterize_text_image(
        &font,
        "A FLOATING DREAM.",
        font_size,
        746,
        113,
        layout,
        SceneTextRasterHorizontalAlign::End,
        SceneTextRasterVerticalAlign::Middle,
        (32.0, 32.0),
        (0.0, 0.0),
        [255, 255, 255, 255],
        None,
    )
    .unwrap();
    let crop = scene_crop_text_image_to_alpha_bounds(&mut image).unwrap();
    let local_right =
        -0.5 * f64::from(crop.original_width) + f64::from(crop.x) + f64::from(crop.width);

    assert!(
        local_right > 300.0,
        "maxwidth is only a constraint when WE limitwidth is enabled"
    );
}

#[test]
fn text_font_apply_user_properties_uses_project_default_registered_asset() {
    let object = serde_json::json!({
        "font": "fonts/SourceHanSans-Heavy.otf",
        "text": {
            "value": "A FLOATING DREAM.",
            "script": "'use strict';\nlet font1 = engine.registerAsset(\"fonts/Jost-Medium.ttf\");\nlet font2 = engine.registerAsset(\"fonts/Atami-Regular.otf\");\nexport function applyUserProperties(userProperties) {\n  if(userProperties.hasOwnProperty('text3')){\n    switch(userProperties.text3){\n      case(\"1\"):\n        thisLayer.font = font1;\n        break;\n      case(\"2\"):\n        thisLayer.font = font2;\n        break;\n    }\n  }\n}"
        }
    })
    .as_object()
    .unwrap()
    .clone();
    let mut context = SceneDocumentBuildContext::default();
    context
        .project_property_defaults
        .insert("text3".to_owned(), json!("2"));

    let (font, lowered) = scene_effective_text_font_family_from_object(&object, &context).unwrap();

    assert!(lowered);
    assert_eq!(font, "fonts/Atami-Regular.otf");
}

#[test]
fn right_aligned_text_crop_uses_text_align_anchor() {
    let mut node = Map::new();
    node.insert("width".to_owned(), json!(746));
    node.insert("height".to_owned(), json!(113));
    node.insert(
        "transform".to_owned(),
        json!({
            "x": 3221.0,
            "y": 229.0,
            "scale_x": 1.0,
            "scale_y": 1.0,
            "rotation_deg": 0.0,
            "anchor_x": 0.5,
            "anchor_y": 0.5
        }),
    );
    let object = serde_json::json!({
        "size": "746.00000 113.00000",
        "horizontalalign": "right",
        "verticalalign": "center"
    })
    .as_object()
    .unwrap()
    .clone();

    scene_apply_text_raster_default_anchor_to_node(&mut node, &object);
    scene_apply_generated_text_crop_to_node(
        &mut node,
        Some(SceneTextRasterCrop {
            original_width: 746,
            original_height: 113,
            x: 213,
            y: 37,
            width: 498,
            height: 39,
        }),
    );

    let transform = node["transform"].as_object().unwrap();
    assert!(
        (transform["x"].as_f64().unwrap() - 2972.0).abs() < 1.0e-6,
        "cropping a right-aligned WE label must keep the right boundary fixed"
    );
}

#[test]
fn generated_logo_text_aligns_to_audio_mark_right_edge() {
    let mut context = SceneDocumentBuildContext::default();
    let mut nodes = vec![
        json!({
            "id": "node-text",
            "type": "image",
            "resource": "resource-font-text-raster",
            "width": 20.0,
            "height": 10.0,
            "transform": {
                "x": 90.0,
                "y": 0.0,
                "scale_x": 1.0,
                "scale_y": 1.0,
                "anchor_x": 0.5,
                "anchor_y": 0.5
            }
        }),
        json!({
            "id": "node-audio",
            "type": "image",
            "width": 100.0,
            "height": 100.0,
            "transform": {
                "x": 60.0,
                "y": 0.0,
                "scale_x": 1.0,
                "scale_y": 1.0,
                "anchor_x": 0.5,
                "anchor_y": 0.5
            },
            "effects": [
                {
                    "file": "effects/workshop/3082978660/enhanced_simple_audio_bars/effect.json"
                },
                {
                    "file": "effects/skew/effect.json",
                    "passes": [{
                        "constantshadervalues": {
                            "bottom": -0.4,
                            "top": 0.0
                        }
                    }]
                }
            ]
        }),
    ];

    scene_align_audio_mark_text_right_edges(&mut nodes, &mut context);

    let text = nodes[0].as_object().unwrap();
    let transform = text["transform"].as_object().unwrap();
    assert!((transform["x"].as_f64().unwrap() - 80.0).abs() < 1.0e-6);
    assert!(
        context
            .converted_features
            .contains(&"wallpaper-engine-audio-mark-text-right-edge-alignment".to_owned())
    );
}

#[test]
fn font_text_crop_reanchors_node_to_visible_bounds() {
    let mut node = Map::new();
    node.insert("width".to_owned(), json!(100));
    node.insert("height".to_owned(), json!(40));
    node.insert(
        "transform".to_owned(),
        json!({
            "x": 10.0,
            "y": 20.0,
            "scale_x": 2.0,
            "scale_y": 3.0,
            "rotation_deg": 0.0,
            "anchor_x": 0.5,
            "anchor_y": 0.5
        }),
    );

    scene_apply_generated_text_crop_to_node(
        &mut node,
        Some(SceneTextRasterCrop {
            original_width: 100,
            original_height: 40,
            x: 60,
            y: 15,
            width: 20,
            height: 10,
        }),
    );

    assert_eq!(node["width"], json!(20));
    assert_eq!(node["height"], json!(10));
    let transform = node["transform"].as_object().unwrap();
    assert_eq!(transform["anchor_x"], json!(0.5));
    assert_eq!(transform["anchor_y"], json!(0.5));
    assert!((transform["x"].as_f64().unwrap() - 50.0).abs() < 1.0e-6);
    assert!((transform["y"].as_f64().unwrap() - 20.0).abs() < 1.0e-6);
}

fn binary_chunk_count(path: &Path, kind: SceneBinaryChunkKind) -> u32 {
    let bytes = fs::read(path).unwrap();
    let layout = decode_scene_binary_container(&bytes).unwrap();
    layout.chunk(kind).map_or(0, |chunk| chunk.record_count)
}

fn legacy_scene_json_path(root: &Path) -> PathBuf {
    root.join("assets")
        .join(["scene", "gscene", "json"].join("."))
}

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(name: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("gilder-test-{name}-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write_file(&self, relative_path: &str, contents: &str) {
        let path = self.path.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    fn remove(&self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
