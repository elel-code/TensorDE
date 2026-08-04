//! Typed draw recording over retained shared scene resources.

mod draw;
mod graph;
#[cfg(feature = "video")]
mod video;
mod video_draw;

use vulkan_renderer::{
    BufferState, ColorAttachment, ColorImageCopy, CommandEncoder, ComputePassDescriptor, Extent2D,
    Image, IndexFormat, LoadOp, Origin2D, Rect2D, RenderingDescriptor, RenderingEncoder,
    ResolveMode, StoreOp, TextureLayout, TextureState, Viewport,
};

use crate::engine::scene::SceneRenderingDeviceDrawPrimitive;

use super::super::draw_recording::{SceneGpuDrawRange, SceneGpuScissor};
use super::super::effect_target::{
    SceneColorCopyCoverage, SharedSceneEffectCommand, SharedSceneEffectCommandKind,
    SharedSceneEffectCopySource, SharedSceneEffectExecutionPlan, SharedSceneEffectLoadOp,
};
use super::super::local_read::SceneLocalReadScopePassRole;
use super::SharedSceneGpuResources;
use draw::{record_bound_draw, scene_scissor};

impl SharedSceneGpuResources {
    /// Binds the one resource heap and optional sampler heap used by all scene
    /// draws in this command buffer. Draw recording only pushes absolute dense
    /// heap-element indices and never rebinds a per-draw heap slice.
    pub(super) fn bind_frame_heaps(
        &self,
        encoder: &mut CommandEncoder,
        frame_slot: usize,
        reference_phase: usize,
    ) -> Result<(), String> {
        let frame = self
            .frames
            .get(frame_slot)
            .ok_or_else(|| format!("shared scene frame slot {frame_slot} is missing"))?;
        let descriptors = frame.descriptor_phase(reference_phase)?;
        descriptors.validate_external_video_bound()?;
        unsafe {
            encoder
                .bind_descriptor_heap(&descriptors.resource_heap)
                .map_err(|error| format!("bind shared scene resource heap: {error}"))?;
            if let Some(sampler_heap) = descriptors.sampler_heap.as_ref() {
                encoder
                    .bind_descriptor_heap(sampler_heap)
                    .map_err(|error| format!("bind shared scene sampler heap: {error}"))?;
            }
        }
        Ok(())
    }

    pub(super) fn record_draw_range(
        &self,
        rendering: &mut RenderingEncoder<'_>,
        frame_slot: usize,
        range: SceneGpuDrawRange,
        extent: Extent2D,
    ) -> Result<(), String> {
        if extent.is_empty() {
            return Err("shared scene draw range has an empty extent".into());
        }
        let frame = self
            .frames
            .get(frame_slot)
            .ok_or_else(|| format!("shared scene frame slot {frame_slot} is missing"))?;
        let start = range.start as usize;
        let end = start
            .checked_add(range.count as usize)
            .ok_or_else(|| "shared scene draw range overflows".to_owned())?;
        let draws = self.draw_commands.get(start..end).ok_or_else(|| {
            format!(
                "shared scene draw range {start}..{end} exceeds {} draws",
                self.draw_commands.len()
            )
        })?;
        retain_descriptor_resources(self, rendering, frame);
        rendering
            .set_viewport(Viewport {
                x: 0.0,
                y: 0.0,
                width: extent.width as f32,
                height: extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            })
            .map_err(|error| format!("set shared scene viewport: {error}"))?;
        if draws.iter().any(|draw| {
            draw.enabled && draw.primitive == SceneRenderingDeviceDrawPrimitive::ObjectMesh
        }) {
            unsafe {
                rendering
                    .set_index_buffer(&self.cold.mesh.index, 0, IndexFormat::Uint32)
                    .map_err(|error| format!("bind shared scene index buffer: {error}"))?;
            }
        }

        for draw in draws.iter().filter(|draw| draw.enabled) {
            if draw.video_media_instance.is_some() {
                self.record_video_draw(rendering, frame, draw, extent)?;
                continue;
            }
            let pipeline = self
                .pipelines
                .entries
                .get(draw.pipeline_index as usize)
                .ok_or_else(|| {
                    format!(
                        "shared scene draw references missing pipeline {}",
                        draw.pipeline_index
                    )
                })?;
            rendering
                .bind_machine_code_pipeline(&pipeline.pipeline)
                .map_err(|error| format!("bind shared scene graphics pipeline: {error}"))?;
            if let Some(push) = draw.active_descriptor_push() {
                rendering
                    .push_data(0, push.bytes())
                    .map_err(|error| format!("push shared scene heap indices: {error}"))?;
            }
            if draw.dynamic_text {
                let offset =
                    self.draw_commands.len() as u64 * super::super::SCENE_DRAW_UNIFORM_BYTES;
                unsafe {
                    rendering
                        .set_vertex_buffer(1, &frame.transform, offset)
                        .map_err(|error| format!("bind shared dynamic-text instances: {error}"))?;
                }
            } else if let Some(offset) = draw.vertex_buffer_byte_offset {
                unsafe {
                    rendering
                        .set_vertex_buffer(0, &self.cold.mesh.vertex, offset)
                        .map_err(|error| format!("bind shared scene vertices: {error}"))?;
                }
            }
            rendering
                .set_scissor(scene_scissor(draw.scissor, extent))
                .map_err(|error| format!("set shared scene scissor: {error}"))?;
            unsafe {
                record_bound_draw(
                    rendering,
                    draw,
                    self.cold
                        .particles
                        .as_ref()
                        .map(|particles| &particles.indirect),
                )?
            };
        }
        Ok(())
    }

