use super::*;

#[test]
fn pulse_catalog_strictly_resolves_semantic_variants_and_preserves_typed_bindings() {
    let audio = native_vulkan_scene_shader_for_key(
        "effects/pulse__SLOTS_3__AUDIOPROCESSING_3__BLENDMODE_2",
    )
    .expect("stereo Pulse catalog shader");
    assert_eq!(audio.parameter_layout, BuiltinSceneParameterLayout::Pulse);
    assert_eq!(
        audio.vertex_primitive,
        crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
    );
    assert_eq!(
        audio
            .fragment_bindings
            .iter()
            .filter(|binding| binding.kind == BuiltinSceneDescriptorBindingKind::SampledImage)
            .map(|binding| binding.register)
            .collect::<Vec<_>>(),
        [0]
    );
    let masked_audio = native_vulkan_scene_shader_for_key(
        "effects/pulse__SLOTS_7__AUDIOPROCESSING_3__MASK_1__BLENDMODE_2",
    )
    .expect("masked stereo Pulse catalog shader");
    assert_eq!(
        masked_audio
            .fragment_bindings
            .iter()
            .filter(|binding| binding.kind == BuiltinSceneDescriptorBindingKind::SampledImage)
            .map(|binding| binding.register)
            .collect::<Vec<_>>(),
        [0, 2]
    );
    assert!(audio.vertex.bindings.iter().any(|binding| {
        binding.kind == BuiltinSceneDescriptorBindingKind::UniformBuffer && binding.register == 3
    }));
    let object_vertex = native_vulkan_scene_vertex_shader_for_primitive(
        audio,
        crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh,
    )
    .expect("Pulse object-mesh vertex");
    for register in [2, 3] {
        assert!(object_vertex.bindings.iter().any(|binding| {
            binding.kind == BuiltinSceneDescriptorBindingKind::UniformBuffer
                && binding.register == register
        }));
    }

    let time = native_vulkan_scene_shader_for_key("effects/pulse__SLOTS_3")
        .expect("time/noise Pulse catalog shader");
    assert_eq!(
        time.fragment_bindings
            .iter()
            .filter(|binding| binding.kind == BuiltinSceneDescriptorBindingKind::SampledImage)
            .map(|binding| binding.register)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    for invalid in [
        "effects/pulse__SLOTS_3__AUDIOPROCESSING_4",
        "effects/pulse__SLOTS_3__AUDIOPROCESSING_3__AUDIOPROCESSING_3",
        "effects/pulse__SLOTS_1__AUDIOPROCESSING_3",
        "effects/pulse__SLOTS_1__UNSUPPORTED_1",
        "effects/pulse__SLOTS_1__PULSECOLOR_2",
    ] {
        assert!(
            native_vulkan_scene_shader_for_key(invalid).is_none(),
            "malformed Pulse semantic key {invalid:?} must miss"
        );
    }
}

#[test]
fn depth_parallax_catalog_resolves_quality_and_mask_without_legacy_keys() {
    let quality = native_vulkan_scene_shader_for_key(
        "effects/depthparallax__SLOTS_3__QUALITY_2",
    )
    .expect("quality Depth Parallax catalog shader");
    assert_eq!(
        quality.parameter_layout,
        BuiltinSceneParameterLayout::DepthParallax
    );
    assert_eq!(
        quality.vertex_primitive,
        crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
    );
    for kind in [
        BuiltinSceneDescriptorBindingKind::SampledImage,
        BuiltinSceneDescriptorBindingKind::Sampler,
    ] {
        assert_eq!(
            quality
                .fragment_bindings
                .iter()
                .filter(|binding| binding.kind == kind)
                .map(|binding| binding.register)
                .collect::<Vec<_>>(),
            [0, 1]
        );
    }
    for register in [2, 3] {
        assert!(quality.vertex.bindings.iter().any(|binding| {
            binding.kind == BuiltinSceneDescriptorBindingKind::UniformBuffer
                && binding.register == register
        }));
    }

    let masked = native_vulkan_scene_shader_for_key(
        "effects/depthparallax__SLOTS_7__QUALITY_0__MASK_1",
    )
    .expect("masked basic Depth Parallax catalog shader");
    assert_eq!(
        masked
            .fragment_bindings
            .iter()
            .filter(|binding| binding.kind == BuiltinSceneDescriptorBindingKind::SampledImage)
            .map(|binding| binding.register)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    for invalid in [
        "effects/depthparallax__SLOTS_1__QUALITY_2",
        "effects/depthparallax__SLOTS_3__QUALITY_3",
        "effects/depthparallax__SLOTS_3__QUALITY_2__QUALITY_2",
        "effects/depthparallax__SLOTS_3__QUALITY_2__MASK_1",
        "effects/depthparallax__SLOTS_3__UNKNOWN_1",
    ] {
        assert!(
            native_vulkan_scene_shader_for_key(invalid).is_none(),
            "malformed Depth Parallax semantic key {invalid:?} must miss"
        );
    }
}

#[test]
fn shake_catalog_resolves_direction_with_authored_default_slot_identity() {
    let shader = native_vulkan_scene_shader_for_key("effects/shake__SLOTS_7__DIRECTION_1")
        .expect("left-direction Shake catalog shader");
    assert_eq!(shader.parameter_layout, BuiltinSceneParameterLayout::Shake);
    assert_eq!(
        shader
            .fragment_bindings
            .iter()
            .filter(|binding| binding.kind == BuiltinSceneDescriptorBindingKind::SampledImage)
            .map(|binding| binding.register)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    assert!(shader.vertex.bindings.iter().any(|binding| {
        binding.kind == BuiltinSceneDescriptorBindingKind::UniformBuffer && binding.register == 3
    }));
    assert!(shader.fragment_source.contains("float2 uvOffset = offset"));
    assert!(!shader.fragment_source.contains("clamp(input.texCoord"));

    for invalid in [
        "effects/shake__SLOTS_3__DIRECTION_1",
        "effects/shake__SLOTS_7__DIRECTION_3",
        "effects/shake__SLOTS_7__DIRECTION_1__DIRECTION_1",
        "effects/shake__SLOTS_7__DIRECTION_1__TIMEOFFSET_1",
        "effects/shake__SLOTS_f__DIRECTION_1__MASK_1",
    ] {
        assert!(
            native_vulkan_scene_shader_for_key(invalid).is_none(),
            "unsupported Shake semantic key {invalid:?} must miss"
        );
    }
}

#[test]
fn waterwaves_slots_7_keeps_default_identity_but_only_live_o2_accesses() {
    let shader = native_vulkan_scene_shader_for_key("effects/waterwaves__SLOTS_7")
        .expect("masked WaterWaves shader with Texture2 default identity");
    for kind in [
        BuiltinSceneDescriptorBindingKind::SampledImage,
        BuiltinSceneDescriptorBindingKind::Sampler,
    ] {
        assert_eq!(
            shader
                .fragment_bindings
                .iter()
                .filter(|binding| binding.kind == kind)
                .map(|binding| binding.register)
                .collect::<Vec<_>>(),
            [0, 1]
        );
    }
}
