use super::*;

#[path = "tests/framebuffer_water.rs"]
mod framebuffer_water;
mod workshop_effects;

const SPIRV_OP_VARIABLE: u16 = 59;
const SPIRV_OP_DECORATE: u16 = 71;
const SPIRV_OP_CAPABILITY: u16 = 17;
const SPIRV_OP_TYPE_IMAGE: u16 = 25;
const SPIRV_CAPABILITY_INPUT_ATTACHMENT: u32 = 40;
const SPIRV_CAPABILITY_DESCRIPTOR_HEAP_EXT: u32 = 5_128;
const SPIRV_BUILT_IN_RESOURCE_HEAP_EXT: u32 = 5_123;
const SPIRV_DIM_SUBPASS_DATA: u32 = 6;
const SPIRV_DECORATION_BUILT_IN: u32 = 11;
const SPIRV_DECORATION_BINDING: u32 = 33;
const SPIRV_DECORATION_DESCRIPTOR_SET: u32 = 34;
const SPIRV_DECORATION_ARRAY_STRIDE: u32 = 6;
const SPIRV_STORAGE_INPUT: u32 = 1;
const SPIRV_STORAGE_OUTPUT: u32 = 3;
const SPIRV_DECORATION_LOCATION: u32 = 30;

fn spirv_instructions(words: &[u32]) -> Vec<&[u32]> {
    assert!(words.len() >= 5, "SPIR-V module must contain its header");
    let mut instructions = Vec::new();
    let mut cursor = 5;
    while cursor < words.len() {
        let word_count = (words[cursor] >> 16) as usize;
        assert!(word_count > 0, "SPIR-V instruction cannot be empty");
        let end = cursor
            .checked_add(word_count)
            .expect("SPIR-V instruction offset overflow");
        assert!(end <= words.len(), "SPIR-V instruction exceeds module");
        instructions.push(&words[cursor..end]);
        cursor = end;
    }
    instructions
}

