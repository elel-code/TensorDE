use serde_json::{Map, Value, json};

use super::super::{
    scene_color_from_value, string_field, value_to_bool_unwrapped, value_to_f64_unwrapped,
    value_to_i64, vector3_components_from_value,
};

#[derive(Debug, Clone, PartialEq)]
pub(in crate::convert::wallpaper_engine) struct SceneParticleIr {
    particle: Map<String, Value>,
}

impl SceneParticleIr {
    pub(in crate::convert::wallpaper_engine) fn from_wallpaper_engine_object(
        object: &Map<String, Value>,
        fallback_seed: Option<u64>,
        spawn_size: Option<(f64, f64)>,
        particle_definition: Option<&Value>,
    ) -> Option<Self> {
        let type_hint = string_field(object, &["type", "class", "kind"])
            .unwrap_or_default()
            .to_ascii_lowercase();
        let source = string_field(object, &["particle", "emitter"]);
        if source.is_none() && !type_hint.contains("particle") && !type_hint.contains("emitter") {
            return None;
        }

        let mut particle = Map::new();
        if let Some(source) = source {
            particle.insert("source".to_owned(), Value::String(source));
        }
        if let Some(seed) = fallback_seed {
            particle.insert("seed".to_owned(), json!(seed));
        }
        if let Some((width, height)) = spawn_size {
            if width.is_finite() && width > 0.0 {
                particle.insert("spawn_width".to_owned(), json!(width));
            }
            if height.is_finite() && height > 0.0 {
                particle.insert("spawn_height".to_owned(), json!(height));
            }
        }

        let has_particle_definition = particle_definition.is_some();
        if let Some(definition) = particle_definition.and_then(Value::as_object) {
            scene_particle_merge_definition(definition, &mut particle);
        }
        scene_particle_merge_source(object, &mut particle, false, true);
        scene_particle_merge_gravity(object, &mut particle, true);
        if let Some(instance_override) = object
            .get("instanceoverride")
            .or_else(|| object.get("instanceOverride"))
            .and_then(Value::as_object)
        {
            scene_particle_merge_instance_override(
                instance_override,
                &mut particle,
                has_particle_definition,
            );
        }

        Some(Self { particle })
    }

    pub(in crate::convert::wallpaper_engine) fn properties_value(&self) -> Value {
        json!({ "particle": self.particle })
    }
}

fn scene_particle_merge_source(
    source: &Map<String, Value>,
    particle: &mut Map<String, Value>,
    allow_we_size_vector: bool,
    overwrite: bool,
) {
    scene_particle_insert_u32_any(
        source,
        particle,
        &["count", "max_count", "maxcount"],
        "count",
        overwrite,
    );
    scene_particle_insert_u64_any(
        source,
        particle,
        &["lifetime_ms", "lifetimeMs", "lifetimems"],
        "lifetime_ms",
        overwrite,
    );
    for (target, keys) in [
        (
            "rate",
            &["rate", "spawn_rate", "spawnrate", "emissionrate"][..],
        ),
        ("speed", &["speed", "velocity"][..]),
        (
            "speed_min",
            &["speed_min", "speedMin", "speedmin", "minspeed", "minSpeed"][..],
        ),
        (
            "speed_max",
            &["speed_max", "speedMax", "speedmax", "maxspeed", "maxSpeed"][..],
        ),
        ("size", &["size"][..]),
        ("width", &["width", "sizex", "sizeX"][..]),
        ("height", &["height", "sizey", "sizeY"][..]),
        (
            "lifetime",
            &["lifetime", "lifetime_seconds", "lifetimeSeconds"][..],
        ),
        (
            "direction_deg",
            &[
                "direction_deg",
                "directionDeg",
                "direction",
                "angle",
                "angledeg",
            ][..],
        ),
        (
            "spread_deg",
            &[
                "spread_deg",
                "spreadDeg",
                "spread",
                "anglespread",
                "angleSpread",
            ][..],
        ),
        ("gravity_x", &["gravity_x", "gravityX"][..]),
        ("gravity_y", &["gravity_y", "gravityY"][..]),
        (
            "spawn_width",
            &["spawn_width", "spawnWidth", "emitter_width", "emitterWidth"][..],
        ),
        (
            "spawn_height",
            &[
                "spawn_height",
                "spawnHeight",
                "emitter_height",
                "emitterHeight",
            ][..],
        ),
    ] {
        scene_particle_insert_number_any(source, particle, keys, target, overwrite);
    }
    scene_particle_insert_size_vector(source, particle, allow_we_size_vector, overwrite);
    for (target, keys) in [
        ("loop", &["loop", "looping"][..]),
        (
            "fade",
            &["fade", "fadeout", "fadeOut", "alpha_fade", "alphaFade"][..],
        ),
    ] {
        scene_particle_insert_bool_any(source, particle, keys, target, overwrite);
    }
    if (overwrite || !particle.contains_key("shape"))
        && let Some(shape) = string_field(source, &["shape", "particle_shape", "particleShape"])
    {
        particle.insert("shape".to_owned(), Value::String(shape));
    }
    if (overwrite || !particle.contains_key("color"))
        && let Some(color) = source
            .get("color")
            .or_else(|| source.get("tint"))
            .or_else(|| source.get("colorn"))
            .or_else(|| source.get("colorN"))
            .or_else(|| source.get("particlecolor"))
            .or_else(|| source.get("particleColor"))
            .and_then(scene_color_from_value)
    {
        particle.insert("color".to_owned(), Value::String(color));
    }
}