    pub(in crate::renderer::rendering_device::scene_present::scene::runtime) fn record_particle_compute(
        &self,
        encoder: &mut CommandEncoder,
        frame_slot: usize,
        reference_phase: usize,
    ) -> Result<bool, String> {
        let (Some(pipeline), Some(particles)) = (
            self.pipelines.particle_compute.as_ref(),
            self.cold.particles.as_ref(),
        ) else {
            if self.pipelines.particle_compute.is_some() || self.cold.particles.is_some() {
                return Err("shared particle pipeline and resources must exist together".into());
            }
            return Ok(false);
        };
        if particles.emitter_count == 0 || particles.max_capacity == 0 {
            return Ok(false);
        }
        self.bind_frame_heaps(encoder, frame_slot, reference_phase)?;
        encoder
            .transition_buffer(
                &particles.frame_time,
                BufferState::ComputeStorageReadWrite,
                BufferState::TransferDestination,
            )
            .map_err(|error| {
                format!("transition shared particle frame time for update: {error}")
            })?;
        unsafe {
            let frame_state_bytes = std::slice::from_raw_parts(
                self.particle_frame_scratch.as_ptr().cast::<u8>(),
                std::mem::size_of_val(self.particle_frame_scratch.as_slice()),
            );
            encoder
                .update_buffer(&particles.frame_time, 0, frame_state_bytes)
                .map_err(|error| format!("update shared particle frame time: {error}"))?;
        }
        encoder
            .transition_buffer(
                &particles.frame_time,
                BufferState::TransferDestination,
                BufferState::ComputeStorageReadWrite,
            )
            .map_err(|error| {
                format!("transition shared particle frame time for compute: {error}")
            })?;
        encoder
            .transition_buffer(
                &particles.indirect,
                BufferState::IndirectRead,
                BufferState::ComputeStorageReadWrite,
            )
            .map_err(|error| format!("transition shared particle indirect for compute: {error}"))?;
        encoder
            .transition_buffer(
                &particles.simulation,
                BufferState::StorageReadWrite,
                BufferState::ComputeStorageReadWrite,
            )
            .map_err(|error| {
                format!("transition shared particle simulation for compute: {error}")
            })?;
        encoder
            .transition_buffer(
                &particles.random,
                BufferState::StorageReadWrite,
                BufferState::ComputeStorageReadWrite,
            )
            .map_err(|error| {
                format!("transition shared particle random state for compute: {error}")
            })?;
        encoder.retain_resource(&particles.state);
        encoder.retain_resource(&particles.indirect);
        encoder.retain_resource(&particles.frame_time);
        encoder.retain_resource(&particles.simulation);
        encoder.retain_resource(&particles.random);
        {
            let mut compute = encoder.begin_compute(&ComputePassDescriptor {
                label: Some("tensor-wallpaper-scene-particle-update"),
            });
            compute
                .bind_machine_code_pipeline(pipeline.pipeline())
                .map_err(|error| format!("bind shared particle compute pipeline: {error}"))?;
            compute
                .push_data(0, pipeline.descriptor_push())
                .map_err(|error| format!("push shared particle heap indices: {error}"))?;
            unsafe {
                compute
                    .dispatch(particles.emitter_count, 1, 1)
                    .map_err(|error| format!("dispatch shared particle update: {error}"))?;
            }
        }
        encoder
            .transition_buffer(
                &particles.indirect,
                BufferState::ComputeStorageReadWrite,
                BufferState::IndirectRead,
            )
            .map_err(|error| format!("transition shared particle indirect for draw: {error}"))?;
        encoder
            .transition_buffer(
                &particles.simulation,
                BufferState::ComputeStorageReadWrite,
                BufferState::StorageReadWrite,
            )
            .map_err(|error| format!("transition shared particle simulation for draw: {error}"))?;
        encoder
            .transition_buffer(
                &particles.random,
                BufferState::ComputeStorageReadWrite,
                BufferState::StorageReadWrite,
            )
            .map_err(|error| {
                format!("transition shared particle random state for reuse: {error}")
            })?;
        Ok(true)
    }

