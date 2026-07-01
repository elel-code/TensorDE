use super::*;

pub(super) fn native_vulkan_scene_rectangle_recordable_kind(
    op: &NativeVulkanSceneDrawOp,
) -> &'static str {
    if op
        .corner_radius
        .is_some_and(|radius| radius.is_finite() && radius > 0.0)
    {
        "rounded-rectangle"
    } else {
        "rectangle"
    }
}

pub(super) fn native_vulkan_scene_rgba_from_hex(color: &str, opacity: f64) -> Option<[f32; 4]> {
    let hex = color.trim().strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
    Some([r, g, b, opacity.clamp(0.0, 1.0) as f32])
}

pub(super) fn native_vulkan_scene_tint_from_color(color: Option<&str>) -> [f32; 4] {
    color
        .filter(|color| !color.is_empty())
        .and_then(|color| native_vulkan_scene_rgba_from_hex(color, 1.0))
        .unwrap_or(SCENE_SAMPLED_IMAGE_DEFAULT_TINT)
}