fn assert_spirv_stage_location(words: &[u32], storage_class: u32, location: u32) {
    let instructions = spirv_instructions(words);
    let ids = instructions
        .iter()
        .filter_map(|instruction| {
            if instruction.len() >= 4
                && (instruction[0] & 0xffff) as u16 == SPIRV_OP_VARIABLE
                && instruction[3] == storage_class
            {
                Some(instruction[2])
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(
        instructions.iter().any(|instruction| {
            (instruction[0] & 0xffff) as u16 == SPIRV_OP_DECORATE
                && instruction.len() >= 4
                && ids.contains(&instruction[1])
                && instruction[2] == SPIRV_DECORATION_LOCATION
                && instruction[3] == location
        }),
        "SPIR-V storage class {storage_class} has no interface at location {location}"
    );
}

#[test]
fn shader_catalog_resolves_we_material_names_without_runtime_files() {
    let shader = rendering_device_scene_shader_for_key("we/genericimage4")
        .expect("genericimage4 built-in shader");
    assert_eq!(shader.key, "we/genericimage4");
    assert_eq!(
        shader.parameter_layout,
        BuiltinSceneParameterLayout::StandardMaterial
    );
    assert_eq!(
        shader.vertex_primitive,
        crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh
    );
    assert!(!shader.vertex.spirv.is_empty());
    assert!(shader.object_mesh_vertex.is_none());
    assert!(!shader.fragment_spirv.is_empty());
    assert!(shader.local_read_shader.is_none());
    assert!(rendering_device_scene_shader_for_key("missing-shader").is_none());
    assert!(
        rendering_device_scene_shader_for_key("effects/lut_loader__SLOTS_3__CLAMP_0__QUAD_SIZE_64")
            .is_none()
    );
    assert!(rendering_device_scene_shader_for_key("genericimage4").is_none());
    assert!(rendering_device_scene_shader_for_key("WE/genericimage4").is_none());
    assert!(rendering_device_scene_shader_for_key(" we/genericimage4").is_none());
}

#[test]
fn masked_tint_catalog_preserves_mask_uv_and_descriptor_heap_bindings() {
    let shader = rendering_device_scene_shader_for_key("effects/tint__SLOTS_3")
        .expect("masked Tint catalog shader");
    assert_eq!(shader.parameter_layout, BuiltinSceneParameterLayout::Tint);
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
    assert!(shader.vertex.bindings.iter().any(|binding| {
        binding.kind == BuiltinSceneDescriptorBindingKind::UniformBuffer && binding.register == 3
    }));
    let object_vertex = rendering_device_scene_vertex_shader_for_primitive(
        shader,
        crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh,
    )
    .expect("masked Tint object-mesh vertex");
    for register in [2, 3] {
        assert!(object_vertex.bindings.iter().any(|binding| {
            binding.kind == BuiltinSceneDescriptorBindingKind::UniformBuffer
                && binding.register == register
        }));
    }
}

#[test]
fn generic_image_vertex_preserves_authored_mesh_input_locations() {
    let shader =
        rendering_device_scene_shader_for_key("we/genericimage4").expect("generic image shader");

    // Slang O2 may omit source variable names; locations are the stable vertex ABI.
    assert_spirv_stage_location(shader.vertex.spirv, SPIRV_STORAGE_INPUT, 0);
    assert_spirv_stage_location(shader.vertex.spirv, SPIRV_STORAGE_INPUT, 1);
    assert_spirv_stage_location(shader.vertex.spirv, SPIRV_STORAGE_INPUT, 2);
}

#[test]
fn scene_color_blend_applies_the_d3d_to_vulkan_y_normalization_once() {
    let source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/shaders/scene/genericimage4_scene_color_blend.vert.slang"
    ));
    assert!(source.contains("float3(projected.x, projected.y, projected.w)"));
    assert!(!source.contains("float3(projected.x, -projected.y, projected.w)"));

    let d3d_clip_y = 3.928_578_6_f32;
    let vm_screen_y = -d3d_clip_y * 0.5 + 0.5;
    let vulkan_clip_y = -d3d_clip_y;
    let rendered_screen_y = vulkan_clip_y * 0.5 + 0.5;
    assert_eq!(rendered_screen_y.to_bits(), vm_screen_y.to_bits());
}

#[test]
fn particle_compute_uses_storage_heap_push_data() {
    let shader = rendering_device_particle_compute_shader();
    assert_eq!(shader.push_constant_bytes, 20);
    assert_eq!(
        shader.bindings,
        &[
            BuiltinSceneDescriptorBinding {
                kind: BuiltinSceneDescriptorBindingKind::StorageBuffer,
                register: 0,
                push_offset: 0,
            },
            BuiltinSceneDescriptorBinding {
                kind: BuiltinSceneDescriptorBindingKind::StorageBuffer,
                register: 1,
                push_offset: 4,
            },
            BuiltinSceneDescriptorBinding {
                kind: BuiltinSceneDescriptorBindingKind::StorageBuffer,
                register: 2,
                push_offset: 8,
            },
            BuiltinSceneDescriptorBinding {
                kind: BuiltinSceneDescriptorBindingKind::StorageBuffer,
                register: 3,
                push_offset: 12,
            },
            BuiltinSceneDescriptorBinding {
                kind: BuiltinSceneDescriptorBindingKind::StorageBuffer,
                register: 4,
                push_offset: 16,
            },
        ]
    );
    let instructions = spirv_instructions(shader.spirv);
    assert!(instructions.iter().any(|instruction| {
        (instruction[0] & 0xffff) as u16 == SPIRV_OP_CAPABILITY
            && instruction.get(1) == Some(&SPIRV_CAPABILITY_DESCRIPTOR_HEAP_EXT)
    }));
    assert!(instructions.iter().any(|instruction| {
        (instruction[0] & 0xffff) as u16 == SPIRV_OP_DECORATE
            && instruction.get(2) == Some(&SPIRV_DECORATION_BUILT_IN)
            && instruction.get(3) == Some(&SPIRV_BUILT_IN_RESOURCE_HEAP_EXT)
    }));
    assert!(!instructions.iter().any(|instruction| {
        (instruction[0] & 0xffff) as u16 == SPIRV_OP_DECORATE
            && matches!(
                instruction.get(2),
                Some(&SPIRV_DECORATION_BINDING) | Some(&SPIRV_DECORATION_DESCRIPTOR_SET)
            )
    }));
    let array_strides = instructions
        .iter()
        .filter_map(|instruction| {
            ((instruction[0] & 0xffff) as u16 == SPIRV_OP_DECORATE
                && instruction.get(2) == Some(&SPIRV_DECORATION_ARRAY_STRIDE))
            .then(|| instruction.get(3).copied())
            .flatten()
        })
        .collect::<Vec<_>>();
    for required in [4, 16, 64, 304] {
        assert!(
            array_strides.contains(&required),
            "missing array stride {required}"
        );
    }
}

#[test]
fn effect_catalog_carries_distinct_fullscreen_and_object_mesh_vertex_domains() {
    let shimmer =
        rendering_device_scene_shader_for_key("effects/shimmer__SLOTS_9").expect("shimmer shader");
    assert_eq!(
        shimmer.vertex_primitive,
        crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
    );
    let object_mesh = rendering_device_scene_vertex_spirv_for_primitive(
        shimmer,
        crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh,
    )
    .expect("shimmer object-mesh vertex shader");
    assert_ne!(object_mesh, shimmer.vertex.spirv);
    assert_eq!(
        rendering_device_scene_vertex_spirv_for_primitive(
            shimmer,
            crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
        ),
        Some(shimmer.vertex.spirv)
    );

    let iris = rendering_device_scene_shader_for_key("effects/iris__SLOTS_3__MASK_1")
        .expect("iris shader");
    let iris_object_mesh = rendering_device_scene_vertex_shader_for_primitive(
        iris,
        crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh,
    )
    .expect("iris object-mesh vertex shader");
    assert_ne!(iris_object_mesh.spirv, iris.vertex.spirv);
    for register in [2, 3] {
        assert!(iris_object_mesh.bindings.iter().any(|binding| {
            binding.kind == BuiltinSceneDescriptorBindingKind::UniformBuffer
                && binding.register == register
        }));
    }

    for key in [
        "effects/depthparallax__SLOTS_3__QUALITY_2",
        "effects/shake__SLOTS_7__DIRECTION_1",
    ] {
        let shader = rendering_device_scene_shader_for_key(key).expect("typed effect shader");
        let object_mesh = rendering_device_scene_vertex_shader_for_primitive(
            shader,
            crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh,
        )
        .expect("typed effect object-mesh vertex shader");
        for register in [2, 3] {
            assert!(object_mesh.bindings.iter().any(|binding| {
                binding.kind == BuiltinSceneDescriptorBindingKind::UniformBuffer
                    && binding.register == register
            }));
        }
    }
}

#[test]
fn cloudmotion_hoists_affine_noise_uv_without_changing_stage_interfaces() {
    let fullscreen_source = include_str!(concat!(
        env!("OUT_DIR"),
        "/scene_shader_catalog/effects_cloudmotion__SLOTS_5.vert.source.slang"
    ));
    let object_mesh_source = include_str!(concat!(
        env!("OUT_DIR"),
        "/scene_shader_catalog/effects_cloudmotion__SLOTS_5__OBJECT_MESH.vert.source.slang"
    ));
    for source in [fullscreen_source, object_mesh_source] {
        assert!(
            source.contains("float aspect_scaled_x = u_Effect.g_ScaleScaleXAspectUnused.z * uv.x;")
        );
        assert!(source.contains("* u_Effect.g_ScaleScaleXAspectUnused.x;"));
        assert!(
            source.contains("scaled_uv.x * u_Effect.g_ScaleScaleXAspectUnused.y + time_offset")
        );
        assert!(source.contains("v_NoiseTexCoord = vec2("));
        assert!(!source.contains("max("));
    }

    for key in [
        "effects/cloudmotion__SLOTS_1",
        "effects/cloudmotion__SLOTS_5",
    ] {
        let shader = rendering_device_scene_shader_for_key(key).expect("cloudmotion shader");
        let object_mesh = rendering_device_scene_vertex_shader_for_primitive(
            shader,
            crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh,
        )
        .expect("cloudmotion object-mesh vertex shader");
        for vertex in [shader.vertex, object_mesh] {
            assert_spirv_stage_location(vertex.spirv, SPIRV_STORAGE_OUTPUT, 1);
            assert!(vertex.bindings.iter().any(|binding| {
                binding.kind == BuiltinSceneDescriptorBindingKind::UniformBuffer
                    && binding.register == 3
            }));
        }
        assert_spirv_stage_location(shader.fragment_spirv, SPIRV_STORAGE_INPUT, 1);
        assert!(
            shader.fragment_bindings.iter().any(|binding| {
                binding.kind == BuiltinSceneDescriptorBindingKind::UniformBuffer
            })
        );
        assert!(
            shader
                .fragment_source
                .contains("float noise = texture(g_Texture2, v_NoiseTexCoord).x * 2.0 - 1.0;")
                || shader
                    .fragment_source
                    .contains("float noise = valueNoise(v_NoiseTexCoord) * 2.0 - 1.0;")
        );
        assert!(
            shader
                .fragment_source
                .contains("float angle = u_Effect.g_TimeSpeedAmountDirection.w + 1.570796;")
        );
        assert!(
            shader
                .fragment_source
                .contains("o_Color = texture(g_Texture0, v_TexCoord + offset);")
        );
        assert!(!shader.fragment_source.contains("clamp("));
        assert!(!shader.fragment_source.contains("noiseUv"));
    }
}

#[test]
fn every_builtin_fragment_uses_typed_descriptor_heap_push_data() {
    for shader in rendering_device_scene_shader_catalog() {
        assert!(!shader.fragment_bindings.is_empty(), "{}", shader.key);
        assert_eq!(
            shader.fragment_push_constant_bytes as usize,
            shader.fragment_bindings.len() * 4,
            "{}",
            shader.key
        );
    }
}

#[test]
fn every_builtin_vertex_uses_descriptor_free_or_pipeline_global_push_data() {
    for shader in rendering_device_scene_shader_catalog() {
        for vertex in [Some(shader.vertex), shader.object_mesh_vertex]
            .into_iter()
            .flatten()
        {
            if vertex.bindings.is_empty() {
                assert_eq!(vertex.push_constant_bytes, 0, "{}", shader.key);
                continue;
            }
            assert_eq!(
                vertex.push_constant_bytes as usize,
                shader.fragment_push_constant_bytes as usize + vertex.bindings.len() * 4,
                "{}",
                shader.key
            );
            for binding in vertex.bindings {
                assert!(
                    binding.push_offset >= shader.fragment_push_constant_bytes,
                    "{}",
                    shader.key
                );
                assert!(matches!(
                    (binding.kind, binding.register),
                    (BuiltinSceneDescriptorBindingKind::UniformBuffer, 2 | 3)
                        | (BuiltinSceneDescriptorBindingKind::StorageBuffer, 4 | 5)
                ));
            }
        }
    }
}

#[test]
fn generic_particle_vertex_reads_the_retained_particle_storage_lane() {
    let shader = rendering_device_scene_shader_for_key("we/genericparticle")
        .expect("generic particle shader");
    assert!(shader.vertex.bindings.iter().any(|binding| {
        binding.kind == BuiltinSceneDescriptorBindingKind::StorageBuffer && binding.register == 5
    }));
}

#[test]
fn clipping_final_catalog_preserves_authored_non_contiguous_texture_slots() {
    let shader = rendering_device_scene_shader_for_key("we/puppet-iris-waterripple-clipping-final")
        .expect("puppet iris clipping shader");
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
            [0, 1, 2, 8, 35],
        );
    }
}

