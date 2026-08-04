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

    #[test]
    fn particle_stages_preserve_random_frame_and_interpolated_sequence_sampling() {
        let (vertex, fragment) = generic_particle_sources();
        assert!(vertex.contains("bool randomFrame ="));
        assert!(vertex.contains("coordinates.next = particle_texture_frame_uv"));
        assert!(vertex.contains("coordinates.blend = randomFrame ? 0.0 : fract(framePosition)"));
        assert!(vertex.contains("fmod(frame, rowStride) * frameWidth"));
        assert!(vertex.contains("floor(frame / rowStride) * frameHeight"));
        assert!(fragment.contains("texture(g_Texture0, v_TexCoordNext)"));
        assert!(fragment.contains("mix(texel, nextTexel, v_TextureSequenceBlend)"));
    }
}