    /// Records cold-planned batch atlas draws once before their authored
    /// graph consumers. Each unique tile is generated exactly once.
    pub(super) fn record_effect_batches(
        &self,
        encoder: &mut CommandEncoder,
        frame_slot: usize,
        reference_phase: usize,
    ) -> Result<(), String> {
        let plan = self.effect_execution_plan(reference_phase)?;
        for batch in &self.frame_execution_plan.effect_batches {
            let target = self.effect_target(batch.physical_slot)?;
            encoder
                .transition_image(
                    &target.image,
                    TextureState::FragmentSampledRead,
                    TextureState::ColorAttachmentWrite,
                )
                .map_err(|error| format!("transition shared effect batch target: {error}"))?;
            let attachments = [Some(color_attachment(
                &target.view,
                LoadOp::Discard,
                TextureLayout::ColorAttachment,
            ))];
            let descriptor = rendering_descriptor(
                "tensor-wallpaper-scene-effect-batch",
                target.descriptor.extent,
                &attachments,
            );
            unsafe {
                let mut rendering = encoder
                    .begin_rendering(&descriptor)
                    .map_err(|error| format!("begin shared effect batch: {error}"))?;
                for command_index in &batch.command_indices {
                    let command = plan.commands.get(*command_index).ok_or_else(|| {
                        format!(
                            "shared effect phase {reference_phase} is missing batch command {command_index}"
                        )
                    })?;
                    let SharedSceneEffectCommandKind::DynamicRender {
                        draw_start,
                        draw_count,
                        ..
                    } = command.kind
                    else {
                        unreachable!();
                    };
                    self.record_draw_range(
                        &mut rendering,
                        frame_slot,
                        SceneGpuDrawRange {
                            start: draw_start,
                            count: draw_count,
                        },
                        target.descriptor.extent,
                    )?;
                }
            }
            encoder
                .transition_image(
                    &target.image,
                    TextureState::ColorAttachmentWrite,
                    TextureState::FragmentSampledRead,
                )
                .map_err(|error| format!("transition shared effect batch for sampling: {error}"))?;
        }
        Ok(())
    }

    fn effect_execution_plan(
        &self,
        reference_phase: usize,
    ) -> Result<&SharedSceneEffectExecutionPlan, String> {
        self.effect_execution_plans
            .get(reference_phase)
            .filter(|plan| plan.reference_phase == reference_phase)
            .ok_or_else(|| format!("shared scene effect phase {reference_phase} is missing"))
    }

    fn effect_target(
        &self,
        physical_slot: u32,
    ) -> Result<&super::super::shared_resources::SharedSceneEffectTargetResource, String> {
        self.cold
            .effect_targets
            .target(physical_slot)
            .ok_or_else(|| format!("shared effect target physical slot {physical_slot} is missing"))
    }