#[test]
fn puppet_effect_composite_clipping_catalog_exposes_terminal_mask_slot() {
    let shader = rendering_device_scene_shader_for_key("we/puppet-effect-composite-clipping")
        .expect("puppet effect clipping shader");
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
            [0, 8]
        );
    }
    assert!(shader.vertex.bindings.iter().any(|binding| {
        binding.kind == BuiltinSceneDescriptorBindingKind::StorageBuffer && binding.register == 4
    }));
}

#[test]
fn puppet_effect_composite_clipping_samples_the_mask_in_projected_screen_space() {
    let vertex_source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/shaders/scene/puppet_effect_composite_clipping.vert.slang"
    ));
    let fragment_source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/shaders/scene/puppet_effect_composite_clipping.frag.slang"
    ));
    assert!(
        vertex_source.contains("[[vk::location(2)]] float3 clippingScreenPosition : TEXCOORD2;")
    );
    assert!(vertex_source.contains("output.clippingScreenPosition = projectedPosition.xyw;"));
    assert!(fragment_source.contains(
        "float2 screenUv = (input.clippingScreenPosition.xy / input.clippingScreenPosition.z) * 0.5 + 0.5;"
    ));
    assert!(fragment_source.contains("clippingMask.Sample(clippingMaskSampler, screenUv).r"));
    assert!(
        !fragment_source
            .contains("clippingMask.Sample(clippingMaskSampler, input.effectTexCoord).r")
    );

    let shader = rendering_device_scene_shader_for_key("we/puppet-effect-composite-clipping")
        .expect("puppet effect clipping shader");
    assert_spirv_stage_location(shader.vertex.spirv, SPIRV_STORAGE_OUTPUT, 2);
    assert_spirv_stage_location(shader.fragment_spirv, SPIRV_STORAGE_INPUT, 2);
}