fn scene_particle_merge_definition(
    definition: &Map<String, Value>,
    particle: &mut Map<String, Value>,
) {
    scene_particle_merge_source(definition, particle, false, false);
    if let Some(material) = string_field(definition, &["material"]) {
        scene_particle_insert_string(particle, "material", material, false);
    }
    scene_particle_for_each_object(definition.get("emitter"), |emitter| {
        scene_particle_merge_definition_emitter(emitter, particle);
    });
    scene_particle_for_each_object(definition.get("initializer"), |initializer| {
        scene_particle_merge_definition_initializer(initializer, particle);
    });
    scene_particle_for_each_object(definition.get("operator"), |operator| {
        scene_particle_merge_definition_operator(operator, particle);
    });
    scene_particle_for_each_object(definition.get("renderer"), |renderer| {
        scene_particle_merge_definition_renderer(renderer, particle);
    });
    scene_particle_insert_u32_value(particle, "count", 100, false);
    scene_particle_insert_number_value(particle, "size", 20.0, false);
    scene_particle_insert_number_value(particle, "lifetime", 1.0, false);
}

fn scene_particle_for_each_object(value: Option<&Value>, mut f: impl FnMut(&Map<String, Value>)) {
    match value {
        Some(Value::Array(values)) => {
            for object in values.iter().filter_map(Value::as_object) {
                f(object);
            }
        }
        Some(Value::Object(object)) => f(object),
        Some(Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null) | None => {}
    }
}

fn scene_particle_merge_definition_emitter(
    emitter: &Map<String, Value>,
    particle: &mut Map<String, Value>,
) {
    scene_particle_merge_source(emitter, particle, false, false);
    scene_particle_insert_number_value(particle, "rate", 10.0, false);
    scene_particle_insert_u32_any(
        emitter,
        particle,
        &["instantaneous", "maxcount", "count"],
        "count",
        false,
    );
    if let Some((max_x, max_y, _)) = emitter
        .get("distancemax")
        .or_else(|| emitter.get("distanceMax"))
        .and_then(scene_particle_vector3_or_scalar)
        .or_else(|| Some((256.0, 256.0, 0.0)))
    {
        let directions = emitter
            .get("directions")
            .and_then(scene_particle_vector3_or_scalar)
            .unwrap_or((1.0, 1.0, 0.0));
        let spawn_width = (max_x * directions.0).abs() * 2.0;
        let spawn_height = (max_y * directions.1).abs() * 2.0;
        scene_particle_insert_number_value(particle, "spawn_width", spawn_width, false);
        scene_particle_insert_number_value(particle, "spawn_height", spawn_height, false);
    }
    if let Some(name) = string_field(emitter, &["name"]) {
        let emitter_shape = match name.to_ascii_lowercase().as_str() {
            "boxrandom" => Some("box"),
            "sphererandom" => Some("sphere"),
            _ => None,
        };
        if let Some(emitter_shape) = emitter_shape {
            scene_particle_insert_string(
                particle,
                "emitter_shape",
                emitter_shape.to_owned(),
                false,
            );
        }
    }
}

