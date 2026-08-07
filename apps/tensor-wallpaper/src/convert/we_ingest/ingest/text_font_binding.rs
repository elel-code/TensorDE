//! Convert-time execution of SceneScript user-property font defaults.

use std::collections::BTreeMap;

use rquickjs::{CatchResultExt, Context, Function, Module, Runtime};
use serde_json::{Map, Value};

use super::super::script_analysis::analyze_scene_script;
use super::{json_value::value_u32, normalize_we_path};
use crate::engine::scene::script::standard_library;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct TextFontOverrides {
    pub(super) by_object: BTreeMap<u32, String>,
    pub(super) by_name: BTreeMap<String, String>,
}

const FONT_HOST: &str = r#"
globalThis.__tensor_wallpaperLayers = Object.create(null);
globalThis.__tensor_wallpaperObjectFonts = Object.create(null);
globalThis.__tensor_wallpaperCurrentLayerId = null;
globalThis.shared = Object.create(null);
globalThis.engine = {
    registerAsset(path) { return path; },
};
globalThis.Vec3 = class Vec3 {
    constructor(x = 0, y = 0, z = 0) {
        this.x = Number(x);
        this.y = Number(y);
        this.z = Number(z);
    }
};
globalThis.thisScene = {
    getLayer(name) {
        return __tensor_wallpaperLayers[name] ||= { font: null };
    },
    destroyLayer() {},
};
globalThis.__tensor_wallpaperSetLayer = (name, objectId) => {
    globalThis.__tensor_wallpaperCurrentLayerId = String(objectId);
    globalThis.thisLayer = thisScene.getLayer(name);
};
globalThis.__tensor_wallpaperRecordCurrentLayerFont = () => {
    if (globalThis.__tensor_wallpaperCurrentLayerId !== null
        && typeof globalThis.thisLayer?.font === 'string') {
        globalThis.__tensor_wallpaperObjectFonts[globalThis.__tensor_wallpaperCurrentLayerId]
            = globalThis.thisLayer.font;
    }
};
globalThis.createScriptProperties = () => {
    const values = Object.create(null);
    const builder = {
        addSlider(definition) { values[definition.name] = definition.value; return builder; },
        addCheckbox(definition) { values[definition.name] = definition.value; return builder; },
        addCombo(definition) { values[definition.name] = definition.value; return builder; },
        addColor(definition) { values[definition.name] = definition.value; return builder; },
        addText(definition) { values[definition.name] = definition.value; return builder; },
        finish() { return values; },
    };
    return builder;
};
globalThis.__tensor_wallpaperFontResult = () => JSON.stringify({
    objects: Object.fromEntries(Object.entries(__tensor_wallpaperObjectFonts)),
    names: Object.fromEntries(Object.entries(__tensor_wallpaperLayers)
        .filter(([, layer]) => typeof layer.font === 'string')
        .map(([name, layer]) => [name, layer.font])),
});
"#;

pub(super) fn text_font_overrides(
    scene: &Value,
    project: &Value,
) -> Result<TextFontOverrides, String> {
    let scripts = user_property_scripts(scene)?;
    if scripts.is_empty() {
        return Ok(TextFontOverrides::default());
    }
    let runtime = Runtime::new().map_err(|error| error.to_string())?;
    runtime.set_memory_limit(32 * 1024 * 1024);
    runtime.set_max_stack_size(512 * 1024);
    standard_library::install(&runtime);
    let context = Context::full(&runtime).map_err(|error| error.to_string())?;
    context.with(|ctx| {
        ctx.eval::<(), _>(FONT_HOST)
            .catch(&ctx)
            .map_err(|error| error.to_string())?;
        let properties = ctx
            .json_parse(flat_project_properties(project).to_string())
            .map_err(|error| error.to_string())?;
        for (index, script) in scripts.iter().enumerate() {
            let set_layer: Function = ctx
                .globals()
                .get("__tensor_wallpaperSetLayer")
                .map_err(|error| error.to_string())?;
            set_layer
                .call::<_, ()>((script.layer, script.object_id))
                .catch(&ctx)
                .map_err(|error| error.to_string())?;
            let module = Module::declare(
                ctx.clone(),
                format!("tensor-wallpaper:font-property/{index}"),
                script.source.as_bytes(),
            )
            .catch(&ctx)
            .map_err(|error| error.to_string())?;
            let (module, promise) = module
                .eval()
                .catch(&ctx)
                .map_err(|error| error.to_string())?;
            promise
                .finish::<()>()
                .catch(&ctx)
                .map_err(|error| error.to_string())?;
            let namespace = module
                .namespace()
                .catch(&ctx)
                .map_err(|error| error.to_string())?;
            bind_script_properties(&namespace, script.properties)?;
            let apply: Function = namespace
                .get("applyUserProperties")
                .map_err(|error| error.to_string())?;
            apply
                .call::<_, ()>((properties.clone(),))
                .catch(&ctx)
                .map_err(|error| error.to_string())?;
            let record: Function = ctx
                .globals()
                .get("__tensor_wallpaperRecordCurrentLayerFont")
                .map_err(|error| error.to_string())?;
            record
                .call::<_, ()>(())
                .catch(&ctx)
                .map_err(|error| error.to_string())?;
        }
        let result: Function = ctx
            .globals()
            .get("__tensor_wallpaperFontResult")
            .map_err(|error| error.to_string())?;
        let json: String = result.call(()).map_err(|error| error.to_string())?;
        let bindings: RawTextFontOverrides =
            serde_json::from_str(&json).map_err(|error| error.to_string())?;
        Ok(TextFontOverrides {
            by_object: bindings
                .objects
                .into_iter()
                .filter_map(|(object, path)| {
                    object
                        .parse()
                        .ok()
                        .map(|object| (object, normalize_we_path(&path)))
                })
                .collect(),
            by_name: bindings
                .names
                .into_iter()
                .map(|(name, path)| (name, normalize_we_path(&path)))
                .collect(),
        })
    })
}

