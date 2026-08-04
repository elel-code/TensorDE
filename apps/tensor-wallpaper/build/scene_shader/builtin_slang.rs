//! Version-controlled Slang sources for the first-class scene stages.

pub(crate) fn generic_image_sources() -> (String, String) {
    (
        mesh_vertex_source(),
        include_str!("../../shaders/scene/genericimage4.frag.slang").to_owned(),
    )
}

pub(crate) fn composelayer_sources() -> (String, String) {
    (
        include_str!("../../shaders/scene/composelayer.vert.slang").to_owned(),
        include_str!("../../shaders/scene/composelayer.frag.slang").to_owned(),
    )
}

pub(crate) fn waterripple_slots_5_sources() -> (String, String) {
    (
        fullscreen_vertex_source(),
        include_str!("../../shaders/scene/waterripple_slots_5.frag.slang").to_owned(),
    )
}

pub(crate) fn image_effect_source_sources() -> (String, String) {
    (
        include_str!("../../shaders/scene/image_effect_source.vert.slang").to_owned(),
        include_str!("../../shaders/scene/image_effect_source.frag.slang").to_owned(),
    )
}

pub(crate) fn puppet_effect_composite_clipping_sources() -> (String, String) {
    (
        include_str!("../../shaders/scene/puppet_effect_composite_clipping.vert.slang").to_owned(),
        include_str!("../../shaders/scene/puppet_effect_composite_clipping.frag.slang").to_owned(),
    )
}

pub(crate) fn mesh_vertex_source() -> String {
    include_str!("../../shaders/scene/mesh_standard.vert.slang").to_owned()
}

pub(crate) fn iris_object_mesh_vertex_source() -> String {
    include_str!("../../shaders/scene/iris_object_mesh.vert.slang").to_owned()
}

fn fullscreen_vertex_source() -> String {
    include_str!("../../shaders/scene/effect_fullscreen.vert.slang").to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_stage_sources_are_slang_not_glsl_frontends() {
        let mut sources = vec![
            generic_image_sources().0,
            generic_image_sources().1,
            composelayer_sources().0,
            composelayer_sources().1,
            waterripple_slots_5_sources().0,
            waterripple_slots_5_sources().1,
            image_effect_source_sources().0,
            image_effect_source_sources().1,
            puppet_effect_composite_clipping_sources().0,
            puppet_effect_composite_clipping_sources().1,
        ];
        sources.push(mesh_vertex_source());
        sources.push(iris_object_mesh_vertex_source());

        for source in sources {
            assert!(source.contains("[[shader("));
            assert!(!source.contains("#version"));
            assert!(!source.contains("layout(set"));
        }
    }
}
