use super::*;

pub(super) fn encode_resources(resources: &[SceneResourceRecord]) -> Vec<u8> {
    let mut out = Vec::new();
    put_u32(&mut out, resources.len() as u32);
    for record in resources {
        put_resource_id(&mut out, record.id);
        put_u32(&mut out, record.kind.to_u32());
        put_string_id(&mut out, record.path);
        put_string_id(&mut out, record.source);
        put_u64(&mut out, record.payload_offset);
        put_u64(&mut out, record.payload_len);
    }
    out
}

pub(super) fn decode_resources(data: &[u8]) -> Result<Vec<SceneResourceRecord>, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let count = decoder.u32()? as usize;
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let id = decoder.resource_id()?;
        let kind_raw = decoder.u32()?;
        let kind = SceneResourceKind::from_u32(kind_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("resource kind", kind_raw),
        )?;
        records.push(SceneResourceRecord {
            id,
            kind,
            path: decoder.string_id()?,
            source: decoder.string_id()?,
            payload_offset: decoder.u64()?,
            payload_len: decoder.u64()?,
        });
    }
    Ok(records)
}

pub(super) fn encode_scene_objects(
    objects: &[SceneObjectRecord],
    object_effects: &[SceneObjectEffectRecord],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
    put_u32(&mut out, checked_u32(objects.len(), "object count")?);
    for record in objects {
        put_u32(&mut out, record.id.0);
        put_u32(&mut out, record.we_id);
        put_string_id(&mut out, record.name);
        put_u32(&mut out, record.kind.to_u32());
        put_resource_id(&mut out, record.resource);
        put_u32(&mut out, record.material.0);
        put_u32(&mut out, record.parent_we_id);
        put_string_id(&mut out, record.attachment);
        put_vec3(&mut out, record.origin);
        put_vec3(&mut out, record.angles);
        put_vec3(&mut out, record.scale);
        put_vec3(&mut out, record.color);
        put_f32(&mut out, record.alpha);
        put_bool(&mut out, record.visible);
        put_i32(&mut out, record.color_blend_mode);
        put_i32(&mut out, record.sort_order);
        put_u32(&mut out, record.effect_start);
        put_u32(&mut out, record.effect_count);
        put_u32(&mut out, record.render_graph);
    }
    put_u32(
        &mut out,
        checked_u32(object_effects.len(), "object effect count")?,
    );
    for record in object_effects {
        put_u32(&mut out, record.object.0);
        put_u32(&mut out, record.effect.0);
        put_string_id(&mut out, record.name);
        put_u32(&mut out, record.instance_id);
        put_bool(&mut out, record.visible);
    }
    Ok(out)
}

pub(super) fn decode_scene_objects(
    data: &[u8],
) -> Result<(Vec<SceneObjectRecord>, Vec<SceneObjectEffectRecord>), SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let object_count = decoder.u32()? as usize;
    let mut objects = Vec::with_capacity(object_count);
    for _ in 0..object_count {
        let id = SceneObjectHandle(decoder.u32()?);
        let we_id = decoder.u32()?;
        let name = decoder.string_id()?;
        let kind_raw = decoder.u32()?;
        let kind = SceneObjectKind::from_u32(kind_raw)
            .ok_or(SceneBinaryError::InvalidChunkValue("object kind", kind_raw))?;
        objects.push(SceneObjectRecord {
            id,
            we_id,
            name,
            kind,
            resource: decoder.resource_id()?,
            material: SceneMaterialHandle(decoder.u32()?),
            parent_we_id: decoder.u32()?,
            attachment: decoder.string_id()?,
            origin: decoder.vec3()?,
            angles: decoder.vec3()?,
            scale: decoder.vec3()?,
            color: decoder.vec3()?,
            alpha: decoder.f32()?,
            visible: decoder.bool()?,
            color_blend_mode: decoder.i32()?,
            sort_order: decoder.i32()?,
            effect_start: decoder.u32()?,
            effect_count: decoder.u32()?,
            render_graph: decoder.u32()?,
        });
    }
    let effect_count = decoder.u32()? as usize;
    let mut object_effects = Vec::with_capacity(effect_count);
    for _ in 0..effect_count {
        object_effects.push(SceneObjectEffectRecord {
            object: SceneObjectHandle(decoder.u32()?),
            effect: SceneEffectHandle(decoder.u32()?),
            name: decoder.string_id()?,
            instance_id: decoder.u32()?,
            visible: decoder.bool()?,
        });
    }
    Ok((objects, object_effects))
}

