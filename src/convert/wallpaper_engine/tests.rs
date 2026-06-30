use super::*;
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
    source.write_file("preview.jpg", "not real jpg");
    source.write_file(
        PROJECT_FILE,
        r##"{
              "type": "image",
              "title": "Static Example",
              "file": "wallpaper.png",
              "preview": "preview.jpg",
              "general": {
                "properties": {
                  "accent": { "type": "color", "value": "1 0.5 0" }
                }
              }
            }"##,
    );

    let summary = convert_project(source.path(), output.path()).unwrap();
    assert_eq!(summary.source_type, "image");
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
            .unwrap();
    assert_eq!(manifest["kind"], "static-image");
    assert_eq!(manifest["properties"]["accent"]["default"], "#ff8000");
    assert!(
        output
            .path()
            .join("metadata/conversion-report.json")
            .exists()
    );
}

#[test]
fn lowers_wallpaper_engine_scenetexture_property_as_manifest_metadata() {
    let source = TestDir::new("we-scenetexture-property-source");
    let output = TestDir::new("we-scenetexture-property-output");
    output.remove();
    source.write_file("wallpaper.png", "not real png");
    source.write_file(
        PROJECT_FILE,
        r##"{
              "type": "image",
              "title": "Scene Texture Property",
              "file": "wallpaper.png",
              "general": {
                "properties": {
                  "banner": { "type": "scenetexture", "text": "<img src=preview>", "value": "" }
                }
              }
            }"##,
    );

    convert_project(source.path(), output.path()).unwrap();
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
            .unwrap();
    assert_eq!(manifest["properties"]["banner"]["type"], "text");
    assert_eq!(manifest["properties"]["banner"]["default"], "");
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"wallpaper-engine-scenetexture-property-lowering".to_owned())
    );
    assert!(
        !report
            .unsupported_features
            .contains(&"property:scenetexture".to_owned())
    );
}

#[test]
fn converts_static_image_project_without_preview_from_source_image() {
    let source = TestDir::new("we-static-no-preview-source");
    let output = TestDir::new("we-static-no-preview-output");
    output.remove();
    source.write_file("wallpaper.png", "not real png");
    source.write_file(
        PROJECT_FILE,
        r##"{
              "type": "image",
              "title": "Static Without Preview",
              "file": "wallpaper.png"
            }"##,
    );

    convert_project(source.path(), output.path()).unwrap();
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
            .unwrap();
    assert_eq!(manifest["preview"]["poster"], "previews/poster.png");
    assert_eq!(manifest["preview"]["thumbnail"], "previews/thumbnail.png");
    assert!(output.path().join("previews/poster.png").exists());
    assert!(output.path().join("previews/thumbnail.png").exists());
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .generated_assets
            .contains(&"previews/poster.png".to_owned())
    );
    assert!(
        report
            .generated_assets
            .contains(&"previews/thumbnail.png".to_owned())
    );
}

#[test]
fn converts_static_image_audio_to_scene_audio_cue() {
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
    assert_eq!(manifest["entry"]["type"], "scene");
    assert_eq!(manifest["entry"]["source"], "assets/scene.gscene.json");
    assert!(manifest["entry"].get("max_fps").is_none());
    assert_eq!(manifest["runtime"]["allow_audio"], true);
    assert!(output.path().join("assets/wallpaper.gtex").exists());
    assert!(output.path().join("assets/audio-cue-0.ogg").exists());

    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(scene["resources"][0]["type"], "image");
    assert_eq!(scene["resources"][0]["source"], "assets/wallpaper.gtex");
    assert_eq!(scene["resources"][1]["type"], "audio");
    assert_eq!(scene["resources"][1]["source"], "assets/audio-cue-0.ogg");
    assert_eq!(scene["nodes"][0]["type"], "image");
    assert_eq!(scene["nodes"][0]["resource"], "static-image");
    assert_eq!(scene["nodes"][0]["audio"][0]["resource"], "static-audio-0");
    assert_eq!(scene["nodes"][0]["audio"][0]["playback_mode"], "loop");

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"static-image-audio-scene".to_owned())
    );
    assert!(
        report
            .converted_features
            .contains(&"scene-audio-cue-pipewire-present-runtime".to_owned())
    );
    assert!(
        !report
            .unsupported_features
            .contains(&"audio-runtime".to_owned())
    );
}

#[test]
fn converts_static_image_mp4_audio_field_to_scene_audio_cue() {
    let source = TestDir::new("we-static-mp4-audio-source");
    let output = TestDir::new("we-static-mp4-audio-output");
    output.remove();
    write_test_png(&source.path().join("wallpaper.png"));
    source.write_file("music.mp4", "not real mp4");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "image",
              "title": "Static With MP4 Audio",
              "file": "wallpaper.png",
              "audio": "music.mp4"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
            .unwrap();
    assert_eq!(manifest["kind"], "scene");
    assert_eq!(manifest["entry"]["type"], "scene");
    assert_eq!(manifest["runtime"]["allow_audio"], true);
    assert!(output.path().join("assets/wallpaper.gtex").exists());
    assert!(output.path().join("assets/audio-cue-0.mp4").exists());

    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(scene["resources"][0]["source"], "assets/wallpaper.gtex");
    assert_eq!(scene["resources"][1]["type"], "audio");
    assert_eq!(scene["resources"][1]["source"], "assets/audio-cue-0.mp4");
    assert_eq!(scene["nodes"][0]["audio"][0]["resource"], "static-audio-0");

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"static-image-bc7-gtex-conversion".to_owned())
    );
}

#[test]
fn converts_static_image_project_with_resolution_variants() {
    let source = TestDir::new("we-static-variant-source");
    let output = TestDir::new("we-static-variant-output");
    let tools = TestDir::new("we-static-variant-tools");
    source.write_file("wallpaper.png", "fake large image");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "image",
              "title": "Static Variant Source",
              "file": "wallpaper.png"
            }"#,
    );
    let ffprobe = tools.path().join("ffprobe");
    write_executable_script(
        &ffprobe,
        r#"#!/bin/sh
printf '{"streams":[{"width":7680,"height":4320}]}'
exit 0
"#,
    );
    let ffmpeg = tools.path().join("ffmpeg");
    write_executable_script(
        &ffmpeg,
        r#"#!/bin/sh
out=""
for arg in "$@"; do
  out="$arg"
done
printf 'png-variant' > "$out"
exit 0
"#,
    );
    let project = WallpaperEngineProject::load(source.path()).unwrap();
    let mut report = ConversionReport::new("image");

    let manifest = convert_static_image_with_variant_tools(
        &project,
        output.path(),
        &mut report,
        Some(StaticImageVariantTools {
            ffmpeg: &ffmpeg,
            ffprobe: &ffprobe,
        }),
    )
    .unwrap();
    write_json_pretty(output.path().join(MANIFEST_FILE), &manifest).unwrap();
    load_gwpdir(output.path()).unwrap();

    let variants = manifest["variants"].as_array().unwrap();
    assert_eq!(manifest["entry"]["width"], 7680);
    assert_eq!(manifest["entry"]["height"], 4320);
    let ids = variants
        .iter()
        .map(|variant| variant["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![
            "landscape-1080p",
            "landscape-2160p",
            "ultrawide-1080p",
            "ultrawide-1440p",
            "portrait-1080p",
            "portrait-2160p",
        ]
    );
    assert_eq!(variants[0]["width"], 1920);
    assert_eq!(variants[0]["height"], 1080);
    assert_eq!(variants[3]["width"], 3440);
    assert_eq!(variants[3]["height"], 1440);
    assert_eq!(variants[5]["width"], 2160);
    assert_eq!(variants[5]["height"], 3840);
    for id in &ids {
        assert!(output.path().join(format!("variants/{id}.png")).exists());
    }
    assert!(
        report
            .generated_assets
            .contains(&"variants/landscape-1080p.png".to_owned())
    );
    assert!(
        report
            .converted_features
            .contains(&"static-image:variant:portrait-2160p".to_owned())
    );
}

#[test]
fn converts_video_project() {
    let source = TestDir::new("we-video-source");
    let output = TestDir::new("we-video-output");
    output.remove();
    source.write_file("loop.mp4", "not real mp4");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "video",
              "title": "Video Example",
              "file": "loop.mp4"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
            .unwrap();
    assert_eq!(manifest["kind"], "video");
    assert_eq!(manifest["entry"]["source"], "assets/loop.mp4");
    assert_eq!(manifest["entry"]["muted"], true);
    assert_eq!(manifest["runtime"]["allow_audio"], false);
    assert_eq!(manifest["entry"]["poster"], "previews/poster.svg");
    assert_eq!(manifest["preview"]["poster"], "previews/poster.svg");
    assert_eq!(manifest["preview"]["thumbnail"], "previews/thumbnail.svg");
    assert!(output.path().join("previews/poster.svg").exists());
    assert!(output.path().join("previews/thumbnail.svg").exists());
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .generated_assets
            .contains(&"previews/poster.svg".to_owned())
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("metadata-based video poster"))
    );
    assert!(!report.detected_features.contains(&"audio".to_owned()));
}

#[test]
fn converts_video_audio_intent_to_runtime_audio_policy() {
    let source = TestDir::new("we-video-audio-source");
    let output = TestDir::new("we-video-audio-output");
    output.remove();
    source.write_file("loop.mp4", "not real mp4");
    source.write_file("music.ogg", "not real audio");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "video",
              "title": "Video With Audio",
              "file": "loop.mp4",
              "audio": "music.ogg"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
            .unwrap();
    assert_eq!(manifest["entry"]["muted"], false);
    assert_eq!(manifest["runtime"]["allow_audio"], true);
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(report.detected_features.contains(&"audio".to_owned()));
    assert!(
        report
            .converted_features
            .contains(&"audio-policy".to_owned())
    );
}

#[test]
fn generates_video_preview_from_first_frame_with_ffmpeg() {
    let source = TestDir::new("we-video-frame-source");
    let output = TestDir::new("we-video-frame-output");
    let tools = TestDir::new("we-video-frame-tools");
    output.remove();
    source.write_file("loop.mp4", "not real mp4");
    let ffmpeg = tools.path().join("ffmpeg");
    write_executable_script(
        &ffmpeg,
        r#"#!/bin/sh
for arg in "$@"; do
  case "$arg" in
    *.jpg) printf 'jpeg-frame' > "$arg" ;;
  esac
done
exit 0
"#,
    );

    let mut report = ConversionReport::new("video");
    let preview = generate_video_first_frame_preview_with_ffmpeg(
        &ffmpeg,
        &source.path().join("loop.mp4"),
        output.path(),
        &mut report,
    )
    .unwrap();

    assert_eq!(preview.poster.as_deref(), Some("previews/poster.jpg"));
    assert_eq!(preview.thumbnail.as_deref(), Some("previews/thumbnail.jpg"));
    assert_eq!(
        fs::read(output.path().join("previews/poster.jpg")).unwrap(),
        b"jpeg-frame"
    );
    assert_eq!(
        fs::read(output.path().join("previews/thumbnail.jpg")).unwrap(),
        b"jpeg-frame"
    );
    assert!(
        report
            .generated_assets
            .contains(&"previews/poster.jpg".to_owned())
    );
    assert!(
        report
            .generated_assets
            .contains(&"previews/thumbnail.jpg".to_owned())
    );
}

#[test]
fn finds_executable_on_path_list() {
    let tools = TestDir::new("we-path-tools");
    let ffmpeg = tools.path().join("ffmpeg");
    write_executable_script(&ffmpeg, "#!/bin/sh\nexit 0\n");

    let found = find_executable_in_path_list("ffmpeg", [tools.path().to_path_buf()]);
    assert_eq!(found.as_deref(), Some(ffmpeg.as_path()));
}

#[test]
fn skips_binary_entry_content_when_collecting_feature_hints() {
    let source = TestDir::new("we-feature-binary-source");
    source.write_file(
        "loop.mp4",
        "https://example.invalid\nwindow.wallpaperRegisterAudioListener(() => {});",
    );

    let mut features = BTreeSet::new();
    collect_feature_hints_from_entry(SourceType::Video, source.path(), "loop.mp4", &mut features);

    assert!(!features.contains("network"));
    assert!(!features.contains("web-audio-listener"));
}

#[test]
fn caps_large_text_entry_feature_scan() {
    let source = TestDir::new("we-feature-large-source");
    let mut html = "<script>window.wallpaperPropertyListener = {};</script>".to_owned();
    html.push_str(&" ".repeat(FEATURE_SCAN_MAX_BYTES as usize + 128));
    source.write_file("index.html", &html);

    let mut features = BTreeSet::new();
    collect_feature_hints_from_entry(SourceType::Web, source.path(), "index.html", &mut features);

    assert!(features.contains("web-property-listener"));
}

#[test]
fn converts_web_project() {
    let source = TestDir::new("we-web-source");
    let output = TestDir::new("we-web-output");
    output.remove();
    source.write_file(
        "index.html",
        "<!doctype html><script src=\"app.js\"></script>",
    );
    source.write_file("app.js", "window.ready = true;");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "web",
              "title": "Web Example",
              "file": "index.html",
              "general": {
                "properties": {
                  "enabled": { "type": "bool", "value": true },
                  "speed": { "type": "slider", "min": 0, "max": 2, "step": 0.1, "value": 1 }
                }
              }
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
            .unwrap();
    assert_eq!(manifest["kind"], "web");
    assert_eq!(manifest["entry"]["root"], "assets/web");
    assert_eq!(manifest["entry"]["index"], "index.html");
    assert!(output.path().join("assets/web/app.js").exists());
    assert!(output.path().join("assets/web/gilder-bridge.js").exists());
    assert_eq!(manifest["properties"]["enabled"]["type"], "bool");
    assert_eq!(manifest["properties"]["speed"]["type"], "range");
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .unsupported_features
            .contains(&"web-runtime".to_owned())
    );
}

#[test]
fn reports_web_runtime_feature_gaps() {
    let source = TestDir::new("we-web-runtime-source");
    let output = TestDir::new("we-web-runtime-output");
    output.remove();
    source.write_file(
        "index.html",
        r#"<!doctype html>
<script>
window.wallpaperPropertyListener = {};
window.wallpaperRegisterAudioListener(() => {});
fetch("https://example.invalid/data.json");
</script>"#,
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "web",
              "title": "Web Runtime Features",
              "file": "index.html"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    for feature in ["web-property-listener", "web-audio-listener", "network"] {
        assert!(
            report.detected_features.contains(&feature.to_owned()),
            "missing detected feature {feature}: {:?}",
            report.detected_features
        );
    }
    for feature in ["web-runtime", "web-permissions"] {
        assert!(
            report.unsupported_features.contains(&feature.to_owned()),
            "missing unsupported feature {feature}: {:?}",
            report.unsupported_features
        );
    }
}

#[test]
fn web_audio_intent_is_reported_as_unsupported_runtime_feature() {
    let source = TestDir::new("we-web-audio-source");
    let output = TestDir::new("we-web-audio-output");
    output.remove();
    source.write_file("index.html", "<!doctype html>");
    source.write_file("music.mp3", "not real audio");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "web",
              "title": "Web Audio Example",
              "file": "index.html",
              "audiofile": "music.mp3"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
            .unwrap();
    assert_eq!(manifest["runtime"]["allow_audio"], false);
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(report.detected_features.contains(&"audio".to_owned()));
    assert!(
        report
            .unsupported_features
            .contains(&"audio-runtime".to_owned())
    );
}

#[test]
fn converts_shader_project_with_fallback_manifest() {
    let source = TestDir::new("we-shader-source");
    let output = TestDir::new("we-shader-output");
    output.remove();
    source.write_file(
        "main.frag",
        r#"
uniform float u_time;
uniform vec2 u_resolution;
uniform float u_intensity;
void main() {}
"#,
    );
    source.write_file(
        PROJECT_FILE,
        r##"{
              "type": "shader",
              "title": "Shader Example",
              "file": "main.frag",
              "general": {
                "properties": {
                  "Intensity": { "type": "slider", "min": 0, "max": 1, "value": 0.5 }
                }
              }
            }"##,
    );

    convert_project(source.path(), output.path()).unwrap();
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
            .unwrap();
    assert_eq!(manifest["kind"], "shader");
    assert_eq!(manifest["entry"]["type"], "shader");
    assert_eq!(manifest["entry"]["source"], "shaders/main.frag");
    assert_eq!(manifest["entry"]["fallback"], "previews/poster.svg");
    assert_eq!(manifest["entry"]["language"], "glsl");
    assert_eq!(manifest["entry"]["max_fps"], 60);
    assert_eq!(manifest["properties"]["Intensity"]["type"], "range");
    let uniforms = manifest["entry"]["uniforms"].as_array().unwrap();
    assert!(
        uniforms
            .iter()
            .any(|uniform| { uniform["name"] == "u_time" && uniform["source"] == "time" })
    );
    assert!(
        uniforms.iter().any(|uniform| {
            uniform["name"] == "u_resolution" && uniform["source"] == "resolution"
        })
    );
    assert!(
        uniforms
            .iter()
            .any(|uniform| { uniform["name"] == "u_mouse" && uniform["source"] == "mouse" })
    );
    assert!(uniforms.iter().any(|uniform| {
        uniform["name"] == "u_intensity"
            && uniform["source"] == "property"
            && uniform["property"] == "Intensity"
    }));
    assert!(output.path().join("shaders/main.frag").exists());
    assert!(output.path().join("previews/poster.svg").exists());

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(report.source_type, "shader");
    assert!(report.detected_features.contains(&"shader".to_owned()));
    assert!(report.converted_features.contains(&"shader".to_owned()));
    assert!(
        report
            .unsupported_features
            .contains(&"shader-runtime".to_owned())
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("fallback poster"))
    );
}

#[test]
fn converts_scene_project_to_native_scene_document() {
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
    assert_eq!(manifest["entry"]["type"], "scene");
    assert_eq!(manifest["entry"]["source"], "assets/scene.gscene.json");
    assert!(manifest["entry"].get("max_fps").is_none());
    assert!(manifest["entry"].get("fallback").is_none());
    assert_eq!(manifest["preview"]["poster"], "previews/poster.svg");
    assert!(output.path().join("metadata/source-scene.json").exists());
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(scene["version"], 1);
    assert_eq!(scene["profile"], "native-vulkan-full-scene");
    assert_eq!(scene["source"]["metadata"], "metadata/source-scene.json");
    assert_eq!(scene["source"]["entry"], "scene.json");
    assert_eq!(scene["nodes"][0]["type"], "image");
    assert_eq!(scene["nodes"][0]["resource"], "resource-1-background");
    assert_eq!(
        scene["resources"][0]["source"],
        "assets/scene-resources/scene/resource-1-background.png"
    );
    assert!(scene["native_lowering"].get("fallback").is_none());
    assert!(output.path().join("previews/poster.svg").exists());
    assert!(output.path().join("previews/thumbnail.svg").exists());
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(report.converted_features.contains(&"scene".to_owned()));
    let full_scene = report.full_scene.as_ref().expect("full scene status");
    assert_eq!(full_scene.target_runtime, "native-vulkan-full-scene");
    assert_eq!(full_scene.current_runtime, "native-vulkan-scene-runtime");
    assert_eq!(full_scene.progress_estimate_percent, 100);
    assert!(full_scene.full_scene_complete);
    assert!(
        full_scene
            .source_scene_metadata
            .contains(&"metadata/source-scene.json".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"descriptor-heap-sampled-image-resources".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"wallpaper-engine-parent-graph-lowering".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"native-scene-graph-transform-opacity-execution".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"render-clear-color-snapshot-layer".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"wallpaper-engine-text-and-visible-property-lowering".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"wallpaper-engine-deterministic-scenescript-expression-lowering".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"wallpaper-engine-animation-layer-keyframe-lowering".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"native-vulkan-full-scene-runtime-status".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"native-runtime-layer-coverage-metric".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"deterministic-text-glyph-geometry-runtime".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"stroke-geometry-runtime".to_owned())
    );
    assert!(
        !full_scene
            .completed_boundaries
            .contains(&"vulkan-video-scene-layer-composition".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"timeline-animation-runtime".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"scene-geometry-field-animation-runtime".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"per-frame-timeline-geometry-runtime".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"wallpaper-engine-particle-field-lowering".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"native-particle-system-runtime".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"wallpaper-engine-tex-bc7-gtex-conversion".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"wallpaper-engine-scene-pkg-import".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"scene-we-spritesheet-atlas-runtime".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"parallax-property-camera-model".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"property-update-runtime".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"pause-resume-policy-runtime".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"package-state-persistence".to_owned())
    );
    assert!(
        !full_scene
            .pending_boundaries
            .contains(&"mixed-video-scene-composition".to_owned())
    );
    assert!(
        !full_scene
            .pending_boundaries
            .contains(&"timeline-animation-runtime".to_owned())
    );
    assert!(
        !full_scene
            .pending_boundaries
            .contains(&"particle-systems".to_owned())
    );
    assert!(
        !full_scene
            .pending_boundaries
            .contains(&"package-state-persistence".to_owned())
    );
    assert!(
        !full_scene
            .pending_boundaries
            .contains(&"full-wallpaper-engine-scene-graph".to_owned())
    );
    assert!(
        !full_scene
            .pending_boundaries
            .contains(&"spritesheet-animation-runtime".to_owned())
    );
    assert!(
        !full_scene
            .pending_boundaries
            .contains(&"cursor-parallax-input-source".to_owned())
    );
    assert!(
        full_scene
            .unsupported_boundaries
            .contains(&"cursor-parallax-input-source".to_owned())
    );
    assert!(
        !report
            .unsupported_features
            .contains(&"scene-runtime".to_owned())
    );
    assert!(
        report
            .detected_features
            .contains(&"scene-layers".to_owned())
    );
    assert!(report.detected_features.contains(&"image-layer".to_owned()));
}

