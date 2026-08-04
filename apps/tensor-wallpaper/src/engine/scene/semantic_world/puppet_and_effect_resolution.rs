// Attachment anchors, puppet palettes, and effect visibility resolve from one semantic snapshot.
impl<'a> SceneSemanticWorld<'a> {
    fn resolve_attachment_link_at(
        &self,
        child: SceneObjectHandle,
        parent_state: &ResolvedObjectState,
        attachment: SceneStringId,
        retained_puppets: &mut Option<&mut [RetainedPuppetTopology]>,
        scene_time_seconds: f32,
    ) -> (ResolvedAttachmentLink, Option<[f32; 16]>) {
        let parent = parent_state.object;
        let Some(parent_puppet) = self.puppet_binding(parent) else {
            return (
                ResolvedAttachmentLink::unresolved(child, parent, attachment),
                None,
            );
        };
        let Some(puppet) = self
            .storage
            .puppets()
            .get(parent_puppet.puppet_index as usize)
        else {
            return (
                ResolvedAttachmentLink::unresolved(child, parent, attachment),
                None,
            );
        };
        let requested = self.storage.string(attachment);
        for record in self.storage.puppet_attachments(puppet) {
            let matches_name = record.name == attachment
                || requested
                    .zip(self.storage.string(record.name))
                    .is_some_and(|(requested, candidate)| requested == candidate);
            if matches_name {
                return (
                    ResolvedAttachmentLink::resolved(
                        child,
                        parent,
                        attachment,
                        parent_puppet.puppet_index,
                        record.bone_index,
                    ),
                    retained_puppets
                        .as_deref_mut()
                        .and_then(|topologies| {
                            resolve_retained_attachment_anchor(topologies, parent_state, record)
                        })
                        .or_else(|| {
                            self.resolve_puppet_attachment_anchor(
                                parent_state,
                                puppet,
                                record,
                                scene_time_seconds,
                            )
                        }),
                );
            }
        }
        (
            ResolvedAttachmentLink::with_parent_puppet(
                child,
                parent,
                attachment,
                parent_puppet.puppet_index,
            ),
            None,
        )
    }

    fn resolve_puppet_bone_palettes(
        &self,
        objects: &[ResolvedObjectState],
        scene_time_seconds: f32,
    ) -> Result<
        (
            Vec<ResolvedPuppetBonePalette>,
            Vec<ResolvedPuppetBoneMatrix>,
        ),
        SceneSemanticWorldError,
    > {
        let mut palettes = Vec::new();
        let mut matrices = Vec::new();
        for object in objects {
            if object.puppet_index == INVALID_RESOLVED_INDEX {
                continue;
            }
            let puppet = self
                .storage
                .puppets()
                .get(object.puppet_index as usize)
                .ok_or(SceneSemanticWorldError::MissingPuppetRecord {
                    object: object.object,
                    puppet_index: object.puppet_index,
                })?;
            let bone_start = u32::try_from(matrices.len()).map_err(|_| {
                SceneSemanticWorldError::TooManyPuppetBones {
                    count: matrices.len(),
                }
            })?;
            let local_start = matrices.len();
            let mut bind_world = Vec::<(u32, [f32; 16])>::new();
            let mut animated_world = Vec::<(u32, [f32; 16])>::new();
            for bone in self.storage.puppet_bones(puppet) {
                let bind_parent = if bone.parent_index >= 0 {
                    bind_world
                        .iter()
                        .find(|(index, _)| *index == bone.parent_index as u32)
                        .map(|(_, matrix)| *matrix)
                } else {
                    None
                };
                let animated_parent = if bone.parent_index >= 0 {
                    animated_world
                        .iter()
                        .find(|(index, _)| *index == bone.parent_index as u32)
                        .map(|(_, matrix)| *matrix)
                } else {
                    None
                };
                let bind_matrix = multiply_matrix(
                    &bind_parent.unwrap_or_else(identity_matrix),
                    &bone.local_bind_matrix,
                );
                let animated_local = sampled_puppet_bone_local_state(
                    self.storage,
                    object.object,
                    object.puppet_index,
                    bone,
                    scene_time_seconds,
                );
                let animated_matrix = multiply_matrix(
                    &animated_parent.unwrap_or_else(identity_matrix),
                    &animated_local.matrix,
                );
                let inverse_bind = inverse_affine_matrix(&bind_matrix).ok_or(
                    SceneSemanticWorldError::NonInvertiblePuppetBindMatrix {
                        object: object.object,
                        bone_index: bone.bone_index,
                    },
                )?;
                bind_world.push((bone.bone_index, bind_matrix));
                animated_world.push((bone.bone_index, animated_matrix));
                matrices.push(ResolvedPuppetBoneMatrix {
                    puppet_index: object.puppet_index,
                    bone_index: bone.bone_index,
                    parent_index: bone.parent_index,
                    matrix: multiply_matrix(&animated_matrix, &inverse_bind),
                    alpha: animated_local.alpha,
                });
            }
            let bone_count = u32::try_from(matrices.len() - local_start).map_err(|_| {
                SceneSemanticWorldError::TooManyPuppetBones {
                    count: matrices.len(),
                }
            })?;
            palettes.push(ResolvedPuppetBonePalette {
                object: object.object,
                puppet_index: object.puppet_index,
                bone_start,
                bone_count,
                resolved_visible: object.resolved_visible,
            });
        }
        Ok((palettes, matrices))
    }

