use super::*;

#[test]
fn quantized_framebuffer_water_catalog_exposes_three_typed_stages() {
    let prepass = rendering_device_scene_shader_for_key(
        "effects/caustics__SLOTS_3d__BLENDMODE_6__TENSOR_WALLPAPER_FRAMEBUFFER_QUANTIZED_OVERLAY_1",
    )
    .expect("quantized caustics prepass shader");
    assert_eq!(
        prepass.parameter_layout,
        BuiltinSceneParameterLayout::Caustics
    );
    assert_eq!(
        prepass.vertex_primitive,
        crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
    );
    assert!(
        rendering_device_scene_vertex_spirv_for_primitive(
            prepass,
            crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh,
        )
        .is_none()
    );
    let prepass_vertex_source = include_str!(concat!(
        env!("OUT_DIR"),
        "/scene_shader_catalog/effects_caustics__SLOTS_3d__BLENDMODE_6__TENSOR_WALLPAPER_FRAMEBUFFER_QUANTIZED_OVERLAY_1.vert.source.slang"
    ));
    assert!(
        prepass_vertex_source.contains("dot(u_Draw.g_ObjectUvToScreenUvRow0.xyz, vec3(uv, 1.0))")
    );
    assert!(
        prepass_vertex_source.contains("[[vk::location(0)]] vec2 v_FramebufferCoord : TEXCOORD0;")
    );
    assert!(prepass_vertex_source.contains("v_EffectCoord = uv;"));
    assert!(
        !prepass_vertex_source.contains("dot(u_Draw.g_ScreenUvToObjectUvRow0.xyz, vec3(uv, 1.0))")
    );
    assert!(!prepass.fragment_spirv.is_empty());
    assert!(
        prepass
            .fragment_source
            .contains("texture(g_Texture3, noiseCoords).ba")
    );
    assert!(
        prepass
            .fragment_source
            .contains("texture(g_Texture3, noiseCoords2).rg")
    );
    assert!(
        prepass
            .fragment_source
            .contains("texture(g_Texture4, shiftCoords).ba")
    );

    let intermediate =
        rendering_device_scene_shader_for_key("we/framebuffer-water-quantized-water-opacity")
            .expect("quantized framebuffer-water water-opacity shader");
    assert_eq!(
        intermediate.parameter_layout,
        BuiltinSceneParameterLayout::FinalEffectProgram
    );
    assert_eq!(
        intermediate.vertex_primitive,
        crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
    );
    assert!(
        rendering_device_scene_vertex_spirv_for_primitive(
            intermediate,
            crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh,
        )
        .is_none()
    );
    assert!(!intermediate.vertex.spirv.is_empty());
    assert!(!intermediate.fragment_spirv.is_empty());
    assert!(
        intermediate
            .fragment_source
            .contains("texture(g_CausticsPrepass, v_TexCoord + waterOffset(v_TexCoord))")
    );
    assert!(
        intermediate
            .fragment_source
            .contains("if (u_Effect.g_StageEnabled.x <= 0.5) {\n        return vec2(0.0);")
    );
    assert!(intermediate
            .fragment_source
            .contains("if (u_Effect.g_StageEnabled.y > 0.5) {\n        color.a *= u_Effect.g_WavesDirectionExponentOpacityUnused.z;"));
    assert_eq!(
        intermediate
            .fragment_source
            .matches("color = quantizeUnorm8(color);")
            .count(),
        1,
        "water output must cross one explicit UNORM8 boundary before opacity"
    );
    assert!(!intermediate.fragment_source.contains("opacityTexel"));

    let final_program =
        rendering_device_scene_shader_for_key("we/framebuffer-water-quantized-shake-final")
            .expect("quantized framebuffer-water shake shader");
    assert_eq!(
        final_program.parameter_layout,
        BuiltinSceneParameterLayout::FinalEffectProgram
    );
    assert_eq!(
        final_program.vertex_primitive,
        crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh
    );
    assert!(!final_program.vertex.spirv.is_empty());
    assert!(!final_program.fragment_spirv.is_empty());
    assert!(
        final_program
            .fragment_source
            .contains("fract(time * 0.159155) * 6.283185")
    );
    assert!(
        final_program
            .fragment_source
            .contains("if (u_Effect.g_StageEnabled.x > 0.5) {")
    );
    assert!(
        final_program
            .fragment_source
            .contains("o_Color = texture(g_OpacityTarget, shake_uv);")
    );
    assert!(!final_program.fragment_source.contains("quantizeUnorm8"));
}