#[test]
fn passthrough_catalog_exposes_only_the_explicit_exact_pixel_variant() {
    let passthrough =
        rendering_device_scene_shader_for_key("we/passthrough").expect("passthrough shader");
    let variant = passthrough
        .local_read_shader
        .expect("exact-pixel input-attachment variant");
    assert!(!variant.fragment_spirv.is_empty());
    assert_eq!(variant.push_constant_bytes, 12);
    assert_eq!(
        variant.bindings,
        &[BuiltinSceneDescriptorBinding {
            kind: BuiltinSceneDescriptorBindingKind::InputAttachment,
            register: 64,
            push_offset: passthrough.fragment_push_constant_bytes,
        }]
    );
    assert_eq!(
        variant.input_attachments,
        &[BuiltinSceneInputAttachment {
            slot: 0,
            input_attachment_index: 0,
            binding: 64,
        }]
    );
    assert_eq!(variant.color_output_locations, &[0]);
    let instructions = spirv_instructions(variant.fragment_spirv);
    assert!(instructions.iter().any(|instruction| {
        (instruction[0] & 0xffff) as u16 == SPIRV_OP_CAPABILITY
            && instruction.get(1) == Some(&SPIRV_CAPABILITY_INPUT_ATTACHMENT)
    }));
    assert!(instructions.iter().any(|instruction| {
        (instruction[0] & 0xffff) as u16 == SPIRV_OP_CAPABILITY
            && instruction.get(1) == Some(&SPIRV_CAPABILITY_DESCRIPTOR_HEAP_EXT)
    }));
    assert!(instructions.iter().any(|instruction| {
        (instruction[0] & 0xffff) as u16 == SPIRV_OP_TYPE_IMAGE
            && instruction.get(3) == Some(&SPIRV_DIM_SUBPASS_DATA)
    }));
    assert!(instructions.iter().any(|instruction| {
        (instruction[0] & 0xffff) as u16 == SPIRV_OP_DECORATE
            && instruction.get(2) == Some(&SPIRV_DECORATION_BUILT_IN)
            && instruction.get(3) == Some(&SPIRV_BUILT_IN_RESOURCE_HEAP_EXT)
    }));
    assert!(!instructions.iter().any(|instruction| {
        (instruction[0] & 0xffff) as u16 == SPIRV_OP_DECORATE
            && matches!(
                instruction.get(2),
                Some(&SPIRV_DECORATION_BINDING) | Some(&SPIRV_DECORATION_DESCRIPTOR_SET)
            )
    }));
    assert!(
        rendering_device_scene_shader_for_key("we/genericimage4")
            .expect("generic image shader")
            .local_read_shader
            .is_none()
    );
}