pub(super) fn encode_materials(
    materials: &[SceneMaterialRecord],
    passes: &[SceneMaterialPassRecord],
    textures: &[SceneMaterialTextureRecord],
    constants: &[SceneMaterialConstantRecord],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
    put_u32(&mut out, checked_u32(materials.len(), "material count")?);
    for record in materials {
        put_u32(&mut out, record.id.0);
        put_resource_id(&mut out, record.resource);
        put_u32(&mut out, record.pass_start);
        put_u32(&mut out, record.pass_count);
    }
    put_u32(&mut out, checked_u32(passes.len(), "material pass count")?);
    for record in passes {
        put_u32(&mut out, record.material.0);
        put_string_id(&mut out, record.shader_key);
        put_string_id(&mut out, record.target);
        put_u32(&mut out, record.texture_start);
        put_u32(&mut out, record.texture_count);
        put_u32(&mut out, record.constant_start);
        put_u32(&mut out, record.constant_count);
        put_u32(&mut out, record.pipeline_blend.to_u32());
        put_u32(&mut out, record.depth_test.to_u32());
        put_bool(&mut out, record.depth_write);
        put_u32(&mut out, record.cull_mode.to_u32());
        put_string_id(&mut out, record.alpha_writing);
        put_bool(&mut out, record.clear_target);
    }
    put_u32(
        &mut out,
        checked_u32(textures.len(), "material texture count")?,
    );
    for record in textures {
        put_u32(&mut out, record.slot);
        put_resource_id(&mut out, record.resource);
        put_string_id(&mut out, record.path);
    }
    put_u32(
        &mut out,
        checked_u32(constants.len(), "material constant count")?,
    );
    for record in constants {
        put_string_id(&mut out, record.name);
        put_string_id(&mut out, record.value_json);
    }
    Ok(out)
}

type MaterialDecode = (
    Vec<SceneMaterialRecord>,
    Vec<SceneMaterialPassRecord>,
    Vec<SceneMaterialTextureRecord>,
    Vec<SceneMaterialConstantRecord>,
);

