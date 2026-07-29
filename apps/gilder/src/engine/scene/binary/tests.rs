use super::*;

#[test]
fn scene_binary_rejects_the_immediately_previous_layout() {
    let mut bytes = Vec::new();
    write_scene_binary(&SceneBinaryDocument::default(), &mut bytes).expect("write current scene");
    let previous = SCENE_BINARY_VERSION - 1;
    bytes[8..12].copy_from_slice(&previous.to_le_bytes());
    assert!(matches!(
        read_scene_binary_bytes(&bytes),
        Err(SceneBinaryError::UnsupportedVersion(version)) if version == previous
    ));
}

#[test]
fn scene_binary_round_trip_preserves_typed_image_access_masks() {
    let document = SceneBinaryDocument {
        strings: vec!["local-read".to_owned(), "pipeline".to_owned()],
        shader_contracts: vec![SceneShaderContractRecord {
            shader_key: SceneStringId(0),
            pipeline_key: SceneStringId(1),
            texture_slot_mask: 1 << 1,
            input_attachment_slot_mask: 1 << 2,
            constant_start: 0,
            constant_count: 0,
            resource_heap_count: 2,
            sampler_heap_count: 1,
        }],
        ..SceneBinaryDocument::default()
    };
    let mut bytes = Vec::new();
    write_scene_binary(&document, &mut bytes).expect("write scene");
    let decoded = read_scene_binary_bytes(&bytes).expect("read scene");

    assert_eq!(decoded.shader_contracts, document.shader_contracts);
}

#[test]
fn scene_binary_round_trip_preserves_only_spirv_and_compact_shader_abi() {
    let document = SceneBinaryDocument {
        strings: vec![
            "workshop/example/effects/wave__SLOTS_1".to_owned(),
            "main".to_owned(),
            "v_TexCoord".to_owned(),
            "fragColor".to_owned(),
            "GlobalParams".to_owned(),
            "g_Time".to_owned(),
            "time".to_owned(),
        ],
        shader_programs: vec![SceneShaderProgramRecord {
            program_key: SceneStringId(0),
            stage: SceneShaderStage::Fragment,
            entry_point: SceneStringId(1),
            spirv_start: 0,
            spirv_count: 6,
            binding_start: 0,
            binding_count: 1,
            stage_io_start: 0,
            stage_io_count: 2,
            uniform_buffer_start: 0,
            uniform_buffer_count: 1,
            push_constant_bytes: 4,
        }],
        shader_bindings: vec![SceneShaderBindingRecord {
            kind: SceneShaderBindingKind::UniformBuffer,
            register: 0,
            descriptor_count: 1,
            push_offset: 0,
        }],
        shader_stage_io: vec![
            SceneShaderStageIoRecord {
                name: SceneStringId(2),
                direction: SceneShaderIoDirection::Input,
                location: 0,
                scalar_type: SceneShaderScalarType::F32,
                rows: 2,
                columns: 1,
                location_count: 1,
            },
            SceneShaderStageIoRecord {
                name: SceneStringId(3),
                direction: SceneShaderIoDirection::Output,
                location: 0,
                scalar_type: SceneShaderScalarType::F32,
                rows: 4,
                columns: 1,
                location_count: 1,
            },
        ],
        shader_uniform_buffers: vec![SceneShaderUniformBufferRecord {
            name: SceneStringId(4),
            register: 0,
            byte_size: 16,
            member_start: 0,
            member_count: 1,
        }],
        shader_uniform_members: vec![SceneShaderUniformMemberRecord {
            name: SceneStringId(5),
            material_parameter: SceneStringId(6),
            byte_offset: 0,
            byte_size: 4,
            scalar_type: SceneShaderScalarType::F32,
            rows: 1,
            columns: 1,
            array_count: 1,
            array_stride: 0,
            matrix_stride: 0,
        }],
        shader_spirv: vec![0x0723_0203, 0x0001_0600, 0, 2, 0, 0x0001_0000],
        ..SceneBinaryDocument::default()
    };
    let mut bytes = Vec::new();
    write_scene_binary(&document, &mut bytes).expect("write scene");
    let decoded = read_scene_binary_bytes(&bytes).expect("read scene");

    assert_eq!(decoded.shader_programs, document.shader_programs);
    assert_eq!(decoded.shader_bindings, document.shader_bindings);
    assert_eq!(decoded.shader_stage_io, document.shader_stage_io);
    assert_eq!(
        decoded.shader_uniform_buffers,
        document.shader_uniform_buffers
    );
    assert_eq!(
        decoded.shader_uniform_members,
        document.shader_uniform_members
    );
    assert_eq!(decoded.shader_spirv, document.shader_spirv);
    assert!(!bytes.windows(4).any(|bytes| bytes == b"GLSL"));
    assert!(!bytes.windows(5).any(|bytes| bytes == b"Slang"));
}

