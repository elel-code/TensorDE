use super::*;

pub(super) fn preserve_authored_puppet_effect_stream(
    object_is_puppet: bool,
    effects_in_authored_texture_space: bool,
) -> bool {
    object_is_puppet && effects_in_authored_texture_space
}

pub(super) fn authored_package_utility_primitive(
    primitive: RenderPassDrawPrimitive,
    shader_origin: WeIrShaderOrigin,
) -> RenderPassDrawPrimitive {
    if primitive == RenderPassDrawPrimitive::FullscreenTriangle
        && shader_origin == WeIrShaderOrigin::AuthoredPackage
    {
        RenderPassDrawPrimitive::ObjectUvSupportQuad
    } else {
        primitive
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_authored_texture_puppets_disable_effect_stream_aggregation() {
        assert!(preserve_authored_puppet_effect_stream(true, true));
        assert!(!preserve_authored_puppet_effect_stream(true, false));
        assert!(!preserve_authored_puppet_effect_stream(false, true));
        assert!(!preserve_authored_puppet_effect_stream(false, false));
    }

    #[test]
    fn authored_package_vertex_effects_retain_quad_geometry() {
        assert_eq!(
            authored_package_utility_primitive(
                RenderPassDrawPrimitive::FullscreenTriangle,
                WeIrShaderOrigin::AuthoredPackage,
            ),
            RenderPassDrawPrimitive::ObjectUvSupportQuad
        );
        assert_eq!(
            authored_package_utility_primitive(
                RenderPassDrawPrimitive::FullscreenTriangle,
                WeIrShaderOrigin::EngineBuiltIn,
            ),
            RenderPassDrawPrimitive::FullscreenTriangle
        );
        assert_eq!(
            authored_package_utility_primitive(
                RenderPassDrawPrimitive::ObjectMesh,
                WeIrShaderOrigin::AuthoredPackage,
            ),
            RenderPassDrawPrimitive::ObjectMesh
        );
    }
}