pub(super) fn decode_materials(data: &[u8]) -> Result<MaterialDecode, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let material_count = decoder.u32()? as usize;
    let mut materials = Vec::with_capacity(material_count);
    for _ in 0..material_count {
        materials.push(SceneMaterialRecord {
            id: SceneMaterialHandle(decoder.u32()?),
            resource: decoder.resource_id()?,
            pass_start: decoder.u32()?,
            pass_count: decoder.u32()?,
        });
    }
    let pass_count = decoder.u32()? as usize;
    let mut passes = Vec::with_capacity(pass_count);
    for _ in 0..pass_count {
        let material = SceneMaterialHandle(decoder.u32()?);
        let shader_key = decoder.string_id()?;
        let target = decoder.string_id()?;
        let texture_start = decoder.u32()?;
        let texture_count = decoder.u32()?;
        let constant_start = decoder.u32()?;
        let constant_count = decoder.u32()?;
        let blend_raw = decoder.u32()?;
        let pipeline_blend = ScenePipelineBlend::from_u32(blend_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("pipeline blend", blend_raw),
        )?;
        let depth_raw = decoder.u32()?;
        let depth_test = SceneDepthTest::from_u32(depth_raw)
            .ok_or(SceneBinaryError::InvalidChunkValue("depth test", depth_raw))?;
        let depth_write = decoder.bool()?;
        let cull_raw = decoder.u32()?;
        let cull_mode = SceneCullMode::from_u32(cull_raw)
            .ok_or(SceneBinaryError::InvalidChunkValue("cull mode", cull_raw))?;
        passes.push(SceneMaterialPassRecord {
            material,
            shader_key,
            target,
            texture_start,
            texture_count,
            constant_start,
            constant_count,
            pipeline_blend,
            depth_test,
            depth_write,
            cull_mode,
            alpha_writing: decoder.string_id()?,
            clear_target: decoder.bool()?,
        });
    }
    let texture_count = decoder.u32()? as usize;
    let mut textures = Vec::with_capacity(texture_count);
    for _ in 0..texture_count {
        textures.push(SceneMaterialTextureRecord {
            slot: decoder.u32()?,
            resource: decoder.resource_id()?,
            path: decoder.string_id()?,
        });
    }
    let constant_count = decoder.u32()? as usize;
    let mut constants = Vec::with_capacity(constant_count);
    for _ in 0..constant_count {
        constants.push(SceneMaterialConstantRecord {
            name: decoder.string_id()?,
            value_json: decoder.string_id()?,
        });
    }
    Ok((materials, passes, textures, constants))
}

pub(super) fn encode_meshes(
    meshes: &[SceneMeshRecord],
    vertices: &[SceneMeshVertexRecord],
    indices: &[u32],
    source_records: &[SceneMeshSourceRecord],
    clipping_subdraws: &[SceneMeshClippingSubdrawRecord],
    clipping_source_ordinals: &[u32],
    clipping_slices: &[SceneMeshClippingSliceRecord],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
    put_u32(&mut out, checked_u32(meshes.len(), "mesh count")?);
    for record in meshes {
        put_u32(&mut out, record.object.0);
        put_u32(&mut out, record.material.0);
        put_u32(&mut out, record.vertex_start);
        put_u32(&mut out, record.vertex_count);
        put_u32(&mut out, record.index_start);
        put_u32(&mut out, record.index_count);
        put_f32(&mut out, record.width);
        put_f32(&mut out, record.height);
        put_vec3(&mut out, record.bounds_min);
        put_vec3(&mut out, record.bounds_max);
    }
    put_u32(&mut out, checked_u32(vertices.len(), "mesh vertex count")?);
    for vertex in vertices {
        put_vec3(&mut out, vertex.position);
        put_f32(&mut out, vertex.uv[0]);
        put_f32(&mut out, vertex.uv[1]);
        for index in vertex.blend_indices {
            put_u32(&mut out, index);
        }
        for weight in vertex.blend_weights {
            put_f32(&mut out, weight);
        }
    }
    put_u32(&mut out, checked_u32(indices.len(), "mesh index count")?);
    for index in indices {
        put_u32(&mut out, *index);
    }
    mesh_clipping::encode(
        &mut out,
        source_records,
        clipping_subdraws,
        clipping_source_ordinals,
        clipping_slices,
    )?;
    Ok(out)
}