#[test]
fn passthrough_catalog_has_object_mesh_vertex_for_effect_visibility_passthrough() {
    let passthrough =
        rendering_device_scene_shader_for_key("we/passthrough").expect("passthrough shader");
    let object_mesh = rendering_device_scene_vertex_shader_for_primitive(
        passthrough,
        crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh,
    )
    .expect("object-mesh passthrough vertex");
    assert!(object_mesh.bindings.iter().any(|binding| {
        binding.kind == BuiltinSceneDescriptorBindingKind::UniformBuffer && binding.register == 2
    }));
}

#[test]
fn passthrough_catalog_has_retained_object_uv_quad_vertex() {
    let passthrough =
        rendering_device_scene_shader_for_key("we/passthrough").expect("passthrough shader");
    let object_uv_quad = rendering_device_scene_vertex_shader_for_primitive(
        passthrough,
        crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad,
    )
    .expect("retained object-UV quad passthrough vertex");

    assert_eq!(
        object_uv_quad.spirv,
        passthrough
            .object_mesh_vertex
            .expect("retained vertex")
            .spirv
    );
    assert!(object_uv_quad.bindings.iter().any(|binding| {
        binding.kind == BuiltinSceneDescriptorBindingKind::UniformBuffer && binding.register == 2
    }));
}