#[test]
fn scene_conversion_does_not_substitute_preview_fallback_node() {
    let source = TestDir::new("we-scene-empty-source");
    let output = TestDir::new("we-scene-empty-output");
    output.remove();
    source.write_file("scene.json", r#"{ "objects": [] }"#);
    source.write_file("preview.png", "not real png");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Empty Scene",
              "file": "scene.json",
              "preview": "preview.png"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert!(scene["nodes"].as_array().unwrap().is_empty());
    assert!(
        scene["resources"]
            .as_array()
            .unwrap()
            .iter()
            .all(|resource| resource["id"] != "resource-preview-fallback")
    );
    assert!(
        scene["unsupported_features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| feature["feature"] == "empty-scene-graph")
    );
}

#[test]
fn converts_wallpaper_engine_scene_pkg_without_preextracted_scene_files() {
    let source = TestDir::new("we-scene-pkg-source");
    let output = TestDir::new("we-scene-pkg-output");
    output.remove();
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Packaged Scene",
              "file": "scene.json",
              "preview": "preview.jpg"
            }"#,
    );
    source.write_bytes("preview.jpg", b"preview");
    source.write_bytes(
        SCENE_PACKAGE_FILE,
        &test_scene_pkg(&[
            (
                "scene.json",
                br#"{"objects":[{"type":"image","path":"background.png"}]}"#,
            ),
            ("background.png", b"not real png"),
        ]),
    );

    convert_project(source.path(), output.path()).unwrap();
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
            .unwrap();
    assert_eq!(manifest["kind"], "scene");
    assert_eq!(manifest["entry"]["source"], "assets/scene.gscene.json");
    assert_eq!(manifest["preview"]["poster"], "previews/poster.jpg");
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(scene["source"]["entry"], "scene.json");
    assert_eq!(scene["nodes"][0]["resource"], "resource-1-background");
    assert_eq!(
        scene["resources"][0]["source"],
        "assets/scene-resources/scene/resource-1-background.png"
    );
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .detected_features
            .contains(&"scene-package".to_owned())
    );
    assert!(
        report
            .converted_features
            .contains(&"scene-we-package-import".to_owned())
    );
    let full_scene = report.full_scene.as_ref().expect("full scene status");
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"wallpaper-engine-scene-pkg-import".to_owned())
    );
}

#[test]
fn scene_pkg_takes_precedence_over_preextracted_scene_files() {
    let source = TestDir::new("we-scene-pkg-precedence-source");
    let output = TestDir::new("we-scene-pkg-precedence-output");
    output.remove();
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Packaged Scene Precedence",
              "file": "scene.json"
            }"#,
    );
    source.write_file(
        "scene.json",
        r#"{"objects":[{"type":"image","path":"loose.png"}]}"#,
    );
    source.write_file("loose.png", "wrong loose png");

    let rgba = vec![
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
    ];
    let tex = test_we_tex_rgba(2, 2, &rgba);
    source.write_bytes(
        SCENE_PACKAGE_FILE,
        &test_scene_pkg(&[
            (
                "scene.json",
                br#"{"objects":[{"id":1,"name":"Packed","image":"models/renderable.json"}]}"#,
            ),
            (
                "models/renderable.json",
                br#"{ "material": "materials/renderable.json", "width": 2, "height": 2 }"#,
            ),
            (
                "materials/renderable.json",
                br#"{ "passes": [{ "textures": ["atlas"] }] }"#,
            ),
            ("materials/atlas.tex", &tex),
        ]),
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(scene["nodes"][0]["name"], "Packed");
    assert_eq!(scene["nodes"][0]["resource"], "resource-3-atlas-frame-0");
    assert!(
        scene["resources"]
            .as_array()
            .unwrap()
            .iter()
            .all(|resource| {
                resource["source"]
                    .as_str()
                    .is_none_or(|source| !source.ends_with("loose.png"))
            })
    );
    assert!(
        output
            .path()
            .join("assets/scene-resources/scene/resource-3-atlas-frame-0.gtex")
            .exists()
    );
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"scene-we-package-import".to_owned())
    );
    assert!(
        report
            .converted_features
            .contains(&"scene-we-tex-bc7-gpu-texture".to_owned())
    );
}

#[test]
fn reports_scene_runtime_feature_gaps() {
    let source = TestDir::new("we-scene-feature-source");
    let output = TestDir::new("we-scene-feature-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                { "type": "image", "path": "background.png" },
                { "type": "particle", "emitter": "sparks" }
              ],
              "timeline": [{ "time": 0, "keyframe": true }],
              "script": "SceneScript.Update = function() {}",
              "shader": "effects/rain.frag",
              "parallax": true,
              "audio_response": true
            }"#,
    );
    source.write_file("background.png", "not real png");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Scene Runtime Features",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    for feature in [
        "scene-layers",
        "image-layer",
        "particles",
        "timeline",
        "scenescript",
        "shader",
        "parallax",
        "audio-response",
    ] {
        assert!(
            report.detected_features.contains(&feature.to_owned()),
            "missing detected feature {feature}: {:?}",
            report.detected_features
        );
    }
    for feature in [
        "timeline-animation",
        "scenescript",
        "custom-shader",
        "cursor-parallax-input-source",
        "audio-response-runtime",
    ] {
        assert!(
            report.unsupported_features.contains(&feature.to_owned()),
            "missing unsupported feature {feature}: {:?}",
            report.unsupported_features
        );
    }
    assert!(
        report
            .converted_features
            .contains(&"native-particle-runtime".to_owned()),
        "missing native particle conversion marker: {:?}",
        report.converted_features
    );
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let particle = scene["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["type"] == "particle-emitter")
        .expect("particle node");
    assert_eq!(particle["properties"]["particle"]["source"], "sparks");
    assert_eq!(scene["systems"]["particles"], "ready");
}

#[test]
fn lowers_wallpaper_engine_particle_runtime_fields_to_native_scene() {
    let source = TestDir::new("we-scene-particle-fields-source");
    let output = TestDir::new("we-scene-particle-fields-output");
    output.remove();
    source.write_file(
        "scene.json",
        r##"{
              "objects": [
                {
                  "id": 42,
                  "type": "particle",
                  "particle": "particles/spark.json",
                  "size": [320, 180, 0],
                  "directionDeg": -45,
                  "spreadDeg": 30,
                  "gravityDirection": [0, 1, 0],
                  "gravityStrength": 16,
                  "instanceoverride": {
                    "count": 12,
                    "speedMin": 8,
                    "speedMax": 24,
                    "size": [6, 10, 0],
                    "lifetime": 2,
                    "fadeOut": false,
                    "colorn": [1, 0.5, 0]
                  }
                }
              ]
            }"##,
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Particle Field Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let particle = &scene["nodes"][0]["properties"]["particle"];
    assert_eq!(particle["source"], "particles/spark.json");
    assert_eq!(particle["seed"], 42);
    assert_eq!(particle["spawn_width"], 320.0);
    assert_eq!(particle["spawn_height"], 180.0);
    assert_eq!(particle["count"], 12);
    assert_eq!(particle["speed_min"], 8.0);
    assert_eq!(particle["speed_max"], 24.0);
    assert_eq!(particle["width"], 6.0);
    assert_eq!(particle["height"], 10.0);
    assert_eq!(particle["lifetime"], 2.0);
    assert_eq!(particle["fade"], false);
    assert_eq!(particle["color"], "#ff8000");
    assert_eq!(particle["direction_deg"], -45.0);
    assert_eq!(particle["spread_deg"], 30.0);
    assert_eq!(particle["gravity_x"], 0.0);
    assert_eq!(particle["gravity_y"], 16.0);

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let snapshot = document.snapshot_at_with_property_resolver(500, |_| None);
    assert_eq!(snapshot.layers.len(), 12);
    assert_eq!(snapshot.layers[0].width, Some(6.0));
    assert_eq!(snapshot.layers[0].height, Some(10.0));
    assert_eq!(snapshot.layers[0].color.as_deref(), Some("#ff8000"));
}

#[test]
fn lowers_wallpaper_engine_particle_definition_file_to_native_scene() {
    let source = TestDir::new("we-scene-particle-definition-source");
    let output = TestDir::new("we-scene-particle-definition-output");
    output.remove();
    source.write_file(
        "scene.json",
        r##"{
              "objects": [
                {
                  "id": 77,
                  "type": "particle",
                  "particle": "particles/spark.json"
                }
              ]
            }"##,
    );
    source.write_file(
        "particles/spark.json",
        r##"{
              "maxcount": 32,
              "material": "materials/spark.json",
              "emitter": [
                {
                  "name": "boxrandom",
                  "distancemax": [96, 48, 0],
                  "directions": [1, 0.5, 0],
                  "rate": 16,
                  "speedmin": 4,
                  "speedmax": 12
                }
              ],
              "initializer": [
                { "name": "sizerandom", "min": 10, "max": 18 },
                { "name": "lifetimerandom", "min": 1, "max": 3 },
                { "name": "colorrandom", "min": [0, 0.5, 1], "max": [1, 0.5, 0] }
              ],
              "operator": [
                { "name": "movement", "gravity": [0, 18, 0] }
              ],
              "renderer": [
                { "name": "sprite", "fadealpha": true }
              ]
            }"##,
    );
    source.write_file(
        "materials/spark.json",
        r#"{ "passes": [{ "textures": ["textures/spark.png"] }] }"#,
    );
    source.write_file("textures/spark.png", "not real png");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Particle Definition Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let particle = &scene["nodes"][0]["properties"]["particle"];
    assert_eq!(particle["source"], "particles/spark.json");
    assert_eq!(particle["seed"], 77);
    assert_eq!(particle["count"], 32);
    assert_eq!(particle["material"], "materials/spark.json");
    assert_eq!(particle["rate"], 16.0);
    assert_eq!(particle["speed_min"], 4.0);
    assert_eq!(particle["speed_max"], 12.0);
    assert_eq!(particle["spawn_width"], 192.0);
    assert_eq!(particle["spawn_height"], 48.0);
    assert_eq!(particle["size"], 7.0);
    assert_eq!(particle["lifetime"], 2.0);
    assert_eq!(particle["color"], "#808080");
    assert_eq!(particle["gravity_x"], 0.0);
    assert_eq!(particle["gravity_y"], 18.0);
    assert_eq!(particle["fade"], true);
    assert_eq!(particle["material_resource"], "resource-1-spark");
    assert_eq!(particle["render_resource"], "resource-2-spark");
    assert_eq!(scene["nodes"][0]["resource"], "resource-2-spark");
    assert_eq!(
        scene["resources"][1]["source"],
        "assets/scene-resources/scene/resource-2-spark.png"
    );

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let snapshot = document.snapshot_at_with_property_resolver(500, |_| None);
    assert_eq!(snapshot.layers.len(), 32);
    assert_eq!(snapshot.layers[0].kind, crate::core::SceneNodeKind::Image);
    assert_eq!(
        snapshot.layers[0].source.as_ref().map(|path| path.as_str()),
        Some("assets/scene-resources/scene/resource-2-spark.png")
    );

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"wallpaper-engine-particle-definition-lowering".to_owned())
    );
    assert!(
        report
            .converted_features
            .contains(&"scene-we-particle-material-runtime".to_owned())
    );
    assert!(
        report
            .full_scene
            .as_ref()
            .unwrap()
            .completed_boundaries
            .contains(&"scene-we-particle-material-runtime".to_owned())
    );
}

#[test]
fn lowers_wallpaper_engine_builtin_particle_bubble_texture_to_native_gtex() {
    let source = TestDir::new("we-scene-builtin-particle-texture-source");
    let output = TestDir::new("we-scene-builtin-particle-texture-output");
    output.remove();
    source.write_file(
        "scene.json",
        r##"{
              "objects": [
                {
                  "id": 78,
                  "type": "particle",
                  "particle": "particles/bubbles.json"
                }
              ]
            }"##,
    );
    source.write_file(
        "particles/bubbles.json",
        r##"{
              "maxcount": 8,
              "material": "materials/bubbles.json",
              "renderer": [{ "name": "sprite", "fadealpha": true }]
            }"##,
    );
    source.write_file(
        "materials/bubbles.json",
        r#"{ "passes": [{ "textures": ["particle/bubbles/bubble3"] }] }"#,
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Builtin Particle Texture Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let particle = &scene["nodes"][0]["properties"]["particle"];
    assert_eq!(particle["textures"][0], "particle/bubbles/bubble3");
    assert_eq!(particle["render_resource"], "resource-2-we-builtin-bubble3");
    assert_eq!(
        scene["nodes"][0]["resource"],
        "resource-2-we-builtin-bubble3"
    );
    assert_eq!(scene["resources"][1]["type"], "image");
    assert_eq!(scene["resources"][1]["role"], "we-builtin-particle-texture");
    assert_eq!(
        scene["resources"][1]["source"],
        "assets/scene-resources/scene/resource-2-we-builtin-bubble3.gtex"
    );
    assert!(
        output
            .path()
            .join("assets/scene-resources/scene/resource-2-we-builtin-bubble3.gtex")
            .is_file()
    );
    assert!(
        !scene["unsupported_features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| matches!(
                feature["feature"].as_str(),
                Some("missing-resource" | "we-particle-material-texture-runtime")
            ))
    );

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"wallpaper-engine-builtin-particle-texture".to_owned())
    );
    assert!(
        report
            .converted_features
            .contains(&"scene-we-particle-material-runtime".to_owned())
    );
    assert!(
        !report
            .unsupported_features
            .contains(&"missing-resource".to_owned())
    );
    assert!(
        !report
            .unsupported_features
            .contains(&"we-particle-material-texture-runtime".to_owned())
    );
}

#[test]
fn lowers_wallpaper_engine_builtin_particle_tex_path_before_file_decode() {
    let source = TestDir::new("we-scene-builtin-particle-tex-path-source");
    let output = TestDir::new("we-scene-builtin-particle-tex-path-output");
    output.remove();
    source.write_file(
        "scene.json",
        r##"{
              "objects": [
                {
                  "id": 79,
                  "type": "particle",
                  "particle": "particles/splash.json"
                }
              ]
            }"##,
    );
    source.write_file(
        "particles/splash.json",
        r##"{
              "maxcount": 8,
              "material": "materials/splash.json",
              "renderer": [{ "name": "sprite", "fadealpha": true }]
            }"##,
    );
    source.write_file(
        "materials/splash.json",
        r#"{ "passes": [{ "textures": ["materials/particle/water/splash_1.tex"] }] }"#,
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Builtin Particle Tex Path Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let particle = &scene["nodes"][0]["properties"]["particle"];
    assert_eq!(
        particle["textures"][0],
        "materials/particle/water/splash_1.tex"
    );
    assert_eq!(
        particle["render_resource"],
        "resource-2-we-builtin-splash-1"
    );
    assert_eq!(
        scene["nodes"][0]["resource"],
        "resource-2-we-builtin-splash-1"
    );
    assert_eq!(scene["resources"][1]["type"], "image");
    assert_eq!(
        scene["resources"][1]["source"],
        "assets/scene-resources/scene/resource-2-we-builtin-splash-1.gtex"
    );
    assert!(
        output
            .path()
            .join("assets/scene-resources/scene/resource-2-we-builtin-splash-1.gtex")
            .is_file()
    );
    assert!(
        !scene["unsupported_features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| matches!(
                feature["feature"].as_str(),
                Some("we-tex-decode" | "we-particle-material-texture-runtime")
            ))
    );
}

#[test]
fn converts_recordable_audio_response_to_native_scene_runtime() {
    let source = TestDir::new("we-scene-audio-response-source");
    let output = TestDir::new("we-scene-audio-response-output");
    output.remove();
    source.write_file(
        "scene.json",
        r##"{
              "objects": [
                {
                  "id": 7,
                  "type": "audio-response",
                  "color": "#44ccff",
                  "width": 320,
                  "height": 48,
                  "sound": "sounds/music.ogg"
                }
              ]
            }"##,
    );
    source.write_file("sounds/music.ogg", "not real ogg");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Scene Audio Response",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"native-audio-response-runtime".to_owned())
    );
    assert!(
        !report
            .unsupported_features
            .contains(&"audio-response-runtime".to_owned())
    );
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(scene["systems"]["audio_response"], "ready");
    let node = &scene["nodes"].as_array().unwrap()[0];
    assert_eq!(node["type"], "audio-response");
    assert_eq!(node["audio"].as_array().unwrap().len(), 1);
    let bindings = scene["property_bindings"].as_array().unwrap();
    assert!(bindings.iter().any(|binding| {
        binding["target_node"] == node["id"]
            && binding["property"] == "audio.bass"
            && binding["target"] == "width"
    }));
    assert!(
        scene["native_lowering"]["completed_boundaries"]
            .as_array()
            .unwrap()
            .contains(&json!("native-audio-response-visual-runtime"))
    );
    assert!(
        !scene["native_lowering"]["pending_boundaries"]
            .as_array()
            .unwrap()
            .contains(&json!("audio-response-runtime"))
    );
    assert!(
        scene["native_lowering"]["pending_boundaries"]
            .as_array()
            .unwrap()
            .contains(&json!("pipewire-audio-spectrum-input-source"))
    );
}

#[test]
fn converts_pure_scene_sound_object_to_audio_cue_node() {
    let source = TestDir::new("we-scene-audio-cue-node-source");
    let output = TestDir::new("we-scene-audio-cue-node-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                {
                  "id": 7,
                  "type": "sound",
                  "name": "Music Cue",
                  "sound": "sounds/music.ogg",
                  "playbackmode": "loop",
                  "startsilent": true
                }
              ]
            }"#,
    );
    source.write_file("sounds/music.ogg", "not real ogg");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Scene Audio Cue",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();

    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(scene["systems"]["audio_response"], "absent");
    let node = &scene["nodes"].as_array().unwrap()[0];
    assert_eq!(node["type"], "audio");
    assert_eq!(node["audio"][0]["resource"], "resource-1-music");
    assert_eq!(node["audio"][0]["playback_mode"], "loop");
    assert_eq!(node["audio"][0]["start_silent"], true);
    assert!(
        !scene["unsupported_features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| feature["feature"] == "audio-response")
    );
    assert!(
        !scene["native_lowering"]["pending_boundaries"]
            .as_array()
            .unwrap()
            .contains(&json!("audio-response-runtime"))
    );

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
    assert_eq!(snapshot.layers[0].kind, crate::core::SceneNodeKind::Audio);
    assert_eq!(snapshot.layers[0].audio.len(), 1);
}

#[test]
fn lowers_wallpaper_engine_audio_controller_scripts_to_native_conditions() {
    let source = TestDir::new("we-scene-audio-controller-source");
    let output = TestDir::new("we-scene-audio-controller-output");
    output.remove();
    source.write_file(
            "scene.json",
            r##"{
              "objects": [
                {
                  "id": 1,
                  "name": "Idle Video",
                  "solid": true,
                  "width": 100,
                  "height": 100,
                  "color": "#ffffff"
                },
                {
                  "id": 2,
                  "name": "Idle Controller",
                  "image": "models/util/fullscreenlayer.json",
                  "visible": {
                    "script": "export function update(value) { return value; }",
                    "scriptproperties": {
                      "targetLayerId": "Idle Video",
                      "defaultHideTarget": true,
                      "mouseInactiveSec": { "value": 1 },
                      "fadeInDuration": 0.25
                    },
                    "value": true
                  }
                },
                {
                  "id": 3,
                  "name": "voice.mp3",
                  "type": "sound",
                  "sound": "sounds/voice.mp3",
                  "startsilent": true
                },
                {
                  "id": 4,
                  "name": "Audio Follows Idle",
                  "visible": {
                    "script": "let t=thisScene.getLayer(scriptProperties.p2g5z?.trim()),e=thisScene.getLayer(scriptProperties.m8b4n?.trim());let i=t.visible&&t.alpha>0;i&&!q1w3e&&e.play(),!i&&q1w3e&&e.pause();",
                    "scriptproperties": {
                      "p2g5z": "Idle Video",
                      "m8b4n": "voice.mp3",
                      "x7s9k": { "user": "voice_enabled", "value": true }
                    },
                    "value": true
                  }
                },
                {
                  "id": 5,
                  "name": "a.mp3",
                  "type": "sound",
                  "sound": "sounds/a.mp3",
                  "playbackmode": "loop",
                  "startsilent": true
                },
                {
                  "id": 6,
                  "name": "b.mp3",
                  "type": "sound",
                  "sound": "sounds/b.mp3",
                  "playbackmode": "loop",
                  "startsilent": true
                },
                {
                  "id": 7,
                  "name": "Music Choice",
                  "visible": {
                    "script": "let songNames = [\"a.mp3\", \"b.mp3\"]; export function applyUserProperties(changedUserProperties) { if (changedUserProperties.music === undefined) return; playTargetMusic(); } function playTargetMusic(){ targetSong.play(); }",
                    "value": true
                  }
                }
              ]
            }"##,
        );
    source.write_file("sounds/voice.mp3", "not real mp3");
    source.write_file("sounds/a.mp3", "not real mp3");
    source.write_file("sounds/b.mp3", "not real mp3");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Scene Audio Controllers",
              "file": "scene.json",
              "properties": {
                "voice_enabled": { "type": "bool", "default": true },
                "music": { "type": "choice", "choices": ["0", "1", "2"], "default": "2" }
              }
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let voice = scene["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["name"] == "voice.mp3")
        .unwrap();
    let voice_conditions = voice["audio"][0]["active_conditions"].as_array().unwrap();
    assert!(voice_conditions.iter().any(|condition| {
        condition["property"]
            .as_str()
            .is_some_and(|property| property.starts_with("scene.controller."))
    }));
    assert!(voice_conditions.iter().any(|condition| {
        condition["property"] == "voice_enabled" && condition["equals"].is_null()
    }));
    let music_b = scene["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["name"] == "b.mp3")
        .unwrap();
    assert_eq!(
        music_b["audio"][0]["active_conditions"][0]["property"],
        "music"
    );
    assert_eq!(music_b["audio"][0]["active_conditions"][0]["equals"], 2.0);
    assert!(
        scene["native_lowering"]["completed_boundaries"]
            .as_array()
            .unwrap()
            .contains(&json!("scene-audio-controller-runtime"))
    );
    assert!(
        scene["native_lowering"]["completed_boundaries"]
            .as_array()
            .unwrap()
            .contains(&json!(
                "wallpaper-engine-detected-scenescript-native-lowering"
            ))
    );
    assert!(
        !scene["native_lowering"]["pending_boundaries"]
            .as_array()
            .unwrap()
            .contains(&json!("arbitrary-scenescript-runtime"))
    );

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        !report
            .unsupported_features
            .contains(&"scenescript".to_owned())
    );

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let inactive = document.snapshot_at_with_property_resolver(1_000, |property| match property {
        "voice_enabled" => Some(1.0),
        "music" => Some(1.0),
        _ if property.starts_with("scene.controller.") => Some(0.0),
        _ => None,
    });
    assert_eq!(
        inactive
            .layers
            .iter()
            .filter(|layer| layer.kind == crate::core::SceneNodeKind::Audio)
            .map(|layer| layer.audio.len())
            .sum::<usize>(),
        1
    );
    let active = document.snapshot_at_with_property_resolver(1_000, |property| match property {
        "voice_enabled" => Some(1.0),
        "music" => Some(2.0),
        _ if property.starts_with("scene.controller.") => Some(1.0),
        _ => None,
    });
    assert_eq!(
        active
            .layers
            .iter()
            .filter(|layer| layer.kind == crate::core::SceneNodeKind::Audio)
            .map(|layer| layer.audio.len())
            .sum::<usize>(),
        2
    );
}

