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