#[test]
fn shader_catalog_carries_typed_effect_parameter_layouts() {
    assert_eq!(
        rendering_device_scene_shader_for_key("effects/iris__SLOTS_3__MASK_1")
            .expect("iris shader")
            .parameter_layout,
        BuiltinSceneParameterLayout::Iris
    );
    assert_eq!(
        rendering_device_scene_shader_for_key("effects/waterwaves__SLOTS_1")
            .expect("waterwaves shader")
            .parameter_layout,
        BuiltinSceneParameterLayout::WaterWaves
    );
    assert_eq!(
        rendering_device_scene_shader_for_key("effects/foliagesway__SLOTS_1")
            .expect("foliage sway shader")
            .parameter_layout,
        BuiltinSceneParameterLayout::FoliageSway
    );
    for (key, layout) in [
        (
            "effects/blur_combine__SLOTS_5__BLENDMODE_1__COMPOSITE_1",
            BuiltinSceneParameterLayout::BlurCombine,
        ),
        (
            "effects/blur_combine__SLOTS_5__BLENDMODE_2__COMPOSITE_1",
            BuiltinSceneParameterLayout::BlurCombine,
        ),
        (
            "effects/blur_gaussian__SLOTS_1__VERTICAL_1",
            BuiltinSceneParameterLayout::BlurGaussian,
        ),
        (
            "effects/scroll__SLOTS_1",
            BuiltinSceneParameterLayout::Scroll,
        ),
        ("effects/skew__SLOTS_1", BuiltinSceneParameterLayout::Skew),
        (
            "effects/waterflow__SLOTS_7",
            BuiltinSceneParameterLayout::WaterFlow,
        ),
        (
            "we/puppet-waterwaves-direct__STAGES_7",
            BuiltinSceneParameterLayout::WaterWavesDirect,
        ),
        (
            "we/effect-waterwaves-direct__STAGES_6",
            BuiltinSceneParameterLayout::WaterWavesDirect,
        ),
    ] {
        let shader = rendering_device_scene_shader_for_key(key).expect("object-local shader");
        assert_eq!(shader.parameter_layout, layout);
        assert!(shader.fragment_spirv.len() > 200);
    }
}