fn scene_particle_merge_instance_override(
    source: &Map<String, Value>,
    particle: &mut Map<String, Value>,
    use_we_multipliers: bool,
) {
    if !use_we_multipliers {
        scene_particle_merge_source(source, particle, true, true);
        return;
    }

    if let Some(multiplier) = scene_particle_number_from_any(source, &["count"]) {
        scene_particle_multiply_u32(particle, "count", multiplier);
    }
    if let Some(multiplier) = scene_particle_number_from_any(source, &["rate"]) {
        scene_particle_multiply_number(particle, "rate", multiplier);
    }
    if let Some(multiplier) = scene_particle_number_from_any(source, &["speed"]) {
        scene_particle_multiply_number(particle, "speed", multiplier);
        scene_particle_multiply_number(particle, "speed_min", multiplier);
        scene_particle_multiply_number(particle, "speed_max", multiplier);
    }
    if let Some(multiplier) = scene_particle_number_from_any(source, &["size"]) {
        scene_particle_multiply_number(particle, "size", multiplier);
        scene_particle_multiply_number(particle, "width", multiplier);
        scene_particle_multiply_number(particle, "height", multiplier);
    } else {
        scene_particle_insert_size_vector(source, particle, true, true);
    }
    if let Some(multiplier) = scene_particle_number_from_any(source, &["lifetime"]) {
        scene_particle_multiply_number(particle, "lifetime", multiplier);
        scene_particle_multiply_u64(particle, "lifetime_ms", multiplier);
    }
    if let Some(color) = source
        .get("color")
        .or_else(|| source.get("colorn"))
        .or_else(|| source.get("colorN"))
        .and_then(scene_color_from_value)
    {
        particle.insert("color".to_owned(), Value::String(color));
    }
    for (target, keys) in [
        (
            "speed_min",
            &["speed_min", "speedMin", "speedmin", "minspeed", "minSpeed"][..],
        ),
        (
            "speed_max",
            &["speed_max", "speedMax", "speedmax", "maxspeed", "maxSpeed"][..],
        ),
    ] {
        scene_particle_insert_number_any(source, particle, keys, target, true);
    }
    scene_particle_insert_bool_any(
        source,
        particle,
        &["fade", "fadeout", "fadeOut", "alpha_fade", "alphaFade"],
        "fade",
        true,
    );
}

fn scene_particle_merge_definition_initializer(
    initializer: &Map<String, Value>,
    particle: &mut Map<String, Value>,
) {
    let Some(name) = string_field(initializer, &["name"]).map(|name| name.to_ascii_lowercase())
    else {
        return;
    };
    match name.as_str() {
        "colorrandom" => {
            if let Some(color) =
                scene_particle_mid_color(initializer.get("min"), initializer.get("max"))
            {
                scene_particle_insert_string(particle, "color", color, false);
            }
        }
        "sizerandom" => {
            if let Some((min, max)) = scene_particle_min_max(initializer) {
                let size = ((min + max) * 0.5 * 0.5).max(0.0);
                scene_particle_insert_number_value(particle, "size", size, false);
            }
        }
        "lifetimerandom" => {
            if let Some((min, max)) = scene_particle_min_max(initializer) {
                let lifetime = ((min + max) * 0.5).max(0.0);
                scene_particle_insert_number_value(particle, "lifetime", lifetime, false);
            }
        }
        "velocityrandom" => {
            scene_particle_insert_speed_range_from_vectors(
                particle,
                initializer.get("min"),
                initializer.get("max"),
                false,
            );
        }
        "turbulentvelocityrandom" => {
            scene_particle_insert_number_any(
                initializer,
                particle,
                &["speedmin", "speedMin", "speed_min"],
                "speed_min",
                false,
            );
            scene_particle_insert_number_any(
                initializer,
                particle,
                &["speedmax", "speedMax", "speed_max"],
                "speed_max",
                false,
            );
        }
        "mapsequencearoundcontrolpoint" => {
            scene_particle_insert_u32_any(initializer, particle, &["count"], "count", false);
            scene_particle_insert_speed_range_from_vectors(
                particle,
                initializer
                    .get("speedmin")
                    .or_else(|| initializer.get("speedMin")),
                initializer
                    .get("speedmax")
                    .or_else(|| initializer.get("speedMax")),
                false,
            );
        }
        _ => {}
    }
}