#[test]
fn lowers_wallpaper_engine_builtin_util_models_without_missing_resource_noise() {
    let source = TestDir::new("we-scene-builtin-util-model-source");
    let output = TestDir::new("we-scene-builtin-util-model-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                {
                  "id": 9,
                  "name": "Click Controller",
                  "image": "models/util/composelayer.json",
                  "visible": {
                    "script": "export function update(value) { return value; }",
                    "value": true
                  },
                  "size": "512 512"
                },
                {
                  "id": 10,
                  "name": "Solid Layer",
                  "image": "models/util/solidlayer.json",
                  "size": "256 128"
                }
              ]
            }"#,
    );
    source.write_file("fonts/Inter.ttf", "not real font");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Builtin Util Model Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();

    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let node = &scene["nodes"][0];
    assert_eq!(node["type"], "script");
    assert_eq!(
        node["provenance"]["model"]["source"],
        "models/util/composelayer.json"
    );
    assert_eq!(node["provenance"]["model"]["builtin"], true);
    assert_eq!(node["provenance"]["model"]["utility"], "composelayer");
    let solid = &scene["nodes"][1];
    assert_eq!(solid["type"], "rectangle");
    assert_eq!(solid["color"], "#ffffff");
    assert_eq!(solid["width"], 256.0);
    assert_eq!(solid["height"], 128.0);
    assert_eq!(
        solid["provenance"]["model"]["source"],
        "models/util/solidlayer.json"
    );
    assert_eq!(solid["provenance"]["model"]["utility"], "solidlayer");
    assert_eq!(solid["provenance"]["model"]["solid_layer"], true);
    assert!(
        !scene["unsupported_features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| {
                matches!(
                    feature["feature"].as_str(),
                    Some("missing-resource" | "we-model-json")
                )
            })
    );
    assert!(
        !scene["unsupported_features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| feature["feature"] == "script")
    );

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"wallpaper-engine-util-model-lowering".to_owned())
    );
    assert!(
        !report
            .warnings
            .iter()
            .any(|warning| warning.contains("models/util/composelayer.json"))
    );
}

#[test]
fn maps_extensionless_wallpaper_engine_material_texture_paths_to_tex_assets() {
    assert_eq!(
        scene_material_texture_path("workshop/2790231929/WC test"),
        "materials/workshop/2790231929/WC test.tex"
    );
    assert_eq!(
        scene_material_texture_path("particle/bubbles/bubble3"),
        "particle/bubbles/bubble3"
    );
    assert_eq!(
        scene_material_texture_path("_rt_FullFrameBuffer"),
        "_rt_FullFrameBuffer"
    );
}

#[test]
fn converts_wallpaper_engine_scene_to_clean_gscene_provenance() {
    let source = TestDir::new("we-scene-provenance-source");
    let output = TestDir::new("we-scene-provenance-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "version": 42,
              "general": {
                "orthogonalprojection": { "width": 3840, "height": 2160 },
                "clearcolor": [0.1, 0.2, 0.3],
                "clearenabled": true,
                "hdr": true,
                "bloomstrength": 1.5,
                "cameraparallaxamount": 0.25
              },
              "camera": {
                "center": [0, 0, 0],
                "eye": { "x": 0, "y": 0, "z": 1 }
              },
              "objects": [
                {
                  "id": 7,
                  "parent": 3,
                  "dependencies": [1, 2],
                  "name": "Hero",
                  "image": "models/hero.json",
                  "origin": [10, 20, 0],
                  "angles": [0, 0, 1.5707963267948966],
                  "scale": [2, 3, 1],
                  "pivot": [0, 0, 0],
                  "alignment": "left",
                  "size": [100, 50, 0],
                  "effects": [
                    {
                      "file": "effects/glow.json",
                      "id": 9,
                      "name": "Glow",
                      "visible": true,
                      "passes": [
                        {
                          "id": 1,
                          "textures": [null, "mask"],
                          "combos": { "MODE": 2 },
                          "constantshadervalues": { "g_Time": 1.0 },
                          "usertextures": ["custom"]
                        }
                      ]
                    }
                  ],
                  "sound": ["sounds/theme.ogg"],
                  "playbackmode": "loop",
                  "volume": 0.75,
                  "startsilent": false,
                  "particle": "particles/spark.json"
                }
              ]
            }"#,
    );
    source.write_file(
        "models/hero.json",
        r#"{ "material": "hero", "solidlayer": false, "passthrough": false }"#,
    );
    source.write_file(
        "materials/hero.json",
        r#"{ "passes": [{ "textures": ["hero_albedo"] }] }"#,
    );
    source.write_file("materials/hero_albedo.tex", "not real tex");
    source.write_file("effects/glow.json", r#"{ "passes": [] }"#);
    source.write_file("sounds/theme.ogg", "not real ogg");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Scene Provenance",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert!(scene.get("wallpaper_engine").is_none());
    assert_eq!(scene["source"]["format"], "wallpaper-engine-scene");
    assert_eq!(scene["import"]["source_format"], "wallpaper-engine-scene");
    assert_eq!(scene["import"]["source_version"], 42);
    assert_eq!(scene["import"]["object_count"], 1);
    assert_eq!(scene["import"]["feature_counts"]["model"], 1);
    assert_eq!(scene["import"]["feature_counts"]["audio"], 1);
    assert_eq!(scene["import"]["feature_counts"]["particle"], 1);
    assert_eq!(scene["import"]["feature_counts"]["effect"], 1);
    assert_eq!(scene["size"]["width"], 3840);
    assert_eq!(scene["size"]["height"], 2160);
    assert_eq!(scene["render"]["clear_color"], "#1a334d");
    assert_eq!(scene["render"]["hdr"], true);
    assert_eq!(scene["render"]["bloom"]["strength"], 1.5);
    assert_eq!(scene["render"]["parallax"]["amount"], 0.25);
    assert_eq!(scene["camera"]["eye"]["z"], 1.0);

    let node = &scene["nodes"][0];
    assert_eq!(node["type"], "image");
    assert!(node.get("resource").is_none());
    assert_eq!(node["width"], 100.0);
    assert_eq!(node["height"], 50.0);
    assert_eq!(node["transform"]["x"], 10.0);
    assert_eq!(node["transform"]["y"], 20.0);
    assert_eq!(node["transform"]["scale_x"], 2.0);
    assert_eq!(node["transform"]["scale_y"], 3.0);
    assert_eq!(node["transform"]["rotation_deg"], 90.0);
    assert_eq!(node["transform"]["anchor_x"], 0.0);
    assert_eq!(node["transform"]["anchor_y"], 0.5);
    assert_eq!(
        node["provenance"]["source_format"],
        "wallpaper-engine-scene"
    );
    assert_eq!(node["provenance"]["source_id"], "7");
    assert_eq!(node["provenance"]["parent_id"], "3");
    assert_eq!(node["provenance"]["dependencies"][0], "1");
    assert_eq!(node["provenance"]["original_path"], "models/hero.json");
    assert_eq!(node["provenance"]["model"]["source"], "models/hero.json");
    assert_eq!(
        node["provenance"]["model"]["material"],
        "materials/hero.json"
    );
    assert_eq!(
        node["provenance"]["model"]["textures"][0],
        "materials/hero_albedo.tex"
    );
    assert_eq!(node["effects"][0]["file"], "effects/glow.json");
    assert_eq!(node["effects"][0]["resource"], "resource-3-glow");
    assert_eq!(node["effects"][0]["runtime"], "wallpaper-engine-effect");
    assert_eq!(node["effects"][0]["passes"][0]["combos"]["MODE"], 2);
    assert_eq!(node["audio"][0]["source"], "sounds/theme.ogg");
    assert_eq!(node["audio"][0]["resource"], "resource-4-theme");
    assert_eq!(node["audio"][0]["playback_mode"], "loop");
    assert_eq!(node["audio"][0]["volume"], 0.75);
    assert_eq!(node["audio"][0]["start_silent"], false);
    assert_eq!(node["provenance"]["particle"], "particles/spark.json");
    assert_eq!(scene["resources"][0]["type"], "model");
    assert_eq!(scene["resources"][1]["type"], "material");
    assert_eq!(scene["resources"][2]["type"], "effect");
    assert_eq!(scene["resources"][3]["type"], "audio");
    assert!(
        scene["resources"]
            .as_array()
            .unwrap()
            .iter()
            .all(|resource| !resource["source"].as_str().unwrap_or("").ends_with(".tex"))
    );
    assert!(
        scene["unsupported_features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| feature["feature"] == "we-model-material-texture-runtime")
    );
}

#[test]
fn resolves_scene_model_material_image_texture_to_renderable_resource() {
    let source = TestDir::new("we-scene-renderable-model-source");
    let output = TestDir::new("we-scene-renderable-model-output");
    output.remove();
    source.write_file(
        "scene.json",
        r##"{
              "objects": [
                {
                  "id": 1,
                  "name": "Renderable",
                  "image": "models/renderable.json"
                }
              ]
            }"##,
    );
    source.write_file(
        "models/renderable.json",
        r#"{ "material": "materials/renderable.json" }"#,
    );
    source.write_file(
        "materials/renderable.json",
        r#"{ "passes": [{
              "shader": "genericimage2",
              "blending": "translucent",
              "cullmode": "nocull",
              "depthtest": "disabled",
              "depthwrite": "disabled",
              "combos": { "LIGHTING": 0, "REFLECTION": 1 },
              "textures": ["textures/albedo.png"]
            }] }"#,
    );
    source.write_file("textures/albedo.png", "not real png");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Renderable Scene Model",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(scene["nodes"][0]["type"], "image");
    assert_eq!(scene["nodes"][0]["resource"], "resource-3-albedo");
    assert_eq!(scene["resources"][2]["type"], "image");
    assert_eq!(
        scene["resources"][2]["source"],
        "assets/scene-resources/scene/resource-3-albedo.png"
    );
    assert_eq!(
        scene["nodes"][0]["provenance"]["model"]["texture_resources"][0],
        "resource-3-albedo"
    );
    assert_eq!(
        scene["nodes"][0]["properties"]["material"]["runtime"],
        "wallpaper-engine-material"
    );
    assert_eq!(
        scene["nodes"][0]["properties"]["material"]["passes"][0]["shader"],
        "genericimage2"
    );
    assert_eq!(
        scene["nodes"][0]["properties"]["material"]["passes"][0]["blending"],
        "translucent"
    );
    assert_eq!(
        scene["nodes"][0]["properties"]["material"]["passes"][0]["depthtest"],
        "disabled"
    );
    assert_eq!(
        scene["nodes"][0]["properties"]["material"]["passes"][0]["combos"]["REFLECTION"],
        1
    );
    assert_eq!(scene["systems"]["shader_material_graph"], "ready");
    assert!(
        scene["native_lowering"]["completed_boundaries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|boundary| boundary == "shader-material-graph")
    );
    assert!(
        !scene["native_lowering"]["pending_boundaries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|boundary| boundary == "shader-material-graph")
    );
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"scene-we-material-graph-runtime".to_owned())
    );
}

#[test]
fn lowers_wallpaper_engine_water_effects_to_native_scene_runtime() {
    let source = TestDir::new("we-scene-native-water-effect-source");
    let output = TestDir::new("we-scene-native-water-effect-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                {
                  "id": 1,
                  "name": "Native Water Effect",
                  "image": "models/renderable.json",
                  "effects": [
                    {
                      "file": "effects/watercaustics/effect.json",
                      "visible": true,
                      "passes": [
                        {
                          "constantshadervalues": {
                            "ui_editor_properties_brightness": 2.5,
                            "ui_editor_properties_speed": 0.3
                          }
                        }
                      ]
                    }
                  ]
                }
              ]
            }"#,
    );
    source.write_file(
        "models/renderable.json",
        r#"{ "material": "materials/renderable.json" }"#,
    );
    source.write_file(
        "materials/renderable.json",
        r#"{ "passes": [{ "textures": ["textures/albedo.png"] }] }"#,
    );
    source.write_file("textures/albedo.png", "not real png");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Native Water Effect Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        scene["nodes"][0]["effects"][0]["runtime"],
        "native-water-caustics"
    );
    assert!(
        !scene["unsupported_features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| feature["feature"] == "we-effect-runtime")
    );
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"native-water-caustics-effect-runtime".to_owned())
    );
}