    fn record_effect_render(
        &self,
        encoder: &mut CommandEncoder,
        frame_slot: usize,
        command: SharedSceneEffectCommand,
    ) -> Result<(), String> {
        let SharedSceneEffectCommandKind::DynamicRender {
            target_physical_slot,
            draw_start,
            draw_count,
            load_op,
            ..
        } = command.kind
        else {
            return Err("shared effect render received a non-render command".into());
        };
        let target = self.effect_target(target_physical_slot)?;
        let attachment_state = if load_op == SharedSceneEffectLoadOp::Load {
            TextureState::ColorAttachmentReadWrite
        } else {
            TextureState::ColorAttachmentWrite
        };
        encoder
            .transition_image(
                &target.image,
                TextureState::FragmentSampledRead,
                attachment_state,
            )
            .map_err(|error| format!("transition shared effect render target: {error}"))?;
        let attachments = [Some(color_attachment(
            &target.view,
            shared_load_op(load_op),
            TextureLayout::ColorAttachment,
        ))];
        let descriptor = rendering_descriptor(
            "tensor-wallpaper-scene-effect-pass",
            target.descriptor.extent,
            &attachments,
        );
        unsafe {
            let mut rendering = encoder
                .begin_rendering(&descriptor)
                .map_err(|error| format!("begin shared effect rendering: {error}"))?;
            self.record_draw_range(
                &mut rendering,
                frame_slot,
                SceneGpuDrawRange {
                    start: draw_start,
                    count: draw_count,
                },
                target.descriptor.extent,
            )?;
        }
        encoder
            .transition_image(
                &target.image,
                attachment_state,
                TextureState::FragmentSampledRead,
            )
            .map_err(|error| format!("transition shared effect target for sampling: {error}"))
    }

    fn record_effect_copy(
        &self,
        encoder: &mut CommandEncoder,
        scene_color: &Image,
        scene_extent: Extent2D,
        source: SharedSceneEffectCopySource,
        destination_physical_slot: u32,
        coverage: SceneColorCopyCoverage,
    ) -> Result<(), String> {
        let destination = self.effect_target(destination_physical_slot)?;
        let (source_image, source_extent, source_state) = match source {
            SharedSceneEffectCopySource::SceneColor => (
                scene_color,
                scene_extent,
                TextureState::ColorAttachmentWrite,
            ),
            SharedSceneEffectCopySource::PhysicalSlot(physical_slot) => {
                let source = self.effect_target(physical_slot)?;
                if physical_slot == destination_physical_slot {
                    return Err(format!(
                        "shared effect copy aliases physical slot {physical_slot}"
                    ));
                }
                (
                    &source.image,
                    source.descriptor.extent,
                    TextureState::FragmentSampledRead,
                )
            }
        };
        let Some(copy) = scene_color_copy(
            source_extent,
            destination.descriptor.extent,
            coverage,
            &self.draw_commands,
        )?
        else {
            return Ok(());
        };
        encoder
            .transition_image(source_image, source_state, TextureState::TransferSource)
            .map_err(|error| format!("transition shared effect copy source: {error}"))?;
        encoder
            .transition_image(
                &destination.image,
                TextureState::FragmentSampledRead,
                TextureState::TransferDestination,
            )
            .map_err(|error| format!("transition shared effect copy destination: {error}"))?;
        unsafe {
            encoder
                .copy_color_image_to_image(
                    source_image,
                    TextureLayout::TransferSource,
                    &destination.image,
                    TextureLayout::TransferDestination,
                    &[copy],
                )
                .map_err(|error| format!("record shared effect color copy: {error}"))?;
        }
        encoder
            .transition_image(source_image, TextureState::TransferSource, source_state)
            .map_err(|error| format!("restore shared effect copy source: {error}"))?;
        encoder
            .transition_image(
                &destination.image,
                TextureState::TransferDestination,
                TextureState::FragmentSampledRead,
            )
            .map_err(|error| format!("restore shared effect copy destination: {error}"))
    }