fn scene_particle_merge_definition_operator(
    operator: &Map<String, Value>,
    particle: &mut Map<String, Value>,
) {
    let Some(name) = string_field(operator, &["name"]).map(|name| name.to_ascii_lowercase()) else {
        return;
    };
    match name.as_str() {
        "movement" => {
            if let Some((x, y, _)) = operator
                .get("gravity")
                .and_then(scene_particle_vector3_or_scalar)
            {
                scene_particle_insert_gravity_vector(particle, x, y, 1.0, false);
            }
        }
        "alphafade" => {
            scene_particle_insert_bool_value(particle, "fade", true, false);
        }
        _ => {}
    }
}

fn scene_particle_merge_definition_renderer(
    renderer: &Map<String, Value>,
    particle: &mut Map<String, Value>,
) {
    if let Some(name) = string_field(renderer, &["name"]) {
        scene_particle_insert_string(particle, "renderer", name, false);
    }
    let fade_alpha = renderer
        .get("fadealpha")
        .or_else(|| renderer.get("fadeAlpha"))
        .and_then(value_to_bool_unwrapped)
        .unwrap_or(false);
    let fade_size = renderer
        .get("fadesize")
        .or_else(|| renderer.get("fadeSize"))
        .and_then(value_to_bool_unwrapped)
        .unwrap_or(false);
    if fade_alpha || fade_size {
        scene_particle_insert_bool_value(particle, "fade", true, false);
    }
}

fn scene_particle_insert_size_vector(
    source: &Map<String, Value>,
    particle: &mut Map<String, Value>,
    allow_we_size_vector: bool,
    overwrite: bool,
) {
    let size = if allow_we_size_vector {
        source.get("size")
    } else {
        None
    };
    let Some((width, height, _)) = size
        .or_else(|| source.get("particle_size"))
        .or_else(|| source.get("particleSize"))
        .and_then(vector3_components_from_value)
    else {
        return;
    };
    if (overwrite || !particle.contains_key("width")) && width.is_finite() && width > 0.0 {
        particle.insert("width".to_owned(), json!(width));
    }
    if (overwrite || !particle.contains_key("height")) && height.is_finite() && height > 0.0 {
        particle.insert("height".to_owned(), json!(height));
    }
}

fn scene_particle_insert_u32_any(
    source: &Map<String, Value>,
    particle: &mut Map<String, Value>,
    source_keys: &[&str],
    target_key: &str,
    overwrite: bool,
) {
    if !overwrite && particle.contains_key(target_key) {
        return;
    }
    let Some(value) = source
        .iter()
        .find(|(key, _)| {
            source_keys
                .iter()
                .any(|source_key| key.eq_ignore_ascii_case(source_key))
        })
        .and_then(|(_, value)| value_to_f64_unwrapped(value))
        .filter(|value| value.is_finite() && *value >= 0.0 && *value <= u32::MAX as f64)
    else {
        return;
    };
    particle.insert(target_key.to_owned(), json!(value.round() as u32));
}

fn scene_particle_insert_u64_any(
    source: &Map<String, Value>,
    particle: &mut Map<String, Value>,
    source_keys: &[&str],
    target_key: &str,
    overwrite: bool,
) {
    if !overwrite && particle.contains_key(target_key) {
        return;
    }
    let Some(value) = source
        .iter()
        .find(|(key, _)| {
            source_keys
                .iter()
                .any(|source_key| key.eq_ignore_ascii_case(source_key))
        })
        .and_then(|(_, value)| value_to_f64_unwrapped(value))
        .filter(|value| value.is_finite() && *value >= 0.0 && *value <= u64::MAX as f64)
    else {
        return;
    };
    particle.insert(target_key.to_owned(), json!(value.round() as u64));
}

fn scene_particle_number_from_any(
    source: &Map<String, Value>,
    source_keys: &[&str],
) -> Option<f64> {
    source
        .iter()
        .find(|(key, _)| {
            source_keys
                .iter()
                .any(|source_key| key.eq_ignore_ascii_case(source_key))
        })
        .and_then(|(_, value)| value_to_f64_unwrapped(value))
        .filter(|value| value.is_finite())
}