#[test]
fn preserves_noop_scene_effect_without_blocking_material_graph_runtime() {
    let source = TestDir::new("we-scene-noop-effect-source");
    let output = TestDir::new("we-scene-noop-effect-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                {
                  "id": 1,
                  "name": "Renderable With Noop Effect",
                  "image": "models/renderable.json",
                  "effects": [
                    { "file": "effects/noop.json", "visible": true, "passes": [] }
                  ]
                }
              ]
            }"#,
    );
    source.write_file(
        "models/renderable.json",
        r#"{ "material": "materials/renderable.json" }"#,
    );
    source.write_file(
        "materials/renderable.json",
        r#"{ "passes": [{ "textures": ["textures/albedo.png"] }] }"#,
    );
    source.write_file("textures/albedo.png", "not real png");
    source.write_file("effects/noop.json", r#"{ "passes": [] }"#);
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Noop Effect Scene Model",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(scene["nodes"][0]["type"], "image");
    assert_eq!(scene["nodes"][0]["effects"][0]["file"], "effects/noop.json");
    assert_eq!(scene["nodes"][0]["effects"][0]["runtime"], "metadata-only");
    assert!(
        !scene["resources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|resource| resource["role"] == "we-effect")
    );
    assert!(
        !scene["unsupported_features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| feature["feature"] == "we-effect-runtime")
    );
    assert!(
        scene["native_lowering"]["completed_boundaries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|boundary| boundary == "shader-material-graph")
    );
    assert!(
        !scene["native_lowering"]["pending_boundaries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|boundary| boundary == "shader-material-graph")
    );
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"scene-we-noop-effect-preserved".to_owned())
    );
}

#[test]
fn lowers_wallpaper_engine_blurprecise_text_effect_to_native_glow() {
    let source = TestDir::new("we-scene-blurprecise-text-effect-source");
    let output = TestDir::new("we-scene-blurprecise-text-effect-output");
    output.remove();
    source.write_file(
        "scene.json",
        r##"{
              "objects": [
                {
                  "id": 1,
                  "name": "Clock Glow",
                  "text": "12:34",
                  "fontSize": 32,
                  "color": "#ffffff",
                  "effects": [
                    {
                      "file": "effects/workshop/3184554659/blurprecise/effect.json",
                      "visible": true,
                      "passes": [
                        { "id": 1, "constantshadervalues": { "scale": "1.25 1.25" } },
                        { "id": 2, "combos": { "ENABLEMASK": 1, "VERTICAL": 1 } }
                      ]
                    }
                  ]
                }
              ]
            }"##,
    );
    source.write_file(
        "effects/workshop/3184554659/blurprecise/effect.json",
        r#"{ "passes": [{ "constantshadervalues": { "scale": "1.25 1.25" } }] }"#,
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Blurprecise Text Effect Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(scene["systems"]["shader_material_graph"], "ready");
    assert_eq!(
        scene["nodes"][0]["effects"][0]["runtime"],
        "native-text-glow"
    );
    assert_eq!(
        scene["nodes"][0]["effects"][0]["properties"]["kind"],
        "blurprecise"
    );
    assert!(
        !scene["unsupported_features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| feature["feature"] == "we-effect-runtime")
    );
    assert!(
        scene["native_lowering"]["completed_boundaries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|boundary| boundary == "shader-material-graph")
    );
    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
    assert_eq!(snapshot.layers.len(), 9);
    assert!(
        snapshot.layers[..8]
            .iter()
            .all(|layer| layer.id.contains("native-text-glow"))
    );
    assert_eq!(snapshot.layers[8].text.as_deref(), Some("12:34"));
}

#[test]
fn keeps_runtime_scene_effect_as_material_graph_boundary() {
    let source = TestDir::new("we-scene-runtime-effect-source");
    let output = TestDir::new("we-scene-runtime-effect-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                {
                  "id": 1,
                  "name": "Renderable With Runtime Effect",
                  "image": "models/renderable.json",
                  "effects": [
                    { "file": "effects/glow.json", "visible": true }
                  ]
                }
              ]
            }"#,
    );
    source.write_file(
        "models/renderable.json",
        r#"{ "material": "materials/renderable.json" }"#,
    );
    source.write_file(
        "materials/renderable.json",
        r#"{ "passes": [{ "textures": ["textures/albedo.png"] }] }"#,
    );
    source.write_file("textures/albedo.png", "not real png");
    source.write_file(
        "effects/glow.json",
        r#"{ "passes": [{ "textures": ["_rt_FullFrameBuffer"], "combos": { "MODE": 1 } }] }"#,
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Runtime Effect Scene Model",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert!(
        scene["unsupported_features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| feature["feature"] == "we-effect-runtime")
    );
    assert_eq!(scene["systems"]["shader_material_graph"], "detected");
    assert_eq!(
        scene["nodes"][0]["effects"][0]["runtime"],
        "wallpaper-engine-effect"
    );
    assert!(
        scene["native_lowering"]["pending_boundaries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|boundary| boundary == "shader-material-graph")
    );
    assert!(
        !scene["native_lowering"]["completed_boundaries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|boundary| boundary == "shader-material-graph")
    );
}

#[test]
fn lowers_wallpaper_engine_opacity_effect_script_to_native_timeline() {
    let source = TestDir::new("we-scene-opacity-effect-source");
    let output = TestDir::new("we-scene-opacity-effect-output");
    output.remove();
    source.write_file(
            "scene.json",
            r##"{
              "objects": [
                {
                  "id": 1,
                  "type": "rectangle",
                  "name": "Fading Layer",
                  "width": 100,
                  "height": 100,
                  "color": "#ffffff",
                  "effects": [
                    {
                      "file": "effects/opacity/effect.json",
                      "passes": [
                        {
                          "constantshadervalues": {
                            "alpha": {
                              "script": "'use strict'; const delayTime = 3; const fadeTime = 2; export function update(value) { return value; }",
                              "value": 1
                            }
                          }
                        }
                      ]
                    }
                  ]
                }
              ]
            }"##,
        );
    source.write_file("effects/opacity/effect.json", r#"{ "passes": [] }"#);
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Opacity Effect Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let node_id = scene["nodes"][0]["id"].as_str().unwrap();
    assert_eq!(scene["nodes"][0]["type"], "rectangle");
    assert_eq!(
        scene["nodes"][0]["effects"][0]["runtime"],
        "native-opacity-timeline"
    );
    assert_eq!(scene["timelines"][0]["target_node"], node_id);
    assert_eq!(scene["timelines"][0]["channels"][0]["property"], "opacity");
    assert_eq!(
        scene["timelines"][0]["channels"][0]["keyframes"][0]["time_ms"],
        0
    );
    assert_eq!(
        scene["timelines"][0]["channels"][0]["keyframes"][1]["time_ms"],
        3000
    );
    assert_eq!(
        scene["timelines"][0]["channels"][0]["keyframes"][2]["time_ms"],
        5000
    );
    assert_eq!(
        scene["timelines"][0]["channels"][0]["keyframes"][2]["value"],
        0.0
    );
    assert!(
        !scene["unsupported_features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| feature["feature"] == "we-effect-runtime")
    );
    assert!(
        !scene["resources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|resource| resource["role"] == "we-effect")
    );
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"scene-we-opacity-effect-timeline".to_owned())
    );
    assert!(
        report
            .converted_features
            .contains(&"scene-keyframe-timeline".to_owned())
    );
}

#[test]
fn lowers_wallpaper_engine_opacity_effect_alias_script_to_native_timeline() {
    let source = TestDir::new("we-scene-opacity-effect-alias-source");
    let output = TestDir::new("we-scene-opacity-effect-alias-output");
    output.remove();
    source.write_file(
            "scene.json",
            r##"{
              "objects": [
                {
                  "id": 1,
                  "type": "rectangle",
                  "name": "Alpha Range Layer",
                  "width": 100,
                  "height": 100,
                  "color": "#ffffff",
                  "effects": [
                    {
                      "file": "effects/opacity/effect.json",
                      "passes": [
                        {
                          "constant_shader_values": {
                            "alpha": {
                              "script": "let startDelay = 1; let fadeDuration = 2; let fromAlpha = 0.25; let targetAlpha = 0.85;",
                              "value": 0
                            }
                          }
                        }
                      ]
                    }
                  ]
                }
              ]
            }"##,
        );
    source.write_file("effects/opacity/effect.json", r#"{ "passes": [] }"#);
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Opacity Effect Alias Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let node_id = scene["nodes"][0]["id"].as_str().unwrap();
    assert_eq!(
        scene["nodes"][0]["effects"][0]["runtime"],
        "native-opacity-timeline"
    );
    assert_eq!(scene["timelines"][0]["target_node"], node_id);
    assert_eq!(
        scene["timelines"][0]["channels"][0]["keyframes"],
        json!([
            { "time_ms": 0, "value": 0.25, "curve": "linear" },
            { "time_ms": 1000, "value": 0.25, "curve": "linear" },
            { "time_ms": 3000, "value": 0.85, "curve": "linear" }
        ])
    );
    assert!(
        !scene["unsupported_features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| feature["feature"] == "we-effect-runtime")
    );
}

#[test]
fn lowers_wallpaper_engine_constant_opacity_effect_to_native_timeline() {
    let source = TestDir::new("we-scene-constant-opacity-effect-source");
    let output = TestDir::new("we-scene-constant-opacity-effect-output");
    output.remove();
    source.write_file(
        "scene.json",
        r##"{
              "objects": [
                {
                  "id": 1,
                  "type": "rectangle",
                  "name": "Tinted Layer",
                  "width": 100,
                  "height": 100,
                  "color": "#ffffff",
                  "effects": [
                    {
                      "file": "effects/opacity/effect.json",
                      "visible": true,
                      "passes": [
                        {
                          "constant_shader_values": {
                            "alpha": 0.35
                          }
                        }
                      ]
                    }
                  ]
                }
              ]
            }"##,
    );
    source.write_file(
        "effects/opacity/effect.json",
        r#"{ "passes": [{ "material": "materials/effects/opacity.json" }] }"#,
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Constant Opacity Effect Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let node_id = scene["nodes"][0]["id"].as_str().unwrap();
    assert_eq!(
        scene["nodes"][0]["effects"][0]["runtime"],
        "native-opacity-timeline"
    );
    assert_eq!(scene["timelines"][0]["target_node"], node_id);
    assert_eq!(
        scene["timelines"][0]["channels"][0]["keyframes"][0]["value"],
        0.35
    );
    assert!(
        !scene["unsupported_features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| feature["feature"] == "we-effect-runtime")
    );
    assert!(
        !scene["resources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|resource| resource["role"] == "we-effect")
    );

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
    assert_eq!(snapshot.layers[0].opacity, 0.35);
}

#[test]
fn lowers_opacity_effect_texture_resources_into_scene_texture_slots() {
    let source = TestDir::new("we-scene-opacity-effect-texture-source");
    let output = TestDir::new("we-scene-opacity-effect-texture-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                {
                  "id": 1,
                  "name": "Masked Eye",
                  "image": "models/eye.json",
                  "effects": [
                    {
                      "file": "effects/opacity/effect.json",
                      "passes": [
                        {
                          "textures": [null, null, null, "masks/opacity_mask"],
                          "constantshadervalues": { "alpha": 1.0 }
                        }
                      ]
                    }
                  ]
                }
              ]
            }"#,
    );
    source.write_file("models/eye.json", r#"{ "material": "materials/eye.json" }"#);
    source.write_file(
        "materials/eye.json",
        r#"{ "passes": [{ "textures": ["textures/eye.png"] }] }"#,
    );
    source.write_file("textures/eye.png", "not real png");
    let mask = test_we_tex_image_payload(2, 2, 9, &[255, 0, 128, 64], 1);
    source.write_bytes("materials/masks/opacity_mask.tex", &mask);
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Opacity Effect Texture Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let pass = &scene["nodes"][0]["effects"][0]["passes"][0];
    assert_eq!(
        pass["textures"],
        json!([null, null, null, "masks/opacity_mask"])
    );
    let texture_resources = pass["texture_resources"].as_array().unwrap();
    assert_eq!(texture_resources.len(), 4);
    assert!(texture_resources[0].is_null());
    assert!(texture_resources[1].is_null());
    assert!(texture_resources[2].is_null());
    let mask_resource_id = texture_resources[3].as_str().unwrap();
    let mask_resource = scene["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|resource| resource["id"] == mask_resource_id)
        .expect("opacity mask resource");
    assert_eq!(mask_resource["type"], "image");
    assert_eq!(
        mask_resource["original_source"],
        "materials/masks/opacity_mask.tex"
    );
    assert_eq!(mask_resource["role"], "we-material-texture-decoded-frame");
    let mask_source = mask_resource["source"].as_str().unwrap().to_owned();
    assert!(mask_source.ends_with(".gtex"));
    assert!(output.path().join(&mask_source).exists());

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
    assert_eq!(snapshot.layers.len(), 1);
    assert_eq!(snapshot.layers[0].alpha_texture_slot, Some(3));
    assert_eq!(snapshot.layers[0].texture_slots.len(), 2);
    assert_eq!(snapshot.layers[0].texture_slots[0].slot, 0);
    assert_eq!(snapshot.layers[0].texture_slots[1].slot, 3);
    assert_eq!(
        snapshot.layers[0].texture_slots[1].source.as_str(),
        mask_source.as_str()
    );

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"scene-we-effect-texture-resource".to_owned())
    );
}

#[test]
fn preserves_locked_opacity_mask_duplicate_as_independent_attachment_layer() {
    let source = TestDir::new("we-scene-locked-opacity-mask-composite-source");
    let output = TestDir::new("we-scene-locked-opacity-mask-composite-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                {
                  "id": 1,
                  "parent": 100,
                  "name": "Eye Base",
                  "image": "models/eye.json",
                  "attachment": "eye"
                },
                {
                  "id": 2,
                  "parent": 100,
                  "name": "Eye Opacity Mask",
                  "image": "models/eye.json",
                  "attachment": "eye",
                  "locktransforms": true,
                  "effects": [
                    {
                      "file": "effects/opacity/effect.json",
                      "passes": [
                        {
                          "textures": [null, "masks/opacity_mask"],
                          "constantshadervalues": { "alpha": 1.0 }
                        }
                      ]
                    }
                  ]
                }
              ]
            }"#,
    );
    source.write_file("models/eye.json", r#"{ "material": "materials/eye.json" }"#);
    source.write_file(
        "materials/eye.json",
        r#"{ "passes": [{ "textures": ["eye"] }] }"#,
    );
    source.write_bytes(
        "materials/eye.tex",
        &test_we_tex_rgba(
            2,
            2,
            &[
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
        ),
    );
    let mask = test_we_tex_image_payload(2, 2, 9, &[255, 0, 128, 64], 1);
    source.write_bytes("materials/masks/opacity_mask.tex", &mask);
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Locked Opacity Mask Composite Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        scene["nodes"][1]["provenance"]["lock_transforms"],
        Value::Bool(true)
    );
    let mask_source = scene["nodes"][1]["effects"][0]["passes"][0]["texture_resources"][1]
        .as_str()
        .and_then(|resource_id| {
            scene["resources"]
                .as_array()
                .unwrap()
                .iter()
                .find(|resource| resource["id"] == resource_id)
        })
        .and_then(|resource| resource["source"].as_str())
        .unwrap()
        .to_owned();

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
    assert_eq!(snapshot.layers.len(), 2);
    assert_eq!(snapshot.layers[0].id, "node-1-models-eye-json");
    assert_eq!(snapshot.layers[0].alpha_texture_slot, None);
    assert_eq!(snapshot.layers[0].texture_slots.len(), 1);
    assert_eq!(snapshot.layers[1].id, "node-2-models-eye-json");
    assert_eq!(snapshot.layers[1].alpha_texture_slot, Some(1));
    assert_eq!(snapshot.layers[1].texture_slots.len(), 2);
    assert_eq!(snapshot.layers[1].texture_slots[1].slot, 1);
    assert_eq!(
        snapshot.layers[1].texture_slots[1].source.as_str(),
        mask_source.as_str()
    );

    let mut sampled = Vec::new();
    document.snapshot_sampled_image_layers_at_with_resolvers(0, |_| None, |_| None, &mut sampled);
    assert_eq!(sampled.len(), 2);
    assert_eq!(sampled[0].alpha_texture_slot, None);
    assert_eq!(sampled[0].texture_slots.len(), 1);
    assert_eq!(sampled[1].alpha_texture_slot, Some(1));
    assert_eq!(sampled[1].texture_slots.len(), 2);
    assert_eq!(sampled[1].texture_slots[1].source.as_str(), mask_source);
}

#[test]
fn decodes_wallpaper_engine_scene_tex_material_to_renderable_frame_resource() {
    let rgba = vec![
        255, 0, 0, 255, 0, 255, 0, 255, 1, 1, 1, 255, 2, 2, 2, 255, 0, 0, 255, 255, 255, 255, 0,
        255, 3, 3, 3, 255, 4, 4, 4, 255,
    ];
    let tex = test_we_tex_rgba(4, 2, &rgba);
    let decoded = tex::decode_we_tex_image(&tex).unwrap();
    assert_eq!(decoded.width, 4);
    assert_eq!(decoded.height, 2);
    assert_eq!(decoded.rgba, rgba);
    let (frame, frame_count) = scene_we_tex_first_frame(
        decoded,
        Some(SceneWeModelFrameSize {
            width: 2,
            height: 2,
        }),
    )
    .unwrap();
    assert_eq!(frame_count, 2);
    assert_eq!(frame.width, 2);
    assert_eq!(frame.height, 2);
    assert_eq!(
        frame.rgba,
        vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
        ]
    );

    let source = TestDir::new("we-scene-tex-renderable-model-source");
    let output = TestDir::new("we-scene-tex-renderable-model-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                {
                  "id": 1,
                  "name": "Renderable Tex",
                  "image": "models/renderable.json"
                }
              ]
            }"#,
    );
    source.write_file(
        "models/renderable.json",
        r#"{ "material": "materials/renderable.json", "width": 2, "height": 2 }"#,
    );
    source.write_file(
        "materials/renderable.json",
        r#"{ "passes": [{ "textures": ["atlas"], "combos": { "SPRITESHEET": 1 } }] }"#,
    );
    source.write_bytes("materials/atlas.tex", &tex);
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Renderable Tex Scene Model",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(scene["nodes"][0]["type"], "image");
    assert_eq!(scene["nodes"][0]["resource"], "resource-3-atlas-atlas");
    assert_eq!(scene["nodes"][0]["mesh"]["vertices"][0]["v"], 0.0);
    assert_eq!(scene["nodes"][0]["mesh"]["vertices"][1]["v"], 0.0);
    assert_eq!(scene["nodes"][0]["mesh"]["vertices"][2]["v"], 1.0);
    assert_eq!(scene["nodes"][0]["mesh"]["vertices"][3]["v"], 1.0);
    assert_eq!(scene["nodes"][0]["mesh"]["vertices"][0]["x"], -1.0);
    assert_eq!(scene["nodes"][0]["mesh"]["vertices"][2]["y"], 1.0);
    assert_eq!(
        scene["nodes"][0]["mesh"]["indices"],
        json!([0, 1, 2, 2, 1, 3])
    );
    assert_eq!(
        scene["nodes"][0]["properties"]["spritesheet"]["type"],
        "atlas-grid"
    );
    assert_eq!(
        scene["nodes"][0]["properties"]["spritesheet"]["atlas_width"],
        4
    );
    assert_eq!(
        scene["nodes"][0]["properties"]["spritesheet"]["frame_width"],
        2
    );
    assert_eq!(
        scene["nodes"][0]["properties"]["spritesheet"]["frame_count"],
        2
    );
    assert_eq!(scene["resources"][2]["type"], "image");
    assert_eq!(
        scene["resources"][2]["source"],
        "assets/scene-resources/scene/resource-3-atlas-atlas.gtex"
    );
    assert_eq!(
        scene["resources"][2]["role"],
        "we-material-texture-decoded-atlas"
    );
    assert_eq!(
        scene["nodes"][0]["provenance"]["model"]["texture_resources"][0],
        "resource-3-atlas-atlas"
    );
    assert!(
        output
            .path()
            .join("assets/scene-resources/scene/resource-3-atlas-atlas.gtex")
            .exists()
    );
    assert!(scene["unsupported_features"].as_array().unwrap().is_empty());
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"scene-we-tex-bc7-gpu-texture".to_owned())
    );
    assert!(
        report
            .converted_features
            .contains(&"scene-we-spritesheet-atlas-runtime".to_owned())
    );
    assert!(
        report
            .converted_features
            .contains(&"wallpaper-engine-model-image-uv-y-flip-lowering".to_owned())
    );
}

#[test]
fn passes_wallpaper_engine_dxt_textures_to_native_bc_gtex() {
    assert_we_block_compressed_tex_conversion(
        7,
        gtex::GILDER_SCENE_TEXTURE_FORMAT_BC1_RGBA_UNORM_BLOCK,
        tex::SceneWeTexBlockCompressedFormat::Bc1RgbaUnormBlock,
        &[1; 8],
        "bc1",
        "we-material-texture-bc1-passthrough",
        "scene-we-tex-bc1-passthrough",
    );
    assert_we_block_compressed_tex_conversion(
        4,
        gtex::GILDER_SCENE_TEXTURE_FORMAT_BC3_UNORM_BLOCK,
        tex::SceneWeTexBlockCompressedFormat::Bc3UnormBlock,
        &[3; 16],
        "bc3",
        "we-material-texture-bc3-passthrough",
        "scene-we-tex-bc3-passthrough",
    );
}

#[test]
fn decodes_wallpaper_engine_r8_and_rg88_scene_tex_to_rgba() {
    let r8 = test_we_tex_image_payload(2, 2, 9, &[255, 128, 64, 0], 1);
    let decoded = tex::decode_we_tex_image(&r8).unwrap();
    assert_eq!(decoded.width, 2);
    assert_eq!(decoded.height, 2);
    assert_eq!(decoded.r8, Some(vec![64, 0, 255, 128]));
    assert_eq!(
        decoded.rgba,
        vec![
            64, 64, 64, 255, 0, 0, 0, 255, 255, 255, 255, 255, 128, 128, 128, 255
        ]
    );

    let rg88 = test_we_tex_image_payload(2, 2, 8, &[255, 0, 128, 64, 32, 16, 8, 4], 1);
    let decoded = tex::decode_we_tex_image(&rg88).unwrap();
    assert_eq!(decoded.width, 2);
    assert_eq!(decoded.height, 2);
    assert_eq!(decoded.r8, None);
    assert_eq!(
        decoded.rgba,
        vec![
            32, 16, 0, 255, 8, 4, 0, 255, 255, 0, 0, 255, 128, 64, 0, 255
        ]
    );
}

#[test]
fn decodes_wallpaper_engine_scene_tex_embedded_png_to_native_gtex() {
    let rgba = [
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
    ];
    let png = test_png_rgba(2, 2, &rgba);
    let tex = test_we_tex_embedded_png(2, 2, &png);
    let decoded = tex::decode_we_tex_image(&tex).unwrap();
    assert_eq!(decoded.width, 2);
    assert_eq!(decoded.height, 2);
    assert_eq!(&decoded.rgba[0..4], &[0, 0, 255, 255]);
    assert_eq!(&decoded.rgba[8..12], &[255, 0, 0, 255]);
    let texb0003 = test_we_texb0003_embedded_png(2, 2, &png);
    let decoded = tex::decode_we_tex_image(&texb0003).unwrap();
    assert_eq!(decoded.width, 2);
    assert_eq!(decoded.height, 2);
    assert_eq!(&decoded.rgba[0..4], &[0, 0, 255, 255]);

    let source = TestDir::new("we-scene-embedded-png-tex-source");
    let output = TestDir::new("we-scene-embedded-png-tex-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                {
                  "id": 1,
                  "name": "Embedded PNG Tex",
                  "image": "models/renderable.json"
                }
              ]
            }"#,
    );
    source.write_file(
        "models/renderable.json",
        r#"{ "material": "materials/renderable.json", "width": 2, "height": 2 }"#,
    );
    source.write_file(
        "materials/renderable.json",
        r#"{ "passes": [{ "textures": ["albedo"] }] }"#,
    );
    source.write_bytes("materials/albedo.tex", &tex);
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Embedded PNG Tex Scene Model",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(scene["nodes"][0]["type"], "image");
    assert_eq!(
        scene["resources"][2]["role"],
        "we-material-texture-decoded-frame"
    );
    let gtex_path = output
        .path()
        .join("assets/scene-resources/scene/resource-3-albedo-frame-0.gtex");
    assert!(gtex_path.exists());
    let bytes = fs::read(&gtex_path).unwrap();
    assert_eq!(
        u32::from_le_bytes(bytes[16..20].try_into().unwrap()),
        gtex::GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK
    );

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"scene-we-tex-bc7-gpu-texture".to_owned())
    );
}

#[test]
fn deduplicates_repeated_wallpaper_engine_scene_tex_resources() {
    let rgba = vec![
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
    ];
    let tex = test_we_tex_rgba(2, 2, &rgba);
    let source = TestDir::new("we-scene-tex-resource-dedup-source");
    let output = TestDir::new("we-scene-tex-resource-dedup-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                { "id": 1, "name": "Shared Tex A", "image": "models/shared.json" },
                { "id": 2, "name": "Shared Tex B", "image": "models/shared.json" }
              ]
            }"#,
    );
    source.write_file(
        "models/shared.json",
        r#"{ "material": "materials/shared.json", "width": 2, "height": 2 }"#,
    );
    source.write_file(
        "materials/shared.json",
        r#"{ "passes": [{ "textures": ["albedo"] }] }"#,
    );
    source.write_bytes("materials/albedo.tex", &tex);
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Deduplicated Tex Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let nodes = scene["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0]["type"], "image");
    assert_eq!(nodes[1]["type"], "image");
    assert_eq!(nodes[0]["resource"], nodes[1]["resource"]);

    let albedo_resources = scene["resources"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|resource| {
            resource["type"] == "image" && resource["original_source"] == "materials/albedo.tex"
        })
        .collect::<Vec<_>>();
    assert_eq!(albedo_resources.len(), 1);

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"scene-we-tex-resource-dedup".to_owned())
    );
}

#[test]
fn keeps_distinct_wallpaper_engine_scene_tex_frame_outputs() {
    let rgba = vec![
        255, 0, 0, 255, 0, 255, 0, 255, 1, 1, 1, 255, 2, 2, 2, 255, 0, 0, 255, 255, 255, 255, 0,
        255, 3, 3, 3, 255, 4, 4, 4, 255,
    ];
    let tex = test_we_tex_rgba(4, 2, &rgba);
    let source = TestDir::new("we-scene-tex-resource-frame-distinct-source");
    let output = TestDir::new("we-scene-tex-resource-frame-distinct-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                { "id": 1, "name": "Frame Tex", "image": "models/frame.json" },
                { "id": 2, "name": "Atlas Tex", "image": "models/full.json" }
              ]
            }"#,
    );
    source.write_file(
        "models/frame.json",
        r#"{ "material": "materials/shared.json", "width": 2, "height": 2 }"#,
    );
    source.write_file(
        "models/full.json",
        r#"{ "material": "materials/shared.json", "width": 4, "height": 2 }"#,
    );
    source.write_file(
        "materials/shared.json",
        r#"{ "passes": [{ "textures": ["albedo"] }] }"#,
    );
    source.write_bytes("materials/albedo.tex", &tex);
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Distinct Tex Frame Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let nodes = scene["nodes"].as_array().unwrap();
    assert_ne!(nodes[0]["resource"], nodes[1]["resource"]);
    let albedo_resources = scene["resources"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|resource| {
            resource["type"] == "image" && resource["original_source"] == "materials/albedo.tex"
        })
        .collect::<Vec<_>>();
    assert_eq!(albedo_resources.len(), 2);
}

fn assert_we_block_compressed_tex_conversion(
    we_format: u32,
    expected_gtex_format: u32,
    expected_tex_format: tex::SceneWeTexBlockCompressedFormat,
    payload: &[u8],
    suffix: &str,
    expected_role: &str,
    expected_feature: &str,
) {
    let tex = test_we_tex_block_compressed(4, 4, we_format, payload, 0);
    let decoded = tex::decode_we_tex_payload(&tex).unwrap();
    let SceneWeTexPayload::BlockCompressedImage(decoded) = decoded else {
        panic!("expected block-compressed WE texture");
    };
    assert_eq!(decoded.width, 4);
    assert_eq!(decoded.height, 4);
    assert_eq!(decoded.format, expected_tex_format);
    assert_eq!(decoded.payload.as_ref(), payload);
    let compressed_tex = test_we_tex_block_compressed(4, 4, we_format, payload, 1);
    let SceneWeTexPayload::BlockCompressedImage(compressed_decoded) =
        tex::decode_we_tex_payload(&compressed_tex).unwrap()
    else {
        panic!("expected LZ4-wrapped block-compressed WE texture");
    };
    assert_eq!(compressed_decoded.format, expected_tex_format);
    assert_eq!(compressed_decoded.payload.as_ref(), payload);

    let source = TestDir::new(&format!("we-scene-{suffix}-tex-source"));
    let output = TestDir::new(&format!("we-scene-{suffix}-tex-output"));
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                {
                  "id": 1,
                  "name": "Native BC Tex",
                  "image": "models/renderable.json"
                }
              ]
            }"#,
    );
    source.write_file(
        "models/renderable.json",
        r#"{ "material": "materials/renderable.json", "width": 4, "height": 4 }"#,
    );
    source.write_file(
        "materials/renderable.json",
        r#"{ "passes": [{ "textures": ["albedo"] }] }"#,
    );
    source.write_bytes("materials/albedo.tex", &tex);
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Native BC Tex Scene Model",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(scene["nodes"][0]["type"], "image");
    let resource_id = scene["nodes"][0]["resource"].as_str().unwrap();
    let resource = scene["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|resource| resource["id"] == resource_id)
        .expect("converted BC texture resource");
    assert_eq!(resource["role"], expected_role);
    let source_path = resource["source"].as_str().unwrap();
    assert!(source_path.ends_with(".gtex"));
    let gtex_path = output.path().join(source_path);
    let bytes = fs::read(&gtex_path).unwrap();
    assert_eq!(&bytes[0..8], gtex::GILDER_SCENE_TEXTURE_MAGIC);
    assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 4);
    assert_eq!(u32::from_le_bytes(bytes[12..16].try_into().unwrap()), 4);
    assert_eq!(
        u32::from_le_bytes(bytes[16..20].try_into().unwrap()),
        expected_gtex_format
    );
    assert_eq!(
        u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
        payload.len() as u64
    );
    assert_eq!(&bytes[32..], payload);

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"scene-we-tex-bc-gpu-texture".to_owned())
    );
    assert!(
        report
            .converted_features
            .contains(&expected_feature.to_owned())
    );
}