#[test]
fn package_only_programs_are_absent_from_the_builtin_catalog() {
    for key in [
        "effects/111__SLOTS_1__BLENDMODE_7",
        "effects/111__SLOTS_1__BLENDMODE_31",
        "effects/audioline__SLOTS_1",
        "effects/audio_responsive_oscilloscope__SLOTS_5__RESOLUTION_16",
        "effects/auto_sway__SLOTS_1__DEBUG_0__DEBUG_NO_ALPHA_1__NODE_COUNT_4",
        "effects/clipping_mask__SLOTS_9",
        "effects/clipping_mask__SLOTS_b",
        "effects/clipping_mask__SLOTS_f",
        "effects/custom_user_texture__SLOTS_3__WRITEALPHA_1",
        "effects/gradient_color__SLOTS_1__AXIS_1__BLENDMODE_0",
        "effects/huan__SLOTS_1",
        "effects/procedural_noise__SLOTS_1__AA_CATEGORY_1__AB_TYPEUV_4",
        "effects/procedural_noise__SLOTS_1__AA_CATEGORY_1__BLENDMODE_20__STEPANIM_1",
        "effects/qiu__SLOTS_1__CUSTOMCOLOR_1__RAINBOW_0__SPHERE_SOLID_COLOR_1",
        "effects/raindrop_on_glass__SLOTS_1",
        "effects/rounded_mask__SLOTS_1__B_SQUARE_0__C_ALPHA_ONLY_0__SOFT_1",
        "effects/rounded_mask__SLOTS_1__B_SQUARE_0__SEDIRECTION_1__SOFT_1",
        "effects/rounded_mask__SLOTS_1__B_SQUARE_0__SOFT_1",
        "effects/rounded_mask__SLOTS_1__SOFT_1",
        "effects/rounded_mask_effect_edit__SLOTS_1__B_SQUARE_0__SOFT_1",
        "effects/simple_audio_bars__SLOTS_1__ANTIALIAS_0__SHAPE_7",
        "effects/simple_audio_bars__SLOTS_1__SHAPE_7",
        "effects/tech_circle__SLOTS_1__SECTOR_SEGMENTS_1",
        "effects/user_texture_alpha_overwrite_workaround__SLOTS_1",
    ] {
        assert!(
            rendering_device_scene_shader_for_key(key).is_none(),
            "package-only shader {key:?} must be carried by the scene-owned SPIR-V ABI"
        );
    }
}

#[test]
fn rounded_hsl_catalog_declares_exact_fragment_coordinate_snapshot_fetch() {
    let shader = rendering_device_scene_shader_for_key("we/flat-rounded-hsl-source")
        .expect("rounded HSL source shader");

    assert_eq!(shader.fragment_coordinate_fetch_slot_mask, 1);
    assert!(shader.fragment_source.contains("texelFetch("));
    assert!(
        shader
            .fragment_source
            .contains("g_SceneSnapshot, ivec2(gl_FragCoord.xy)")
    );
    assert!(shader.fragment_source.contains("ivec2(gl_FragCoord.xy)"));
    assert!(
        shader
            .fragment_source
            .contains("roundEven(clamp(value, 0.0, 1.0) * 255.0)")
    );
    assert_eq!(
        shader
            .fragment_source
            .matches("roundedTargetTexel(")
            .count(),
        5
    );
    assert!(
        shader
            .fragment_source
            .contains("object_uv * vec2(source_extent) - 0.5")
    );
    assert!(shader.fragment_source.contains("destination.a * source.a"));
    // WE Color = source H/S + destination L (common_blending BlendColor), not setLum.
    assert!(
        shader
            .fragment_source
            .contains("blendColor(destination.rgb, source.rgb)")
    );
    assert!(shader.fragment_source.contains("rgbToHsl("));
    assert!(shader.fragment_source.contains("hslToRgb("));
    assert!(!shader.fragment_source.contains("setBlendLum"));
    assert!(
        rendering_device_scene_shader_catalog()
            .iter()
            .filter(|shader| shader.key != "we/flat-rounded-hsl-source")
            .all(|shader| shader.fragment_coordinate_fetch_slot_mask == 0)
    );
}

#[test]
fn effect_target_waterwaves_identity_is_not_emitted_for_mesh_composites() {
    let effect = rendering_device_scene_shader_for_key("we/effect-waterwaves-direct__STAGES_6")
        .expect("effect-target waterwaves shader");
    let image = rendering_device_scene_shader_for_key("we/image-waterwaves-direct__STAGES_6")
        .expect("image waterwaves shader");
    let puppet = rendering_device_scene_shader_for_key("we/puppet-waterwaves-direct__STAGES_6")
        .expect("puppet waterwaves shader");

    assert_ne!(effect.fragment_spirv, image.fragment_spirv);
    assert_eq!(image.fragment_spirv, puppet.fragment_spirv);

    let effect_two = rendering_device_scene_shader_for_key("we/effect-waterwaves-direct__STAGES_2")
        .expect("two-stage effect-target waterwaves shader");
    let image_two = rendering_device_scene_shader_for_key("we/image-waterwaves-direct__STAGES_2")
        .expect("two-stage image waterwaves shader");
    assert_eq!(effect_two.fragment_spirv, image_two.fragment_spirv);
}