fn scene_particle_insert_number_any(
    source: &Map<String, Value>,
    particle: &mut Map<String, Value>,
    source_keys: &[&str],
    target_key: &str,
    overwrite: bool,
) {
    if !overwrite && particle.contains_key(target_key) {
        return;
    }
    let Some(value) = source
        .iter()
        .find(|(key, _)| {
            source_keys
                .iter()
                .any(|source_key| key.eq_ignore_ascii_case(source_key))
        })
        .and_then(|(_, value)| value_to_f64_unwrapped(value))
    else {
        return;
    };
    if value.is_finite() {
        particle.insert(target_key.to_owned(), json!(value));
    }
}

fn scene_particle_insert_bool_any(
    source: &Map<String, Value>,
    particle: &mut Map<String, Value>,
    source_keys: &[&str],
    target_key: &str,
    overwrite: bool,
) {
    if !overwrite && particle.contains_key(target_key) {
        return;
    }
    let Some(value) = source
        .iter()
        .find(|(key, _)| {
            source_keys
                .iter()
                .any(|source_key| key.eq_ignore_ascii_case(source_key))
        })
        .and_then(|(_, value)| value_to_bool_unwrapped(value))
    else {
        return;
    };
    particle.insert(target_key.to_owned(), Value::Bool(value));
}

fn scene_particle_merge_gravity(
    source: &Map<String, Value>,
    particle: &mut Map<String, Value>,
    overwrite: bool,
) {
    if !overwrite && particle.contains_key("gravity_x") && particle.contains_key("gravity_y") {
        return;
    }
    if let Some((x, y, _)) = source
        .get("gravity")
        .and_then(vector3_components_from_value)
    {
        scene_particle_insert_gravity_vector(particle, x, y, 1.0, overwrite);
        return;
    }
    let Some(strength) = source
        .get("gravitystrength")
        .or_else(|| source.get("gravityStrength"))
        .and_then(value_to_f64_unwrapped)
        .filter(|value| value.is_finite())
    else {
        return;
    };
    if let Some((x, y, _)) = source
        .get("gravitydirection")
        .or_else(|| source.get("gravityDirection"))
        .and_then(vector3_components_from_value)
    {
        scene_particle_insert_gravity_vector(particle, x, y, strength, overwrite);
    } else if let Some(direction_deg) = source
        .get("gravitydirection")
        .or_else(|| source.get("gravityDirection"))
        .and_then(value_to_f64_unwrapped)
    {
        let radians = direction_deg.to_radians();
        scene_particle_insert_gravity_vector(
            particle,
            radians.cos(),
            radians.sin(),
            strength,
            overwrite,
        );
    }
}

fn scene_particle_insert_gravity_vector(
    particle: &mut Map<String, Value>,
    x: f64,
    y: f64,
    strength: f64,
    overwrite: bool,
) {
    let gravity_x = x * strength;
    let gravity_y = y * strength;
    if gravity_x.is_finite() && (overwrite || !particle.contains_key("gravity_x")) {
        particle.insert("gravity_x".to_owned(), json!(gravity_x));
    }
    if gravity_y.is_finite() && (overwrite || !particle.contains_key("gravity_y")) {
        particle.insert("gravity_y".to_owned(), json!(gravity_y));
    }
}

fn scene_particle_insert_number_value(
    particle: &mut Map<String, Value>,
    target_key: &str,
    value: f64,
    overwrite: bool,
) {
    if value.is_finite() && (overwrite || !particle.contains_key(target_key)) {
        particle.insert(target_key.to_owned(), json!(value));
    }
}

fn scene_particle_insert_u32_value(
    particle: &mut Map<String, Value>,
    target_key: &str,
    value: u32,
    overwrite: bool,
) {
    if overwrite || !particle.contains_key(target_key) {
        particle.insert(target_key.to_owned(), json!(value));
    }
}

fn scene_particle_multiply_number(
    particle: &mut Map<String, Value>,
    target_key: &str,
    multiplier: f64,
) {
    if !multiplier.is_finite() {
        return;
    }
    let Some(value) = particle.get(target_key).and_then(value_to_f64_unwrapped) else {
        return;
    };
    let value = value * multiplier;
    if value.is_finite() {
        particle.insert(target_key.to_owned(), json!(value));
    }
}