#[test]
fn extracts_wallpaper_engine_scene_tex_video_material_to_native_video_layer() {
    let video_payload = b"\0\0\0\x20ftypisom\0\0\x02\0isomiso2avc1mp41\0\0\0\x08free";
    let tex = test_we_tex_video(3840, 2160, video_payload);
    let source = TestDir::new("we-scene-tex-video-model-source");
    let output = TestDir::new("we-scene-tex-video-model-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                {
                  "id": 1,
                  "name": "Video Tex",
                  "image": "models/video.json"
                }
              ]
            }"#,
    );
    source.write_file(
        "models/video.json",
        r#"{ "material": "materials/video.json", "width": 3840, "height": 2160 }"#,
    );
    source.write_file(
        "materials/video.json",
        r#"{ "passes": [{ "textures": ["clip"] }] }"#,
    );
    source.write_bytes("materials/clip.tex", &tex);
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Video Tex Scene Model",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(scene["nodes"][0]["type"], "video");
    assert_eq!(scene["nodes"][0]["resource"], "resource-3-clip-video");
    assert_eq!(scene["nodes"][0]["width"], 3840.0);
    assert_eq!(scene["nodes"][0]["height"], 2160.0);
    assert_eq!(scene["resources"].as_array().unwrap().len(), 3);
    assert_eq!(scene["resources"][2]["type"], "video");
    assert_eq!(
        scene["resources"][2]["source"],
        "assets/scene-resources/scene/resource-3-clip-video.mp4"
    );
    assert_eq!(scene["resources"][2]["role"], "we-material-video-texture");
    assert_eq!(
        scene["resources"][2]["original_source"],
        "materials/clip.tex"
    );
    assert!(
        scene["resources"]
            .as_array()
            .unwrap()
            .iter()
            .all(|resource| !resource["source"].as_str().unwrap_or("").ends_with(".tex"))
    );
    assert_eq!(
        scene["nodes"][0]["provenance"]["model"]["texture_resources"][0],
        "resource-3-clip-video"
    );
    assert_eq!(
        fs::read(
            output
                .path()
                .join("assets/scene-resources/scene/resource-3-clip-video.mp4")
        )
        .unwrap(),
        video_payload
    );
    assert!(scene["unsupported_features"].as_array().unwrap().is_empty());
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"scene-we-tex-video-layer-runtime".to_owned())
    );
    let full_scene = report.full_scene.unwrap();
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"wallpaper-engine-tex-video-layer-runtime".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"initial-visible-video-scene-composition".to_owned())
    );
    assert!(
        full_scene
            .completed_boundaries
            .contains(&"vulkan-video-scene-layer-composition".to_owned())
    );
    assert!(
        !full_scene
            .pending_boundaries
            .contains(&"mixed-video-scene-composition".to_owned())
    );
}

#[test]
fn keeps_script_controlled_hidden_video_switching_out_of_mixed_video_boundary() {
    let video_payload = b"\0\0\0\x20ftypisom\0\0\x02\0isomiso2avc1mp41\0\0\0\x08free";
    let active_tex = test_we_tex_video(1920, 1080, video_payload);
    let hidden_tex = test_we_tex_video(1920, 1080, video_payload);
    let source = TestDir::new("we-scene-script-video-switch-source");
    let output = TestDir::new("we-scene-script-video-switch-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "script": "export function update(value) { return value; }",
              "objects": [
                {
                  "id": 1,
                  "name": "Loop",
                  "image": "models/loop.json"
                },
                {
                  "id": 2,
                  "name": "Interaction",
                  "visible": false,
                  "image": "models/interaction.json"
                },
                {
                  "id": 3,
                  "name": "Interaction Controller",
                  "image": "models/util/composelayer.json",
                  "visible": {
                    "value": true,
                    "scriptproperties": {
                      "targetLayerId": "Interaction",
                      "defaultHideTarget": true,
                      "togglePlay": true
                    }
                  }
                }
              ]
            }"#,
    );
    source.write_file(
        "models/loop.json",
        r#"{ "material": "materials/loop.json", "width": 1920, "height": 1080 }"#,
    );
    source.write_file(
        "models/interaction.json",
        r#"{ "material": "materials/interaction.json", "width": 1920, "height": 1080 }"#,
    );
    source.write_file(
        "materials/loop.json",
        r#"{ "passes": [{ "textures": ["loop"] }] }"#,
    );
    source.write_file(
        "materials/interaction.json",
        r#"{ "passes": [{ "textures": ["interaction"] }] }"#,
    );
    source.write_bytes("materials/loop.tex", &active_tex);
    source.write_bytes("materials/interaction.tex", &hidden_tex);
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Script Video Switch Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(scene["nodes"][0]["type"], "video");
    assert_eq!(scene["nodes"][1]["type"], "video");
    assert_eq!(scene["nodes"][1]["visible"], true);
    assert_eq!(scene["nodes"][1]["opacity"], 0.0);
    assert_eq!(scene["nodes"][2]["type"], "script");
    assert_eq!(
        scene["nodes"][2]["properties"]["controller"]["kind"],
        "click-video-switch"
    );
    assert_eq!(
        scene["nodes"][2]["properties"]["controller"]["target_node"],
        scene["nodes"][1]["id"]
    );
    let controller_node = scene["nodes"][2]["id"].as_str().unwrap();
    assert!(
        scene["nodes"][2]["properties"]["controller"]["input_aliases"]
            .as_array()
            .unwrap()
            .contains(&json!(format!(
                "scene.input.controller.{controller_node}.active"
            )))
    );
    assert!(
        scene["nodes"][2]["properties"]["controller"]["input_aliases"]
            .as_array()
            .unwrap()
            .contains(&json!(format!(
                "scene.input.controller.{}.active",
                scene["nodes"][1]["id"].as_str().unwrap()
            )))
    );
    assert!(
        scene["property_bindings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|binding| {
                binding["target_node"] == scene["nodes"][1]["id"]
                    && binding["target"] == "opacity"
                    && binding["property"]
                        .as_str()
                        .is_some_and(|property| property.starts_with("scene.controller."))
            })
    );
    assert!(
        scene["native_lowering"]["completed_boundaries"]
            .as_array()
            .unwrap()
            .contains(&json!("initial-visible-video-scene-composition"))
    );
    assert!(
        scene["native_lowering"]["completed_boundaries"]
            .as_array()
            .unwrap()
            .contains(&json!("script-controlled-video-layer-switching"))
    );
    assert!(
        !scene["native_lowering"]["pending_boundaries"]
            .as_array()
            .unwrap()
            .contains(&json!("mixed-video-scene-composition"))
    );
    assert!(
        !scene["native_lowering"]["pending_boundaries"]
            .as_array()
            .unwrap()
            .contains(&json!("script-controlled-video-layer-switching"))
    );
    assert!(
        !scene["native_lowering"]["pending_boundaries"]
            .as_array()
            .unwrap()
            .contains(&json!("scene-controller-input-source"))
    );
    assert!(
        scene["native_lowering"]["unsupported_boundaries"]
            .as_array()
            .unwrap()
            .contains(&json!("scene-controller-input-source"))
    );
    assert!(
        scene["unsupported_features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| feature["feature"] == "scene-controller-input-source")
    );
    let controller_property = scene["property_bindings"][0]["property"]
        .as_str()
        .unwrap()
        .to_owned();
    let target_node = scene["nodes"][1]["id"].as_str().unwrap().to_owned();
    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let inactive = document.snapshot_at_with_property_resolver(0, |_| None);
    assert_eq!(
        inactive
            .layers
            .iter()
            .find(|layer| layer.id == target_node)
            .unwrap()
            .opacity,
        0.0
    );
    let active = document.snapshot_at_with_property_resolver(0, |property| {
        (property == controller_property).then_some(1.0)
    });
    assert_eq!(
        active
            .layers
            .iter()
            .find(|layer| layer.id == target_node)
            .unwrap()
            .opacity,
        1.0
    );
}

#[test]
fn lowers_wallpaper_engine_timed_visibility_script_to_target_timeline() {
    let source = TestDir::new("we-scene-timed-visibility-controller-source");
    let output = TestDir::new("we-scene-timed-visibility-controller-output");
    output.remove();
    source.write_file(
        "scene.json",
        r##"{
              "general": {
                "orthogonalprojection": { "width": 1920, "height": 1080 }
              },
              "objects": [
                {
                  "id": 48,
                  "name": "Cloud",
                  "image": "models/util/fullscreenlayer.json",
                  "visible": false,
                  "color": "#ffffff"
                },
                {
                  "id": 63,
                  "name": "Intro Cloud Controller",
                  "solid": true,
                  "visible": {
                    "value": true,
                    "scriptproperties": {
                      "targetLayerName": "Cloud",
                      "enableAutoControl": { "value": true },
                      "startDelay": "0",
                      "showDuration": "2",
                      "fadeDuration": 0.5,
                      "hideOnStart": true,
                      "loopControl": false,
                      "loopInterval": 1
                    }
                  }
                }
              ]
            }"##,
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Timed Visibility Controller Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let nodes = scene["nodes"].as_array().unwrap();
    let target = nodes.iter().find(|node| node["name"] == "Cloud").unwrap();
    let controller = nodes
        .iter()
        .find(|node| node["name"] == "Intro Cloud Controller")
        .unwrap();
    let target_node = target["id"].as_str().unwrap().to_owned();
    assert_eq!(target["type"], "rectangle");
    assert_eq!(target["visible"], true);
    assert_eq!(target["opacity"], 0.0);
    assert_eq!(target["width"], 1920.0);
    assert_eq!(target["height"], 1080.0);
    assert_eq!(
        controller["properties"]["controller"]["kind"],
        "timed-visibility"
    );
    assert_eq!(
        controller["properties"]["controller"]["target_node"],
        target_node
    );
    assert!(
        !scene["property_bindings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|binding| binding["target_node"] == target_node)
    );
    let timeline = scene["timelines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|timeline| timeline["target_node"] == target_node)
        .unwrap();
    assert_eq!(timeline["channels"][0]["property"], "opacity");
    assert_eq!(timeline["channels"][0]["loop"], false);
    assert_eq!(
        timeline["channels"][0]["keyframes"],
        json!([
            { "time_ms": 0, "value": 0.0 },
            { "time_ms": 500, "value": 1.0 },
            { "time_ms": 2500, "value": 1.0 },
            { "time_ms": 3000, "value": 0.0 }
        ])
    );
    assert!(
        scene["native_lowering"]["completed_boundaries"]
            .as_array()
            .unwrap()
            .contains(&json!(
                "wallpaper-engine-timed-visibility-controller-lowering"
            ))
    );
    assert!(
        scene["native_lowering"]["completed_boundaries"]
            .as_array()
            .unwrap()
            .contains(&json!("scene-controller-fade-ramp-runtime"))
    );
    assert!(
        scene["native_lowering"]["completed_boundaries"]
            .as_array()
            .unwrap()
            .contains(&json!("timeline-animation-runtime"))
    );
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"scene-we-timed-visibility-controller".to_owned())
    );
    assert!(
        report
            .converted_features
            .contains(&"scene-keyframe-timeline".to_owned())
    );

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    for (time_ms, expected_opacity) in [(0, 0.0), (250, 0.5), (500, 1.0), (3000, 0.0)] {
        let snapshot = document.snapshot_at_with_property_resolver(time_ms, |_| None);
        let layer = snapshot
            .layers
            .iter()
            .find(|layer| layer.id == target_node)
            .unwrap();
        assert!(
            (layer.opacity - expected_opacity).abs() < 0.001,
            "opacity at {time_ms}ms was {}, expected {expected_opacity}",
            layer.opacity
        );
    }
}

#[test]
fn lowers_wallpaper_engine_parent_ids_to_gscene_children() {
    let source = TestDir::new("we-scene-parent-graph-source");
    let output = TestDir::new("we-scene-parent-graph-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                {
                  "id": 10,
                  "name": "Parent",
                  "type": "image",
                  "path": "parent.png",
                  "origin": [10, 20, 0],
                  "alpha": 0.5
                },
                {
                  "id": 20,
                  "parent": 10,
                  "name": "Child",
                  "type": "image",
                  "path": "child.png",
                  "origin": [3, 4, 0],
                  "alpha": 0.5
                }
              ]
            }"#,
    );
    source.write_file("parent.png", "not real png");
    source.write_file("child.png", "not real png");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Parented Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(scene["nodes"].as_array().unwrap().len(), 1);
    let parent = &scene["nodes"][0];
    assert_eq!(parent["provenance"]["source_id"], "10");
    assert_eq!(parent["children"].as_array().unwrap().len(), 1);
    assert_eq!(parent["children"][0]["provenance"]["source_id"], "20");
    assert_eq!(parent["children"][0]["provenance"]["parent_id"], "10");

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
    assert_eq!(snapshot.layers.len(), 2);
    assert_eq!(snapshot.layers[0].id, "node-1-image");
    assert_eq!(snapshot.layers[0].transform.x, 10.0);
    assert_eq!(snapshot.layers[0].transform.y, 20.0);
    assert_eq!(snapshot.layers[0].opacity, 0.5);
    assert_eq!(snapshot.layers[1].id, "node-2-image");
    assert_eq!(snapshot.layers[1].transform.x, 13.0);
    assert_eq!(snapshot.layers[1].transform.y, 24.0);
    assert_eq!(snapshot.layers[1].opacity, 0.25);
}

#[test]
fn lowers_wallpaper_engine_puppet_attachments_to_child_transforms() {
    let source = TestDir::new("we-scene-puppet-attachment-source");
    let output = TestDir::new("we-scene-puppet-attachment-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                {
                  "id": 10,
                  "name": "Body",
                  "image": "models/body.json",
                  "origin": [100, 200, 0],
                  "size": [400, 300, 0]
                },
                {
                  "id": 20,
                  "parent": 10,
                  "name": "Eye",
                  "image": "models/eye.json",
                  "attachment": "eye",
                  "size": [40, 20, 0]
                }
              ]
            }"#,
    );
    source.write_file(
        "models/body.json",
        r#"{
              "width": 400,
              "height": 300,
              "puppet": "models/body_puppet.mdl"
            }"#,
    );
    source.write_file(
        "models/eye.json",
        r#"{
              "width": 40,
              "height": 20
            }"#,
    );
    source.write_bytes(
        "models/body_puppet.mdl",
        &test_we_mdl_with_attachment("eye", 1, (210.0, 130.0), (5.0, -7.0)),
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Puppet Attachment Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let parent = &scene["nodes"][0];
    let child = &parent["children"][0];
    assert_eq!(child["provenance"]["attachment"], "eye");
    assert_eq!(child["transform"]["x"], 5.0);
    assert_eq!(child["transform"]["y"], -7.0);
    assert_eq!(
        parent["provenance"]["model"]["puppet_attachments"]["eye"]["bone_index"],
        1
    );
    assert_eq!(
        parent["provenance"]["model"]["puppet_attachments"]["eye"]["placement_source"],
        "mdls-bone-matrix-chain"
    );
    assert_eq!(
        parent["provenance"]["model"]["puppet_attachments"]["eye"]["target_x"],
        15.0
    );

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
    assert_eq!(snapshot.layers.len(), 2);
    assert_eq!(snapshot.layers[0].transform.x, 100.0);
    assert_eq!(snapshot.layers[0].transform.y, 200.0);
    assert_eq!(snapshot.layers[1].transform.x, 105.0);
    assert_eq!(snapshot.layers[1].transform.y, 193.0);
}

#[test]
fn lowers_wallpaper_engine_puppet_attachment_child_bone_translation_chain() {
    let source = TestDir::new("we-scene-puppet-child-bone-attachment-source");
    let output = TestDir::new("we-scene-puppet-child-bone-attachment-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                {
                  "id": 10,
                  "name": "Body",
                  "image": "models/body.json",
                  "origin": [100, 200, 0],
                  "size": [400, 300, 0]
                },
                {
                  "id": 20,
                  "parent": 10,
                  "name": "Hair",
                  "image": "models/hair.json",
                  "attachment": "hair",
                  "size": [40, 20, 0]
                }
              ]
            }"#,
    );
    source.write_file(
        "models/body.json",
        r#"{
              "width": 400,
              "height": 300,
              "puppet": "models/body_puppet.mdl"
            }"#,
    );
    source.write_file(
        "models/hair.json",
        r#"{
              "width": 40,
              "height": 20
            }"#,
    );
    source.write_bytes(
        "models/body_puppet.mdl",
        &test_we_mdl_with_attachment_and_child_translation(
            "hair",
            1,
            (210.0, 130.0),
            (8.0, 9.0),
            (5.0, -7.0),
        ),
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Puppet Child Bone Attachment Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let child = &scene["nodes"][0]["children"][0];
    assert_eq!(child["provenance"]["attachment"], "hair");
    assert_eq!(child["transform"]["x"], 13.0);
    assert_eq!(child["transform"]["y"], 2.0);

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
    assert_eq!(snapshot.layers[1].transform.x, 113.0);
    assert_eq!(snapshot.layers[1].transform.y, 202.0);
}

#[test]
fn lowers_wallpaper_engine_attachment_child_images_to_explicit_we_uv_meshes() {
    let source = TestDir::new("we-scene-puppet-attachment-group-child-uv-source");
    let output = TestDir::new("we-scene-puppet-attachment-group-child-uv-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                {
                  "id": 10,
                  "name": "Body",
                  "image": "models/body.json",
                  "origin": [100, 200, 0],
                  "size": [400, 300, 0]
                },
                {
                  "id": 20,
                  "parent": 10,
                  "name": "Hair Group",
                  "attachment": "hair"
                },
                {
                  "id": 30,
                  "parent": 20,
                  "name": "Hair Strand",
                  "type": "image",
                  "path": "hair.png",
                  "size": [40, 20, 0]
                }
              ]
            }"#,
    );
    source.write_file(
        "models/body.json",
        r#"{
              "width": 400,
              "height": 300,
              "puppet": "models/body_puppet.mdl"
            }"#,
    );
    source.write_file("hair.png", "not real png");
    source.write_bytes(
        "models/body_puppet.mdl",
        &test_we_mdl_with_attachment("hair", 1, (210.0, 130.0), (5.0, -7.0)),
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Puppet Attachment Group Child UV Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let group = &scene["nodes"][0]["children"][0];
    let child = &group["children"][0];

    assert_eq!(group["type"], "group");
    assert_eq!(group["provenance"]["attachment"], "hair");
    assert_eq!(child["type"], "image");
    assert_eq!(child["mesh"]["vertices"][0]["v"], 0.0);
    assert_eq!(child["mesh"]["vertices"][1]["v"], 0.0);
    assert_eq!(child["mesh"]["vertices"][2]["v"], 1.0);
    assert_eq!(child["mesh"]["vertices"][3]["v"], 1.0);
    assert_eq!(child["mesh"]["vertices"][0]["x"], -20.0);
    assert_eq!(child["mesh"]["vertices"][2]["y"], 10.0);
    assert_eq!(child["mesh"]["indices"], json!([0, 1, 2, 2, 1, 3]));
    let child_id = child["id"].as_str().unwrap().to_owned();

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
    let layer = snapshot
        .layers
        .iter()
        .find(|layer| layer.id == child_id)
        .expect("hair strand layer");
    let mesh = layer.mesh.as_ref().expect("attachment child uv mesh");
    assert_eq!(mesh.vertices[0].v, 0.0);
    assert_eq!(mesh.vertices[2].v, 1.0);
}

#[test]
fn lowers_wallpaper_engine_puppet_mesh_to_scene_mesh_geometry() {
    let source = TestDir::new("we-scene-puppet-mesh-bounds-source");
    let output = TestDir::new("we-scene-puppet-mesh-bounds-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [{
                "id": 10,
                "name": "Puppet Part",
                "image": "models/part.json",
                "attachment": "part",
                "origin": [100, 200, 0],
                "size": [200, 100, 0]
              }]
            }"#,
    );
    source.write_file(
        "models/part.json",
        r#"{
              "width": 200,
              "height": 100,
              "puppet": "models/part_puppet.mdl"
            }"#,
    );
    source.write_bytes(
        "models/part_puppet.mdl",
        &test_we_mdl_with_mesh_bounds(&[
            (-20.0, 40.0, 0.0, 0.0),
            (80.0, -60.0, 1.0, 0.0),
            (10.0, 0.0, 0.5, 1.0),
        ]),
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Puppet Mesh Bounds Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let node = &scene["nodes"][0];
    assert_eq!(node["width"], 200.0);
    assert_eq!(node["height"], 100.0);
    assert_eq!(node["mesh"]["vertices"][0]["x"], -20.0);
    assert_eq!(node["mesh"]["vertices"][0]["y"], 40.0);
    assert_eq!(node["mesh"]["vertices"][0]["v"], 1.0);
    assert_eq!(node["mesh"]["vertices"][1]["u"], 1.0);
    assert_eq!(node["mesh"]["vertices"][2]["v"], 0.0);
    assert_eq!(node["mesh"]["indices"], json!([0, 1, 2]));
    assert_eq!(
        node["provenance"]["model"]["puppet_mesh_bounds"]["left"],
        -20.0
    );
    assert_eq!(
        node["provenance"]["model"]["puppet_mesh_bounds"]["top"],
        -60.0
    );

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
    assert_eq!(snapshot.layers[0].width, Some(200.0));
    assert_eq!(snapshot.layers[0].height, Some(100.0));
    let mesh = snapshot.layers[0].mesh.as_ref().expect("puppet mesh");
    assert_eq!(mesh.vertices.len(), 3);
    assert_eq!(mesh.indices, vec![0, 1, 2]);
}

#[test]
fn lowers_wallpaper_engine_puppet_animation_layers_to_sampled_skinning() {
    let source = TestDir::new("we-scene-puppet-animation-source");
    let output = TestDir::new("we-scene-puppet-animation-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [{
                "id": 10,
                "name": "Animated Puppet",
                "image": "models/body.json",
                "origin": [0, 0, 0],
                "size": [32, 32, 0],
                "animationlayers": [
                  {
                    "id": 20,
                    "name": "turn",
                    "animation": 7,
                    "additive": false,
                    "blend": 1.0,
                    "rate": 1.0,
                    "visible": true
                  }
                ]
              }]
            }"#,
    );
    source.write_file(
        "models/body.json",
        r#"{
              "width": 32,
              "height": 32,
              "puppet": "models/body_puppet.mdl"
            }"#,
    );
    source.write_bytes(
        "models/body_puppet.mdl",
        &test_we_mdl_with_skinned_animation(),
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Puppet Animation Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let node = &scene["nodes"][0];
    assert_eq!(node["mesh"]["skin"]["bones"].as_array().unwrap().len(), 2);
    assert_eq!(node["mesh"]["puppet_clips"][0]["id"], 7);
    assert_eq!(node["puppet_animation_layers"][0]["clip_id"], 7);
    assert_eq!(node["puppet_animation_layers"][0]["name"], "turn");
    assert!(
        !scene["unsupported_features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| feature["feature"] == "we-animation-layer-blending")
    );

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let mut first = Vec::new();
    document.snapshot_sampled_image_layers_at_with_resolvers(0, |_| None, |_| None, &mut first);
    let first_mesh = first[0].mesh.as_ref().expect("first puppet mesh");
    assert!((first_mesh.vertices[0].x - 20.0).abs() < 0.000_001);
    assert!(first_mesh.vertices[0].y.abs() < 0.000_001);

    let mut later = Vec::new();
    document.snapshot_sampled_image_layers_at_with_resolvers(1000, |_| None, |_| None, &mut later);
    let later_mesh = later[0].mesh.as_ref().expect("later puppet mesh");
    assert_eq!(later_mesh.indices, first_mesh.indices);
    assert!((later_mesh.vertices[0].x - 10.0).abs() < 0.000_001);
    assert!((later_mesh.vertices[0].y - 10.0).abs() < 0.000_001);

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"wallpaper-engine-puppet-animation-clips".to_owned())
    );
    assert!(
        report
            .converted_features
            .contains(&"wallpaper-engine-puppet-animation-layer-lowering".to_owned())
    );
    assert!(
        !report
            .unsupported_features
            .contains(&"we-animation-layer-blending".to_owned())
    );
}