    fn resolve_object_effects(
        &self,
        objects: &[ResolvedObjectState],
        script_deltas: &[SceneScriptDelta],
    ) -> Result<Vec<ResolvedObjectEffectState>, SceneSemanticWorldError> {
        let mut effects = Vec::new();
        for object in objects {
            let Some(component) = self
                .object_effect_components
                .get(object.entity.index())
                .and_then(Option::as_ref)
            else {
                continue;
            };
            let start = component.binding_start as usize;
            let end = start.saturating_add(component.binding_count as usize);
            let Some(bindings) = self.object_effect_bindings.get(start..end) else {
                return Err(SceneSemanticWorldError::ObjectEffectRangeOutOfBounds {
                    object: object.object,
                    start: component.binding_start,
                    count: component.binding_count,
                    len: self.object_effect_bindings.len(),
                });
            };
            for (local_index, binding) in bindings.iter().enumerate() {
                let binding_index = component.binding_start + local_index as u32;
                let effect_index = binding.effect.0;
                let effect = self.storage.effects().get(effect_index as usize).ok_or(
                    SceneSemanticWorldError::MissingEffectRecord {
                        object: object.object,
                        effect: binding.effect,
                    },
                )?;
                let self_visible = script_deltas
                    .iter()
                    .rev()
                    .find(|delta| {
                        delta.target == SceneScriptTarget::EffectVisible
                            && delta.object == binding.object
                            && delta.selector == binding_index
                    })
                    .map(|delta| delta.numeric[0] != 0.0)
                    .unwrap_or(binding.visible);
                effects.push(ResolvedObjectEffectState {
                    binding_index,
                    entity: object.entity,
                    object: object.object,
                    object_index: object.object_index,
                    effect: binding.effect,
                    effect_index,
                    instance_id: binding.instance_id,
                    self_visible,
                    object_resolved_visible: object.resolved_visible,
                    resolved_visible: self_visible && object.resolved_visible,
                    pass_start: effect.pass_start,
                    pass_count: effect.pass_count,
                    fbo_start: effect.fbo_start,
                    fbo_count: effect.fbo_count,
                });
            }
        }
        Ok(effects)
    }

    fn resolve_puppet_attachment_anchor(
        &self,
        parent_state: &ResolvedObjectState,
        puppet: &ScenePuppetRecord,
        attachment: &ScenePuppetAttachmentRecord,
        scene_time_seconds: f32,
    ) -> Option<[f32; 16]> {
        self.resolve_puppet_bone_world_matrix(
            parent_state.object,
            parent_state.world_matrix,
            parent_state.puppet_index,
            puppet,
            attachment.bone_index,
            scene_time_seconds,
        )
        .map(|bone_matrix| multiply_matrix(&bone_matrix, &attachment.local_matrix))
        .or_else(|| {
            Some(multiply_matrix(
                &parent_state.world_matrix,
                &attachment.local_matrix,
            ))
        })
    }

    fn resolve_puppet_bone_world_matrix(
        &self,
        object: SceneObjectHandle,
        object_world_matrix: [f32; 16],
        puppet_index: u32,
        puppet: &ScenePuppetRecord,
        bone_index: u32,
        scene_time_seconds: f32,
    ) -> Option<[f32; 16]> {
        let bones = self.storage.puppet_bones(puppet);
        let mut matrices = Vec::<ResolvedPuppetBoneMatrix>::with_capacity(bones.len());
        for bone in bones {
            let parent_matrix = if bone.parent_index >= 0 {
                matrices
                    .iter()
                    .find(|matrix| matrix.bone_index == bone.parent_index as u32)
                    .map(|matrix| matrix.matrix)
            } else {
                None
            };
            let base_matrix = parent_matrix.unwrap_or(object_world_matrix);
            let local_state = sampled_puppet_bone_local_state(
                self.storage,
                object,
                puppet_index,
                bone,
                scene_time_seconds,
            );
            let matrix = multiply_matrix(&base_matrix, &local_state.matrix);
            matrices.push(ResolvedPuppetBoneMatrix {
                puppet_index,
                bone_index: bone.bone_index,
                parent_index: bone.parent_index,
                matrix,
                alpha: local_state.alpha,
            });
            if bone.bone_index == bone_index {
                return Some(matrix);
            }
        }
        None
    }
}
