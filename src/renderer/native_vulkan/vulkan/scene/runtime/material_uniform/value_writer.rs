use serde_json::Value;

pub(super) fn set_vector(values: &mut [f32], start: usize, parameter: &[f32], count: usize) {
    for (lane, value) in parameter.iter().take(count).enumerate() {
        if let Some(destination) = values.get_mut(start + lane) {
            *destination = *value;
        }
    }
}

pub(super) fn parse_constant_values(value_json: &str) -> Vec<f32> {
    let Ok(value) = serde_json::from_str::<Value>(value_json) else {
        return Vec::new();
    };
    let mut values = Vec::new();
    collect_constant_values(&value, &mut values);
    values
}

fn collect_constant_values(value: &Value, out: &mut Vec<f32>) {
    match value {
        Value::Number(number) => {
            if let Some(value) = number.as_f64().filter(|value| value.is_finite()) {
                out.push(value as f32);
            }
        }
        Value::Bool(value) => out.push(if *value { 1.0 } else { 0.0 }),
        Value::String(value) => {
            for value in value.split_ascii_whitespace() {
                if let Ok(value) = value.parse::<f32>() {
                    out.push(value);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_constant_values(value, out);
            }
        }
        Value::Object(object) => {
            if let Some(value) = object.get("value") {
                collect_constant_values(value, out);
                return;
            }
            for key in ["x", "y", "z", "w", "r", "g", "b", "a"] {
                if let Some(value) = object.get(key) {
                    collect_constant_values(value, out);
                }
            }
        }
        Value::Null => {}
    }
}