pub(super) fn decode_meshes(data: &[u8]) -> Result<mesh_clipping::MeshDecode, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let mesh_count = decoder.u32()? as usize;
    let mut meshes = Vec::with_capacity(mesh_count);
    for _ in 0..mesh_count {
        meshes.push(SceneMeshRecord {
            object: SceneObjectHandle(decoder.u32()?),
            material: SceneMaterialHandle(decoder.u32()?),
            vertex_start: decoder.u32()?,
            vertex_count: decoder.u32()?,
            index_start: decoder.u32()?,
            index_count: decoder.u32()?,
            width: decoder.f32()?,
            height: decoder.f32()?,
            bounds_min: decoder.vec3()?,
            bounds_max: decoder.vec3()?,
        });
    }
    let vertex_count = decoder.u32()? as usize;
    let mut vertices = Vec::with_capacity(vertex_count);
    for _ in 0..vertex_count {
        vertices.push(SceneMeshVertexRecord {
            position: decoder.vec3()?,
            uv: [decoder.f32()?, decoder.f32()?],
            blend_indices: [
                decoder.u32()?,
                decoder.u32()?,
                decoder.u32()?,
                decoder.u32()?,
            ],
            blend_weights: [
                decoder.f32()?,
                decoder.f32()?,
                decoder.f32()?,
                decoder.f32()?,
            ],
        });
    }
    let index_count = decoder.u32()? as usize;
    let mut indices = Vec::with_capacity(index_count);
    for _ in 0..index_count {
        indices.push(decoder.u32()?);
    }
    let (source_records, clipping_subdraws, clipping_source_ordinals, clipping_slices) =
        mesh_clipping::decode(&mut decoder)?;
    Ok((
        meshes,
        vertices,
        indices,
        source_records,
        clipping_subdraws,
        clipping_source_ordinals,
        clipping_slices,
    ))
}

pub(super) fn encode_puppets(
    puppets: &[ScenePuppetRecord],
    bones: &[ScenePuppetBoneRecord],
    attachments: &[ScenePuppetAttachmentRecord],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
    put_u32(&mut out, checked_u32(puppets.len(), "puppet count")?);
    for record in puppets {
        put_u32(&mut out, record.object.0);
        put_u32(&mut out, record.resource.0);
        put_u32(&mut out, record.mesh_start);
        put_u32(&mut out, record.mesh_count);
        put_u32(&mut out, record.bone_start);
        put_u32(&mut out, record.bone_count);
        put_u32(&mut out, record.attachment_start);
        put_u32(&mut out, record.attachment_count);
    }
    put_u32(&mut out, checked_u32(bones.len(), "puppet bone count")?);
    for record in bones {
        put_u32(&mut out, record.puppet);
        put_u32(&mut out, record.bone_index);
        put_string_id(&mut out, record.name);
        put_i32(&mut out, record.simulation_type);
        put_i32(&mut out, record.parent_index);
        for value in record.local_bind_matrix {
            put_f32(&mut out, value);
        }
        put_string_id(&mut out, record.simulation_json);
    }
    put_u32(
        &mut out,
        checked_u32(attachments.len(), "puppet attachment count")?,
    );
    for record in attachments {
        put_u32(&mut out, record.puppet);
        put_u32(&mut out, record.bone_index);
        put_string_id(&mut out, record.name);
        for value in record.local_matrix {
            put_f32(&mut out, value);
        }
    }
    Ok(out)
}

pub(super) fn decode_puppets(
    data: &[u8],
) -> Result<
    (
        Vec<ScenePuppetRecord>,
        Vec<ScenePuppetBoneRecord>,
        Vec<ScenePuppetAttachmentRecord>,
    ),
    SceneBinaryError,