    fn record_local_read_pair(
        &self,
        encoder: &mut CommandEncoder,
        frame_slot: usize,
        scope_index: usize,
        producer: SharedSceneEffectCommand,
        consumer: SharedSceneEffectCommand,
    ) -> Result<(), String> {
        let (
            SharedSceneEffectCommandKind::DynamicRender {
                target_physical_slot: source_slot,
                draw_start: producer_start,
                draw_count: producer_count,
                load_op: producer_load,
                local_read: Some((producer_scope, SceneLocalReadScopePassRole::Producer)),
                ..
            },
            SharedSceneEffectCommandKind::DynamicRender {
                target_physical_slot: destination_slot,
                draw_start: consumer_start,
                draw_count: consumer_count,
                load_op: consumer_load,
                local_read: Some((consumer_scope, SceneLocalReadScopePassRole::Consumer)),
                ..
            },
        ) = (producer.kind, consumer.kind)
        else {
            return Err(format!(
                "shared local-read scope {scope_index} does not contain a producer/consumer pair"
            ));
        };
        if producer_scope != scope_index || consumer_scope != scope_index {
            return Err(format!(
                "shared local-read scope {scope_index} crosses compiled scope identities"
            ));
        }
        if source_slot == destination_slot {
            return Err(format!(
                "shared local-read scope {scope_index} aliases physical slot {source_slot}"
            ));
        }
        let scope = self
            .local_read_scopes
            .get(scope_index)
            .ok_or_else(|| format!("shared local-read scope {scope_index} is missing"))?;
        let mappings = self
            .local_read_mappings
            .get(scope_index)
            .ok_or_else(|| format!("shared local-read mappings {scope_index} are missing"))?;
        let source = self.effect_target(source_slot)?;
        let destination = self.effect_target(destination_slot)?;
        let extent = scope.extent();
        let formats = scope.color_attachment_formats();
        if (source.descriptor.extent, source.descriptor.format) != (extent, formats[0])
            || (destination.descriptor.extent, destination.descriptor.format)
                != (extent, formats[1])
        {
            return Err(format!(
                "shared local-read scope {scope_index} images differ from its retained plan"
            ));
        }
        for target in [source, destination] {
            encoder
                .transition_image(
                    &target.image,
                    TextureState::FragmentSampledRead,
                    TextureState::RenderingLocalRead,
                )
                .map_err(|error| {
                    format!("transition shared local-read scope {scope_index}: {error}")
                })?;
        }
        let attachments = [
            Some(color_attachment(
                &source.view,
                shared_load_op(producer_load),
                TextureLayout::RenderingLocalRead,
            )),
            Some(color_attachment(
                &destination.view,
                shared_load_op(consumer_load),
                TextureLayout::RenderingLocalRead,
            )),
        ];
        let descriptor =
            rendering_descriptor("tensor-wallpaper-scene-local-read", extent, &attachments);
        unsafe {
            let mut rendering = encoder
                .begin_rendering(&descriptor)
                .map_err(|error| format!("begin shared local-read scope {scope_index}: {error}"))?;
            rendering
                .set_local_read_mapping(&mappings.producer)
                .map_err(|error| format!("set shared local-read producer mapping: {error}"))?;
            self.record_draw_range(
                &mut rendering,
                frame_slot,
                SceneGpuDrawRange {
                    start: producer_start,
                    count: producer_count,
                },
                extent,
            )?;
            rendering
                .local_read_by_region_dependency()
                .map_err(|error| format!("record shared local-read dependency: {error}"))?;
            rendering
                .set_local_read_mapping(&mappings.consumer)
                .map_err(|error| format!("set shared local-read consumer mapping: {error}"))?;
            self.record_draw_range(
                &mut rendering,
                frame_slot,
                SceneGpuDrawRange {
                    start: consumer_start,
                    count: consumer_count,
                },
                extent,
            )?;
        }
        for target in [source, destination] {
            encoder
                .transition_image(
                    &target.image,
                    TextureState::RenderingLocalRead,
                    TextureState::FragmentSampledRead,
                )
                .map_err(|error| {
                    format!("finish shared local-read scope {scope_index}: {error}")
                })?;
        }
        Ok(())
    }
}

fn color_attachment<'a>(
    view: &'a vulkan_renderer::ImageView,
    load_op: LoadOp<[f32; 4]>,
    layout: TextureLayout,
) -> ColorAttachment<'a> {
    ColorAttachment {
        view: view.as_attachment(),
        layout,
        resolve_target: None,
        resolve_layout: TextureLayout::Undefined,
        resolve_mode: ResolveMode::None,
        load_op,
        store_op: StoreOp::Store,
    }
}

fn rendering_descriptor<'a>(
    label: &'a str,
    extent: Extent2D,
    attachments: &'a [Option<ColorAttachment<'a>>],
) -> RenderingDescriptor<'a> {
    RenderingDescriptor {
        label: Some(label),
        render_area: Rect2D::new(0, 0, extent.width, extent.height),
        layer_count: 1,
        view_mask: 0,
        color_attachments: attachments,
        depth_attachment: None,
        stencil_attachment: None,
        multisampled_render_to_single_sampled: None,
    }
}

