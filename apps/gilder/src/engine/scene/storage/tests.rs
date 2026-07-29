use super::*;
use crate::engine::scene::binary::{SceneBinaryDocument, write_scene_binary};

#[test]
fn storage_rejects_overlapping_sampled_and_input_attachment_slots() {
    let document = SceneBinaryDocument {
        strings: vec!["shader".to_owned(), "pipeline".to_owned()],
        shader_contracts: vec![SceneShaderContractRecord {
            shader_key: SceneStringId(0),
            pipeline_key: SceneStringId(1),
            texture_slot_mask: 0b101,
            input_attachment_slot_mask: 0b001,
            constant_start: 0,
            constant_count: 0,
            resource_heap_count: 2,
            sampler_heap_count: 1,
        }],
        ..SceneBinaryDocument::default()
    };

    assert!(matches!(
        SceneStorage::from_document(document),
        Err(SceneStorageError::InvalidShaderAccessMask { overlap: 0b001, .. })
    ));
}

#[test]
fn storage_accepts_native_heap_spirv_and_exposes_borrowed_program_slices() {
    let mut spirv = vec![0x0723_0203, 0x0001_0600, 0, 2, 0];
    spirv.extend(spirv_instruction(17, &[5_128]));
    spirv.extend(spirv_string_instruction(10, "SPV_EXT_descriptor_heap"));
    let document = SceneBinaryDocument {
        strings: vec![
            "workshop/example/effects/wave__SLOTS_1".to_owned(),
            "main".to_owned(),
        ],
        shader_programs: vec![SceneShaderProgramRecord {
            program_key: SceneStringId(0),
            stage: SceneShaderStage::Fragment,
            entry_point: SceneStringId(1),
            spirv_start: 0,
            spirv_count: spirv.len() as u32,
            binding_start: 0,
            binding_count: 1,
            push_constant_bytes: 4,
        }],
        shader_bindings: vec![SceneShaderBindingRecord {
            kind: SceneShaderBindingKind::SampledImage,
            register: 0,
            descriptor_count: 1,
            push_offset: 0,
        }],
        shader_spirv: spirv.clone(),
        ..SceneBinaryDocument::default()
    };

    let storage = SceneStorage::from_document(document).expect("native heap SPIR-V storage");
    let program = &storage.shader_programs()[0];
    assert_eq!(
        storage.shader_program(SceneStringId(0), SceneShaderStage::Fragment),
        Some(program)
    );
    assert_eq!(
        storage.shader_program(SceneStringId(0), SceneShaderStage::Vertex),
        None
    );
    assert_eq!(storage.shader_program_spirv(program), spirv);
    assert_eq!(storage.shader_program_bindings(program).len(), 1);
}

#[test]
fn storage_rejects_scene_spirv_with_legacy_descriptor_decorations() {
    let mut spirv = vec![0x0723_0203, 0x0001_0600, 0, 2, 0];
    spirv.extend(spirv_instruction(71, &[1, 33, 0]));
    let document = SceneBinaryDocument {
        strings: vec!["program".to_owned(), "main".to_owned()],
        shader_programs: vec![SceneShaderProgramRecord {
            program_key: SceneStringId(0),
            stage: SceneShaderStage::Fragment,
            entry_point: SceneStringId(1),
            spirv_start: 0,
            spirv_count: spirv.len() as u32,
            binding_start: 0,
            binding_count: 0,
            push_constant_bytes: 0,
        }],
        shader_spirv: spirv,
        ..SceneBinaryDocument::default()
    };

    assert!(matches!(
        SceneStorage::from_document(document),
        Err(SceneStorageError::InvalidShaderProgram {
            reason: "SPIR-V contains legacy descriptor decorations",
            ..
        })
    ));
}

fn spirv_instruction(opcode: u32, operands: &[u32]) -> Vec<u32> {
    let word_count = u32::try_from(operands.len() + 1).expect("instruction word count");
    std::iter::once((word_count << 16) | opcode)
        .chain(operands.iter().copied())
        .collect()
}

fn spirv_string_instruction(opcode: u32, value: &str) -> Vec<u32> {
    let mut bytes = value.as_bytes().to_vec();
    bytes.push(0);
    bytes.resize(bytes.len().next_multiple_of(4), 0);
    let operands = bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes(chunk.try_into().expect("SPIR-V word")))
        .collect::<Vec<_>>();
    spirv_instruction(opcode, &operands)
}