#[test]
fn scene_binary_round_trip_keeps_chunked_payloads_and_handles() {
    let mut document = SceneBinaryDocument {
        strings: vec![
            "title".to_owned(),
            "scene".to_owned(),
            "scene.json".to_owned(),
            "models/a.json".to_owned(),
            "loose".to_owned(),
            "eye".to_owned(),
            "eye-bone".to_owned(),
            "blink".to_owned(),
            "loop".to_owned(),
            "masks/eye".to_owned(),
        ],
        project: SceneProjectRecord {
            title: SceneStringId(0),
            wallpaper_type: SceneStringId(1),
            scene_file: SceneStringId(2),
            logical_width: 1920,
            logical_height: 1080,
            ..empty_project_record()
        },
        resource_payload: vec![1, 2, 3, 4],
        ..SceneBinaryDocument::default()
    };
    document.resources.push(SceneResourceRecord {
        id: SceneResourceId(0),
        kind: SceneResourceKind::ModelJson,
        path: SceneStringId(3),
        source: SceneStringId(4),
        payload_offset: 0,
        payload_len: 4,
    });
    document
        .object_animation_layers
        .push(SceneObjectAnimationLayerRecord {
            object: SceneObjectHandle(0),
            animation_id: 475,
            layer_index: 2,
            additive: true,
            autosort: false,
            visible: true,
            playback_rate: 0.8,
            blend_weight: 0.7,
            initial_progress: 0.94,
        });
    document
        .object_transform_tracks
        .push(SceneObjectTransformTrackRecord {
            object: SceneObjectHandle(0),
            property: SceneObjectTransformProperty::Origin,
            flags: SCENE_OBJECT_TRANSFORM_TRACK_RELATIVE | SCENE_OBJECT_TRANSFORM_TRACK_WRAP_LOOP,
            playback: SceneStringId(8),
            fps: 30.0,
            frame_count: 360,
            channel_start: 0,
            channel_count: 1,
        });
    document
        .object_transform_channels
        .push(SceneObjectTransformChannelRecord {
            track: 0,
            component: 0,
            kind: SceneObjectTransformChannelKind::Keyframed,
            offset: 0.0,
            amplitude: 0.0,
            frequency: 0.0,
            phase: 0.0,
            keyframe_start: 0,
            keyframe_count: 1,
        });
    document
        .object_transform_keyframes
        .push(SceneObjectTransformKeyframeRecord {
            frame: 180.0,
            value: 24.32251,
            back: [-1.0, 0.0],
            front: [1.0, 0.0],
            flags: SCENE_OBJECT_TRANSFORM_KEYFRAME_BACK_ENABLED
                | SCENE_OBJECT_TRANSFORM_KEYFRAME_FRONT_ENABLED,
        });
    document
        .puppet_animation_transform_samples
        .push(ScenePuppetAnimationTransformSampleRecord {
            translation: SceneVec3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            rotation: SceneVec3::default(),
            scale: SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
        });
    document.puppet_animation_opacity_samples.push(0.5);
    document
        .puppet_animation_tracks
        .push(ScenePuppetAnimationTrackRecord {
            clip: 0,
            bone_index: 41,
            track_flags: 7,
            sample_start: 0,
            sample_count: 1,
            opacity_flags: 9,
            opacity_sample_start: 0,
            opacity_sample_count: 1,
        });
    document
        .puppet_animation_clips
        .push(ScenePuppetAnimationClipRecord {
            puppet: 0,
            clip_id: 475,
            flags: 3,
            name: SceneStringId(7),
            playback: SceneStringId(8),
            fps: 30.0,
            frame_count: 1,
            frame_metadata: 99,
            track_start: 0,
            track_count: 1,
        });
    document.objects.push(SceneObjectRecord {
        id: SceneObjectHandle(0),
        we_id: 7,
        name: SceneStringId::NONE,
        kind: SceneObjectKind::Puppet,
        resource: SceneResourceId(0),
        material: SceneMaterialHandle(INVALID_MATERIAL_ID),
        parent_we_id: INVALID_OBJECT_ID,
        attachment: SceneStringId::NONE,
        origin: SceneVec3::default(),
        angles: SceneVec3::default(),
        scale: SceneVec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
        color: SceneVec3 {
            x: 0.1,
            y: 0.2,
            z: 0.3,
        },
        alpha: 0.4,
        visible: true,
        color_blend_mode: 2,
        sort_order: 0,
        effect_start: u32::MAX,
        effect_count: 0,
        render_graph: u32::MAX,
    });
    document.camera_parallax = SceneCameraParallaxRecord {
        enabled: true,
        amount: 0.5,
        delay: 0.1,
        mouse_influence: 0.5,
    };
    document
        .object_parallax_depths
        .push(SceneObjectParallaxDepthRecord {
            object: SceneObjectHandle(0),
            depth: [-0.1, 0.0],
        });
    document.meshes.push(SceneMeshRecord {
        object: SceneObjectHandle(0),
        material: SceneMaterialHandle(INVALID_MATERIAL_ID),
        vertex_start: 0,
        vertex_count: 4,
        index_start: 0,
        index_count: 6,
        width: 64.0,
        height: 32.0,
        bounds_min: SceneVec3 {
            x: -32.0,
            y: -16.0,
            z: 0.0,
        },
        bounds_max: SceneVec3 {
            x: 32.0,
            y: 16.0,
            z: 0.0,
        },
    });
    document.mesh_vertices.extend([
        SceneMeshVertexRecord {
            position: SceneVec3 {
                x: -32.0,
                y: -16.0,
                z: 0.0,
            },
            uv: [0.0, 1.0],
            blend_indices: [0; 4],
            blend_weights: [0.0; 4],
        },
        SceneMeshVertexRecord {
            position: SceneVec3 {
                x: 32.0,
                y: -16.0,
                z: 0.0,
            },
            uv: [1.0, 1.0],
            blend_indices: [0; 4],
            blend_weights: [0.0; 4],
        },
        SceneMeshVertexRecord {
            position: SceneVec3 {
                x: 32.0,
                y: 16.0,
                z: 0.0,
            },
            uv: [1.0, 0.0],
            blend_indices: [0; 4],
            blend_weights: [0.0; 4],
        },
        SceneMeshVertexRecord {
            position: SceneVec3 {
                x: -32.0,
                y: 16.0,
                z: 0.0,
            },
            uv: [0.0, 0.0],
            blend_indices: [0; 4],
            blend_weights: [0.0; 4],
        },
    ]);
    document.mesh_indices.extend([0, 1, 2, 0, 2, 3]);
    document.mesh_source_records.extend([
        SceneMeshSourceRecord {
            mesh: 0,
            source_index: 37,
            local_index_offset: 0,
            index_start: 0,
            index_count: 3,
        },
        SceneMeshSourceRecord {
            mesh: 0,
            source_index: 30,
            local_index_offset: 0,
            index_start: 3,
            index_count: 3,
        },
    ]);
    document.mesh_clipping_source_ordinals.extend([1, 0]);
    document
        .mesh_clipping_subdraws
        .push(SceneMeshClippingSubdrawRecord {
            mesh: 0,
            source_qword: 0x690,
            mask: SceneStringId(9),
            mask_resource: SceneResourceId::NONE,
            raw_flags: 0,
            target_source_start: 0,
            target_source_count: 1,
            mask_source_start: 1,
            mask_source_count: 1,
        });
    document
        .mesh_clipping_slices
        .push(SceneMeshClippingSliceRecord {
            mesh: 0,
            subdraw: 0,
            role: SceneMeshClippingSliceRole::ClippedTarget,
            index_start: 3,
            index_count: 3,
        });
    document.puppets.push(ScenePuppetRecord {
        object: SceneObjectHandle(0),
        resource: SceneResourceId(0),
        mesh_start: 0,
        mesh_count: 1,
        bone_start: 0,
        bone_count: 1,
        attachment_start: 0,
        attachment_count: 1,
    });
    document.puppet_bones.push(ScenePuppetBoneRecord {
        puppet: 0,
        bone_index: 41,
        name: SceneStringId(6),
        simulation_type: 3,
        parent_index: -1,
        local_bind_matrix: [1.0; 16],
        simulation_json: SceneStringId::NONE,
    });
    document
        .puppet_attachments
        .push(ScenePuppetAttachmentRecord {
            puppet: 0,
            bone_index: 41,
            name: SceneStringId(5),
            local_matrix: [1.0; 16],
        });

    let mut bytes = Vec::new();
    write_scene_binary(&document, &mut bytes).expect("write scene binary");
    let decoded = read_scene_binary_bytes(&bytes).expect("read scene binary");

    assert_eq!(
        decoded.feature_flags & SCENE_FEATURE_DESCRIPTOR_HEAP,
        SCENE_FEATURE_DESCRIPTOR_HEAP
    );
    assert_eq!(decoded.strings[0], "title");
    assert_eq!(decoded.project.logical_width, 1920);
    assert_eq!(decoded.resources[0].payload_len, 4);
    assert_eq!(decoded.resource_payload, vec![1, 2, 3, 4]);
    assert_eq!(decoded.object_animation_layers[0].animation_id, 475);
    assert!(decoded.object_animation_layers[0].additive);
    assert_eq!(decoded.object_animation_layers[0].initial_progress, 0.94);
    assert_eq!(
        decoded.object_transform_tracks[0].property,
        SceneObjectTransformProperty::Origin
    );
    assert_eq!(decoded.object_transform_channels[0].component, 0);
    assert_eq!(decoded.object_transform_keyframes[0].value, 24.32251);
    assert_eq!(decoded.objects[0].color.x, 0.1);
    assert_eq!(decoded.objects[0].alpha, 0.4);
    assert_eq!(decoded.camera_parallax.amount, 0.5);
    assert_eq!(decoded.object_parallax_depths[0].depth, [-0.1, 0.0]);
    assert_eq!(decoded.puppet_animation_clips[0].clip_id, 475);
    assert_eq!(decoded.puppet_animation_clips[0].track_count, 1);
    assert_eq!(decoded.puppet_animation_tracks[0].bone_index, 41);
    assert_eq!(decoded.puppet_animation_tracks[0].opacity_flags, 9);
    assert_eq!(decoded.puppet_animation_opacity_samples, [0.5]);
    assert_eq!(
        decoded.puppet_animation_transform_samples[0].translation.y,
        2.0
    );
    assert_eq!(decoded.meshes[0].width, 64.0);
    assert_eq!(decoded.mesh_vertices.len(), 4);
    assert_eq!(decoded.mesh_indices, vec![0, 1, 2, 0, 2, 3]);
    assert_eq!(decoded.mesh_source_records, document.mesh_source_records);
    assert_eq!(
        decoded.mesh_clipping_subdraws,
        document.mesh_clipping_subdraws
    );
    assert_eq!(
        decoded.mesh_clipping_source_ordinals,
        document.mesh_clipping_source_ordinals
    );
    assert_eq!(decoded.mesh_clipping_slices, document.mesh_clipping_slices);
    assert_eq!(decoded.puppets[0].bone_count, 1);
    assert_eq!(decoded.puppet_bones[0].bone_index, 41);
    assert_eq!(decoded.puppet_bones[0].parent_index, -1);
    assert_eq!(decoded.puppets[0].attachment_count, 1);
    assert_eq!(decoded.puppet_attachments[0].bone_index, 41);
    assert_eq!(
        decoded.strings[decoded.puppet_attachments[0].name.0 as usize],
        "eye"
    );
}