fn shared_load_op(load_op: SharedSceneEffectLoadOp) -> LoadOp<[f32; 4]> {
    match load_op {
        SharedSceneEffectLoadOp::Load => LoadOp::Load,
        SharedSceneEffectLoadOp::Clear => LoadOp::Clear([0.0; 4]),
        SharedSceneEffectLoadOp::Discard => LoadOp::Discard,
    }
}

fn scene_color_copy(
    source_extent: Extent2D,
    destination_extent: Extent2D,
    coverage: SceneColorCopyCoverage,
    draws: &[super::super::draw_recording::SceneGpuDrawCommand],
) -> Result<Option<ColorImageCopy>, String> {
    let extent = Extent2D::new(
        source_extent.width.min(destination_extent.width),
        source_extent.height.min(destination_extent.height),
    );
    if extent.is_empty() {
        return Err("shared scene-color copy has an empty extent".into());
    }
    let SceneColorCopyCoverage::ConsumerDrawScissors {
        draw_start,
        draw_count,
    } = coverage
    else {
        return Ok(Some(color_copy(Origin2D::new(0, 0), extent)));
    };
    let start = draw_start as usize;
    let end = start
        .checked_add(draw_count as usize)
        .ok_or_else(|| "shared scene-color consumer range overflows".to_owned())?;
    let consumers = draws.get(start..end).ok_or_else(|| {
        format!(
            "shared scene-color consumer range {start}..{end} exceeds {} draws",
            draws.len()
        )
    })?;
    let mut bounds = None::<[u32; 4]>;
    for draw in consumers.iter().filter(|draw| draw.enabled) {
        let Some(scissor) = draw.scissor else {
            return Ok(Some(color_copy(Origin2D::new(0, 0), extent)));
        };
        let min_x = scissor.offset[0].max(0) as u32;
        let min_y = scissor.offset[1].max(0) as u32;
        let max_x = min_x.saturating_add(scissor.extent[0]).min(extent.width);
        let max_y = min_y.saturating_add(scissor.extent[1]).min(extent.height);
        if max_x <= min_x || max_y <= min_y {
            continue;
        }
        if let Some(current) = bounds.as_mut() {
            current[0] = current[0].min(min_x);
            current[1] = current[1].min(min_y);
            current[2] = current[2].max(max_x);
            current[3] = current[3].max(max_y);
        } else {
            bounds = Some([min_x, min_y, max_x, max_y]);
        }
    }
    Ok(bounds.map(|[min_x, min_y, max_x, max_y]| {
        color_copy(
            Origin2D::new(min_x as i32, min_y as i32),
            Extent2D::new(max_x - min_x, max_y - min_y),
        )
    }))
}

fn color_copy(origin: Origin2D, extent: Extent2D) -> ColorImageCopy {
    ColorImageCopy {
        source_mip_level: 0,
        source_base_array_layer: 0,
        source_origin: origin,
        destination_mip_level: 0,
        destination_base_array_layer: 0,
        destination_origin: origin,
        extent,
        layer_count: 1,
    }
}

fn retain_descriptor_resources(
    scene: &SharedSceneGpuResources,
    rendering: &mut RenderingEncoder<'_>,
    frame: &super::super::shared_resources::SharedSceneFrameResources,
) {
    rendering.retain_resource(&scene.cold.mesh.vertex);
    rendering.retain_resource(&scene.cold.mesh.index);
    rendering.retain_resource(&frame.transform);
    if let Some(buffer) = frame.material.as_ref() {
        rendering.retain_resource(buffer);
    }
    if let Some(buffer) = frame.skinning.as_ref() {
        rendering.retain_resource(buffer);
    }
    if let Some(buffer) = frame.scene_owned_uniform.as_ref() {
        rendering.retain_resource(buffer);
    }
    for texture in scene
        .cold
        .textures
        .white_fallback
        .iter()
        .chain(&scene.cold.textures.textures)
    {
        rendering.retain_resource(&texture.image);
        rendering.retain_resource(&texture.view);
    }
    for target in &scene.cold.effect_targets.targets {
        rendering.retain_resource(&target.image);
        rendering.retain_resource(&target.view);
    }
    if let Some(particles) = scene.cold.particles.as_ref() {
        rendering.retain_resource(&particles.state);
        rendering.retain_resource(&particles.indirect);
        rendering.retain_resource(&particles.frame_time);
    }
}