#[test]
fn storage_rejects_duplicate_material_scalar_script_selectors() {
    let object = SceneObjectRecord {
        id: SceneObjectHandle(0),
        we_id: 1,
        name: SceneStringId::NONE,
        kind: SceneObjectKind::Image,
        resource: SceneResourceId::NONE,
        material: SceneMaterialHandle(INVALID_MATERIAL_ID),
        parent_we_id: INVALID_OBJECT_ID,
        attachment: SceneStringId::NONE,
        origin: SceneVec3::default(),
        angles: SceneVec3::default(),
        scale: SceneVec3::ONE,
        color: SceneVec3::ONE,
        alpha: 1.0,
        visible: true,
        color_blend_mode: 0,
        sort_order: 0,
        effect_start: u32::MAX,
        effect_count: 0,
        render_graph: u32::MAX,
    };
    let script = SceneScriptProgramRecord {
        object: SceneObjectHandle(0),
        target: SceneScriptTarget::MaterialScalar,
        selector: 0,
        source: SceneStringId(0),
        properties_json: SceneStringId(1),
        initial_text: SceneStringId::NONE,
        subscriptions: SceneScriptSubscriptions::FRAME,
        initial_numeric: [1.0, 0.0, 0.0, 0.0],
    };
    let document = SceneBinaryDocument {
        strings: vec![
            "export function update(value) { return value; }".to_owned(),
            "{}".to_owned(),
            "alpha".to_owned(),
            "1".to_owned(),
        ],
        objects: vec![object],
        material_constants: vec![SceneMaterialConstantRecord {
            name: SceneStringId(2),
            value_json: SceneStringId(3),
        }],
        script_programs: vec![script, script],
        ..SceneBinaryDocument::default()
    };

    assert!(matches!(
        SceneStorage::from_document(document),
        Err(SceneStorageError::InvalidScriptProgram {
            reason: "duplicate material scalar selector",
            ..
        })
    ));
}

#[test]
fn storage_borrows_resource_payload_slices() {
    let mut document = SceneBinaryDocument {
        strings: vec!["scene".to_owned(), "scene.json".to_owned()],
        resource_payload: vec![7, 8, 9],
        ..SceneBinaryDocument::default()
    };
    document.project.wallpaper_type = SceneStringId(0);
    document.project.scene_file = SceneStringId(1);
    document.resources.push(SceneResourceRecord {
        id: SceneResourceId(0),
        kind: SceneResourceKind::SceneJson,
        path: SceneStringId(1),
        source: SceneStringId(1),
        payload_offset: 0,
        payload_len: 3,
    });

    let mut bytes = Vec::new();
    write_scene_binary(&document, &mut bytes).expect("write");
    let mut storage = SceneStorage::from_binary_bytes(&bytes).expect("storage");
    let payload = storage
        .resource_payload(&storage.resources()[0])
        .expect("payload");

    assert_eq!(storage.string(SceneStringId(0)), Some("scene"));
    assert_eq!(payload, &[7, 8, 9]);
    assert_eq!(storage.release_parsed_resource_payload(), 3);
    assert_eq!(storage.resource_payload_bytes(), 0);
    assert_eq!(
        storage.resource_payload(&storage.resources()[0]),
        Some(&[][..])
    );
    validate_document(storage.document()).expect("released storage remains valid");
}

