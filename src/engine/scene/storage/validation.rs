use super::*;

pub(super) fn validate_document(document: &SceneBinaryDocument) -> Result<(), SceneStorageError> {
    validate_project(document)?;
    validate_pointer_parallax(document)?;
    for resource in &document.resources {
        validate_string(document, "resource.path", resource.path)?;
        validate_string(document, "resource.source", resource.source)?;
        validate_payload(document, resource)?;
    }
    for texture in &document.textures {
        validate_resource(document, "texture.resource", texture.resource)?;
        validate_string(document, "texture.texv_tag", texture.texv_tag)?;
        validate_string(document, "texture.texb_tag", texture.texb_tag)?;
        validate_range(
            "texture.mip_range",
            texture.mip_start,
            texture.mip_count,
            document.texture_mips.len(),
        )?;
        validate_texture_payload(
            document,
            texture.resource,
            texture.payload_offset,
            texture.payload_len,
        )?;
        for mip in document
            .texture_mips
            .iter()
            .skip(texture.mip_start as usize)
            .take(texture.mip_count as usize)
        {
            validate_texture_payload(
                document,
                texture.resource,
                mip.payload_offset,
                mip.payload_len,
            )?;
        }
    }
    for object in &document.objects {
        validate_string(document, "object.name", object.name)?;
        validate_optional_resource(document, "object.resource", object.resource)?;
        validate_optional_material(document, "object.material", object.material)?;
        validate_string(document, "object.attachment", object.attachment)?;
        validate_range(
            "object.effect_range",
            object.effect_start,
            object.effect_count,
            document.object_effects.len(),
        )?;
        validate_range(
            "object.render_graph",
            object.render_graph,
            u32::from(object.render_graph != u32::MAX),
            document.render_graphs.len(),
        )?;
    }
    for program in &document.script_programs {
        validate_range(
            "script_program.object",
            program.object.0,
            1,
            document.objects.len(),
        )?;
        validate_string(document, "script_program.source", program.source)?;
        validate_string(
            document,
            "script_program.properties_json",
            program.properties_json,
        )?;
        validate_string(
            document,
            "script_program.initial_text",
            program.initial_text,
        )?;
        if program.subscriptions == SceneScriptSubscriptions::NONE
            || !program.initial_numeric.into_iter().all(f32::is_finite)
        {
            return Err(SceneStorageError::InvalidScriptProgram {
                object: program.object,
                reason: "empty subscriptions or non-finite initial value",
            });
        }
    }
    for effect in &document.object_effects {
        validate_string(document, "object_effect.name", effect.name)?;
        validate_range(
            "object_effect.object",
            effect.object.0,
            1,
            document.objects.len(),
        )?;
        validate_range(
            "object_effect.effect",
            effect.effect.0,
            1,
            document.effects.len(),
        )?;
    }
    for layer in &document.object_animation_layers {
        validate_range(
            "object_animation_layer.object",
            layer.object.0,
            1,
            document.objects.len(),
        )?;
    }
    for (track_index, track) in document.object_transform_tracks.iter().enumerate() {
        validate_range(
            "object_transform_track.object",
            track.object.0,
            1,
            document.objects.len(),
        )?;
        validate_string(document, "object_transform_track.playback", track.playback)?;
        validate_range(
            "object_transform_track.channel_range",
            track.channel_start,
            track.channel_count,
            document.object_transform_channels.len(),
        )?;
        for channel in document
            .object_transform_channels
            .iter()
            .skip(track.channel_start as usize)
            .take(track.channel_count as usize)
        {
            if channel.track as usize != track_index {
                return Err(SceneStorageError::InvalidRange {
                    field: "object_transform_channel.track_owner",
                    start: channel.track,
                    count: 1,
                    len: track_index,
                });
            }
            validate_range(
                "object_transform_channel.component",
                channel.component,
                1,
                3,
            )?;
            validate_range(
                "object_transform_channel.keyframe_range",
                channel.keyframe_start,
                channel.keyframe_count,
                document.object_transform_keyframes.len(),
            )?;
        }
    }
    for (clip_index, clip) in document.puppet_animation_clips.iter().enumerate() {
        validate_range(
            "puppet_animation_clip.puppet",
            clip.puppet,
            1,
            document.puppets.len(),
        )?;
        validate_string(document, "puppet_animation_clip.name", clip.name)?;
        validate_string(document, "puppet_animation_clip.playback", clip.playback)?;
        validate_range(
            "puppet_animation_clip.track_range",
            clip.track_start,
            clip.track_count,
            document.puppet_animation_tracks.len(),
        )?;
        for track in document
            .puppet_animation_tracks
            .iter()
            .skip(clip.track_start as usize)
            .take(clip.track_count as usize)
        {
            if track.clip as usize != clip_index {
                return Err(SceneStorageError::InvalidRange {
                    field: "puppet_animation_track.clip_owner",
                    start: track.clip,
                    count: 1,
                    len: clip_index,
                });
            }
            validate_range(
                "puppet_animation_track.sample_range",
                track.sample_start,
                track.sample_count,
                document.puppet_animation_transform_samples.len(),
            )?;
            validate_range(
                "puppet_animation_track.opacity_sample_range",
                track.opacity_sample_start,
                track.opacity_sample_count,
                document.puppet_animation_opacity_samples.len(),
            )?;
        }
    }
    for material in &document.materials {
        validate_optional_resource(document, "material.resource", material.resource)?;
        validate_range(
            "material.pass_range",
            material.pass_start,
            material.pass_count,
            document.material_passes.len(),
        )?;
    }
    for pass in &document.material_passes {
        validate_string(document, "material_pass.shader_key", pass.shader_key)?;
        validate_string(document, "material_pass.target", pass.target)?;
        validate_string(document, "material_pass.alpha_writing", pass.alpha_writing)?;
        validate_range(
            "material_pass.texture_range",
            pass.texture_start,
            pass.texture_count,
            document.material_textures.len(),
        )?;
        validate_range(
            "material_pass.constant_range",
            pass.constant_start,
            pass.constant_count,
            document.material_constants.len(),
        )?;
    }
    for texture in &document.material_textures {
        validate_optional_resource(document, "material_texture.resource", texture.resource)?;
        validate_string(document, "material_texture.path", texture.path)?;
    }
    for constant in &document.material_constants {
        validate_string(document, "material_constant.name", constant.name)?;
        validate_string(
            document,
            "material_constant.value_json",
            constant.value_json,
        )?;
    }
    for (mesh_index, mesh) in document.meshes.iter().enumerate() {
        validate_range("mesh.object", mesh.object.0, 1, document.objects.len())?;
        validate_optional_material(document, "mesh.material", mesh.material)?;
        validate_range(
            "mesh.vertex_range",
            mesh.vertex_start,
            mesh.vertex_count,
            document.mesh_vertices.len(),
        )?;
        validate_range(
            "mesh.index_range",
            mesh.index_start,
            mesh.index_count,
            document.mesh_indices.len(),
        )?;
        for &index in document
            .mesh_indices
            .iter()
            .skip(mesh.index_start as usize)
            .take(mesh.index_count as usize)
        {
            if index >= mesh.vertex_count {
                return Err(SceneStorageError::InvalidMeshIndex {
                    mesh: mesh_index,
                    index,
                    vertex_count: mesh.vertex_count,
                });
            }
        }
    }
    for source in &document.mesh_source_records {
        let mesh =
            document
                .meshes
                .get(source.mesh as usize)
                .ok_or(SceneStorageError::InvalidRange {
                    field: "mesh_source_record.mesh",
                    start: source.mesh,
                    count: 1,
                    len: document.meshes.len(),
                })?;
        validate_range(
            "mesh_source_record.index_range",
            source.index_start,
            source.index_count,
            mesh.index_count as usize,
        )?;
    }
    for subdraw in &document.mesh_clipping_subdraws {
        validate_range(
            "mesh_clipping_subdraw.mesh",
            subdraw.mesh,
            1,
            document.meshes.len(),
        )?;
        validate_string(document, "mesh_clipping_subdraw.mask", subdraw.mask)?;
        validate_optional_resource(
            document,
            "mesh_clipping_subdraw.mask_resource",
            subdraw.mask_resource,
        )?;
        validate_range(
            "mesh_clipping_subdraw.target_sources",
            subdraw.target_source_start,
            subdraw.target_source_count,
            document.mesh_clipping_source_ordinals.len(),
        )?;
        validate_range(
            "mesh_clipping_subdraw.mask_sources",
            subdraw.mask_source_start,
            subdraw.mask_source_count,
            document.mesh_clipping_source_ordinals.len(),
        )?;
        let source_count = document
            .mesh_source_records
            .iter()
            .filter(|source| source.mesh == subdraw.mesh)
            .count() as u32;
        for ordinal in document
            .mesh_clipping_source_ordinals
            .iter()
            .skip(subdraw.target_source_start as usize)
            .take(subdraw.target_source_count as usize)
            .chain(
                document
                    .mesh_clipping_source_ordinals
                    .iter()
                    .skip(subdraw.mask_source_start as usize)
                    .take(subdraw.mask_source_count as usize),
            )
        {
            if *ordinal >= source_count {
                return Err(SceneStorageError::InvalidRange {
                    field: "mesh_clipping_subdraw.source_ordinal",
                    start: *ordinal,
                    count: 1,
                    len: source_count as usize,
                });
            }
        }
    }
    for slice in &document.mesh_clipping_slices {
        validate_range(
            "mesh_clipping_slice.mesh",
            slice.mesh,
            1,
            document.meshes.len(),
        )?;
        validate_range(
            "mesh_clipping_slice.index_range",
            slice.index_start,
            slice.index_count,
            document.mesh_indices.len(),
        )?;
        if matches!(
            slice.role,
            SceneMeshClippingSliceRole::MaskProducer | SceneMeshClippingSliceRole::ClippedTarget
        ) {
            let subdraw_count = document
                .mesh_clipping_subdraws
                .iter()
                .filter(|subdraw| subdraw.mesh == slice.mesh)
                .count();
            validate_range(
                "mesh_clipping_slice.subdraw",
                slice.subdraw,
                1,
                subdraw_count,
            )?;
        }
    }
    for (puppet_index, puppet) in document.puppets.iter().enumerate() {
        validate_range("puppet.object", puppet.object.0, 1, document.objects.len())?;
        validate_optional_resource(document, "puppet.resource", puppet.resource)?;
        validate_range(
            "puppet.mesh_range",
            puppet.mesh_start,
            puppet.mesh_count,
            document.meshes.len(),
        )?;
        validate_range(
            "puppet.bone_range",
            puppet.bone_start,
            puppet.bone_count,
            document.puppet_bones.len(),
        )?;
        validate_range(
            "puppet.attachment_range",
            puppet.attachment_start,
            puppet.attachment_count,
            document.puppet_attachments.len(),
        )?;
        for (mesh_index, mesh) in document
            .meshes
            .iter()
            .enumerate()
            .skip(puppet.mesh_start as usize)
            .take(puppet.mesh_count as usize)
        {
            for (vertex_index, vertex) in document
                .mesh_vertices
                .iter()
                .skip(mesh.vertex_start as usize)
                .take(mesh.vertex_count as usize)
                .enumerate()
            {
                for slot in 0..4 {
                    let weight = vertex.blend_weights[slot];
                    if !weight.is_finite() || weight < 0.0 {
                        return Err(SceneStorageError::InvalidPuppetBlendWeight {
                            puppet: puppet_index,
                            mesh: mesh_index,
                            vertex: vertex_index,
                            slot,
                        });
                    }
                    let bone_index = vertex.blend_indices[slot];
                    if weight > 1.0e-6 && bone_index >= puppet.bone_count {
                        return Err(SceneStorageError::InvalidPuppetBlendIndex {
                            puppet: puppet_index,
                            mesh: mesh_index,
                            vertex: vertex_index,
                            slot,
                            bone_index,
                            bone_count: puppet.bone_count,
                        });
                    }
                }
            }
        }
        for bone in document
            .puppet_bones
            .iter()
            .skip(puppet.bone_start as usize)
            .take(puppet.bone_count as usize)
        {
            validate_range("puppet_bone.puppet", bone.puppet, 1, document.puppets.len())?;
            if bone.puppet as usize != puppet_index {
                return Err(SceneStorageError::InvalidRange {
                    field: "puppet_bone.puppet_owner",
                    start: bone.puppet,
                    count: 1,
                    len: puppet_index,
                });
            }
            validate_string(document, "puppet_bone.name", bone.name)?;
            validate_string(
                document,
                "puppet_bone.simulation_json",
                bone.simulation_json,
            )?;
        }
        for attachment in document
            .puppet_attachments
            .iter()
            .skip(puppet.attachment_start as usize)
            .take(puppet.attachment_count as usize)
        {
            validate_range(
                "puppet_attachment.puppet",
                attachment.puppet,
                1,
                document.puppets.len(),
            )?;
            if attachment.puppet as usize != puppet_index {
                return Err(SceneStorageError::InvalidRange {
                    field: "puppet_attachment.puppet_owner",
                    start: attachment.puppet,
                    count: 1,
                    len: puppet_index,
                });
            }
            validate_string(document, "puppet_attachment.name", attachment.name)?;
        }
    }
    for particle in &document.particles {
        validate_range(
            "particle.object",
            particle.object.0,
            1,
            document.objects.len(),
        )?;
        validate_resource(document, "particle.resource", particle.resource)?;
        validate_optional_material(document, "particle.material", particle.material)?;
        if matches!(
            particle.simulation,
            SceneParticleSimulationKind::FallingLeaves
                | SceneParticleSimulationKind::AmbientSparkles
                | SceneParticleSimulationKind::FloralOscillation
        ) && (particle.max_count == 0
            || !particle.rate.is_finite()
            || particle.rate <= 0.0
            || !particle.lifetime_min.is_finite()
            || !particle.lifetime_max.is_finite()
            || particle.lifetime_min <= 0.0
            || particle.lifetime_max < particle.lifetime_min
            || !particle.size_min.is_finite()
            || !particle.size_max.is_finite()
            || particle.size_min <= 0.0
            || particle.size_max < particle.size_min)
        {
            return Err(SceneStorageError::InvalidRange {
                field: "particle.procedural_profile",
                start: particle.max_count,
                count: 1,
                len: document.particles.len(),
            });
        }
        if particle.simulation == SceneParticleSimulationKind::AmbientSparkles
            && (!particle.oscillation_frequency_min.is_finite()
                || !particle.oscillation_frequency_max.is_finite()
                || particle.oscillation_frequency_min < 0.0
                || particle.oscillation_frequency_max < particle.oscillation_frequency_min
                || !particle.oscillation_phase_min.is_finite()
                || !particle.oscillation_phase_max.is_finite()
                || particle.oscillation_phase_max < particle.oscillation_phase_min
                || !particle.oscillation_scale_min.is_finite()
                || !particle.oscillation_scale_max.is_finite()
                || particle.oscillation_scale_min < 0.0
                || particle.oscillation_scale_max < particle.oscillation_scale_min)
        {
            return Err(SceneStorageError::InvalidRange {
                field: "particle.ambient_sparkles_oscillation",
                start: particle.max_count,
                count: 1,
                len: document.particles.len(),
            });
        }
        if particle.simulation == SceneParticleSimulationKind::FloralOscillation
            && (!valid_oscillation_range(
                particle.position_oscillation_frequency_min,
                particle.position_oscillation_frequency_max,
                0.0,
            ) || !valid_oscillation_range(
                particle.position_oscillation_phase_min,
                particle.position_oscillation_phase_max,
                f32::NEG_INFINITY,
            ) || !valid_oscillation_range(
                particle.position_oscillation_scale_min,
                particle.position_oscillation_scale_max,
                0.0,
            ) || !valid_vec3(particle.position_oscillation_mask)
                || !valid_oscillation_range(
                    particle.size_oscillation_frequency_min,
                    particle.size_oscillation_frequency_max,
                    0.0,
                )
                || !valid_oscillation_range(
                    particle.size_oscillation_phase_min,
                    particle.size_oscillation_phase_max,
                    f32::NEG_INFINITY,
                )
                || !valid_oscillation_range(
                    particle.size_oscillation_scale_min,
                    particle.size_oscillation_scale_max,
                    0.0,
                ))
        {
            return Err(SceneStorageError::InvalidRange {
                field: "particle.floral_oscillation",
                start: particle.max_count,
                count: 1,
                len: document.particles.len(),
            });
        }
    }
    for effect in &document.effects {
        validate_optional_resource(document, "effect.resource", effect.resource)?;
        validate_string(document, "effect.replacement_key", effect.replacement_key)?;
        validate_range(
            "effect.pass_range",
            effect.pass_start,
            effect.pass_count,
            document.effect_passes.len(),
        )?;
        validate_range(
            "effect.fbo_range",
            effect.fbo_start,
            effect.fbo_count,
            document.effect_fbos.len(),
        )?;
    }
    for pass in &document.effect_passes {
        validate_optional_material(document, "effect_pass.material", pass.material)?;
        validate_string(document, "effect_pass.command", pass.command)?;
        validate_string(document, "effect_pass.source", pass.source)?;
        validate_string(document, "effect_pass.target", pass.target)?;
        validate_range(
            "effect_pass.binding_range",
            pass.binding_start,
            pass.binding_count,
            document.effect_bindings.len(),
        )?;
        validate_range(
            "effect_pass.combo_range",
            pass.combo_start,
            pass.combo_count,
            document.effect_combos.len(),
        )?;
    }
    for graph in &document.render_graphs {
        validate_range(
            "render_graph.pass_range",
            graph.pass_start,
            graph.pass_count,
            document.render_passes.len(),
        )?;
        validate_range(
            "render_graph.unsupported_range",
            graph.unsupported_start,
            graph.unsupported_count,
            document.unsupported.len(),
        )?;
        if graph.activation_policy == SceneRenderGraphActivationPolicy::AnyEffectVisible {
            let effect_binding_count = document
                .render_passes
                .get(
                    graph.pass_start as usize
                        ..graph.pass_start.saturating_add(graph.pass_count) as usize,
                )
                .into_iter()
                .flatten()
                .filter(|pass| pass.object == graph.object)
                .map(|pass| pass.effect_binding_count)
                .sum::<u32>();
            if effect_binding_count == 0 {
                return Err(SceneStorageError::InvalidRange {
                    field: "render_graph.activation_effect_binding_range",
                    start: graph.pass_start,
                    count: graph.pass_count,
                    len: document.render_passes.len(),
                });
            }
        }
    }
    for pass in &document.render_passes {
        validate_optional_material(document, "render_pass.material", pass.material)?;
        validate_string(document, "render_pass.shader_key", pass.shader_key)?;
        validate_string(document, "render_pass.target_name", pass.target_name)?;
        validate_range(
            "render_pass.binding_range",
            pass.binding_start,
            pass.binding_count,
            document.render_bindings.len(),
        )?;
        validate_range(
            "render_pass.effect_binding_range",
            pass.effect_binding_start,
            pass.effect_binding_count,
            document.object_effects.len(),
        )?;
        let valid_effect_policy = match pass.effect_visibility_policy {
            SceneRenderEffectVisibilityPolicy::None => {
                pass.effect_binding_start == u32::MAX && pass.effect_binding_count == 0
            }
            SceneRenderEffectVisibilityPolicy::Passthrough
            | SceneRenderEffectVisibilityPolicy::FlatRoundedMask => pass.effect_binding_count == 1,
            SceneRenderEffectVisibilityPolicy::WaterWavesStages => {
                (2..=9).contains(&pass.effect_binding_count)
            }
            SceneRenderEffectVisibilityPolicy::MaterialStages => {
                (1..=32).contains(&pass.effect_binding_count)
            }
        };
        let owned_effects_match_object = document
            .object_effects
            .get(
                pass.effect_binding_start as usize
                    ..pass
                        .effect_binding_start
                        .saturating_add(pass.effect_binding_count) as usize,
            )
            .is_some_and(|effects| effects.iter().all(|effect| effect.object == pass.object));
        if !valid_effect_policy
            || (pass.effect_visibility_policy != SceneRenderEffectVisibilityPolicy::None
                && !owned_effects_match_object)
        {
            return Err(SceneStorageError::InvalidRange {
                field: "render_pass.effect_visibility_contract",
                start: pass.effect_binding_start,
                count: pass.effect_binding_count,
                len: document.object_effects.len(),
            });
        }
    }
    for binding in &document.render_bindings {
        validate_string(document, "render_binding.name", binding.name)?;
    }
    for target in &document.image_targets {
        validate_string(document, "image_target.name", target.name)?;
        validate_string(document, "image_target.format", target.format)?;
    }
    for contract in &document.shader_contracts {
        validate_string(document, "shader_contract.shader_key", contract.shader_key)?;
        validate_string(
            document,
            "shader_contract.pipeline_key",
            contract.pipeline_key,
        )?;
        validate_range(
            "shader_contract.constant_range",
            contract.constant_start,
            contract.constant_count,
            document.shader_constant_names.len(),
        )?;
    }
    for name in &document.shader_constant_names {
        validate_string(document, "shader_contract.constant_name", *name)?;
    }
    Ok(())
}