#[test]
fn scene_binary_rejects_chunk_table_item_count_mismatch() {
    let document = SceneBinaryDocument {
        strings: vec!["scene".to_owned()],
        ..SceneBinaryDocument::default()
    };
    let mut bytes = Vec::new();
    write_scene_binary(&document, &mut bytes).expect("write scene binary");
    let string_chunk_item_count_offset = HEADER_LEN + 24;
    bytes[string_chunk_item_count_offset..string_chunk_item_count_offset + 4]
        .copy_from_slice(&2u32.to_le_bytes());

    let err = read_scene_binary_bytes(&bytes).expect_err("count mismatch");

    assert!(matches!(
        err,
        SceneBinaryError::CountMismatch {
            chunk: "string table",
            expected: 2,
            actual: 1,
        }
    ));
}

#[test]
fn scene_binary_round_trip_preserves_typed_draw_primitive_and_composite_blend() {
    let document = SceneBinaryDocument {
        render_graphs: vec![SceneRenderGraphRecord {
            object: SceneObjectHandle(0),
            activation_policy: SceneRenderGraphActivationPolicy::Always,
            pass_start: 0,
            pass_count: 1,
            unsupported_start: 0,
            unsupported_count: 0,
        }],
        render_passes: vec![SceneRenderPassRecord {
            id: 0,
            role: SceneRenderPassKind::SceneComposite,
            draw_primitive: SceneRenderPassDrawPrimitive::ObjectMesh,
            object: SceneObjectHandle(0),
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            pass_index: 0,
            shader_key: SceneStringId::NONE,
            target: SceneRenderTargetKind::SceneColor,
            target_name: SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            effect_binding_start: u32::MAX,
            effect_binding_count: 0,
            effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
            pipeline_blend: ScenePipelineBlend::Normal,
            scene_blend: SceneCompositeBlend::Modulate,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            color_write_mask: SceneColorWriteMask::Rgba,
            clear_target: false,
        }],
        ..SceneBinaryDocument::default()
    };
    let mut bytes = Vec::new();

    write_scene_binary(&document, &mut bytes).expect("write scene binary");
    let decoded = read_scene_binary_bytes(&bytes).expect("read scene binary");

    assert_eq!(
        decoded.render_passes[0].scene_blend,
        SceneCompositeBlend::Modulate
    );
    assert_eq!(
        decoded.render_passes[0].draw_primitive,
        SceneRenderPassDrawPrimitive::ObjectMesh
    );
}