#[test]
fn storage_rejects_duplicate_or_non_bool_visible_user_bindings() {
    let object = SceneObjectRecord {
        id: SceneObjectHandle(0),
        we_id: 1,
        name: SceneStringId::NONE,
        kind: SceneObjectKind::Image,
        resource: SceneResourceId::NONE,
        material: SceneMaterialHandle(INVALID_MATERIAL_ID),
        parent_we_id: INVALID_OBJECT_ID,
        attachment: SceneStringId::NONE,
        origin: SceneVec3::default(),
        angles: SceneVec3::default(),
        scale: SceneVec3::ONE,
        color: SceneVec3::ONE,
        alpha: 1.0,
        visible: false,
        color_blend_mode: 0,
        sort_order: 0,
        effect_start: u32::MAX,
        effect_count: 0,
        render_graph: u32::MAX,
    };
    let binding = SceneUserPropertyBindingRecord {
        object: SceneObjectHandle(0),
        property: SceneStringId(1),
        target: SceneUserPropertyTarget::Visible,
        predicate: SceneUserPropertyPredicate::BooleanValue,
    };
    let document = |schema: &str, bindings: Vec<SceneUserPropertyBindingRecord>| {
        let mut document = SceneBinaryDocument {
            strings: vec![schema.to_owned(), "rain".to_owned()],
            objects: vec![object],
            user_property_bindings: bindings,
            ..SceneBinaryDocument::default()
        };
        document.project.properties_json = SceneStringId(0);
        document
    };

    SceneStorage::from_document(document(
        r#"{"rain":{"type":"bool","value":false}}"#,
        vec![binding],
    ))
    .expect("strict bool binding");
    assert!(matches!(
        SceneStorage::from_document(document(
            r#"{"rain":{"type":"bool","value":false}}"#,
            vec![binding, binding],
        )),
        Err(SceneStorageError::InvalidUserPropertyBinding { .. })
    ));
    assert!(matches!(
        SceneStorage::from_document(document(
            r#"{"rain":{"type":"slider","value":0}}"#,
            vec![binding],
        )),
        Err(SceneStorageError::InvalidUserPropertyBinding { .. })
    ));
    let mut mismatched = document(r#"{"rain":{"type":"bool","value":true}}"#, vec![binding]);
    mismatched.objects[0].visible = false;
    assert!(matches!(
        SceneStorage::from_document(mismatched),
        Err(SceneStorageError::InvalidUserPropertyBinding { .. })
    ));
}

#[test]
fn storage_validates_combo_visibility_predicate_and_authored_default() {
    let object = SceneObjectRecord {
        id: SceneObjectHandle(0),
        we_id: 1,
        name: SceneStringId::NONE,
        kind: SceneObjectKind::Image,
        resource: SceneResourceId::NONE,
        material: SceneMaterialHandle(INVALID_MATERIAL_ID),
        parent_we_id: INVALID_OBJECT_ID,
        attachment: SceneStringId::NONE,
        origin: SceneVec3::default(),
        angles: SceneVec3::default(),
        scale: SceneVec3::ONE,
        color: SceneVec3::ONE,
        alpha: 1.0,
        visible: false,
        color_blend_mode: 0,
        sort_order: 0,
        effect_start: u32::MAX,
        effect_count: 0,
        render_graph: u32::MAX,
    };
    let binding = SceneUserPropertyBindingRecord {
        object: SceneObjectHandle(0),
        property: SceneStringId(1),
        target: SceneUserPropertyTarget::Visible,
        predicate: SceneUserPropertyPredicate::StringEquals(SceneStringId(2)),
    };
    let document = |visible: bool, condition: &str| {
        let mut object = object;
        object.visible = visible;
        let mut document = SceneBinaryDocument {
            strings: vec![
                r#"{"theme":{"type":"combo","value":"1","options":[{"value":"1"},{"value":"2"}]}}"#
                    .to_owned(),
                "theme".to_owned(),
                condition.to_owned(),
            ],
            objects: vec![object],
            user_property_bindings: vec![binding],
            ..SceneBinaryDocument::default()
        };
        document.project.properties_json = SceneStringId(0);
        document
    };

    SceneStorage::from_document(document(false, "2")).expect("strict combo predicate");
    for invalid in [document(true, "2"), document(false, "3")] {
        assert!(matches!(
            SceneStorage::from_document(invalid),
            Err(SceneStorageError::InvalidUserPropertyBinding { .. })
        ));
    }
}

#[test]
fn storage_releases_uploaded_texture_payload_but_keeps_texture_metadata() {
    let resource = SceneResourceId(7);
    let mut document = SceneBinaryDocument {
        texture_payload: vec![1, 2, 3, 4, 5, 6],
        ..SceneBinaryDocument::default()
    };
    document.resources.push(SceneResourceRecord {
        id: resource,
        kind: SceneResourceKind::TextureTex,
        path: SceneStringId::NONE,
        source: SceneStringId::NONE,
        payload_offset: 0,
        payload_len: 0,
    });
    document.textures.push(SceneTextureRecord {
        resource,
        format: SceneTextureFormat::R8Unorm,
        source_runtime_format: 9,
        payload_format: 0,
        sampler_filter: SceneTextureSamplerFilter::Anisotropic8,
        sampler_address_mode: SceneTextureSamplerAddressMode::Repeat,
        width: 3,
        height: 2,
        storage_width: 4,
        storage_height: 2,
        mip_start: 0,
        mip_count: 1,
        texv_tag: SceneStringId::NONE,
        texb_tag: SceneStringId::NONE,
        payload_offset: 0,
        payload_len: 6,
        alpha_coverage_rows: [u32::MAX;
            crate::engine::scene::SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE],
    });
    document.texture_mips.push(SceneTextureMipRecord {
        width: 3,
        height: 2,
        payload_offset: 0,
        payload_len: 6,
    });
    let mut storage = SceneStorage::from_document(document).expect("storage");

    assert_eq!(storage.release_uploaded_texture_payload(), 6);
    assert_eq!(storage.texture_payload_bytes(), 0);
    assert_eq!(storage.textures()[0].width, 3);
    assert_eq!(storage.textures()[0].storage_width, 4);
    assert!(storage.texture_payload(&storage.textures()[0]).is_empty());
    assert!(
        storage
            .texture_mip_payload(&storage.document.texture_mips[0])
            .is_empty()
    );
    validate_document(storage.document()).expect("released storage remains valid");
}

#[test]
fn storage_releases_uploaded_mesh_payload_without_rewriting_mesh_metadata() {
    let document = SceneBinaryDocument {
        objects: vec![SceneObjectRecord {
            id: SceneObjectHandle(0),
            we_id: 1,
            name: SceneStringId::NONE,
            kind: SceneObjectKind::Image,
            resource: SceneResourceId::NONE,
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            parent_we_id: INVALID_OBJECT_ID,
            attachment: SceneStringId::NONE,
            origin: SceneVec3::default(),
            angles: SceneVec3::default(),
            scale: SceneVec3::ONE,
            color: SceneVec3::ONE,
            alpha: 1.0,
            visible: true,
            color_blend_mode: 0,
            sort_order: 0,
            effect_start: u32::MAX,
            effect_count: 0,
            render_graph: u32::MAX,
        }],
        meshes: vec![SceneMeshRecord {
            object: SceneObjectHandle(0),
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            vertex_start: 0,
            vertex_count: 1,
            index_start: 0,
            index_count: 3,
            width: 1.0,
            height: 1.0,
            bounds_min: SceneVec3::default(),
            bounds_max: SceneVec3::ONE,
        }],
        mesh_vertices: vec![SceneMeshVertexRecord {
            position: SceneVec3::default(),
            uv: [0.0; 2],
            blend_indices: [0; 4],
            blend_weights: [0.0; 4],
        }],
        mesh_indices: vec![0, 0, 0],
        ..SceneBinaryDocument::default()
    };
    let mut storage = SceneStorage::from_document(document).expect("storage");

    assert_eq!(storage.mesh_vertex_payload_bytes(), 52);
    assert_eq!(storage.mesh_index_payload_bytes(), 12);
    assert_eq!(storage.release_uploaded_mesh_payload(), (52, 12));
    assert_eq!(storage.mesh_vertex_payload_bytes(), 0);
    assert_eq!(storage.mesh_index_payload_bytes(), 0);
    assert_eq!(storage.meshes()[0].vertex_count, 1);
    assert_eq!(storage.meshes()[0].index_count, 3);
}

#[test]
fn storage_rejects_invalid_material_handles() {
    let mut document = SceneBinaryDocument {
        strings: vec!["scene".to_owned(), "scene.json".to_owned()],
        ..SceneBinaryDocument::default()
    };
    document.project.wallpaper_type = SceneStringId(0);
    document.project.scene_file = SceneStringId(1);
    document.objects.push(SceneObjectRecord {
        id: SceneObjectHandle(0),
        we_id: 1,
        name: SceneStringId::NONE,
        kind: SceneObjectKind::Image,
        resource: SceneResourceId::NONE,
        material: SceneMaterialHandle(42),
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
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
        alpha: 1.0,
        visible: true,
        color_blend_mode: 0,
        sort_order: 0,
        effect_start: u32::MAX,
        effect_count: 0,
        render_graph: u32::MAX,
    });

    let err = SceneStorage::from_document(document).expect_err("invalid material");

    assert!(matches!(
        err,
        SceneStorageError::InvalidMaterialHandle {
            field: "object.material",
            handle: SceneMaterialHandle(42)
        }
    ));
}

#[test]
fn storage_rejects_mesh_indices_outside_local_vertex_range() {
    let mut document = SceneBinaryDocument::default();
    document.objects.push(SceneObjectRecord {
        id: SceneObjectHandle(0),
        we_id: 1,
        name: SceneStringId::NONE,
        kind: SceneObjectKind::Image,
        resource: SceneResourceId::NONE,
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
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
        alpha: 1.0,
        visible: true,
        color_blend_mode: 0,
        sort_order: 0,
        effect_start: u32::MAX,
        effect_count: 0,
        render_graph: u32::MAX,
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
    document.mesh_vertices.resize(
        4,
        SceneMeshVertexRecord {
            position: SceneVec3::default(),
            uv: [0.0, 0.0],
            blend_indices: [0; 4],
            blend_weights: [0.0; 4],
        },
    );
    document.mesh_indices = vec![0, 1, 2, 0, 2, 4];

    let err = SceneStorage::from_document(document).expect_err("invalid mesh index");

    assert!(matches!(
        err,
        SceneStorageError::InvalidMeshIndex {
            mesh: 0,
            index: 4,
            vertex_count: 4
        }
    ));
}

#[test]
fn storage_rejects_effect_visibility_ownership_crossing_objects() {
    let object = |id: u32, effect_start: u32| SceneObjectRecord {
        id: SceneObjectHandle(id),
        we_id: id + 1,
        name: SceneStringId::NONE,
        kind: SceneObjectKind::Image,
        resource: SceneResourceId::NONE,
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
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
        alpha: 1.0,
        visible: true,
        color_blend_mode: 0,
        sort_order: 0,
        effect_start,
        effect_count: 1,
        render_graph: u32::MAX,
    };
    let document = SceneBinaryDocument {
        objects: vec![object(0, 0), object(1, 1)],
        effects: vec![SceneEffectRecord {
            id: SceneEffectHandle(0),
            resource: SceneResourceId::NONE,
            replacement_key: SceneStringId::NONE,
            pass_start: 0,
            pass_count: 0,
            fbo_start: 0,
            fbo_count: 0,
        }],
        object_effects: vec![
            SceneObjectEffectRecord {
                object: SceneObjectHandle(0),
                effect: SceneEffectHandle(0),
                name: SceneStringId::NONE,
                instance_id: 0,
                visible: true,
            },
            SceneObjectEffectRecord {
                object: SceneObjectHandle(1),
                effect: SceneEffectHandle(0),
                name: SceneStringId::NONE,
                instance_id: 0,
                visible: true,
            },
        ],
        render_passes: vec![SceneRenderPassRecord {
            id: 0,
            role: SceneRenderPassKind::SceneComposite,
            draw_primitive: SceneRenderPassDrawPrimitive::FullscreenTriangle,
            object: SceneObjectHandle(0),
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            pass_index: 0,
            shader_key: SceneStringId::NONE,
            target: SceneRenderTargetKind::SceneColor,
            target_name: SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            effect_binding_start: 0,
            effect_binding_count: 2,
            effect_visibility_policy: SceneRenderEffectVisibilityPolicy::MaterialStages,
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

    let err = SceneStorage::from_document(document).expect_err("cross-object effect ownership");

    assert!(matches!(
        err,
        SceneStorageError::InvalidRange {
            field: "render_pass.effect_visibility_contract",
            start: 0,
            count: 2,
            len: 2,
        }
    ));
}

#[test]
fn storage_rejects_effect_gated_graph_without_owned_effect_binding() {
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

    let err = SceneStorage::from_document(document).expect_err("missing activation binding");

    assert!(matches!(
        err,
        SceneStorageError::InvalidRange {
            field: "render_graph.activation_effect_binding_range",
            start: 0,
            count: 0,
            len: 0,
        }
    ));
}