fn validate_pointer_parallax(document: &SceneBinaryDocument) -> Result<(), SceneStorageError> {
    let camera = document.camera_parallax;
    if ![camera.amount, camera.delay, camera.mouse_influence]
        .into_iter()
        .all(f32::is_finite)
        || camera.delay < 0.0
    {
        return Err(SceneStorageError::InvalidPointerParallaxBinding {
            object: SceneObjectHandle(INVALID_OBJECT_ID),
            reason: "non-finite scalar or negative delay",
        });
    }
    for binding in &document.object_parallax_depths {
        validate_range(
            "object_parallax_depth.object",
            binding.object.0,
            1,
            document.objects.len(),
        )?;
        if !binding.depth.into_iter().all(f32::is_finite) {
            return Err(SceneStorageError::InvalidPointerParallaxBinding {
                object: binding.object,
                reason: "non-finite depth",
            });
        }
    }
    if document
        .object_parallax_depths
        .windows(2)
        .any(|pair| pair[0].object.0 >= pair[1].object.0)
    {
        return Err(SceneStorageError::InvalidPointerParallaxBinding {
            object: SceneObjectHandle(INVALID_OBJECT_ID),
            reason: "records are not strictly ordered by object",
        });
    }
    Ok(())
}

fn valid_oscillation_range(min: f32, max: f32, lower_bound: f32) -> bool {
    min.is_finite() && max.is_finite() && min >= lower_bound && max >= min
}

