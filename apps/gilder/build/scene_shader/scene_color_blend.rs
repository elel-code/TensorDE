//! Exact native Slang sources for WE object color blending.

pub(crate) fn scene_color_blend_sources() -> (String, String) {
    (
        include_str!("../../shaders/scene/genericimage4_scene_color_blend.vert.slang").to_owned(),
        include_str!("../../shaders/scene/genericimage4_scene_color_blend.frag.slang").to_owned(),
    )
}