fn scene_particle_multiply_u32(
    particle: &mut Map<String, Value>,
    target_key: &str,
    multiplier: f64,
) {
    if !multiplier.is_finite() {
        return;
    }
    let Some(value) = particle.get(target_key).and_then(value_to_f64_unwrapped) else {
        return;
    };
    let value = (value * multiplier).round().clamp(0.0, u32::MAX as f64);
    particle.insert(target_key.to_owned(), json!(value as u32));
}

fn scene_particle_multiply_u64(
    particle: &mut Map<String, Value>,
    target_key: &str,
    multiplier: f64,
) {
    if !multiplier.is_finite() {
        return;
    }
    let Some(value) = particle.get(target_key).and_then(value_to_f64_unwrapped) else {
        return;
    };
    let value = (value * multiplier).round().clamp(0.0, u64::MAX as f64);
    particle.insert(target_key.to_owned(), json!(value as u64));
}

fn scene_particle_insert_bool_value(
    particle: &mut Map<String, Value>,
    target_key: &str,
    value: bool,
    overwrite: bool,
) {
    if overwrite || !particle.contains_key(target_key) {
        particle.insert(target_key.to_owned(), Value::Bool(value));
    }
}

fn scene_particle_insert_string(
    particle: &mut Map<String, Value>,
    target_key: &str,
    value: String,
    overwrite: bool,
) {
    if !value.is_empty() && (overwrite || !particle.contains_key(target_key)) {
        particle.insert(target_key.to_owned(), Value::String(value));
    }
}

fn scene_particle_vector3_or_scalar(value: &Value) -> Option<(f64, f64, f64)> {
    vector3_components_from_value(value).or_else(|| {
        let value = value_to_f64_unwrapped(value)?;
        Some((value, value, value))
    })
}

fn scene_particle_min_max(source: &Map<String, Value>) -> Option<(f64, f64)> {
    let min = source.get("min").and_then(value_to_f64_unwrapped)?;
    let max = source.get("max").and_then(value_to_f64_unwrapped)?;
    if min.is_finite() && max.is_finite() {
        Some((min.min(max), min.max(max)))
    } else {
        None
    }
}

fn scene_particle_mid_color(min: Option<&Value>, max: Option<&Value>) -> Option<String> {
    let (min_r, min_g, min_b) = min.and_then(vector3_components_from_value)?;
    let (max_r, max_g, max_b) = max.and_then(vector3_components_from_value)?;
    scene_color_from_value(&json!([
        (min_r + max_r) * 0.5,
        (min_g + max_g) * 0.5,
        (min_b + max_b) * 0.5
    ]))
}

fn scene_particle_insert_speed_range_from_vectors(
    particle: &mut Map<String, Value>,
    min: Option<&Value>,
    max: Option<&Value>,
    overwrite: bool,
) {
    let Some(min_speed) = min
        .and_then(scene_particle_vector3_or_scalar)
        .map(scene_particle_vector_length)
    else {
        return;
    };
    let Some(max_speed) = max
        .and_then(scene_particle_vector3_or_scalar)
        .map(scene_particle_vector_length)
    else {
        return;
    };
    let lower = min_speed.min(max_speed);
    let upper = min_speed.max(max_speed);
    scene_particle_insert_number_value(particle, "speed_min", lower, overwrite);
    scene_particle_insert_number_value(particle, "speed_max", upper, overwrite);
}

fn scene_particle_vector_length((x, y, z): (f64, f64, f64)) -> f64 {
    (x.mul_add(x, y.mul_add(y, z * z))).sqrt()
}