fn valid_vec3(value: SceneVec3) -> bool {
    value.x.is_finite() && value.y.is_finite() && value.z.is_finite()
}

pub(super) fn validate_project(document: &SceneBinaryDocument) -> Result<(), SceneStorageError> {
    let project = &document.project;
    validate_string(document, "project.title", project.title)?;
    validate_string(document, "project.wallpaper_type", project.wallpaper_type)?;
    validate_string(document, "project.scene_file", project.scene_file)?;
    validate_string(document, "project.preview", project.preview)?;
    validate_string(document, "project.properties_json", project.properties_json)
}

pub(super) fn validate_string(
    document: &SceneBinaryDocument,
    field: &'static str,
    id: SceneStringId,
) -> Result<(), SceneStorageError> {
    if !id.is_some() || (id.0 as usize) < document.strings.len() {
        Ok(())
    } else {
        Err(SceneStorageError::InvalidStringId { field, id })
    }
}

pub(super) fn validate_resource(
    document: &SceneBinaryDocument,
    field: &'static str,
    id: SceneResourceId,
) -> Result<(), SceneStorageError> {
    if document.resources.iter().any(|resource| resource.id == id) {
        Ok(())
    } else {
        Err(SceneStorageError::InvalidResourceId { field, id })
    }
}

pub(super) fn validate_optional_resource(
    document: &SceneBinaryDocument,
    field: &'static str,
    id: SceneResourceId,
) -> Result<(), SceneStorageError> {
    if !id.is_some() {
        Ok(())
    } else {
        validate_resource(document, field, id)
    }
}

