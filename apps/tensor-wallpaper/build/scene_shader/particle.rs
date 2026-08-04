//! Version-controlled Slang particle stages.

pub(crate) fn generic_particle_sources() -> (String, String) {
    (
        include_str!("../../shaders/scene/genericparticle.vert.slang").to_owned(),
        include_str!("../../shaders/scene/genericparticle.frag.slang").to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::generic_particle_sources;

    #[test]
    fn particle_stages_are_slang_sources() {
        for source in [generic_particle_sources().0, generic_particle_sources().1] {
            assert!(source.contains("[[shader("));
            assert!(!source.contains("#version"));
            assert!(!source.contains("layout(set"));
        }
    }

}