> {
    let mut decoder = Decoder::new(data);
    let puppet_count = decoder.u32()? as usize;
    let mut puppets = Vec::with_capacity(puppet_count);
    for _ in 0..puppet_count {
        puppets.push(ScenePuppetRecord {
            object: SceneObjectHandle(decoder.u32()?),
            resource: decoder.resource_id()?,
            mesh_start: decoder.u32()?,
            mesh_count: decoder.u32()?,
            bone_start: decoder.u32()?,
            bone_count: decoder.u32()?,
            attachment_start: decoder.u32()?,
            attachment_count: decoder.u32()?,
        });
    }
    let bone_count = decoder.u32()? as usize;
    let mut bones = Vec::with_capacity(bone_count);
    for _ in 0..bone_count {
        let mut local_matrix = [0.0; 16];
        let puppet = decoder.u32()?;
        let bone_index = decoder.u32()?;
        let name = decoder.string_id()?;
        let simulation_type = decoder.i32()?;
        let parent_index = decoder.i32()?;
        for item in &mut local_matrix {
            *item = decoder.f32()?;
        }
        let simulation_json = decoder.string_id()?;
        bones.push(ScenePuppetBoneRecord {
            puppet,
            bone_index,
            name,
            simulation_type,
            parent_index,
            local_bind_matrix: local_matrix,
            simulation_json,
        });
    }
    let attachment_count = decoder.u32()? as usize;
    let mut attachments = Vec::with_capacity(attachment_count);
    for _ in 0..attachment_count {
        let mut local_matrix = [0.0; 16];
        let puppet = decoder.u32()?;
        let bone_index = decoder.u32()?;
        let name = decoder.string_id()?;
        for item in &mut local_matrix {
            *item = decoder.f32()?;
        }
        attachments.push(ScenePuppetAttachmentRecord {
            puppet,
            bone_index,
            name,
            local_matrix,
        });
    }
    Ok((puppets, bones, attachments))
}

pub(super) fn encode_effects(
    effects: &[SceneEffectRecord],
    passes: &[SceneEffectPassRecord],
    bindings: &[SceneEffectBindingRecord],
    combos: &[SceneEffectComboRecord],
    fbos: &[SceneEffectFboRecord],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
    put_u32(&mut out, checked_u32(effects.len(), "effect count")?);
    for record in effects {
        put_u32(&mut out, record.id.0);
        put_resource_id(&mut out, record.resource);
        put_string_id(&mut out, record.replacement_key);
        put_u32(&mut out, record.pass_start);
        put_u32(&mut out, record.pass_count);
        put_u32(&mut out, record.fbo_start);
        put_u32(&mut out, record.fbo_count);
    }
    put_u32(&mut out, checked_u32(passes.len(), "effect pass count")?);
    for record in passes {
        put_u32(&mut out, record.effect.0);
        put_u32(&mut out, record.pass_index);
        put_u32(&mut out, record.material.0);
        put_string_id(&mut out, record.command);
        put_string_id(&mut out, record.source);
        put_string_id(&mut out, record.target);
        put_u32(&mut out, record.binding_start);
        put_u32(&mut out, record.binding_count);
        put_u32(&mut out, record.combo_start);
        put_u32(&mut out, record.combo_count);
    }
    put_u32(
        &mut out,
        checked_u32(bindings.len(), "effect binding count")?,
    );
    for record in bindings {
        put_u32(&mut out, record.slot);
        put_string_id(&mut out, record.target);
    }
    put_u32(&mut out, checked_u32(combos.len(), "effect combo count")?);
    for record in combos {
        put_string_id(&mut out, record.name);
        put_i64(&mut out, record.value);
    }
    put_u32(&mut out, checked_u32(fbos.len(), "effect fbo count")?);
    for record in fbos {
        put_string_id(&mut out, record.name);
        put_string_id(&mut out, record.format);
        put_f32(&mut out, record.scale);
    }
    Ok(out)
}

type EffectDecode = (
    Vec<SceneEffectRecord>,
    Vec<SceneEffectPassRecord>,
    Vec<SceneEffectBindingRecord>,
    Vec<SceneEffectComboRecord>,
    Vec<SceneEffectFboRecord>,
);