pub(super) fn validate_optional_material(
    document: &SceneBinaryDocument,
    field: &'static str,
    handle: SceneMaterialHandle,
) -> Result<(), SceneStorageError> {
    if handle.0 == INVALID_MATERIAL_ID || (handle.0 as usize) < document.materials.len() {
        Ok(())
    } else {
        Err(SceneStorageError::InvalidMaterialHandle { field, handle })
    }
}

pub(super) fn validate_payload(
    document: &SceneBinaryDocument,
    resource: &SceneResourceRecord,
) -> Result<(), SceneStorageError> {
    let Ok(start) = usize::try_from(resource.payload_offset) else {
        return Err(SceneStorageError::InvalidPayloadRange {
            resource: resource.id,
            offset: resource.payload_offset,
            len: resource.payload_len,
            payload_len: document.resource_payload.len(),
        });
    };
    let Ok(len) = usize::try_from(resource.payload_len) else {
        return Err(SceneStorageError::InvalidPayloadRange {
            resource: resource.id,
            offset: resource.payload_offset,
            len: resource.payload_len,
            payload_len: document.resource_payload.len(),
        });
    };
    let Some(end) = start.checked_add(len) else {
        return Err(SceneStorageError::InvalidPayloadRange {
            resource: resource.id,
            offset: resource.payload_offset,
            len: resource.payload_len,
            payload_len: document.resource_payload.len(),
        });
    };
    if end <= document.resource_payload.len() {
        Ok(())
    } else {
        Err(SceneStorageError::InvalidPayloadRange {
            resource: resource.id,
            offset: resource.payload_offset,
            len: resource.payload_len,
            payload_len: document.resource_payload.len(),
        })
    }
}

pub(super) fn validate_texture_payload(
    document: &SceneBinaryDocument,
    texture: SceneResourceId,
    offset: u64,
    len: u64,
) -> Result<(), SceneStorageError> {
    let valid = usize::try_from(offset)
        .ok()
        .and_then(|start| {
            usize::try_from(len)
                .ok()
                .and_then(|len| start.checked_add(len))
        })
        .is_some_and(|end| end <= document.texture_payload.len());
    if valid {
        Ok(())
    } else {
        Err(SceneStorageError::InvalidTexturePayloadRange {
            texture,
            offset,
            len,
            payload_len: document.texture_payload.len(),
        })
    }
}

pub(super) fn validate_range(
    field: &'static str,
    start: u32,
    count: u32,
    len: usize,
) -> Result<(), SceneStorageError> {
    if start == u32::MAX && count == 0 {
        return Ok(());
    }
    let start_usize = start as usize;
    let count_usize = count as usize;
    let Some(end) = start_usize.checked_add(count_usize) else {
        return Err(SceneStorageError::InvalidRange {
            field,
            start,
            count,
            len,
        });
    };
    if end <= len {
        Ok(())
    } else {
        Err(SceneStorageError::InvalidRange {
            field,
            start,
            count,
            len,
        })
    }
}
