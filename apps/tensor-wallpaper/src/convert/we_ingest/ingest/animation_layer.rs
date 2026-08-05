//! Wallpaper Engine object animation-layer script initialization semantics.

use rquickjs::{Context, Function, Module, Object, Runtime};
use serde_json::Value;

use super::super::script_analysis::analyze_scene_script;
use crate::engine::scene::script::standard_library;

const ANIMATION_HOST: &str = r#"
globalThis.__tensor_wallpaperFrame = 0;
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
globalThis.thisObject = {
    frameCount: 1,
    getAnimation() { return this; },
    play() {},
    pause() {},
    stop() {},
    addEndedCallback() {},
    setFrame(frame) { __tensor_wallpaperFrame = Number(frame); },
};
"#;

pub(super) fn animation_layer_initial_progress(layer: &Value) -> Result<f32, String> {
    let Some(binding) = layer.get("visible").and_then(Value::as_object) else {
        return Ok(0.0);
    };
    let Some(source) = binding.get("script").and_then(Value::as_str) else {
        return Ok(0.0);
    };
    let analysis = analyze_scene_script(source).map_err(|error| error.to_string())?;
    if !analysis.exports_init {
        return Ok(0.0);
    }

    let runtime = Runtime::new().map_err(|error| error.to_string())?;
    runtime.set_memory_limit(16 * 1024 * 1024);
    runtime.set_max_stack_size(256 * 1024);
    standard_library::install(&runtime);
    let context = Context::full(&runtime).map_err(|error| error.to_string())?;
    context.with(|ctx| {
        ctx.eval::<(), _>(ANIMATION_HOST)
            .map_err(|error| error.to_string())?;
        let module = Module::declare(
            ctx.clone(),
            "tensor-wallpaper:animation-layer",
            source.as_bytes(),
        )
        .map_err(|error| error.to_string())?;
        let (module, promise) = module.eval().map_err(|error| error.to_string())?;
        promise.finish::<()>().map_err(|error| error.to_string())?;
        let namespace = module.namespace().map_err(|error| error.to_string())?;
        bind_script_properties(
            &namespace,
            binding.get("scriptproperties").unwrap_or(&Value::Null),
        )?;
        let init: Function = namespace.get("init").map_err(|error| error.to_string())?;
        init.call::<_, ()>((true,))
            .map_err(|error| error.to_string())?;
        let progress: f64 = ctx
            .globals()
            .get("__tensor_wallpaperFrame")
            .map_err(|error| error.to_string())?;
        Ok(if progress.is_finite() {
            (progress as f32).clamp(0.0, 1.0)
        } else {
            0.0
        })
    })
}

fn bind_script_properties(namespace: &Object<'_>, properties: &Value) -> Result<(), String> {
    let Ok(script_properties) = namespace.get::<_, Object>("scriptProperties") else {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executes_authored_init_to_capture_animation_progress() {
        let layer = serde_json::json!({
            "visible": {
                "value": true,
                "script": r#"
                    export var scriptProperties = createScriptProperties()
                        .addSlider({name: 'percentage', value: 1}).finish();
                    export function init(value) {
                        const animation = 'addEndedCallback' in thisObject
                            ? thisObject : thisObject.getAnimation();
                        animation.play();
                        animation.setFrame(animation.frameCount * scriptProperties.percentage);
                        return value;
                    }
                "#,
                "scriptproperties": { "percentage": 0.94 }
            }
        });
        assert_eq!(
            animation_layer_initial_progress(&layer).expect("initial progress"),
            0.94
        );
    }

    #[test]
    fn comments_and_unrelated_modules_do_not_create_animation_progress() {
        let layer = serde_json::json!({
            "visible": {
                "script": "// setFrame(frameCount * 0.8)\nexport function update(value) { return value; }"
            }
        });
        assert_eq!(
            animation_layer_initial_progress(&layer).expect("initial progress"),
            0.0
        );
    }
}