pub(super) fn decode_effects(data: &[u8]) -> Result<EffectDecode, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let effect_count = decoder.u32()? as usize;
    let mut effects = Vec::with_capacity(effect_count);
    for _ in 0..effect_count {
        effects.push(SceneEffectRecord {
            id: SceneEffectHandle(decoder.u32()?),
            resource: decoder.resource_id()?,
            replacement_key: decoder.string_id()?,
            pass_start: decoder.u32()?,
            pass_count: decoder.u32()?,
            fbo_start: decoder.u32()?,
            fbo_count: decoder.u32()?,
        });
    }
    let pass_count = decoder.u32()? as usize;
    let mut passes = Vec::with_capacity(pass_count);
    for _ in 0..pass_count {
        passes.push(SceneEffectPassRecord {
            effect: SceneEffectHandle(decoder.u32()?),
            pass_index: decoder.u32()?,
            material: SceneMaterialHandle(decoder.u32()?),
            command: decoder.string_id()?,
            source: decoder.string_id()?,
            target: decoder.string_id()?,
            binding_start: decoder.u32()?,
            binding_count: decoder.u32()?,
            combo_start: decoder.u32()?,
            combo_count: decoder.u32()?,
        });
    }
    let binding_count = decoder.u32()? as usize;
    let mut bindings = Vec::with_capacity(binding_count);
    for _ in 0..binding_count {
        bindings.push(SceneEffectBindingRecord {
            slot: decoder.u32()?,
            target: decoder.string_id()?,
        });
    }
    let combo_count = decoder.u32()? as usize;
    let mut combos = Vec::with_capacity(combo_count);
    for _ in 0..combo_count {
        combos.push(SceneEffectComboRecord {
            name: decoder.string_id()?,
            value: decoder.i64()?,
        });
    }
    let fbo_count = decoder.u32()? as usize;
    let mut fbos = Vec::with_capacity(fbo_count);
    for _ in 0..fbo_count {
        fbos.push(SceneEffectFboRecord {
            name: decoder.string_id()?,
            format: decoder.string_id()?,
            scale: decoder.f32()?,
        });
    }
    Ok((effects, passes, bindings, combos, fbos))
}

pub(super) fn encode_render_graphs(
    graphs: &[SceneRenderGraphRecord],
    passes: &[SceneRenderPassRecord],
    bindings: &[SceneRenderBindingRecord],
    unsupported: &[SceneUnsupportedRecord],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
    put_u32(&mut out, checked_u32(graphs.len(), "render graph count")?);
    for record in graphs {
        put_u32(&mut out, record.object.0);
        put_u32(&mut out, record.activation_policy.to_u32());
        put_u32(&mut out, record.pass_start);
        put_u32(&mut out, record.pass_count);
        put_u32(&mut out, record.unsupported_start);
        put_u32(&mut out, record.unsupported_count);
    }
    put_u32(&mut out, checked_u32(passes.len(), "render pass count")?);
    for record in passes {
        put_u32(&mut out, record.id);
        put_u32(&mut out, record.role.to_u32());
        put_u32(&mut out, record.object.0);
        put_u32(&mut out, record.material.0);
        put_u32(&mut out, record.pass_index);
        put_string_id(&mut out, record.shader_key);
        put_u32(&mut out, record.target.to_u32());
        put_string_id(&mut out, record.target_name);
        put_u32(&mut out, record.binding_start);
        put_u32(&mut out, record.binding_count);
        put_u32(&mut out, record.effect_binding_start);
        put_u32(&mut out, record.effect_binding_count);
        put_u32(&mut out, record.effect_visibility_policy.to_u32());
        put_u32(&mut out, record.pipeline_blend.to_u32());
        put_u32(&mut out, record.scene_blend.to_u32());
        put_u32(&mut out, record.depth_test.to_u32());
        put_bool(&mut out, record.depth_write);
        put_u32(&mut out, record.cull_mode.to_u32());
        put_u32(&mut out, record.color_write_mask.to_u32());
        put_bool(&mut out, record.clear_target);
    }
    put_u32(
        &mut out,
        checked_u32(bindings.len(), "render binding count")?,
    );
    for record in bindings {
        put_u32(&mut out, record.kind.to_u32());
        put_u32(&mut out, record.slot);
        put_u32(&mut out, record.target.to_u32());
        put_string_id(&mut out, record.name);
    }
    put_u32(
        &mut out,
        checked_u32(unsupported.len(), "unsupported boundary count")?,
    );
    for record in unsupported {
        put_u32(&mut out, record.object.0);
        put_u32(&mut out, record.pass_index);
        put_string_id(&mut out, record.feature);
        put_string_id(&mut out, record.expected_subsystem);
        put_string_id(&mut out, record.containment);
    }
    Ok(out)
}

