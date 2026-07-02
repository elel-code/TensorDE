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