#[test]
fn lowers_wallpaper_engine_puppet_attachments_to_runtime_bone_pose() {
    let source = TestDir::new("we-scene-puppet-attachment-animation-source");
    let output = TestDir::new("we-scene-puppet-attachment-animation-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                {
                  "id": 10,
                  "name": "Animated Body",
                  "image": "models/body.json",
                  "origin": [100, 200, 0],
                  "size": [32, 32, 0],
                  "animationlayers": [
                    {
                      "id": 20,
                      "name": "turn",
                      "animation": 7,
                      "additive": false,
                      "blend": 1.0,
                      "rate": 1.0,
                      "visible": true
                    }
                  ]
                },
                {
                  "id": 30,
                  "parent": 10,
                  "name": "Eye",
                  "type": "image",
                  "path": "eye.png",
                  "attachment": "eye",
                  "size": [8, 4, 0]
                }
              ]
            }"#,
    );
    source.write_file(
        "models/body.json",
        r#"{
              "width": 32,
              "height": 32,
              "puppet": "models/body_puppet.mdl"
            }"#,
    );
    source.write_file("eye.png", "not real png");
    source.write_bytes(
        "models/body_puppet.mdl",
        &test_we_mdl_with_skinned_animation_and_attachment("eye", 1, (10.0, 0.0)),
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Puppet Attachment Animation Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let parent = &scene["nodes"][0];
    let child = &parent["children"][0];
    assert_eq!(child["puppet_attachment"], "eye");
    assert_eq!(child["transform"]["x"], 20.0);
    assert_eq!(child["transform"]["y"], 0.0);
    assert_eq!(parent["mesh"]["skin"]["attachments"][0]["name"], "eye");
    assert_eq!(
        parent["mesh"]["skin"]["attachments"][0]["local_position"],
        json!([10.0, 0.0, 0.0])
    );
    assert_eq!(
        parent["mesh"]["skin"]["attachments"][0]["bind_position"],
        json!([20.0, 0.0, 0.0])
    );

    let child_id = child["id"].as_str().unwrap().to_owned();
    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let first = document.snapshot_at_with_property_resolver(0, |_| None);
    let first_eye = first
        .layers
        .iter()
        .find(|layer| layer.id == child_id)
        .expect("first eye layer");
    assert!((first_eye.transform.x - 120.0).abs() < 0.000_001);
    assert!((first_eye.transform.y - 200.0).abs() < 0.000_001);
    assert!(first_eye.transform.rotation_deg.abs() < 0.000_001);

    let later = document.snapshot_at_with_property_resolver(1000, |_| None);
    let later_eye = later
        .layers
        .iter()
        .find(|layer| layer.id == child_id)
        .expect("later eye layer");
    assert!((later_eye.transform.x - 110.0).abs() < 0.000_001);
    assert!((later_eye.transform.y - 210.0).abs() < 0.000_001);
    assert!((later_eye.transform.rotation_deg - 90.0).abs() < 0.000_01);
}

#[test]
fn converts_wallpaper_engine_scene_text_and_visible_property_binding() {
    let source = TestDir::new("we-scene-text-binding-source");
    let output = TestDir::new("we-scene-text-binding-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                {
                  "id": 30,
                  "name": "Title",
                  "type": "text",
                  "text": { "value": "Hello Scene" },
                  "pointsize": { "value": 48 },
                  "font": { "value": "fonts/Inter.ttf" },
                  "horizontalalign": "center",
                  "color": [1, 1, 1],
                  "visible": { "value": false, "user": "show_title" }
                }
              ]
            }"#,
    );
    source.write_file("fonts/Inter.ttf", "not real font");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Text Binding Scene",
              "file": "scene.json",
              "general": {
                "properties": {
                  "show_title": { "type": "bool", "value": false }
                }
              }
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let node = &scene["nodes"][0];
    assert_eq!(node["type"], "text");
    assert_eq!(node["text"], "Hello Scene");
    assert_eq!(node["font_size"], 48.0);
    assert_eq!(node["font_family"], "fonts/Inter.ttf");
    assert_eq!(node["font_resource"], "resource-1-inter");
    assert_eq!(scene["resources"][0]["type"], "font");
    assert_eq!(scene["resources"][0]["role"], "we-font");
    assert_eq!(
        scene["resources"][0]["source"],
        "assets/scene-resources/scene/resource-1-inter.ttf"
    );
    assert_eq!(node["text_align"], "middle");
    assert_eq!(node["visible"], true);
    assert_eq!(node["opacity"], 0.0);
    assert_eq!(scene["property_bindings"][0]["property"], "show_title");
    assert_eq!(scene["property_bindings"][0]["target_node"], node["id"]);
    assert_eq!(scene["property_bindings"][0]["target"], "opacity");
    assert_eq!(scene["properties"]["show_title"]["type"], "bool");
    assert_eq!(scene["properties"]["show_title"]["default"], false);

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let hidden = document.snapshot_at_with_property_resolver(0, |_| None);
    assert_eq!(hidden.layers[0].opacity, 0.0);
    let visible = document.snapshot_at_with_property_resolver(0, |property| {
        if property == "show_title" {
            Some(1.0)
        } else {
            None
        }
    });
    assert_eq!(visible.layers[0].kind, crate::core::SceneNodeKind::Text);
    assert_eq!(visible.layers[0].text.as_deref(), Some("Hello Scene"));
    assert_eq!(
        visible.layers[0].font_source.as_ref().unwrap().as_str(),
        "assets/scene-resources/scene/resource-1-inter.ttf"
    );
    assert_eq!(visible.layers[0].opacity, 1.0);
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"wallpaper-engine-font-resource-lowering".to_owned())
    );
    assert!(
        report
            .full_scene
            .as_ref()
            .unwrap()
            .completed_boundaries
            .contains(&"wallpaper-engine-font-resource-lowering".to_owned())
    );
}

#[test]
fn lowers_wallpaper_engine_layer_blend_to_opacity() {
    let source = TestDir::new("we-scene-layer-blend-source");
    let output = TestDir::new("we-scene-layer-blend-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                {
                  "id": 31,
                  "name": "Blend panel",
                  "image": "models/util/solidlayer.json",
                  "size": "320 180",
                  "color": "1 1 1",
                  "alpha": 0.5,
                  "blend": 0.7,
                  "blendin": false,
                  "blendout": false,
                  "blendtime": 0.5
                }
              ]
            }"#,
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Layer Blend Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let node = &scene["nodes"][0];
    assert_eq!(node["opacity"], 0.35);
    assert_eq!(node["properties"]["wallpaper_engine_blend"]["blend"], 0.7);
    assert_eq!(
        node["properties"]["wallpaper_engine_blend"]["blendtime"],
        0.5
    );
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"wallpaper-engine-layer-blend-lowering".to_owned())
    );
}

#[test]
fn infers_wallpaper_engine_color_blend_opacity_from_matching_model_instance() {
    let source = TestDir::new("we-scene-color-blend-opacity-source");
    let output = TestDir::new("we-scene-color-blend-opacity-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                {
                  "id": 10,
                  "name": "Authored translucent water",
                  "image": "models/water.json",
                  "alpha": 0.24,
                  "colorBlendMode": 7,
                  "size": "320 180"
                },
                {
                  "id": 11,
                  "name": "Repeated translucent water",
                  "image": "models/water.json",
                  "colorBlendMode": 7,
                  "size": "320 180"
                }
              ]
            }"#,
    );
    source.write_file(
        "models/water.json",
        r#"{ "material": "materials/water.json" }"#,
    );
    source.write_file(
        "materials/water.json",
        r#"{ "passes": [{
              "shader": "genericimage2",
              "blending": "translucent",
              "textures": ["textures/water.png"]
            }] }"#,
    );
    source.write_file("textures/water.png", "not real png");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Color Blend Opacity Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(scene["nodes"][0]["opacity"], 0.24);
    assert_eq!(scene["nodes"][1]["opacity"], 0.24);
    assert_eq!(
        scene["nodes"][1]["properties"]["wallpaper_engine_blend"]["colorBlendMode"],
        7
    );

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
    assert_eq!(snapshot.layers[1].opacity, 0.24);

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"wallpaper-engine-color-blend-opacity-inference".to_owned())
    );
}

#[test]
fn converts_wallpaper_engine_user_color_binding() {
    let source = TestDir::new("we-scene-user-color-source");
    let output = TestDir::new("we-scene-user-color-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                {
                  "id": 41,
                  "name": "Tinted panel",
                  "image": "models/util/solidlayer.json",
                  "size": "320 180",
                  "color": { "user": "accent", "value": "0.00000 0.59216 0.73725" }
                }
              ]
            }"#,
    );
    source.write_file(
        PROJECT_FILE,
        r##"{
              "type": "scene",
              "title": "User Color Scene",
              "file": "scene.json",
              "general": {
                "properties": {
                  "accent": { "type": "color", "value": "0 0.235294 0.643137" }
                }
              }
            }"##,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let node = &scene["nodes"][0];
    assert_eq!(node["type"], "rectangle");
    assert_eq!(node["color"], "#0097bc");
    assert_eq!(
        node["properties"]["color_binding"],
        json!({
            "runtime": "wallpaper-engine-user-color",
            "property": "accent",
            "default": "#0097bc"
        })
    );
    assert_eq!(scene["properties"]["accent"]["type"], "color");
    assert_eq!(scene["properties"]["accent"]["default"], "#003ca4");
}

#[test]
fn converts_wallpaper_engine_conditional_visibility_from_user_property_default() {
    let source = TestDir::new("we-scene-conditional-visibility-source");
    let output = TestDir::new("we-scene-conditional-visibility-output");
    output.remove();
    source.write_file(
        "scene.json",
        r#"{
              "objects": [
                {
                  "id": 10,
                  "name": "Default Theme",
                  "type": "text",
                  "text": { "value": "default" },
                  "visible": {
                    "value": true,
                    "user": { "name": "newproperty", "condition": "1" }
                  }
                },
                {
                  "id": 11,
                  "name": "Solid Theme",
                  "type": "text",
                  "text": { "value": "solid" },
                  "visible": {
                    "value": false,
                    "user": { "name": "newproperty", "condition": "2" }
                  }
                }
              ]
            }"#,
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Conditional Visibility Scene",
              "file": "scene.json",
              "general": {
                "properties": {
                  "newproperty": {
                    "type": "combo",
                    "options": [
                      { "label": "default", "value": "1" },
                      { "label": "solid", "value": "2" }
                    ],
                    "value": "1"
                  }
                }
              }
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();

    assert_eq!(scene["properties"]["newproperty"]["default"], "1");
    assert_eq!(scene["nodes"][0]["visible"], true);
    assert_eq!(scene["nodes"][1]["visible"], true);
    assert_eq!(
        scene["nodes"][0]["properties"]["visibility_condition"]["property"],
        "newproperty"
    );
    assert_eq!(
        scene["nodes"][0]["properties"]["visibility_condition"]["default_visible"],
        true
    );
    assert_eq!(
        scene["nodes"][1]["properties"]["visibility_condition"]["condition"],
        "2"
    );
    assert_eq!(
        scene["nodes"][1]["properties"]["visibility_condition"]["default_visible"],
        false
    );
}

#[test]
fn lowers_wallpaper_engine_clock_text_scripts_to_native_text_bindings() {
    let source = TestDir::new("we-scene-clock-text-source");
    let output = TestDir::new("we-scene-clock-text-output");
    output.remove();
    source.write_file(
            "scene.json",
            r#"{
              "objects": [
                {
                  "id": 86,
                  "name": "Clock",
                  "type": "text",
                  "text": {
                    "script": "export function update(value) { let time = new Date(); var hours = time.getHours(); if (!scriptProperties.use24hFormat) { hours %= 12; } let minutes = time.getMinutes(); return hours + scriptProperties.delimiter + minutes; }",
                    "scriptproperties": {
                      "delimiter": ":",
                      "showSeconds": false,
                      "use24hFormat": true
                    },
                    "value": "12:34"
                  }
                },
                {
                  "id": 113,
                  "name": "Date",
                  "type": "text",
                  "text": {
                    "script": "export function update(value) { let date = new Date(); return dtt[date.getDate()] + delimiterValue + months[date.getMonth()] + delimiterValue + date.getFullYear(); }",
                    "scriptproperties": {
                      "alignVertical": true,
                      "monthFormat": "2",
                      "showDay": false,
                      "useDelimiter": false
                    },
                    "value": "1\n5\n\nN\nO\nV\n\n2\n0\n2\n3"
                  }
                },
                {
                  "id": 105,
                  "name": "D a y",
                  "type": "text",
                  "text": {
                    "script": "export function update(value) { let date = new Date(); return day[date.getDay()]; }",
                    "scriptproperties": {
                      "alignVertical": true,
                      "dayFormat": "1",
                      "showDay": true,
                      "useDelimiter": false
                    },
                    "value": "S\nU\nN"
                  }
                }
              ]
            }"#,
        );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Clock Text Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let nodes = scene["nodes"].as_array().unwrap();
    let clock = nodes.iter().find(|node| node["name"] == "Clock").unwrap();
    let date = nodes.iter().find(|node| node["name"] == "Date").unwrap();
    let day = nodes.iter().find(|node| node["name"] == "D a y").unwrap();
    assert_eq!(
        clock["properties"]["text_binding"]["property"],
        "scene.clock.local.time.hm24"
    );
    assert_eq!(
        date["properties"]["text_binding"]["property"],
        "scene.clock.local.we-date.vertical-month-abbrev"
    );
    assert_eq!(
        day["properties"]["text_binding"]["property"],
        "scene.clock.local.we-day.vertical-weekday-abbrev-upper"
    );
    assert!(
        scene["native_lowering"]["completed_boundaries"]
            .as_array()
            .unwrap()
            .contains(&json!("wallpaper-engine-deterministic-clock-text-lowering"))
    );

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"scene-we-deterministic-clock-text".to_owned())
    );

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let snapshot = document.snapshot_at_with_resolvers(
        0,
        |_| None,
        |property| match property {
            "scene.clock.local.time.hm24" => Some("23:45".to_owned()),
            "scene.clock.local.we-date.vertical-month-abbrev" => {
                Some("2\n8\n\nJ\nU\nN\n\n2\n0\n2\n6".to_owned())
            }
            "scene.clock.local.we-day.vertical-weekday-abbrev-upper" => Some("S\nU\nN".to_owned()),
            _ => None,
        },
    );
    assert_eq!(snapshot.layers[0].text.as_deref(), Some("23:45"));
    assert_eq!(
        snapshot.layers[1].text.as_deref(),
        Some("2\n8\n\nJ\nU\nN\n\n2\n0\n2\n6")
    );
    assert_eq!(snapshot.layers[2].text.as_deref(), Some("S\nU\nN"));
}

#[test]
fn converts_wallpaper_engine_scene_shape_objects_to_native_nodes() {
    let source = TestDir::new("we-scene-shape-source");
    let output = TestDir::new("we-scene-shape-output");
    output.remove();
    source.write_file(
        "scene.json",
        r##"{
              "objects": [
                {
                  "id": 40,
                  "shape": "rectangle",
                  "solid": true,
                  "backgroundcolor": "0.2 0.4 0.6",
                  "size": [200, 100, 0],
                  "radius": { "value": 12 }
                },
                {
                  "id": 41,
                  "shape": { "value": "ellipse" },
                  "color": [1, 0, 0],
                  "size": [50, 60, 0]
                },
                {
                  "id": 42,
                  "d": "M0 0 L100 0 L100 100 L0 100 Z M25 25 L75 25 L75 75 L25 75 Z",
                  "fillRule": "evenodd",
                  "color": "#22aa88"
                }
              ]
            }"##,
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Shape Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let nodes = scene["nodes"].as_array().unwrap();
    assert_eq!(nodes[0]["type"], "rectangle");
    assert_eq!(nodes[0]["color"], "#336699");
    assert_eq!(nodes[0]["width"], 200.0);
    assert_eq!(nodes[0]["height"], 100.0);
    assert_eq!(nodes[0]["corner_radius"], 12.0);
    assert_eq!(nodes[1]["type"], "ellipse");
    assert_eq!(nodes[1]["color"], "#ff0000");
    assert_eq!(nodes[1]["width"], 50.0);
    assert_eq!(nodes[1]["height"], 60.0);
    assert_eq!(nodes[2]["type"], "path");
    assert_eq!(
        nodes[2]["path"],
        "M0 0 L100 0 L100 100 L0 100 Z M25 25 L75 25 L75 75 L25 75 Z"
    );
    assert_eq!(nodes[2]["path_fill_rule"], "evenodd");
    assert_eq!(nodes[2]["color"], "#22aa88");

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
    assert_eq!(snapshot.layers.len(), 3);
    assert_eq!(
        snapshot.layers[0].kind,
        crate::core::SceneNodeKind::Rectangle
    );
    assert_eq!(snapshot.layers[0].color.as_deref(), Some("#336699"));
    assert_eq!(snapshot.layers[0].corner_radius, Some(12.0));
    assert_eq!(snapshot.layers[1].kind, crate::core::SceneNodeKind::Ellipse);
    assert_eq!(snapshot.layers[1].color.as_deref(), Some("#ff0000"));
    assert_eq!(snapshot.layers[2].kind, crate::core::SceneNodeKind::Path);
    assert_eq!(
        snapshot.layers[2].path_fill_rule,
        crate::core::ScenePathFillRule::Evenodd
    );
}

#[test]
fn converts_wallpaper_engine_scene_wrapped_geometry_properties() {
    let source = TestDir::new("we-scene-wrapped-geometry-source");
    let output = TestDir::new("we-scene-wrapped-geometry-output");
    output.remove();
    source.write_file(
            "scene.json",
            r##"{
              "general": {
                "cameraparallaxamount": 10
              },
              "objects": [
                {
                  "id": 45,
                  "shape": "rectangle",
                  "backgroundcolor": { "script": "return [0.2, 0.4, 0.6];", "value": [0.2, 0.4, 0.6] },
                  "x": { "value": 10, "user": "panel_x" },
                  "y": { "value": 20, "user": "panel_y" },
                  "width": { "value": 100, "user": "panel_width" },
                  "height": { "value": 50, "user": "panel_height" },
                  "radius": { "value": 6, "user": "panel_radius" },
                  "parallax_depth": { "script": "return 0.5;", "value": 0.5 },
                  "alpha": { "value": 0.4, "user": "panel_alpha" }
                }
              ]
            }"##,
        );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Wrapped Geometry Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let node = &scene["nodes"][0];
    assert_eq!(node["type"], "rectangle");
    assert_eq!(node["color"], "#336699");
    assert_eq!(node["transform"]["x"], 10.0);
    assert_eq!(node["transform"]["y"], 20.0);
    assert_eq!(node["width"], 100.0);
    assert_eq!(node["height"], 50.0);
    assert_eq!(node["corner_radius"], 6.0);
    assert_eq!(node["parallax_depth"], 0.5);
    assert_eq!(node["opacity"], 0.4);
    let bindings = scene["property_bindings"].as_array().unwrap();
    for (property, target) in [
        ("panel_x", "x"),
        ("panel_y", "y"),
        ("panel_width", "width"),
        ("panel_height", "height"),
        ("panel_radius", "corner-radius"),
        ("panel_alpha", "opacity"),
    ] {
        assert!(
            bindings
                .iter()
                .any(|binding| { binding["property"] == property && binding["target"] == target }),
            "missing property binding {property} -> {target}: {bindings:?}"
        );
    }

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let snapshot = document.snapshot_at_with_property_resolver(0, |property| match property {
        "panel_x" => Some(30.0),
        "panel_y" => Some(40.0),
        "panel_width" => Some(220.0),
        "panel_height" => Some(90.0),
        "panel_radius" => Some(18.0),
        "panel_alpha" => Some(0.75),
        "scene.parallax.x" => Some(2.0),
        "scene.parallax.y" => Some(-1.0),
        _ => None,
    });
    assert_eq!(snapshot.layers[0].transform.x, 40.0);
    assert_eq!(snapshot.layers[0].transform.y, 35.0);
    assert_eq!(snapshot.layers[0].width, Some(220.0));
    assert_eq!(snapshot.layers[0].height, Some(90.0));
    assert_eq!(snapshot.layers[0].corner_radius, Some(18.0));
    assert_eq!(snapshot.layers[0].parallax_depth, Some(0.5));
    assert_eq!(snapshot.layers[0].opacity, 0.75);
}

#[test]
fn lowers_wallpaper_engine_scene_linear_scenescript_bindings_without_js_engine() {
    let source = TestDir::new("we-scene-linear-script-binding-source");
    let output = TestDir::new("we-scene-linear-script-binding-output");
    output.remove();
    source.write_file(
        "scene.json",
        r##"{
              "objects": [
                {
                  "id": 46,
                  "shape": "rectangle",
                  "backgroundcolor": "#203040",
                  "x": {
                    "value": 10,
                    "user": "panel_x",
                    "script": "return value + user * 2 + 5;"
                  },
                  "width": {
                    "value": 100,
                    "script": "return this.user.panel_width.value / 2 + value;"
                  },
                  "height": {
                    "value": 50,
                    "script": "return getUserProperty(\"panel_height\") * 0.25 + value;"
                  },
                  "alpha": {
                    "value": 0.2,
                    "user": "panel_alpha",
                    "script": "return Number(user) * 0.5 + 0.1;"
                  }
                }
              ]
            }"##,
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Linear SceneScript Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let bindings = scene["property_bindings"].as_array().unwrap();
    for (property, target, scale, offset) in [
        ("panel_x", "x", 2.0, 15.0),
        ("panel_width", "width", 0.5, 100.0),
        ("panel_height", "height", 0.25, 50.0),
        ("panel_alpha", "opacity", 0.5, 0.1),
    ] {
        let binding = bindings
            .iter()
            .find(|binding| binding["property"] == property && binding["target"] == target)
            .unwrap_or_else(|| panic!("missing binding {property} -> {target}: {bindings:?}"));
        assert_eq!(binding["scale"], scale);
        assert_eq!(binding["offset"], offset);
    }

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let snapshot = document.snapshot_at_with_property_resolver(0, |property| match property {
        "panel_x" => Some(7.0),
        "panel_width" => Some(80.0),
        "panel_height" => Some(40.0),
        "panel_alpha" => Some(1.0),
        _ => None,
    });
    assert_eq!(snapshot.layers[0].transform.x, 29.0);
    assert_eq!(snapshot.layers[0].width, Some(140.0));
    assert_eq!(snapshot.layers[0].height, Some(60.0));
    assert_eq!(snapshot.layers[0].opacity, 0.6);

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"scene-deterministic-scenescript-expression".to_owned())
    );
}