pub(in crate::convert::wallpaper_engine) fn scene_particle_seed_from_object(
    object: &Map<String, Value>,
) -> Option<u64> {
    object
        .get("id")
        .and_then(value_to_i64)
        .and_then(|value| u64::try_from(value).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particle_ir_lowers_runtime_supported_emitter_fields() {
        let source = json!({
            "id": 42,
            "type": "particle",
            "particle": "particles/spark.json",
            "size": [320, 180, 0],
            "instanceoverride": {
                "count": 48,
                "speedMin": 10,
                "speedMax": 40,
                "lifetime": 2.5,
                "fadeOut": false,
                "color": [1, 0.5, 0]
            },
            "directionDeg": -90,
            "spreadDeg": 45,
            "gravityDirection": [0, 1, 0],
            "gravityStrength": 12
        });
        let ir = SceneParticleIr::from_wallpaper_engine_object(
            source.as_object().unwrap(),
            Some(42),
            Some((320.0, 180.0)),
            None,
        )
        .unwrap();

        assert_eq!(
            ir.properties_value()["particle"],
            json!({
                "source": "particles/spark.json",
                "seed": 42,
                "spawn_width": 320.0,
                "spawn_height": 180.0,
                "count": 48,
                "speed_min": 10.0,
                "speed_max": 40.0,
                "lifetime": 2.5,
                "fade": false,
                "color": "#ff8000",
                "direction_deg": -90.0,
                "spread_deg": 45.0,
                "gravity_x": 0.0,
                "gravity_y": 12.0
            })
        );
    }

    #[test]
    fn particle_ir_lowers_wallpaper_engine_particle_definition_defaults() {
        let object = json!({
            "id": 7,
            "type": "particle",
            "particle": "particles/spark.json"
        });
        let definition = json!({
            "maxcount": 24,
            "material": "materials/spark.json",
            "emitter": [{
                "name": "boxrandom",
                "distancemax": [120, 60, 0],
                "directions": [1, 0.5, 0],
                "rate": 12,
                "speedmin": 3,
                "speedmax": 9
            }],
            "initializer": [
                { "name": "sizerandom", "min": 8, "max": 12 },
                { "name": "lifetimerandom", "min": 1, "max": 3 },
                { "name": "colorrandom", "min": [1, 0, 0], "max": [1, 1, 0] }
            ],
            "operator": [
                { "name": "movement", "gravity": [0, 12, 0] }
            ],
            "renderer": [
                { "name": "sprite", "fadealpha": true }
            ]
        });
        let ir = SceneParticleIr::from_wallpaper_engine_object(
            object.as_object().unwrap(),
            Some(7),
            None,
            Some(&definition),
        )
        .unwrap();

        assert_eq!(
            ir.properties_value()["particle"],
            json!({
                "source": "particles/spark.json",
                "seed": 7,
                "count": 24,
                "material": "materials/spark.json",
                "rate": 12.0,
                "speed_min": 3.0,
                "speed_max": 9.0,
                "spawn_width": 240.0,
                "spawn_height": 60.0,
                "emitter_shape": "box",
                "size": 5.0,
                "lifetime": 2.0,
                "color": "#ff8000",
                "gravity_x": 0.0,
                "gravity_y": 12.0,
                "renderer": "sprite",
                "fade": true
            })
        );
    }

    #[test]
    fn particle_ir_applies_wallpaper_engine_instance_override_multipliers() {
        let object = json!({
            "id": 9,
            "type": "particle",
            "particle": "particles/spark.json",
            "instanceoverride": {
                "count": 0.5,
                "rate": 2,
                "speed": 3,
                "size": 1.5,
                "lifetime": 4,
                "colorn": [0.25, 0.5, 1]
            }
        });
        let definition = json!({
            "maxcount": 80,
            "emitter": [{
                "name": "sphererandom",
                "rate": 10,
                "speedmin": 2,
                "speedmax": 6,
                "distancemax": 64
            }],
            "initializer": [
                { "name": "sizerandom", "min": 8, "max": 16 },
                { "name": "lifetimerandom", "min": 1, "max": 2 }
            ]
        });

        let ir = SceneParticleIr::from_wallpaper_engine_object(
            object.as_object().unwrap(),
            Some(9),
            None,
            Some(&definition),
        )
        .unwrap();

        assert_eq!(
            ir.properties_value()["particle"],
            json!({
                "source": "particles/spark.json",
                "seed": 9,
                "count": 40,
                "rate": 20.0,
                "speed_min": 6.0,
                "speed_max": 18.0,
                "spawn_width": 128.0,
                "spawn_height": 128.0,
                "emitter_shape": "sphere",
                "size": 9.0,
                "lifetime": 6.0,
                "color": "#4080ff"
            })
        );
    }
}
