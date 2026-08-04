//! Exact Slang stages for the authored Waterflow effect.

pub(crate) fn waterflow_sources() -> (String, String) {
    (
        include_str!("../../shaders/scene/waterflow.vert.slang").to_owned(),
        include_str!("../../shaders/scene/waterflow.frag.slang").to_owned(),
    )
}

pub(crate) fn waterflow_object_mesh_vertex_source() -> String {
    include_str!("../../shaders/scene/waterflow_object_mesh.vert.slang").to_owned()
}