#[test]
fn lowers_wallpaper_engine_origin_component_scenescript_bindings() {
    let source = TestDir::new("we-scene-origin-component-script-binding-source");
    let output = TestDir::new("we-scene-origin-component-script-binding-output");
    output.remove();
    source.write_file(
            "scene.json",
            r##"{
              "objects": [
                {
                  "id": 10,
                  "name": "Character Root",
                  "solid": true,
                  "origin": {
                    "value": "1910 1366 0",
                    "script": "export var scriptProperties = createScriptProperties().addSlider({ name: 'newSlider', value: 50 }).finish();\nexport function update(value) {\n  value.x = scriptProperties.newSlider;\n  return value;\n}",
                    "scriptproperties": {
                      "newSlider": {
                        "user": "character_x",
                        "value": 50
                      }
                    }
                  }
                },
                {
                  "id": 20,
                  "parent": 10,
                  "name": "Character Body",
                  "type": "image",
                  "image": "body.png",
                  "origin": "12 -20 0",
                  "size": "100 200 0"
                }
              ]
            }"##,
        );
    source.write_file("body.png", "not real png");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Origin Component Script Binding Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let root = &scene["nodes"][0];
    assert_eq!(root["type"], "group");
    assert_eq!(root["children"][0]["type"], "image");
    let child_id = root["children"][0]["id"].clone();
    let binding = scene["property_bindings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|binding| binding["property"] == "character_x" && binding["target"] == "x")
        .expect("origin x binding");
    assert_eq!(binding["target_node"], root["id"]);
    assert_eq!(binding["scale"], 1.0);
    assert_eq!(binding["offset"], 0.0);

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let snapshot = document.snapshot_at_with_property_resolver(0, |property| {
        (property == "character_x").then_some(2000.0)
    });
    assert_eq!(snapshot.layers.len(), 1);
    assert_eq!(snapshot.layers[0].id, child_id);
    assert_eq!(snapshot.layers[0].transform.x, 2012.0);
    assert_eq!(snapshot.layers[0].transform.y, 1346.0);

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"scene-deterministic-scenescript-expression".to_owned())
    );
}

#[test]
fn lowers_wallpaper_engine_runtime_sine_origin_scenescript_to_timelines() {
    let source = TestDir::new("we-scene-runtime-sine-origin-script-source");
    let output = TestDir::new("we-scene-runtime-sine-origin-script-output");
    output.remove();
    source.write_file(
        "scene.json",
        r##"{
              "objects": [
                {
                  "id": 30,
                  "name": "Floating Star",
                  "type": "image",
                  "image": "star.png",
                  "size": "64 64 0",
                  "origin": {
                    "value": "100 200 0",
                    "script": "export var scriptProperties = createScriptProperties().addSlider({ name: 'xa', value: 100 }).addSlider({ name: 'xb', value: 1.57079632679 }).addSlider({ name: 'xc', value: 10 }).addSlider({ name: 'ya', value: 200 }).addSlider({ name: 'yb', value: 1.57079632679 }).addSlider({ name: 'yc', value: 20 }).finish();\nexport function update(value) {\n  value.x = scriptProperties.xa + (Math.sin(engine.runtime * scriptProperties.xb) * scriptProperties.xc);\n  value.y = scriptProperties.ya + (Math.sin(engine.runtime * scriptProperties.yb) * scriptProperties.yc);\n  return value;\n}",
                    "scriptproperties": {
                      "xa": 100,
                      "xb": 1.57079632679,
                      "xc": 10,
                      "ya": 200,
                      "yb": 1.57079632679,
                      "yc": 20
                    }
                  }
                }
              ]
            }"##,
    );
    source.write_file("star.png", "not real png");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Runtime Sine Origin Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let timelines = scene["timelines"].as_array().unwrap();
    assert_eq!(timelines.len(), 2);
    assert!(
        timelines
            .iter()
            .any(|timeline| timeline["channels"][0]["property"] == "x")
    );
    assert!(
        timelines
            .iter()
            .any(|timeline| timeline["channels"][0]["property"] == "y")
    );

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let snapshot = document.snapshot_at_with_property_resolver(1000, |_| None);
    assert!((snapshot.layers[0].transform.x - 110.0).abs() < 0.001);
    assert!((snapshot.layers[0].transform.y - 220.0).abs() < 0.001);

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"scene-deterministic-scenescript-sine-timeline".to_owned())
    );
}

#[test]
fn lowers_wallpaper_engine_embedded_property_keyframes_to_gscene_timelines() {
    let source = TestDir::new("we-scene-embedded-property-timeline-source");
    let output = TestDir::new("we-scene-embedded-property-timeline-output");
    output.remove();
    source.write_file(
        "scene.json",
        r##"{
              "objects": [
                {
                  "id": 81,
                  "shape": "rectangle",
                  "backgroundcolor": "#203040",
                  "size": "100 50",
                  "origin": {
                    "value": "0 0 0",
                    "easing": "linear",
                    "keyframes": [
                      { "time": 0, "value": "0 0 0" },
                      { "time": { "value": 1 }, "value": { "value": "100 40 0" } }
                    ]
                  },
                  "alpha": {
                    "value": 1,
                    "frames": [
                      [0, 1],
                      [0.5, { "value": 0.25 }],
                      { "time": 1, "value": { "value": false } }
                    ]
                  }
                }
              ]
            }"##,
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Embedded Property Timeline Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let timelines = scene["timelines"].as_array().unwrap();
    assert_eq!(timelines.len(), 2);
    assert!(timelines.iter().any(|timeline| {
        let channels = timeline["channels"].as_array().unwrap();
        channels.iter().any(|channel| channel["property"] == "x")
            && channels.iter().any(|channel| channel["property"] == "y")
    }));
    assert!(timelines.iter().any(|timeline| {
        timeline["channels"]
            .as_array()
            .unwrap()
            .iter()
            .any(|channel| channel["property"] == "opacity")
    }));

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let snapshot = document.snapshot_at_with_property_resolver(500, |_| None);
    assert_eq!(snapshot.layers[0].transform.x, 50.0);
    assert_eq!(snapshot.layers[0].transform.y, 20.0);
    assert_eq!(snapshot.layers[0].opacity, 0.25);

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"scene-we-embedded-property-timeline".to_owned())
    );
}

#[test]
fn lowers_wallpaper_engine_embedded_component_animation_to_gscene_timelines() {
    let source = TestDir::new("we-scene-embedded-component-animation-source");
    let output = TestDir::new("we-scene-embedded-component-animation-output");
    output.remove();
    source.write_file(
        "scene.json",
        r##"{
              "objects": [
                {
                  "id": 81,
                  "shape": "rectangle",
                  "backgroundcolor": "#203040",
                  "size": "100 50",
                  "origin": {
                    "value": "10 20 0",
                    "animation": {
                      "relative": true,
                      "c0": [
                        { "frame": 0, "value": 0 },
                        { "frame": 15, "value": 30 }
                      ],
                      "c1": [
                        { "frame": 0, "value": 0 },
                        { "frame": 15, "value": -10 }
                      ],
                      "options": {
                        "fps": 30,
                        "length": 30,
                        "mode": "loop",
                        "wraploop": true
                      }
                    }
                  }
                }
              ]
            }"##,
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Embedded Component Animation Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    let timeline = scene["timelines"][0].clone();
    assert_eq!(timeline["channels"][0]["property"], "x");
    assert_eq!(timeline["channels"][0]["loop"], true);
    assert_eq!(timeline["channels"][0]["keyframes"][0]["value"], 10.0);
    assert_eq!(timeline["channels"][0]["keyframes"][1]["time_ms"], 500);
    assert_eq!(timeline["channels"][0]["keyframes"][1]["value"], 40.0);
    assert_eq!(timeline["channels"][0]["keyframes"][2]["time_ms"], 1000);
    assert_eq!(timeline["channels"][0]["keyframes"][2]["value"], 10.0);

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let peak = document.snapshot_at_with_property_resolver(500, |_| None);
    assert_eq!(peak.layers[0].transform.x, 40.0);
    assert_eq!(peak.layers[0].transform.y, 10.0);
    let returning = document.snapshot_at_with_property_resolver(750, |_| None);
    assert_eq!(returning.layers[0].transform.x, 25.0);
    assert_eq!(returning.layers[0].transform.y, 15.0);

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"scene-we-embedded-property-timeline".to_owned())
    );
}

#[test]
fn converts_wallpaper_engine_scene_explicit_keyframes_to_gscene_timelines() {
    let source = TestDir::new("we-scene-keyframe-source");
    let output = TestDir::new("we-scene-keyframe-output");
    output.remove();
    source.write_file(
        "scene.json",
        r##"{
              "objects": [
                {
                  "id": 50,
                  "shape": "rectangle",
                  "backgroundcolor": "#203040",
                  "size": [320, 180, 0],
                  "timeline": [
                    {
                      "property": "scale",
                      "keyframes": [
                        { "time_ms": 0, "value": [1, 1, 0] },
                        { "time_ms": 1000, "value": [2, 3, 0] }
                      ]
                    }
                  ]
                }
              ],
              "timelines": [
                {
                  "name": "panel-motion",
                  "target": 50,
                  "channels": [
                    {
                      "property": "origin",
                      "keyframes": [
                        { "time_ms": 0, "value": [0, 0, 0] },
                        { "time_ms": 1000, "value": [100, 50, 0] }
                      ]
                    },
                    {
                      "property": "alpha",
                      "keyframes": [
                        { "time_ms": 0, "value": 0.25 },
                        { "time_ms": 1000, "value": 0.75 }
                      ]
                    }
                  ]
                }
              ]
            }"##,
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Keyframe Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(scene["timelines"].as_array().unwrap().len(), 2);
    assert_eq!(
        scene["timelines"][0]["target_node"],
        scene["nodes"][0]["id"]
    );
    assert_eq!(scene["timelines"][0]["channels"][0]["property"], "scale-x");
    assert_eq!(scene["timelines"][0]["channels"][1]["property"], "scale-y");
    assert_eq!(scene["timelines"][1]["id"], "timeline-2-panel-motion");
    assert_eq!(
        scene["timelines"][1]["target_node"],
        scene["nodes"][0]["id"]
    );
    assert_eq!(scene["timelines"][1]["channels"][0]["property"], "x");
    assert_eq!(scene["timelines"][1]["channels"][1]["property"], "y");
    assert_eq!(scene["timelines"][1]["channels"][2]["property"], "opacity");

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let snapshot = document.snapshot_at_with_property_resolver(500, |_| None);
    assert_eq!(snapshot.layers.len(), 1);
    assert_eq!(
        snapshot.layers[0].kind,
        crate::core::SceneNodeKind::Rectangle
    );
    assert_eq!(snapshot.layers[0].transform.x, 50.0);
    assert_eq!(snapshot.layers[0].transform.y, 25.0);
    assert_eq!(snapshot.layers[0].transform.scale_x, 1.5);
    assert_eq!(snapshot.layers[0].transform.scale_y, 2.0);
    assert_eq!(snapshot.layers[0].opacity, 0.5);

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"scene-keyframe-timeline".to_owned())
    );
}

#[test]
fn lowers_wallpaper_engine_animation_layer_keyframes_to_gscene_timelines() {
    let source = TestDir::new("we-scene-animation-layer-source");
    let output = TestDir::new("we-scene-animation-layer-output");
    output.remove();
    source.write_file(
        "scene.json",
        r##"{
              "objects": [
                {
                  "id": 70,
                  "type": "rectangle",
                  "name": "Animated Panel",
                  "width": 100,
                  "height": 60,
                  "color": "#203040",
                  "animationlayers": [
                    {
                      "name": "slide",
                      "rate": 2.0,
                      "property": "origin",
                      "keyframes": [
                        { "time_ms": 0, "value": [0, 0, 0] },
                        { "time_ms": 1000, "value": [120, 40, 0] }
                      ]
                    }
                  ]
                }
              ]
            }"##,
    );
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "scene",
              "title": "Animation Layer Scene",
              "file": "scene.json"
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(scene["timelines"].as_array().unwrap().len(), 1);
    assert_eq!(scene["timelines"][0]["id"], "timeline-1-slide");
    assert_eq!(scene["timelines"][0]["channels"][0]["property"], "x");
    assert_eq!(
        scene["timelines"][0]["channels"][0]["keyframes"][1]["time_ms"],
        500
    );
    assert_eq!(scene["timelines"][0]["channels"][1]["property"], "y");
    assert!(
        !scene["unsupported_features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| feature["feature"] == "we-animation-layer-blending")
    );

    let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
    document.validate().unwrap();
    let snapshot = document.snapshot_at_with_property_resolver(250, |_| None);
    assert_eq!(snapshot.layers[0].transform.x, 60.0);
    assert_eq!(snapshot.layers[0].transform.y, 20.0);

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"scene-we-animation-layer-timeline".to_owned())
    );
    assert!(
        report
            .converted_features
            .contains(&"scene-we-animation-layer-rate-time-scale".to_owned())
    );
    assert!(
        report
            .full_scene
            .as_ref()
            .unwrap()
            .completed_boundaries
            .contains(&"wallpaper-engine-animation-layer-rate-time-scale".to_owned())
    );
    assert!(
        !report
            .unsupported_features
            .contains(&"we-animation-layer-blending".to_owned())
    );
}

#[test]
fn converts_playlist_project_with_image_and_video_items() {
    let source = TestDir::new("we-playlist-source");
    let output = TestDir::new("we-playlist-output");
    output.remove();
    source.write_file("day.jpg", "not real jpg");
    source.write_file("night.mp4", "not real mp4");
    source.write_file("preview.jpg", "not real preview");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "playlist",
              "title": "Playlist Example",
              "preview": "preview.jpg",
              "items": [
                {
                  "title": "Day Image",
                  "type": "image",
                  "file": "day.jpg",
                  "weight": 3
                },
                {
                  "title": "Night Video",
                  "type": "video",
                  "file": "night.mp4"
                }
              ]
            }"#,
    );

    let summary = convert_project(source.path(), output.path()).unwrap();
    assert_eq!(summary.source_type, "playlist");
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
            .unwrap();
    assert_eq!(manifest["kind"], "playlist");
    assert_eq!(manifest["entry"]["type"], "playlist");
    assert_eq!(manifest["entry"]["selection"], "first-match");
    let items = manifest["entry"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["id"], "item-0-day-image");
    assert_eq!(items[0]["weight"], 3);
    assert_eq!(items[0]["entry"]["type"], "static-image");
    assert_eq!(items[0]["entry"]["source"], "assets/playlist-0.jpg");
    assert_eq!(items[1]["id"], "item-1-night-video");
    assert_eq!(items[1]["entry"]["type"], "video");
    assert_eq!(items[1]["entry"]["source"], "assets/playlist-1.mp4");
    assert_eq!(items[1]["entry"]["muted"], true);
    assert!(output.path().join("assets/playlist-0.jpg").exists());
    assert!(output.path().join("assets/playlist-1.mp4").exists());
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(report.detected_features.contains(&"playlist".to_owned()));
    assert!(report.converted_features.contains(&"playlist".to_owned()));
    assert!(
        report
            .converted_features
            .contains(&"playlist-item:image".to_owned())
    );
    assert!(
        report
            .converted_features
            .contains(&"playlist-item:video".to_owned())
    );
}

#[test]
fn converts_playlist_project_with_web_and_scene_items() {
    let source = TestDir::new("we-playlist-web-scene-source");
    let output = TestDir::new("we-playlist-web-scene-output");
    output.remove();
    source.write_file(
        "web/index.html",
        "<!doctype html><script src=\"app.js\"></script>",
    );
    source.write_file("web/app.js", "window.wallpaperPropertyListener = {};");
    source.write_file(
        "scene.json",
        r#"{"objects":[{"type":"image","path":"background.png"}]}"#,
    );
    source.write_file("background.png", "not real png");
    source.write_file("preview.jpg", "not real preview");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "playlist",
              "title": "Mixed Playlist",
              "preview": "preview.jpg",
              "items": [
                {
                  "title": "Web Item",
                  "type": "web",
                  "file": "web/index.html"
                },
                {
                  "title": "Scene Item",
                  "type": "scene",
                  "file": "scene.json"
                }
              ]
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
            .unwrap();
    let items = manifest["entry"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["id"], "item-0-web-item");
    assert_eq!(items[0]["entry"]["type"], "web");
    assert_eq!(items[0]["entry"]["root"], "assets/playlist-0-web");
    assert_eq!(items[0]["entry"]["index"], "web/index.html");
    assert_eq!(items[0]["entry"]["fallback"], "previews/poster.jpg");
    assert_eq!(items[1]["id"], "item-1-scene-item");
    assert_eq!(items[1]["entry"]["type"], "scene");
    assert_eq!(
        items[1]["entry"]["source"],
        "assets/playlist-1-scene.gscene.json"
    );
    assert!(items[1]["entry"].get("max_fps").is_none());
    assert!(items[1]["entry"].get("fallback").is_none());
    assert!(
        output
            .path()
            .join("assets/playlist-0-web/web/index.html")
            .exists()
    );
    assert!(
        output
            .path()
            .join("assets/playlist-0-web/gilder-bridge.js")
            .exists()
    );
    assert!(
        output
            .path()
            .join("metadata/playlist-1-source-scene.json")
            .exists()
    );
    let scene: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("assets/playlist-1-scene.gscene.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(scene["nodes"][0]["type"], "image");
    assert_eq!(
        scene["resources"][0]["source"],
        "assets/scene-resources/playlist-1-scene/resource-1-background.png"
    );
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .converted_features
            .contains(&"playlist-item:web".to_owned())
    );
    assert!(
        report
            .converted_features
            .contains(&"playlist-item:scene".to_owned())
    );
    assert!(
        report
            .unsupported_features
            .contains(&"web-runtime".to_owned())
    );
    assert!(
        !report
            .unsupported_features
            .contains(&"scene-runtime".to_owned())
    );
}

#[test]
fn converts_playlist_project_with_shader_item() {
    let source = TestDir::new("we-playlist-shader-source");
    let output = TestDir::new("we-playlist-shader-output");
    output.remove();
    source.write_file("waves.wgsl", "@fragment fn main() {}");
    source.write_file("preview.jpg", "not real preview");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "playlist",
              "title": "Shader Playlist",
              "preview": "preview.jpg",
              "items": [
                {
                  "title": "Waves",
                  "type": "shader",
                  "file": "waves.wgsl"
                }
              ]
            }"#,
    );

    convert_project(source.path(), output.path()).unwrap();
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
            .unwrap();
    let items = manifest["entry"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], "item-0-waves");
    assert_eq!(items[0]["entry"]["type"], "shader");
    assert_eq!(items[0]["entry"]["source"], "shaders/playlist-0.wgsl");
    assert_eq!(items[0]["entry"]["fallback"], "previews/poster.jpg");
    assert_eq!(items[0]["entry"]["language"], "wgsl");
    assert!(output.path().join("shaders/playlist-0.wgsl").exists());

    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .detected_features
            .contains(&"playlist-item:shader".to_owned())
    );
    assert!(
        report
            .converted_features
            .contains(&"playlist-item:shader".to_owned())
    );
    assert!(
        report
            .unsupported_features
            .contains(&"shader-runtime".to_owned())
    );
}

#[test]
fn reports_unsupported_playlist_items_when_none_can_convert() {
    let source = TestDir::new("we-playlist-unsupported-source");
    let output = TestDir::new("we-playlist-unsupported-output");
    output.remove();
    source.write_file("app.exe", "");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "playlist",
              "title": "Executable Playlist",
              "items": [
                {
                  "title": "Executable Item",
                  "type": "application",
                  "file": "app.exe"
                }
              ]
            }"#,
    );

    let error = convert_project(source.path(), output.path()).unwrap_err();
    assert!(matches!(error, ConversionError::MissingEntry(_)));
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .unsupported_features
            .contains(&"playlist-item:application".to_owned())
    );
    assert!(
        report
            .errors
            .contains(&"Playlist did not contain convertible items.".to_owned())
    );
}

#[test]
fn rejects_application_project_and_writes_report() {
    let source = TestDir::new("we-app-source");
    let output = TestDir::new("we-app-output");
    output.remove();
    source.write_file("app.exe", "");
    source.write_file(
        PROJECT_FILE,
        r#"{
              "type": "application",
              "title": "Executable Example",
              "file": "app.exe"
            }"#,
    );

    let error = convert_project(source.path(), output.path()).unwrap_err();
    assert!(matches!(error, ConversionError::UnsupportedType { .. }));
    let report: ConversionReport = serde_json::from_str(
        &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
    )
    .unwrap();
    assert!(
        report
            .unsupported_features
            .contains(&"executable-application".to_owned())
    );
}

#[test]
fn scene_tex_encoder_writes_native_bc7_gtex_payload() {
    let output = TestDir::new("scene-bc7-gtex-output");
    let image = SceneWeTexImage {
        width: 4,
        height: 4,
        rgba: vec![0; tex::rgba_len(4, 4).unwrap()],
        r8: None,
    };
    let path = output.path().join("transparent.gtex");

    gtex::write_bc7_gtex(&path, &image).unwrap();
    let bytes = fs::read(&path).unwrap();

    assert_eq!(&bytes[0..8], gtex::GILDER_SCENE_TEXTURE_MAGIC);
    assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 4);
    assert_eq!(u32::from_le_bytes(bytes[12..16].try_into().unwrap()), 4);
    assert_eq!(
        u32::from_le_bytes(bytes[16..20].try_into().unwrap()),
        gtex::GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK
    );
    assert_eq!(u64::from_le_bytes(bytes[24..32].try_into().unwrap()), 16);
    assert_eq!(bytes.len(), 48);
    assert_eq!(bytes[32], 0x40);
}