#[derive(Debug, serde::Deserialize)]
struct RawTextFontOverrides {
    objects: BTreeMap<String, String>,
    names: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy)]
struct FontPropertyScript<'a> {
    object_id: u32,
    layer: &'a str,
    source: &'a str,
    properties: &'a Value,
}

fn user_property_scripts(scene: &Value) -> Result<Vec<FontPropertyScript<'_>>, String> {
    let mut scripts = Vec::new();
    for object in scene
        .get("objects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let object_id = value_u32(object.get("id")).unwrap_or(scripts.len() as u32);
        let layer = object.get("name").and_then(Value::as_str).unwrap_or("");
        visit_scripts(object, &mut |binding, source| {
            let analysis = analyze_scene_script(source).map_err(|error| error.to_string())?;
            if analysis.handles_user_properties && analysis.may_assign_font {
                scripts.push(FontPropertyScript {
                    object_id,
                    layer,
                    source,
                    properties: binding.get("scriptproperties").unwrap_or(&Value::Null),
                });
            }
            Ok(())
        })?;
    }
    Ok(scripts)
}

fn visit_scripts<'a>(
    value: &'a Value,
    visitor: &mut impl FnMut(&'a Map<String, Value>, &'a str) -> Result<(), String>,
) -> Result<(), String> {
    match value {
        Value::Object(object) => {
            if let Some(script) = object.get("script").and_then(Value::as_str) {
                visitor(object, script)?;
            }
            for child in object.values() {
                visit_scripts(child, visitor)?;
            }
        }
        Value::Array(values) => {
            for child in values {
                visit_scripts(child, visitor)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn bind_script_properties(
    namespace: &rquickjs::Object<'_>,
    properties: &Value,
) -> Result<(), String> {
    let Ok(script_properties) = namespace.get::<_, rquickjs::Object>("scriptProperties") else {
        return Ok(());
    };
    let Some(properties) = properties.as_object() else {
        return Ok(());
    };
    for (name, bound) in properties {
        let value = bound.get("value").unwrap_or(bound);
        match value {
            Value::Bool(value) => script_properties.set(name.as_str(), *value),
            Value::Number(value) => script_properties.set(name.as_str(), value.as_f64()),
            Value::String(value) => script_properties.set(name.as_str(), value.as_str()),
            _ => continue,
        }
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn flat_project_properties(project: &Value) -> Value {
    let mut values = Map::new();
    for (name, definition) in project
        .pointer("/general/properties")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        values.insert(
            name.clone(),
            definition.get("value").cloned().unwrap_or(Value::Null),
        );
    }
    Value::Object(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executes_authored_property_callback_to_resolve_font_defaults() {
        let scene = serde_json::json!({
            "objects": [{
                "name": "字体控制器",
                "visible": {"script": r#"
                    const fonts = [null,
                        engine.registerAsset("fonts/Latin.ttf"),
                        engine.registerAsset("fonts/中文.otf")];
                    function setLayerFont(name, index) {
                        const layer = thisScene.getLayer(name);
                        if (layer && index >= 1 && index <= 2) layer.font = fonts[index];
                    }
                    export function applyUserProperties(userProperties) {
                        setLayerFont('年', parseInt(userProperties.text1));
                        setLayerFont('时间', parseInt(userProperties.text1));
                    }
                "#}
            }]
        });
        let project = serde_json::json!({
            "general": {"properties": {"text1": {"value": "2"}}}
        });
        assert_eq!(
            text_font_overrides(&scene, &project).expect("font overrides"),
            TextFontOverrides {
                by_object: BTreeMap::new(),
                by_name: BTreeMap::from([
                    ("年".to_owned(), "fonts/中文.otf".to_owned()),
                    ("时间".to_owned(), "fonts/中文.otf".to_owned()),
                ]),
            }
        );
    }

    #[test]
    fn scene_without_user_property_callback_needs_no_js_context_output() {
        let scene = serde_json::json!({
            "objects": [{"script": "engine.registerAsset('fonts/a.ttf')"}]
        });
        assert_eq!(
            text_font_overrides(&scene, &serde_json::json!({})).expect("font overrides"),
            TextFontOverrides::default()
        );
    }

    #[test]
    fn ignores_non_font_user_property_callbacks() {
        let scene = serde_json::json!({
            "objects": [{
                "name": "音乐控制器",
                "origin": {"script": r#"
                    const songs = ['first', 'second'];
                    export function applyUserProperties() {
                        songs.forEach(song => song.stop());
                    }
                "#}
            }]
        });
        assert_eq!(
            text_font_overrides(&scene, &serde_json::json!({}))
                .expect("non-font callback does not enter the font resolver"),
            TextFontOverrides::default()
        );
    }

    #[test]
    fn current_layer_font_assignment_uses_the_containing_scene_object() {
        let scene = serde_json::json!({
            "objects": [{
                "name": "日期",
                "text": {"script": r#"
                    const font = engine.registerAsset('fonts/date.ttf');
                    export function applyUserProperties(properties) {
                        if (properties.hasOwnProperty('font')) thisLayer.font = font;
                    }
                "#}
            }]
        });
        let project = serde_json::json!({
            "general": {"properties": {"font": {"value": "1"}}}
        });
        assert_eq!(
            text_font_overrides(&scene, &project).expect("font overrides"),
            TextFontOverrides {
                by_object: BTreeMap::from([(0, "fonts/date.ttf".to_owned())]),
                by_name: BTreeMap::from([("日期".to_owned(), "fonts/date.ttf".to_owned())]),
            }
        );
    }

    #[test]
    fn anonymous_current_layer_font_assignments_keep_object_identity() {
        let scene = serde_json::json!({
            "objects": [
                {
                    "id": 3465,
                    "name": "",
                    "text": {"script": r#"
                        const font = engine.registerAsset('fonts/DouyinSansBold.ttf');
                        export function applyUserProperties(properties) {
                            if (properties.text3 === '32') thisLayer.font = font;
                        }
                    "#}
                },
                {
                    "id": 955,
                    "name": "",
                    "text": {"script": r#"
                        const font = engine.registerAsset('fonts/Atami-Regular.otf');
                        export function applyUserProperties(properties) {
                            if (properties.text4 === '2') thisLayer.font = font;
                        }
                    "#}
                }
            ]
        });
        let project = serde_json::json!({
            "general": {"properties": {"text3": {"value": "32"}, "text4": {"value": "2"}}}
        });

        assert_eq!(
            text_font_overrides(&scene, &project).expect("font overrides"),
            TextFontOverrides {
                by_object: BTreeMap::from([
                    (3465, "fonts/DouyinSansBold.ttf".to_owned()),
                    (955, "fonts/Atami-Regular.otf".to_owned()),
                ]),
                by_name: BTreeMap::from([("".to_owned(), "fonts/Atami-Regular.otf".to_owned())]),
            }
        );
    }

    #[test]
    fn font_property_module_can_initialize_authored_vec3_state() {
        let scene = serde_json::json!({
            "objects": [{
                "name": "字体控制器",
                "visible": {"script": r#"
                    const retained = new Vec3(1, 2, 3);
                    const font = engine.registerAsset('fonts/retained.ttf');
                    export function applyUserProperties() {
                        if (retained.x === 1 && retained.y === 2 && retained.z === 3) {
                            thisScene.getLayer('时间').font = font;
                        }
                    }
                "#}
            }]
        });
        assert_eq!(
            text_font_overrides(&scene, &serde_json::json!({})).expect("font overrides"),
            TextFontOverrides {
                by_object: BTreeMap::new(),
                by_name: BTreeMap::from([("时间".to_owned(), "fonts/retained.ttf".to_owned())]),
            }
        );
    }
}
