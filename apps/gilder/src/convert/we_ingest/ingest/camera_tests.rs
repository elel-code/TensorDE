use super::*;
use std::fs;

#[test]
fn camera_layer_is_typed_and_preserves_authored_zoom_binding() {
    let root = std::env::temp_dir().join(format!(
        "gilder-we-camera-layer-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("test root");
    fs::write(
        root.join("project.json"),
        r#"{"type":"scene","file":"scene.json"}"#,
    )
    .expect("project");
    fs::write(
        root.join("scene.json"),
        r#"{
            "general":{"orthogonalprojection":{"width":3440,"height":1440}},
            "objects":[{
                "id":420,
                "camera":"default",
                "origin":{
                    "value":"1708.52234 1053.12646 500",
                    "animation":{
                        "c0":[{"frame":0,"value":-1911.189},{"frame":180,"value":-1708.5223}],
                        "options":{"fps":30,"length":180,"mode":"single"},
                        "relative":true
                    }
                },
                "zoom":{
                    "value":1.0,
                    "animation":{
                        "c0":[{"frame":0,"value":2.55},{"frame":180,"value":1.0}],
                        "options":{"fps":30,"length":180,"mode":"single"}
                    }
                }
            }]
        }"#,
    )
    .expect("scene");

    let ir = ingest_wallpaper_engine_project(&root).expect("camera IR");

    assert_eq!(ir.objects.len(), 1);
    assert_eq!(ir.objects[0].kind, SceneAbiObjectKind::Camera);
    assert_eq!(ir.objects[0].camera_zoom, 1.0);
    assert!(ir.object_transform_tracks.iter().any(|track| {
        track.object == 0 && track.property == WeIrObjectTransformProperty::CameraZoom
    }));
    assert!(ir.unsupported.is_empty());

    let _ = fs::remove_dir_all(root);
}
