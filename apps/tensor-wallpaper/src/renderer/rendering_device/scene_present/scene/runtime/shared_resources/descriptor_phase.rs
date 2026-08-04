//! Immutable descriptor heaps for one frame-slot/reference-phase pair.

use super::*;

impl SharedSceneFrameResources {
    pub(in super::super) fn lower_descriptor_phase(
        &mut self,
        device: &Backend,
        frame_slot: usize,
        reference_phase: usize,
        inputs: SharedSceneDescriptorInputs<'_>,
    ) -> Result<(), String> {
        if reference_phase != self.descriptor_phases.len() {
            return Err(format!(
                "scene frame slot {frame_slot} descriptor phase {reference_phase} is not dense"
            ));
        }
        let mut phase = self.create_descriptor_phase(device, frame_slot, reference_phase)?;
        if inputs.slot_kinds.len() != self.resource_descriptor_count {
            return Err(format!(
                "scene resource slot kinds differ from heap element count ({} vs {})",
                inputs.slot_kinds.len(),
                self.resource_descriptor_count
            ));
        }
        let mut resources = (0..self.resource_descriptor_count)
            .map(|_| None)
            .collect::<Vec<Option<SharedSceneResourceDescriptor<'_>>>>();
        let mut samplers = vec![None; self.sampler_descriptor_count];
        for (draw_index, draw) in inputs.draw_commands.iter().enumerate() {
            assign_resource(
                &mut resources,
                draw.resource_descriptor_base,
                SharedSceneResourceDescriptor::UniformBuffer {
                    buffer: &self.transform,
                    offset: draw_index as u64 * SCENE_DRAW_UNIFORM_BYTES,
                    size: SCENE_DRAW_UNIFORM_BYTES,
                },
                draw_index,
            )?;
            if let Some(index) = draw.material_resource_descriptor {
                let buffer = self.material.as_ref().ok_or_else(|| {
                    format!(
                        "scene draw {draw_index} has a material descriptor without a material buffer"
                    )
                })?;
                assign_resource(
                    &mut resources,
                    index,
                    SharedSceneResourceDescriptor::UniformBuffer {
                        buffer,
                        offset: draw_index as u64 * SCENE_MATERIAL_UNIFORM_BYTES,
                        size: SCENE_MATERIAL_UNIFORM_BYTES,
                    },
                    draw_index,
                )?;
            }
            if let Some(index) = draw.skinning_resource_descriptor {
                let buffer = self.skinning.as_ref().ok_or_else(|| {
                    format!(
                        "scene draw {draw_index} has a skinning descriptor without a skinning buffer"
                    )
                })?;
                assign_resource(
                    &mut resources,
                    index,
                    SharedSceneResourceDescriptor::StorageBuffer {
                        buffer,
                        offset: draw.skinning_byte_offset,
                        size: draw.skinning_byte_count,
                    },
                    draw_index,
                )?;
            }
            if let Some(index) = draw.particle_resource_descriptor {
                let buffer = inputs
                    .cold
                    .particles
                    .as_ref()
                    .map(|particles| &particles.simulation)
                    .ok_or_else(|| {
                        format!(
                            "scene draw {draw_index} has a particle descriptor without particle state"
                        )
                    })?;
                assign_resource(
                    &mut resources,
                    index,
                    SharedSceneResourceDescriptor::StorageBuffer {
                        buffer,
                        offset: 0,
                        size: buffer.size(),
                    },
                    draw_index,
                )?;
            }
            lower_sampled_descriptors(
                &mut resources,
                &mut samplers,
                draw_index,
                draw,
                inputs.sampled_binding_plan,
                inputs.cold,
                inputs.scene_color,
            )?;
            lower_input_attachment_descriptors(
                &mut resources,
                draw_index,
                draw,
                inputs.input_attachment_binding_plan,
                inputs.cold,
            )?;
        }
        lower_particle_descriptors(&mut resources, &inputs)?;
        self.lower_scene_owned_uniforms(&mut resources, &inputs)?;
        let resources = resources
            .into_iter()
            .zip(inputs.slot_kinds.iter().copied())
            .enumerate()
            .map(|(index, (descriptor, kind))| match descriptor {
                Some(descriptor) if descriptor.slot_kind() == kind => Ok(descriptor),
                Some(descriptor) => Err(format!(
                    "scene resource descriptor {index} is {:?}, but its retained slot is {kind:?}",
                    descriptor.slot_kind()
                )),
                None => Ok(SharedSceneResourceDescriptor::Reserved { kind }),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let samplers = samplers
            .into_iter()
            .enumerate()
            .map(|(index, sampler)| {
                sampler.ok_or_else(|| format!("scene sampler descriptor {index} is unbound"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        phase.descriptor_bindings = Some(SharedSceneDescriptorBindings::create(
            &phase.resource_heap,
            phase.sampler_heap.as_ref(),
            &resources,
            &samplers,
        )?);
        self.descriptor_phases.push(phase);
        Ok(())
    }

    fn create_descriptor_phase(
        &self,
        device: &Backend,
        frame_slot: usize,
        reference_phase: usize,
    ) -> Result<SharedSceneDescriptorPhaseResources, String> {
        let resource_capacity = device
            .descriptor_heap_capacity_bytes(
                DescriptorHeapKind::Resource,
                self.resource_descriptor_count as u64,
            )
            .map_err(|error| format!("size scene resource heap phase: {error}"))?;
        let resource_heap = device
            .create_descriptor_heap(&DescriptorHeapDescriptor {
                label: Some(format!(
                    "tensor-wallpaper-scene-resource-heap-slot-{frame_slot}-phase-{reference_phase}"
                )),
                kind: DescriptorHeapKind::Resource,
                descriptor_capacity: resource_capacity,
                embedded_samplers: false,
            })
            .map_err(|error| format!("create scene resource heap phase: {error}"))?;
        let sampler_heap = (self.sampler_descriptor_count != 0)
            .then(|| {
                let capacity = device.descriptor_heap_capacity_bytes(
                    DescriptorHeapKind::Sampler,
                    self.sampler_descriptor_count as u64,
                )?;
                device.create_descriptor_heap(&DescriptorHeapDescriptor {
                    label: Some(format!(
                        "tensor-wallpaper-scene-sampler-heap-slot-{frame_slot}-phase-{reference_phase}"
                    )),
                    kind: DescriptorHeapKind::Sampler,
                    descriptor_capacity: capacity,
                    embedded_samplers: false,
                })
            })
            .transpose()
            .map_err(|error| format!("create scene sampler heap phase: {error}"))?;
        Ok(SharedSceneDescriptorPhaseResources {
            resource_heap,
            sampler_heap,
            reference_phase,
            descriptor_bindings: None,
        })
    }

    fn lower_scene_owned_uniforms<'a>(
        &'a self,
        resources: &mut [Option<SharedSceneResourceDescriptor<'a>>],
        inputs: &SharedSceneDescriptorInputs<'a>,
    ) -> Result<(), String> {
        if inputs.scene_owned_uniform_plan.is_empty() {
            return Ok(());
        }
        let buffer = self.scene_owned_uniform.as_ref().ok_or_else(|| {
            "scene-owned uniform descriptor plan has no retained buffer".to_owned()
        })?;
        for (draw_index, descriptor_lane, offset, size) in
            inputs.scene_owned_uniform_plan.descriptor_slices()
        {
            let draw = inputs.draw_commands.get(draw_index).ok_or_else(|| {
                format!("scene-owned uniform descriptor draw {draw_index} is missing")
            })?;
            assign_resource(
                resources,
                draw.scene_owned_uniform_descriptor_base + descriptor_lane,
                SharedSceneResourceDescriptor::UniformBuffer {
                    buffer,
                    offset,
                    size,
                },
                draw_index,
            )?;
        }
        Ok(())
    }

    pub(in super::super) fn descriptor_phase(
        &self,
        reference_phase: usize,
    ) -> Result<&SharedSceneDescriptorPhaseResources, String> {
        self.descriptor_phases
            .get(reference_phase)
            .filter(|phase| phase.reference_phase == reference_phase)
            .ok_or_else(|| format!("scene descriptor phase {reference_phase} is missing"))
    }

    #[cfg(feature = "video")]
    pub(in super::super) fn bind_decoded_video_frame(
        &mut self,
        reference_phase: usize,
        media_instance: u32,
        frame: &vulkan_renderer::DecodedVideoFrame,
    ) -> Result<(), String> {
        let phase = self
            .descriptor_phases
            .get_mut(reference_phase)
            .filter(|phase| phase.reference_phase == reference_phase)
            .ok_or_else(|| format!("scene descriptor phase {reference_phase} is missing"))?;
        let bindings = phase.descriptor_bindings.as_mut().ok_or_else(|| {
            format!("scene descriptor phase {reference_phase} has no retained bindings")
        })?;
        bindings.bind_decoded_video_frame(&phase.resource_heap, media_instance, frame)
    }
}

impl SharedSceneDescriptorPhaseResources {
    pub(in super::super) fn validate_external_video_bound(&self) -> Result<(), String> {
        self.descriptor_bindings
            .as_ref()
            .ok_or_else(|| {
                format!(
                    "scene descriptor phase {} has no retained bindings",
                    self.reference_phase
                )
            })?
            .validate_external_video_bound()
    }
}

fn lower_particle_descriptors<'a>(
    resources: &mut [Option<SharedSceneResourceDescriptor<'a>>],
    inputs: &SharedSceneDescriptorInputs<'a>,
) -> Result<(), String> {
    match (
        inputs.particle_global_descriptor_base,
        inputs.cold.particles.as_ref(),
    ) {
        (None, None) => Ok(()),
        (Some(base), Some(particles)) => {
            for (lane, buffer) in [
                &particles.state,
                &particles.indirect,
                &particles.frame_time,
                &particles.simulation,
                &particles.random,
            ]
            .into_iter()
            .enumerate()
            {
                assign_resource(
                    resources,
                    base + lane,
                    SharedSceneResourceDescriptor::StorageBuffer {
                        buffer,
                        offset: 0,
                        size: buffer.size(),
                    },
                    inputs.draw_commands.len(),
                )?;
            }
            Ok(())
        }
        _ => Err("shared particle resources and descriptor base must exist together".into()),
    }
}
