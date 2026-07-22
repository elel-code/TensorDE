use super::*;
use crate::engine::scene::SceneUserPropertyPredicate;

#[test]
fn visible_user_property_binding_reaches_strict_typed_binary() {
    let root = write_visible_user_binding_fixture(
        "valid",
        serde_json::json!({"type": "bool", "value": false}),
        serde_json::json!({"user": "rain", "value": false}),
    );
    let ir = ingest_wallpaper_engine_project(&root).expect("visible user binding IR");

    assert!(!ir.objects[0].visible);
    assert_eq!(ir.user_property_bindings.len(), 1);
    assert_eq!(ir.user_property_bindings[0].object, 0);
    assert_eq!(ir.user_property_bindings[0].property, "rain");
    assert_eq!(
        ir.user_property_bindings[0].predicate,
        WeIrUserPropertyPredicate::BooleanEquals(false)
    );
    assert_eq!(
        ir.user_property_bindings[0].target,
        crate::engine::scene::SceneUserPropertyTarget::Visible
    );

    let document = crate::convert::we_ingest::lower::lower_ir_to_scene_binary(&ir)
        .expect("lower user binding");
    assert_eq!(document.user_property_bindings.len(), 1);
    assert_eq!(
        document.user_property_bindings[0].predicate,
        SceneUserPropertyPredicate::BooleanEquals(false)
    );
    crate::engine::scene::SceneStorage::from_document(document)
        .expect("validate user binding storage");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn combo_visibility_condition_reaches_strict_typed_binary() {
    let root = write_visible_user_binding_fixture(
        "combo-condition",
        serde_json::json!({
            "type": "combo",
            "value": "1",
            "options": [
                {"label": "First", "value": "1"},
                {"label": "Second", "value": "2"}
            ]
        }),
        serde_json::json!({
            "user": {"condition": "2", "name": "rain"},
            "value": false
        }),
    );
    let ir = ingest_wallpaper_engine_project(&root).expect("combo visible binding IR");

    assert!(!ir.objects[0].visible);
    assert_eq!(
        ir.user_property_bindings[0].predicate,
        WeIrUserPropertyPredicate::StringEquals("2".to_owned())
    );
    let document = crate::convert::we_ingest::lower::lower_ir_to_scene_binary(&ir)
        .expect("lower combo binding");
    let condition = match document.user_property_bindings[0].predicate {
        SceneUserPropertyPredicate::StringEquals(condition) => condition,
        predicate => panic!("unexpected predicate {predicate:?}"),
    };
    assert_eq!(document.strings[condition.0 as usize], "2");
    crate::engine::scene::SceneStorage::from_document(document)
        .expect("validate combo binding storage");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn visible_user_property_binding_rejects_wrong_schema_case_value_and_condition() {
    for (name, property, visibility) in [
        (
            "wrong-type",
            serde_json::json!({"type": "slider", "value": 0}),
            serde_json::json!({"user": "rain", "value": false}),
        ),
        (
            "wrong-case",
            serde_json::json!({"type": "bool", "value": false}),
            serde_json::json!({"user": "Rain", "value": false}),
        ),
        (
            "wrong-value",
            serde_json::json!({"type": "bool", "value": false}),
            serde_json::json!({"user": "rain", "value": 0}),
        ),
        (
            "condition",
            serde_json::json!({"type": "bool", "value": false}),
            serde_json::json!({
                "user": "rain",
                "value": false,
                "condition": "rain.value"
            }),
        ),
    ] {
        let root = write_visible_user_binding_fixture(name, property, visibility);
        assert!(ingest_wallpaper_engine_project(&root).is_err(), "{name}");
        let _ = fs::remove_dir_all(root);
    }
}

fn write_visible_user_binding_fixture(name: &str, property: Value, visibility: Value) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "gilder-we-visible-user-binding-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("models")).expect("models");
    fs::create_dir_all(root.join("materials")).expect("materials");
    fs::write(
        root.join("project.json"),
        serde_json::to_vec(&serde_json::json!({
            "type": "scene",
            "file": "scene.json",
            "general": {"properties": {"rain": property}}
        }))
        .expect("project JSON"),
    )
    .expect("project");
    fs::write(
        root.join("scene.json"),
        serde_json::to_vec(&serde_json::json!({
            "objects": [{
                "id": 7,
                "image": "models/layer.json",
                "visible": visibility
            }]
        }))
        .expect("scene JSON"),
    )
    .expect("scene");
    fs::write(
        root.join("models/layer.json"),
        r#"{"width":64,"height":64,"material":"materials/layer.json"}"#,
    )
    .expect("model");
    fs::write(
        root.join("materials/layer.json"),
        r#"{"passes":[{"shader":"genericimage4","blending":"translucent","textures":[null]}]}"#,
    )
    .expect("material");
    root
}