#[test]
fn scene_tex_encoder_writes_native_r8_gtex_payload() {
    let output = TestDir::new("scene-r8-gtex-output");
    let path = output.path().join("mask.gtex");

    gtex::write_r8_gtex(&path, 2, 2, &[64, 0, 255, 128]).unwrap();
    let bytes = fs::read(&path).unwrap();

    assert_eq!(&bytes[0..8], gtex::GILDER_SCENE_TEXTURE_MAGIC);
    assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 2);
    assert_eq!(u32::from_le_bytes(bytes[12..16].try_into().unwrap()), 2);
    assert_eq!(
        u32::from_le_bytes(bytes[16..20].try_into().unwrap()),
        gtex::GILDER_SCENE_TEXTURE_FORMAT_R8_UNORM
    );
    assert_eq!(u64::from_le_bytes(bytes[24..32].try_into().unwrap()), 4);
    assert_eq!(&bytes[32..36], &[64, 0, 255, 128]);
}

#[test]
fn converts_png_to_native_bc7_gtex_offline() {
    let output = TestDir::new("png-bc7-gtex-output");
    let png_path = output.path().join("source.png");
    let gtex_path = output.path().join("source.gtex");
    let rgba = [
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
    ];
    {
        let file = fs::File::create(&png_path).unwrap();
        let writer = std::io::BufWriter::new(file);
        let mut encoder = png::Encoder::new(writer, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&rgba).unwrap();
    }

    let summary = convert_png_to_native_gtex(&png_path, &gtex_path).unwrap();
    let bytes = fs::read(&gtex_path).unwrap();

    assert_eq!(summary.width, 2);
    assert_eq!(summary.height, 2);
    assert_eq!(summary.format, "BC7_UNORM_BLOCK");
    assert_eq!(summary.payload_bytes, 16);
    assert_eq!(&bytes[0..8], gtex::GILDER_SCENE_TEXTURE_MAGIC);
    assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 2);
    assert_eq!(u32::from_le_bytes(bytes[12..16].try_into().unwrap()), 2);
    assert_eq!(
        u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
        summary.payload_bytes
    );
    assert_eq!(bytes.len(), 48);
}

#[test]
fn png_to_native_gtex_uses_bottom_first_texture_rows() {
    let top_left = [255, 0, 0, 255];
    let top_right = [0, 255, 0, 255];
    let bottom_left = [0, 0, 255, 255];
    let bottom_right = [255, 255, 255, 255];
    let mut rgba = [top_left, top_right, bottom_left, bottom_right].concat();

    gtex::flip_rgba_rows_vertically(&mut rgba, 2, 2).unwrap();

    assert_eq!(&rgba[0..4], &bottom_left);
    assert_eq!(&rgba[4..8], &bottom_right);
    assert_eq!(&rgba[8..12], &top_left);
    assert_eq!(&rgba[12..16], &top_right);
}

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(prefix: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("gilder-{prefix}-{}-{nonce}", std::process::id()));
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

    fn write_bytes(&self, relative_path: &str, contents: &[u8]) {
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

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}

fn write_executable_script(path: &Path, contents: &str) {
    use std::io::Write;

    let temporary_path = path.with_extension("tmp");
    if let Some(parent) = temporary_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    {
        let mut file = fs::File::create(&temporary_path).unwrap();
        file.write_all(contents.as_bytes()).unwrap();
        file.sync_all().unwrap();
    }
    make_executable(&temporary_path);
    fs::rename(&temporary_path, path).unwrap();
}

fn test_png_rgba(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let writer = std::io::Cursor::new(&mut bytes);
        let mut encoder = png::Encoder::new(writer, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(rgba).unwrap();
    }
    bytes
}

fn test_we_tex_rgba(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    assert_eq!(rgba.len(), tex::rgba_len(width, height).unwrap());
    let compressed = test_lz4_literal_block(rgba);
    let mut bytes = vec![0; 91];
    bytes[0..8].copy_from_slice(b"TEXV0005");
    bytes[9..17].copy_from_slice(b"TEXI0001");
    test_write_u32_le(&mut bytes, 22, 7);
    test_write_u32_le(&mut bytes, 26, width);
    test_write_u32_le(&mut bytes, 30, height);
    test_write_u32_le(&mut bytes, 34, width);
    test_write_u32_le(&mut bytes, 38, height);
    bytes[46..54].copy_from_slice(b"TEXB0004");
    test_write_u32_le(&mut bytes, 55, 1);
    test_write_u32_le(&mut bytes, 67, 1);
    test_write_u32_le(&mut bytes, 71, width);
    test_write_u32_le(&mut bytes, 75, height);
    test_write_u32_le(&mut bytes, 79, 1);
    test_write_u32_le(&mut bytes, 83, u32::try_from(rgba.len()).unwrap());
    test_write_u32_le(&mut bytes, 87, u32::try_from(compressed.len()).unwrap());
    bytes.extend_from_slice(&compressed);
    bytes
}

fn test_we_tex_embedded_png(width: u32, height: u32, png: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; 91];
    bytes[0..8].copy_from_slice(b"TEXV0005");
    bytes[9..17].copy_from_slice(b"TEXI0001");
    test_write_u32_le(&mut bytes, 18, 0);
    test_write_u32_le(&mut bytes, 22, 2);
    test_write_u32_le(&mut bytes, 26, width);
    test_write_u32_le(&mut bytes, 30, height);
    test_write_u32_le(&mut bytes, 34, width);
    test_write_u32_le(&mut bytes, 38, height);
    bytes[46..54].copy_from_slice(b"TEXB0004");
    test_write_u32_le(&mut bytes, 55, 1);
    test_write_u32_le(&mut bytes, 59, 13);
    test_write_u32_le(&mut bytes, 63, 0);
    test_write_u32_le(&mut bytes, 67, 1);
    test_write_u32_le(&mut bytes, 71, width);
    test_write_u32_le(&mut bytes, 75, height);
    test_write_u32_le(&mut bytes, 79, 0);
    test_write_u32_le(&mut bytes, 83, 0);
    test_write_u32_le(&mut bytes, 87, u32::try_from(png.len()).unwrap());
    bytes.extend_from_slice(png);
    bytes
}

fn test_we_texb0003_embedded_png(width: u32, height: u32, png: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; 87];
    bytes[0..8].copy_from_slice(b"TEXV0005");
    bytes[9..17].copy_from_slice(b"TEXI0001");
    test_write_u32_le(&mut bytes, 18, 0);
    test_write_u32_le(&mut bytes, 22, 2);
    test_write_u32_le(&mut bytes, 26, width);
    test_write_u32_le(&mut bytes, 30, height);
    test_write_u32_le(&mut bytes, 34, width);
    test_write_u32_le(&mut bytes, 38, height);
    bytes[46..54].copy_from_slice(b"TEXB0003");
    test_write_u32_le(&mut bytes, 55, 1);
    test_write_u32_le(&mut bytes, 59, 13);
    test_write_u32_le(&mut bytes, 63, 1);
    test_write_u32_le(&mut bytes, 67, width);
    test_write_u32_le(&mut bytes, 71, height);
    test_write_u32_le(&mut bytes, 75, 0);
    test_write_u32_le(&mut bytes, 79, 0);
    test_write_u32_le(&mut bytes, 83, u32::try_from(png.len()).unwrap());
    bytes.extend_from_slice(png);
    bytes
}

fn test_we_tex_image_payload(
    width: u32,
    height: u32,
    we_format: u32,
    payload: &[u8],
    compression: u32,
) -> Vec<u8> {
    let encoded = match compression {
        0 => payload.to_vec(),
        1 => test_lz4_literal_block(payload),
        other => panic!("unsupported test compression {other}"),
    };
    let mut bytes = vec![0; 91];
    bytes[0..8].copy_from_slice(b"TEXV0005");
    bytes[9..17].copy_from_slice(b"TEXI0001");
    test_write_u32_le(&mut bytes, 18, we_format);
    test_write_u32_le(&mut bytes, 26, width);
    test_write_u32_le(&mut bytes, 30, height);
    test_write_u32_le(&mut bytes, 34, width);
    test_write_u32_le(&mut bytes, 38, height);
    bytes[46..54].copy_from_slice(b"TEXB0004");
    test_write_u32_le(&mut bytes, 55, 1);
    test_write_u32_le(&mut bytes, 67, 1);
    test_write_u32_le(&mut bytes, 71, width);
    test_write_u32_le(&mut bytes, 75, height);
    test_write_u32_le(&mut bytes, 79, compression);
    test_write_u32_le(&mut bytes, 83, u32::try_from(payload.len()).unwrap());
    test_write_u32_le(&mut bytes, 87, u32::try_from(encoded.len()).unwrap());
    bytes.extend_from_slice(&encoded);
    bytes
}

fn test_we_tex_block_compressed(
    width: u32,
    height: u32,
    we_format: u32,
    payload: &[u8],
    compression: u32,
) -> Vec<u8> {
    let encoded = match compression {
        0 => payload.to_vec(),
        1 => test_lz4_literal_block(payload),
        other => panic!("unsupported test compression {other}"),
    };
    let mut bytes = vec![0; 91];
    bytes[0..8].copy_from_slice(b"TEXV0005");
    bytes[9..17].copy_from_slice(b"TEXI0001");
    test_write_u32_le(&mut bytes, 18, we_format);
    test_write_u32_le(&mut bytes, 26, width);
    test_write_u32_le(&mut bytes, 30, height);
    test_write_u32_le(&mut bytes, 34, width);
    test_write_u32_le(&mut bytes, 38, height);
    bytes[46..54].copy_from_slice(b"TEXB0004");
    test_write_u32_le(&mut bytes, 55, 1);
    test_write_u32_le(&mut bytes, 67, 1);
    test_write_u32_le(&mut bytes, 71, width);
    test_write_u32_le(&mut bytes, 75, height);
    test_write_u32_le(&mut bytes, 79, compression);
    test_write_u32_le(&mut bytes, 83, u32::try_from(payload.len()).unwrap());
    test_write_u32_le(&mut bytes, 87, u32::try_from(encoded.len()).unwrap());
    bytes.extend_from_slice(&encoded);
    bytes
}

fn test_we_tex_video(width: u32, height: u32, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; 91];
    bytes[0..8].copy_from_slice(b"TEXV0005");
    bytes[9..17].copy_from_slice(b"TEXI0001");
    test_write_u32_le(&mut bytes, 22, 7);
    test_write_u32_le(&mut bytes, 26, width);
    test_write_u32_le(&mut bytes, 30, height);
    test_write_u32_le(&mut bytes, 34, width);
    test_write_u32_le(&mut bytes, 38, height);
    bytes[46..54].copy_from_slice(b"TEXB0004");
    test_write_u32_le(&mut bytes, 55, 1);
    test_write_u32_le(&mut bytes, 67, 1);
    test_write_u32_le(&mut bytes, 71, width);
    test_write_u32_le(&mut bytes, 75, height);
    test_write_u32_le(&mut bytes, 79, 1);
    test_write_u32_le(&mut bytes, 83, 0);
    test_write_u32_le(&mut bytes, 87, u32::try_from(payload.len()).unwrap());
    bytes.extend_from_slice(payload);
    bytes
}

fn test_we_mdl_with_attachment(
    attachment_name: &str,
    attachment_bone: u16,
    root_tp: (f32, f32),
    attachment_offset: (f32, f32),
) -> Vec<u8> {
    test_we_mdl_with_attachment_and_child_translation(
        attachment_name,
        attachment_bone,
        root_tp,
        (0.0, 0.0),
        attachment_offset,
    )
}

fn test_we_mdl_with_attachment_and_child_translation(
    attachment_name: &str,
    attachment_bone: u16,
    root_tp: (f32, f32),
    child_translation: (f32, f32),
    attachment_offset: (f32, f32),
) -> Vec<u8> {
    let mut bytes = b"MDLV0023\0".to_vec();
    let mdls_offset = bytes.len();
    bytes.extend_from_slice(b"MDLS0004");
    bytes.push(0);
    let mdls_end_offset = bytes.len();
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&2u32.to_le_bytes());
    test_push_mdl_bone(&mut bytes, 0, -1, Some(root_tp));
    test_push_mdl_bone_with_translation(&mut bytes, 1, 0, None, child_translation);
    let mdls_end = u32::try_from(bytes.len()).unwrap();
    bytes[mdls_end_offset..mdls_end_offset + 4].copy_from_slice(&mdls_end.to_le_bytes());

    let mdat_offset = bytes.len();
    bytes.extend_from_slice(b"MDAT0001");
    bytes.push(0);
    let mdat_end_offset = bytes.len();
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&attachment_bone.to_le_bytes());
    bytes.extend_from_slice(attachment_name.as_bytes());
    bytes.push(0);
    let mut attachment_matrix = test_mdl_identity_matrix();
    attachment_matrix[12] = attachment_offset.0;
    attachment_matrix[13] = attachment_offset.1;
    test_push_mdl_matrix(&mut bytes, attachment_matrix);
    let mdat_end = u32::try_from(bytes.len()).unwrap();
    bytes[mdat_end_offset..mdat_end_offset + 4].copy_from_slice(&mdat_end.to_le_bytes());
    assert!(bytes[mdls_offset..].starts_with(b"MDLS"));
    assert!(bytes[mdat_offset..].starts_with(b"MDAT"));
    bytes
}

fn test_we_mdl_with_mesh_bounds(vertices: &[(f32, f32, f32, f32)]) -> Vec<u8> {
    let mut bytes = b"MDLV0023\0".to_vec();
    test_push_mdl_mesh_block(&mut bytes, vertices);
    let mdls_offset = bytes.len();
    bytes.extend_from_slice(b"MDLS0004");
    bytes.push(0);
    let mdls_end_offset = bytes.len();
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    test_push_mdl_bone(&mut bytes, 0, -1, Some((100.0, 50.0)));
    let mdls_end = u32::try_from(bytes.len()).unwrap();
    bytes[mdls_end_offset..mdls_end_offset + 4].copy_from_slice(&mdls_end.to_le_bytes());
    assert!(bytes[mdls_offset..].starts_with(b"MDLS"));
    bytes
}

fn test_we_mdl_with_skinned_animation() -> Vec<u8> {
    let mut bytes = b"MDLV0023\0".to_vec();
    test_push_mdl_mesh_block_with_skin(
        &mut bytes,
        &[
            (20.0, 0.0, 0.0, 0.0, 1),
            (20.0, 1.0, 0.0, 1.0, 1),
            (21.0, 0.0, 1.0, 0.0, 1),
        ],
    );
    let mdls_offset = bytes.len();
    bytes.extend_from_slice(b"MDLS0004");
    bytes.push(0);
    let mdls_end_offset = bytes.len();
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&2u32.to_le_bytes());
    test_push_mdl_bone_with_translation(&mut bytes, 0, -1, Some((16.0, 16.0)), (0.0, 0.0));
    test_push_mdl_bone_with_translation(&mut bytes, 1, 0, None, (10.0, 0.0));
    let mdls_end = u32::try_from(bytes.len()).unwrap();
    bytes[mdls_end_offset..mdls_end_offset + 4].copy_from_slice(&mdls_end.to_le_bytes());

    let mdla_offset = bytes.len();
    bytes.extend_from_slice(b"MDLA0006");
    bytes.push(0);
    let mdla_end_offset = bytes.len();
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&7u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(b"turn\0");
    bytes.extend_from_slice(b"once\0");
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&2u32.to_le_bytes());
    test_push_mdl_animation_bone_frames(
        &mut bytes,
        [
            test_puppet_frame((0.0, 0.0, 0.0), 0.0),
            test_puppet_frame((0.0, 0.0, 0.0), 0.0),
        ],
    );
    test_push_mdl_animation_bone_frames(
        &mut bytes,
        [
            test_puppet_frame((10.0, 0.0, 0.0), 0.0),
            test_puppet_frame((10.0, 0.0, 0.0), std::f32::consts::FRAC_PI_2),
        ],
    );
    let mdla_end = u32::try_from(bytes.len()).unwrap();
    bytes[mdla_end_offset..mdla_end_offset + 4].copy_from_slice(&mdla_end.to_le_bytes());
    assert!(bytes[mdls_offset..].starts_with(b"MDLS"));
    assert!(bytes[mdla_offset..].starts_with(b"MDLA"));
    bytes
}

fn test_we_mdl_with_skinned_animation_and_attachment(
    attachment_name: &str,
    attachment_bone: u16,
    attachment_offset: (f32, f32),
) -> Vec<u8> {
    let mut bytes = test_we_mdl_with_skinned_animation();
    let mdat_offset = bytes.len();
    bytes.extend_from_slice(b"MDAT0001");
    bytes.push(0);
    let mdat_end_offset = bytes.len();
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&attachment_bone.to_le_bytes());
    bytes.extend_from_slice(attachment_name.as_bytes());
    bytes.push(0);
    let mut attachment_matrix = test_mdl_identity_matrix();
    attachment_matrix[12] = attachment_offset.0;
    attachment_matrix[13] = attachment_offset.1;
    test_push_mdl_matrix(&mut bytes, attachment_matrix);
    let mdat_end = u32::try_from(bytes.len()).unwrap();
    bytes[mdat_end_offset..mdat_end_offset + 4].copy_from_slice(&mdat_end.to_le_bytes());
    assert!(bytes[mdat_offset..].starts_with(b"MDAT"));
    bytes
}

fn test_push_mdl_mesh_block(bytes: &mut Vec<u8>, vertices: &[(f32, f32, f32, f32)]) {
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&u32::try_from(vertices.len() * 80).unwrap().to_le_bytes());
    for (x, y, u, v) in vertices {
        let mut vertex = [0u8; 80];
        vertex[0..4].copy_from_slice(&x.to_le_bytes());
        vertex[4..8].copy_from_slice(&y.to_le_bytes());
        vertex[8..12].copy_from_slice(&0.0f32.to_le_bytes());
        vertex[72..76].copy_from_slice(&u.to_le_bytes());
        vertex[76..80].copy_from_slice(&v.to_le_bytes());
        bytes.extend_from_slice(&vertex);
    }
    bytes.extend_from_slice(&6u32.to_le_bytes());
    for index in [0u16, 1, 2] {
        bytes.extend_from_slice(&index.to_le_bytes());
    }
}

fn test_push_mdl_mesh_block_with_skin(bytes: &mut Vec<u8>, vertices: &[(f32, f32, f32, f32, u32)]) {
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&u32::try_from(vertices.len() * 80).unwrap().to_le_bytes());
    for (x, y, u, v, bone) in vertices {
        let mut vertex = [0u8; 80];
        vertex[0..4].copy_from_slice(&x.to_le_bytes());
        vertex[4..8].copy_from_slice(&y.to_le_bytes());
        vertex[8..12].copy_from_slice(&0.0f32.to_le_bytes());
        vertex[40..44].copy_from_slice(&bone.to_le_bytes());
        vertex[56..60].copy_from_slice(&1.0f32.to_le_bytes());
        vertex[72..76].copy_from_slice(&u.to_le_bytes());
        vertex[76..80].copy_from_slice(&v.to_le_bytes());
        bytes.extend_from_slice(&vertex);
    }
    bytes.extend_from_slice(&6u32.to_le_bytes());
    for index in [0u16, 1, 2] {
        bytes.extend_from_slice(&index.to_le_bytes());
    }
}

fn test_push_mdl_animation_bone_frames(bytes: &mut Vec<u8>, frames: [[f32; 9]; 2]) {
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&72u32.to_le_bytes());
    for frame in frames {
        for value in frame {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
}

fn test_puppet_frame(translation: (f32, f32, f32), rotation_z: f32) -> [f32; 9] {
    [
        translation.0,
        translation.1,
        translation.2,
        0.0,
        0.0,
        rotation_z,
        1.0,
        1.0,
        1.0,
    ]
}

fn test_push_mdl_bone(bytes: &mut Vec<u8>, index: u32, parent: i32, tp: Option<(f32, f32)>) {
    test_push_mdl_bone_with_translation(bytes, index, parent, tp, (0.0, 0.0));
}

fn test_push_mdl_bone_with_translation(
    bytes: &mut Vec<u8>,
    index: u32,
    parent: i32,
    tp: Option<(f32, f32)>,
    translation: (f32, f32),
) {
    bytes.extend_from_slice(&index.to_le_bytes());
    bytes.push(0);
    bytes.extend_from_slice(&parent.to_le_bytes());
    bytes.extend_from_slice(&64u32.to_le_bytes());
    let mut matrix = test_mdl_identity_matrix();
    matrix[12] = translation.0;
    matrix[13] = translation.1;
    test_push_mdl_matrix(bytes, matrix);
    if let Some((x, y)) = tp {
        bytes.extend_from_slice(format!(r#"{{"tp":"{x:.5} {y:.5} 0.00000"}}"#).as_bytes());
    }
    bytes.push(0);
}

fn test_mdl_identity_matrix() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn test_push_mdl_matrix(bytes: &mut Vec<u8>, matrix: [f32; 16]) {
    for value in matrix {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}

fn test_scene_pkg(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let version = b"PKGV0023";
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(version.len() as u32).to_le_bytes());
    bytes.extend_from_slice(version);
    bytes.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    let mut payload = Vec::new();
    for (path, contents) in entries {
        bytes.extend_from_slice(&(path.len() as u32).to_le_bytes());
        bytes.extend_from_slice(path.as_bytes());
        bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(contents.len() as u32).to_le_bytes());
        payload.extend_from_slice(contents);
    }
    bytes.extend_from_slice(&payload);
    bytes
}

fn test_lz4_literal_block(bytes: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(bytes.len() + 8);
    let literal_len = bytes.len();
    if literal_len < 15 {
        output.push((literal_len as u8) << 4);
    } else {
        output.push(0xf0);
        let mut remaining = literal_len - 15;
        while remaining >= 255 {
            output.push(255);
            remaining -= 255;
        }
        output.push(remaining as u8);
    }
    output.extend_from_slice(bytes);
    output
}

fn test_write_u32_le(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