#[test]
fn scene_binary_rejects_unknown_render_pass_draw_primitive() {
    let document = SceneBinaryDocument {
        render_graphs: vec![SceneRenderGraphRecord {
            object: SceneObjectHandle(INVALID_OBJECT_ID),
            activation_policy: SceneRenderGraphActivationPolicy::Always,
            pass_start: 0,
            pass_count: 1,
            unsupported_start: 0,
            unsupported_count: 0,
        }],
        render_passes: vec![SceneRenderPassRecord {
            id: 0,
            role: SceneRenderPassKind::EffectMaterial,
            draw_primitive: SceneRenderPassDrawPrimitive::FullscreenTriangle,
            object: SceneObjectHandle(INVALID_OBJECT_ID),
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            pass_index: 0,
            shader_key: SceneStringId::NONE,
            target: SceneRenderTargetKind::SceneColor,
            target_name: SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            effect_binding_start: u32::MAX,
            effect_binding_count: 0,
            effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
            pipeline_blend: ScenePipelineBlend::Normal,
            scene_blend: SceneCompositeBlend::Alpha,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            color_write_mask: SceneColorWriteMask::Rgba,
            clear_target: false,
        }],
        ..SceneBinaryDocument::default()
    };
    let mut bytes = Vec::new();
    write_scene_binary(&document, &mut bytes).expect("write scene binary");

    let chunk_count = u32::from_le_bytes(bytes[24..28].try_into().unwrap()) as usize;
    let table_offset = u64::from_le_bytes(bytes[28..36].try_into().unwrap()) as usize;
    let render_graph_offset = (0..chunk_count)
        .find_map(|index| {
            let entry = table_offset + index * CHUNK_ENTRY_LEN;
            let kind = u32::from_le_bytes(bytes[entry..entry + 4].try_into().unwrap());
            (kind == CHUNK_RENDER_GRAPH).then(|| {
                u64::from_le_bytes(bytes[entry + 8..entry + 16].try_into().unwrap()) as usize
            })
        })
        .expect("render graph chunk");
    let first_pass_draw_primitive = render_graph_offset + 40;
    bytes[first_pass_draw_primitive..first_pass_draw_primitive + 4]
        .copy_from_slice(&u32::MAX.to_le_bytes());

    assert!(matches!(
        read_scene_binary_bytes(&bytes),
        Err(SceneBinaryError::InvalidChunkValue(
            "render pass draw primitive",
            u32::MAX
        ))
    ));
}

