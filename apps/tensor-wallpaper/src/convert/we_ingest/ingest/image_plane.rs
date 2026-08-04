//! Image-plane extent lowering for WE model and scene-object records.
//!
//! References:
//! - `docs/tensor-wallpaper/tensor-wallpaper-scene-engine-architecture.md`
//! - `reverse-engineered/tensor-wallpaper/docs/scene-format.md`
//! - `reverse-engineered/tensor-wallpaper/docs/exe/scene-and-object.md`

use serde_json::Value;

use super::{parse_vec3, value_f32};

pub(super) fn image_plane_extent(model: &Value, object: &Value) -> Option<(f32, f32)> {
    let object_size = parse_vec3(object.get("size"));
    let width = value_f32(model.get("width"))
        .or_else(|| object_size.map(|size| size.x))
        .unwrap_or(0.0);
    let height = value_f32(model.get("height"))
        .or_else(|| object_size.map(|size| size.y))
        .unwrap_or(0.0);
    (width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0)
        .then_some((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autosize_model_uses_scene_object_size() {
        let model = serde_json::json!({"autosize": true, "material": "materials/layer.json"});
        let object = serde_json::json!({"size": "2318.00000 1794.00000"});

        assert_eq!(image_plane_extent(&model, &object), Some((2318.0, 1794.0)));
    }

    #[test]
    fn explicit_model_extent_has_priority() {
        let model = serde_json::json!({"width": 64, "height": 32});
        let object = serde_json::json!({"size": "100 100"});

        assert_eq!(image_plane_extent(&model, &object), Some((64.0, 32.0)));
    }
}
