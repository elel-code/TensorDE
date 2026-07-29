    use super::*;

    const SPIRV_OP_NAME: u16 = 5;
    const SPIRV_OP_VARIABLE: u16 = 59;
    const SPIRV_OP_DECORATE: u16 = 71;
    const SPIRV_STORAGE_UNIFORM_CONSTANT: u32 = 0;
    const SPIRV_STORAGE_INPUT: u32 = 1;
    const SPIRV_STORAGE_UNIFORM: u32 = 2;
    const SPIRV_STORAGE_OUTPUT: u32 = 3;
    const SPIRV_DECORATION_LOCATION: u32 = 30;
    const SPIRV_DECORATION_BINDING: u32 = 33;
    const SPIRV_DECORATION_DESCRIPTOR_SET: u32 = 34;

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

    fn spirv_named_id(words: &[u32], expected_name: &str) -> u32 {
        spirv_instructions(words)
            .into_iter()
            .find_map(|instruction| {
                let opcode = (instruction[0] & 0xffff) as u16;
                if opcode != SPIRV_OP_NAME || instruction.len() < 3 {
                    return None;
                }
                let mut bytes = instruction[2..]
                    .iter()
                    .flat_map(|word| word.to_le_bytes())
                    .collect::<Vec<_>>();
                bytes.truncate(bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len()));
                (bytes == expected_name.as_bytes()).then_some(instruction[1])
            })
            .unwrap_or_else(|| panic!("SPIR-V interface variable {expected_name:?} is missing"))
    }

    fn assert_spirv_variable(words: &[u32], name: &str, storage_class: u32) -> u32 {
        let id = spirv_named_id(words, name);
        assert!(
            spirv_instructions(words).into_iter().any(|instruction| {
                (instruction[0] & 0xffff) as u16 == SPIRV_OP_VARIABLE
                    && instruction.len() >= 4
                    && instruction[2] == id
                    && instruction[3] == storage_class
            }),
            "SPIR-V variable {name:?} does not use storage class {storage_class}"
        );
        id
    }

    fn assert_spirv_decoration(words: &[u32], id: u32, decoration: u32, value: u32) {
        assert!(
            spirv_instructions(words).into_iter().any(|instruction| {
                (instruction[0] & 0xffff) as u16 == SPIRV_OP_DECORATE
                    && instruction.len() >= 4
                    && instruction[1] == id
                    && instruction[2] == decoration
                    && instruction[3] == value
            }),
            "SPIR-V id {id} is missing decoration {decoration}={value}"
        );
    }

    fn assert_spirv_stage_interface(
        words: &[u32],
        name: &str,
        storage_class: u32,
        location: u32,
    ) {
        let id = assert_spirv_variable(words, name, storage_class);
        assert_spirv_decoration(words, id, SPIRV_DECORATION_LOCATION, location);
    }

    fn assert_spirv_material_uniform(words: &[u32]) {
        let id = assert_spirv_variable(words, "u_Effect", SPIRV_STORAGE_UNIFORM);
        assert_spirv_decoration(words, id, SPIRV_DECORATION_DESCRIPTOR_SET, 0);
        assert_spirv_decoration(words, id, SPIRV_DECORATION_BINDING, 3);
    }

    fn assert_spirv_sampled_binding(words: &[u32], name: &str, binding: u32) {
        let id = assert_spirv_variable(words, name, SPIRV_STORAGE_UNIFORM_CONSTANT);
        assert_spirv_decoration(words, id, SPIRV_DECORATION_DESCRIPTOR_SET, 0);
        assert_spirv_decoration(words, id, SPIRV_DECORATION_BINDING, binding);
    }

    #[test]
    fn shader_catalog_resolves_we_material_names_without_runtime_files() {
        let shader = native_vulkan_scene_shader_for_key("we/genericimage4")
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
        assert!(!shader.vertex_spirv.is_empty());
        assert!(shader.object_mesh_vertex_spirv.is_none());
        assert!(!shader.fragment_spirv.is_empty());
        assert!(shader.local_read_shader.is_none());
        assert!(native_vulkan_scene_shader_for_key("missing-shader").is_none());
        assert!(
            native_vulkan_scene_shader_for_key(
                "effects/lut_loader__SLOTS_3__CLAMP_0__QUAD_SIZE_64"
            )
            .is_none()
        );
        assert!(native_vulkan_scene_shader_for_key("genericimage4").is_none());
        assert!(native_vulkan_scene_shader_for_key("WE/genericimage4").is_none());
        assert!(native_vulkan_scene_shader_for_key(" we/genericimage4").is_none());
    }

    #[test]
    fn effect_catalog_carries_distinct_fullscreen_and_object_mesh_vertex_domains() {
        let shimmer = native_vulkan_scene_shader_for_key("effects/shimmer__SLOTS_9")
            .expect("shimmer shader");
        assert_eq!(
            shimmer.vertex_primitive,
            crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
        );
        let object_mesh = native_vulkan_scene_vertex_spirv_for_primitive(
            shimmer,
            crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh,
        )
        .expect("shimmer object-mesh vertex shader");
        assert_ne!(object_mesh, shimmer.vertex_spirv);
        assert_eq!(
            native_vulkan_scene_vertex_spirv_for_primitive(
                shimmer,
                crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
            ),
            Some(shimmer.vertex_spirv)
        );

        let iris = native_vulkan_scene_shader_for_key("effects/iris__SLOTS_3__MASK_1")
            .expect("iris shader");
        assert!(
            native_vulkan_scene_vertex_spirv_for_primitive(
                iris,
                crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh,
            )
            .is_none()
        );
    }

    #[test]
    fn cloudmotion_hoists_affine_noise_uv_without_changing_stage_interfaces() {
        let fullscreen_source = include_str!(concat!(
            env!("OUT_DIR"),
            "/scene_shader_catalog/effects_cloudmotion__SLOTS_5.vert.glsl"
        ));
        let object_mesh_source = include_str!(concat!(
            env!("OUT_DIR"),
            "/scene_shader_catalog/effects_cloudmotion__SLOTS_5__OBJECT_MESH.vert.glsl"
        ));
        for source in [fullscreen_source, object_mesh_source] {
            assert!(source.contains(
                "float aspect_scaled_x = u_Effect.g_ScaleScaleXAspectUnused.z * uv.x;"
            ));
            assert!(source.contains("* u_Effect.g_ScaleScaleXAspectUnused.x;"));
            assert!(source.contains(
                "scaled_uv.x * u_Effect.g_ScaleScaleXAspectUnused.y + time_offset"
            ));
            assert!(source.contains("v_NoiseTexCoord = vec2("));
            assert!(!source.contains("max("));
        }

        for key in [
            "effects/cloudmotion__SLOTS_1",
            "effects/cloudmotion__SLOTS_5",
        ] {
            let shader = native_vulkan_scene_shader_for_key(key).expect("cloudmotion shader");
            let object_mesh = native_vulkan_scene_vertex_spirv_for_primitive(
                shader,
                crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh,
            )
            .expect("cloudmotion object-mesh vertex shader");
            for vertex in [shader.vertex_spirv, object_mesh] {
                assert_spirv_stage_interface(
                    vertex,
                    "v_NoiseTexCoord",
                    SPIRV_STORAGE_OUTPUT,
                    1,
                );
                assert_spirv_material_uniform(vertex);
            }
            assert_spirv_stage_interface(
                shader.fragment_spirv,
                "v_NoiseTexCoord",
                SPIRV_STORAGE_INPUT,
                1,
            );
            assert_spirv_material_uniform(shader.fragment_spirv);
            assert!(shader
                .fragment_source
                .contains("float noise = texture(g_Texture2, v_NoiseTexCoord).x * 2.0 - 1.0;")
                || shader
                    .fragment_source
                    .contains("float noise = valueNoise(v_NoiseTexCoord) * 2.0 - 1.0;"));
            assert!(shader
                .fragment_source
                .contains("float angle = u_Effect.g_TimeSpeedAmountDirection.w + 1.570796;"));
            assert!(shader
                .fragment_source
                .contains("o_Color = texture(g_Texture0, v_TexCoord + offset);"));
            assert!(!shader.fragment_source.contains("clamp("));
            assert!(!shader.fragment_source.contains("noiseUv"));
        }
    }

    #[test]
    fn quantized_framebuffer_water_catalog_exposes_three_typed_stages() {
        let prepass = native_vulkan_scene_shader_for_key(
            "effects/caustics__SLOTS_3d__BLENDMODE_6__GILDER_FRAMEBUFFER_QUANTIZED_OVERLAY_1",
        )
        .expect("quantized caustics prepass shader");
        assert_eq!(prepass.parameter_layout, BuiltinSceneParameterLayout::Caustics);
        assert_eq!(
            prepass.vertex_primitive,
            crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
        );
        assert!(native_vulkan_scene_vertex_spirv_for_primitive(
            prepass,
            crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh,
        )
        .is_none());
        let prepass_vertex_source = include_str!(concat!(
            env!("OUT_DIR"),
            "/scene_shader_catalog/effects_caustics__SLOTS_3d__BLENDMODE_6__GILDER_FRAMEBUFFER_QUANTIZED_OVERLAY_1.vert.glsl"
        ));
        assert!(prepass_vertex_source.contains(
            "dot(u_Draw.g_ObjectUvToScreenUvRow0.xyz, vec3(uv, 1.0))"
        ));
        assert!(prepass_vertex_source
            .contains("layout(location = 0) out vec2 v_FramebufferCoord;"));
        assert!(prepass_vertex_source.contains("v_EffectCoord = uv;"));
        assert!(!prepass_vertex_source.contains(
            "dot(u_Draw.g_ScreenUvToObjectUvRow0.xyz, vec3(uv, 1.0))"
        ));
        assert!(!prepass.fragment_spirv.is_empty());
        assert!(prepass
            .fragment_source
            .contains("texture(g_Texture3, noiseCoords).ba"));
        assert!(prepass
            .fragment_source
            .contains("texture(g_Texture3, noiseCoords2).rg"));
        assert!(prepass
            .fragment_source
            .contains("texture(g_Texture4, shiftCoords).ba"));

        let intermediate = native_vulkan_scene_shader_for_key(
            "we/framebuffer-water-quantized-water-opacity",
        )
        .expect("quantized framebuffer-water water-opacity shader");
        assert_eq!(
            intermediate.parameter_layout,
            BuiltinSceneParameterLayout::FinalEffectProgram
        );
        assert_eq!(
            intermediate.vertex_primitive,
            crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
        );
        assert!(native_vulkan_scene_vertex_spirv_for_primitive(
            intermediate,
            crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh,
        )
        .is_none());
        assert!(!intermediate.vertex_spirv.is_empty());
        assert!(!intermediate.fragment_spirv.is_empty());
        assert!(intermediate
            .fragment_source
            .contains("texture(g_CausticsPrepass, v_TexCoord + waterOffset(v_TexCoord))"));
        assert!(intermediate
            .fragment_source
            .contains("if (u_Effect.g_StageEnabled.x <= 0.5) {\n        return vec2(0.0);"));
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

        let final_program = native_vulkan_scene_shader_for_key(
            "we/framebuffer-water-quantized-shake-final",
        )
        .expect("quantized framebuffer-water shake shader");
        assert_eq!(
            final_program.parameter_layout,
            BuiltinSceneParameterLayout::FinalEffectProgram
        );
        assert_eq!(
            final_program.vertex_primitive,
            crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh
        );
        assert!(!final_program.vertex_spirv.is_empty());
        assert!(!final_program.fragment_spirv.is_empty());
        assert!(final_program
            .fragment_source
            .contains("fract(time * 0.159155) * 6.283185"));
        assert!(final_program
            .fragment_source
            .contains("if (u_Effect.g_StageEnabled.x > 0.5) {"));
        assert!(final_program
            .fragment_source
            .contains("o_Color = texture(g_OpacityTarget, shake_uv);"));
        assert!(!final_program.fragment_source.contains("quantizeUnorm8"));
    }

    #[test]
    fn passthrough_catalog_exposes_only_the_explicit_exact_pixel_variant() {
        let passthrough = native_vulkan_scene_shader_for_key("we/passthrough")
            .expect("passthrough shader");
        let variant = passthrough
            .local_read_shader
            .expect("exact-pixel input-attachment variant");
        assert!(!variant.fragment_spirv.is_empty());
        assert_eq!(
            variant.input_attachments,
            &[BuiltinSceneInputAttachment {
                slot: 0,
                input_attachment_index: 0,
                binding: 64,
            }]
        );
        assert_eq!(variant.color_output_locations, &[0]);
        assert!(native_vulkan_scene_shader_for_key("we/genericimage4")
            .expect("generic image shader")
            .local_read_shader
            .is_none());
    }

    #[test]
    fn shader_catalog_carries_typed_effect_parameter_layouts() {
        assert_eq!(
            native_vulkan_scene_shader_for_key("effects/iris__SLOTS_3__MASK_1")
                .expect("iris shader")
                .parameter_layout,
            BuiltinSceneParameterLayout::Iris
        );
        assert_eq!(
            native_vulkan_scene_shader_for_key("effects/waterwaves__SLOTS_1")
                .expect("waterwaves shader")
                .parameter_layout,
            BuiltinSceneParameterLayout::WaterWaves
        );
        assert_eq!(
            native_vulkan_scene_shader_for_key("effects/foliagesway__SLOTS_1")
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
            let shader = native_vulkan_scene_shader_for_key(key).expect("object-local shader");
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
                native_vulkan_scene_shader_for_key(key).is_none(),
                "package-only shader {key:?} must be carried by the scene-owned SPIR-V ABI"
            );
        }
    }

    #[test]
    fn rounded_hsl_catalog_declares_exact_fragment_coordinate_snapshot_fetch() {
        let shader = native_vulkan_scene_shader_for_key("we/flat-rounded-hsl-source")
            .expect("rounded HSL source shader");

        assert_eq!(shader.fragment_coordinate_fetch_slot_mask, 1);
        assert!(shader.fragment_source.contains("texelFetch("));
        assert!(shader.fragment_source.contains("g_SceneSnapshot, ivec2(gl_FragCoord.xy)"));
        assert!(shader.fragment_source.contains("ivec2(gl_FragCoord.xy)"));
        assert!(shader.fragment_source.contains("roundEven(clamp(value, 0.0, 1.0) * 255.0)"));
        assert_eq!(shader.fragment_source.matches("roundedTargetTexel(").count(), 5);
        assert!(shader.fragment_source.contains("object_uv * vec2(source_extent) - 0.5"));
        assert!(shader.fragment_source.contains("destination.a * source.a"));
        // WE Color = source H/S + destination L (common_blending BlendColor), not setLum.
        assert!(shader.fragment_source.contains("blendColor(destination.rgb, source.rgb)"));
        assert!(shader.fragment_source.contains("rgbToHsl("));
        assert!(shader.fragment_source.contains("hslToRgb("));
        assert!(!shader.fragment_source.contains("setBlendLum"));
        assert!(native_vulkan_scene_shader_catalog()
            .iter()
            .filter(|shader| shader.key != "we/flat-rounded-hsl-source")
            .all(|shader| shader.fragment_coordinate_fetch_slot_mask == 0));
    }

    #[test]
    fn effect_target_waterwaves_identity_is_not_emitted_for_mesh_composites() {
        let effect = native_vulkan_scene_shader_for_key(
            "we/effect-waterwaves-direct__STAGES_6",
        )
        .expect("effect-target waterwaves shader");
        let image = native_vulkan_scene_shader_for_key(
            "we/image-waterwaves-direct__STAGES_6",
        )
        .expect("image waterwaves shader");
        let puppet = native_vulkan_scene_shader_for_key(
            "we/puppet-waterwaves-direct__STAGES_6",
        )
        .expect("puppet waterwaves shader");

        assert_ne!(effect.fragment_spirv, image.fragment_spirv);
        assert_eq!(image.fragment_spirv, puppet.fragment_spirv);

        let effect_two = native_vulkan_scene_shader_for_key(
            "we/effect-waterwaves-direct__STAGES_2",
        )
        .expect("two-stage effect-target waterwaves shader");
        let image_two = native_vulkan_scene_shader_for_key(
            "we/image-waterwaves-direct__STAGES_2",
        )
        .expect("two-stage image waterwaves shader");
        assert_eq!(effect_two.fragment_spirv, image_two.fragment_spirv);
    }