#[test]
fn scene_binary_round_trip_preserves_graph_activation_policy() {
    let document = SceneBinaryDocument {
        render_graphs: vec![SceneRenderGraphRecord {
            object: SceneObjectHandle(0),
            activation_policy: SceneRenderGraphActivationPolicy::AnyEffectVisible,
            pass_start: 0,
            pass_count: 0,
            unsupported_start: 0,
            unsupported_count: 0,
        }],
        ..SceneBinaryDocument::default()
    };
    let mut bytes = Vec::new();

    write_scene_binary(&document, &mut bytes).expect("write scene binary");
    let decoded = read_scene_binary_bytes(&bytes).expect("read scene binary");

    assert_eq!(
        decoded.render_graphs[0].activation_policy,
        SceneRenderGraphActivationPolicy::AnyEffectVisible
    );
}

#[test]
fn scene_binary_round_trip_preserves_effect_visibility_ownership() {
    let document = SceneBinaryDocument {
        render_passes: vec![SceneRenderPassRecord {
            id: 7,
            role: SceneRenderPassKind::SceneComposite,
            draw_primitive: SceneRenderPassDrawPrimitive::FullscreenTriangle,
            object: SceneObjectHandle(3),
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            pass_index: 0,
            shader_key: SceneStringId::NONE,
            target: SceneRenderTargetKind::SceneColor,
            target_name: SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            effect_binding_start: 11,
            effect_binding_count: 2,
            effect_visibility_policy: SceneRenderEffectVisibilityPolicy::MaterialStages,
            pipeline_blend: ScenePipelineBlend::Normal,
            scene_blend: SceneCompositeBlend::Alpha,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            color_write_mask: SceneColorWriteMask::Rgb,
            clear_target: true,
        }],
        ..SceneBinaryDocument::default()
    };
    let mut bytes = Vec::new();

    write_scene_binary(&document, &mut bytes).expect("write scene binary");
    let decoded = read_scene_binary_bytes(&bytes).expect("read scene binary");
    let pass = decoded.render_passes[0];

    assert_eq!(pass.effect_binding_start, 11);
    assert_eq!(pass.effect_binding_count, 2);
    assert_eq!(pass.color_write_mask, SceneColorWriteMask::Rgb);
    assert!(pass.clear_target);
    assert_eq!(
        pass.effect_visibility_policy,
        SceneRenderEffectVisibilityPolicy::MaterialStages
    );
}

#[test]
fn scene_binary_round_trip_preserves_typed_user_property_bindings() {
    let document = SceneBinaryDocument {
        strings: vec!["rain".to_owned(), "theme".to_owned(), "2".to_owned()],
        user_property_bindings: vec![
            SceneUserPropertyBindingRecord {
                object: SceneObjectHandle(3),
                property: SceneStringId(0),
                target: SceneUserPropertyTarget::Visible,
                predicate: SceneUserPropertyPredicate::BooleanValue,
            },
            SceneUserPropertyBindingRecord {
                object: SceneObjectHandle(4),
                property: SceneStringId(1),
                target: SceneUserPropertyTarget::Visible,
                predicate: SceneUserPropertyPredicate::StringEquals(SceneStringId(2)),
            },
        ],
        ..SceneBinaryDocument::default()
    };
    let mut bytes = Vec::new();

    write_scene_binary(&document, &mut bytes).expect("write scene binary");
    let decoded = read_scene_binary_bytes(&bytes).expect("read scene binary");

    assert_eq!(
        decoded.user_property_bindings,
        document.user_property_bindings
    );
}