type RenderGraphDecode = (
    Vec<SceneRenderGraphRecord>,
    Vec<SceneRenderPassRecord>,
    Vec<SceneRenderBindingRecord>,
    Vec<SceneUnsupportedRecord>,
);

pub(super) fn decode_render_graphs(data: &[u8]) -> Result<RenderGraphDecode, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let graph_count = decoder.u32()? as usize;
    let mut graphs = Vec::with_capacity(graph_count);
    for _ in 0..graph_count {
        graphs.push(SceneRenderGraphRecord {
            object: SceneObjectHandle(decoder.u32()?),
            activation_policy: {
                let value = decoder.u32()?;
                SceneRenderGraphActivationPolicy::from_u32(value).ok_or(
                    SceneBinaryError::InvalidChunkValue("render graph activation policy", value),
                )?
            },
            pass_start: decoder.u32()?,
            pass_count: decoder.u32()?,
            unsupported_start: decoder.u32()?,
            unsupported_count: decoder.u32()?,
        });
    }
    let pass_count = decoder.u32()? as usize;
    let mut passes = Vec::with_capacity(pass_count);
    for _ in 0..pass_count {
        let id = decoder.u32()?;
        let role_raw = decoder.u32()?;
        let role = SceneRenderPassKind::from_u32(role_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("render pass role", role_raw),
        )?;
        let object = SceneObjectHandle(decoder.u32()?);
        let material = SceneMaterialHandle(decoder.u32()?);
        let pass_index = decoder.u32()?;
        let shader_key = decoder.string_id()?;
        let target_raw = decoder.u32()?;
        let target = SceneRenderTargetKind::from_u32(target_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("render target", target_raw),
        )?;
        passes.push(SceneRenderPassRecord {
            id,
            role,
            object,
            material,
            pass_index,
            shader_key,
            target,
            target_name: decoder.string_id()?,
            binding_start: decoder.u32()?,
            binding_count: decoder.u32()?,
            effect_binding_start: decoder.u32()?,
            effect_binding_count: decoder.u32()?,
            effect_visibility_policy: {
                let value = decoder.u32()?;
                SceneRenderEffectVisibilityPolicy::from_u32(value).ok_or(
                    SceneBinaryError::InvalidChunkValue("render effect visibility policy", value),
                )?
            },
            pipeline_blend: {
                let value = decoder.u32()?;
                ScenePipelineBlend::from_u32(value)
                    .ok_or(SceneBinaryError::InvalidChunkValue("pipeline blend", value))?
            },
            scene_blend: {
                let value = decoder.u32()?;
                SceneCompositeBlend::from_u32(value)
                    .ok_or(SceneBinaryError::InvalidChunkValue("scene blend", value))?
            },
            depth_test: {
                let value = decoder.u32()?;
                SceneDepthTest::from_u32(value)
                    .ok_or(SceneBinaryError::InvalidChunkValue("depth test", value))?
            },
            depth_write: decoder.bool()?,
            cull_mode: {
                let value = decoder.u32()?;
                SceneCullMode::from_u32(value)
                    .ok_or(SceneBinaryError::InvalidChunkValue("cull mode", value))?
            },
            color_write_mask: {
                let value = decoder.u32()?;
                SceneColorWriteMask::from_u32(value).ok_or(SceneBinaryError::InvalidChunkValue(
                    "color write mask",
                    value,
                ))?
            },
            clear_target: decoder.bool()?,
        });
    }
    let binding_count = decoder.u32()? as usize;
    let mut bindings = Vec::with_capacity(binding_count);
    for _ in 0..binding_count {
        let kind_raw = decoder.u32()?;
        let kind = SceneRenderBindingKind::from_u32(kind_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("render binding kind", kind_raw),
        )?;
        let slot = decoder.u32()?;
        let target_raw = decoder.u32()?;
        let target = SceneRenderTargetKind::from_u32(target_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("render binding target", target_raw),
        )?;
        bindings.push(SceneRenderBindingRecord {
            kind,
            slot,
            target,
            name: decoder.string_id()?,
        });
    }
    let unsupported_count = decoder.u32()? as usize;
    let mut unsupported = Vec::with_capacity(unsupported_count);
    for _ in 0..unsupported_count {
        unsupported.push(SceneUnsupportedRecord {
            object: SceneObjectHandle(decoder.u32()?),
            pass_index: decoder.u32()?,
            feature: decoder.string_id()?,
            expected_subsystem: decoder.string_id()?,
            containment: decoder.string_id()?,
        });
    }
    Ok((graphs, passes, bindings, unsupported))
}

pub(super) fn encode_image_targets(targets: &[SceneImageTargetRecord]) -> Vec<u8> {
    let mut out = Vec::new();
    put_u32(&mut out, targets.len() as u32);
    for record in targets {
        put_string_id(&mut out, record.name);
        put_u32(&mut out, record.role.to_u32());
        put_string_id(&mut out, record.format);
        put_u32(&mut out, record.width_divisor_milli);
        put_u32(&mut out, record.height_divisor_milli);
    }
    out
}

pub(super) fn decode_image_targets(
    data: &[u8],
) -> Result<Vec<SceneImageTargetRecord>, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let count = decoder.u32()? as usize;
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let name = decoder.string_id()?;
        let role_raw = decoder.u32()?;
        let role = SceneRenderTargetKind::from_u32(role_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("image target role", role_raw),
        )?;
        records.push(SceneImageTargetRecord {
            name,
            role,
            format: decoder.string_id()?,
            width_divisor_milli: decoder.u32()?,
            height_divisor_milli: decoder.u32()?,
        });
    }
    Ok(records)
}

pub(super) fn encode_shader_contracts(
    contracts: &[SceneShaderContractRecord],
    constant_names: &[SceneStringId],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
    put_u32(
        &mut out,
        checked_u32(contracts.len(), "shader contract count")?,
    );
    for record in contracts {
        put_string_id(&mut out, record.shader_key);
        put_string_id(&mut out, record.pipeline_key);
        put_u32(&mut out, record.texture_slot_mask);
        put_u32(&mut out, record.constant_start);
        put_u32(&mut out, record.constant_count);
        put_u32(&mut out, record.resource_heap_count);
        put_u32(&mut out, record.sampler_heap_count);
    }
    put_u32(
        &mut out,
        checked_u32(constant_names.len(), "shader constant name count")?,
    );
    for name in constant_names {
        put_string_id(&mut out, *name);
    }
    Ok(out)
}

pub(super) fn decode_shader_contracts(
    data: &[u8],
) -> Result<(Vec<SceneShaderContractRecord>, Vec<SceneStringId>), SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let contract_count = decoder.u32()? as usize;
    let mut contracts = Vec::with_capacity(contract_count);
    for _ in 0..contract_count {
        contracts.push(SceneShaderContractRecord {
            shader_key: decoder.string_id()?,
            pipeline_key: decoder.string_id()?,
            texture_slot_mask: decoder.u32()?,
            constant_start: decoder.u32()?,
            constant_count: decoder.u32()?,
            resource_heap_count: decoder.u32()?,
            sampler_heap_count: decoder.u32()?,
        });
    }
    let constant_count = decoder.u32()? as usize;
    let mut constant_names = Vec::with_capacity(constant_count);
    for _ in 0..constant_count {
        constant_names.push(decoder.string_id()?);
    }
    Ok((contracts, constant_names))
}
