use super::*;

pub(super) fn target_spec(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    allocation: SceneRenderingDeviceTargetAllocation,
    swapchain_format: vk::Format,
    swapchain_extent: vk::Extent2D,
) -> Result<SceneEffectTargetImagePlan, String> {
    let image_target =
        storage.document().image_targets.iter().find(|target| {
            target.name == allocation.target_name && target.role == allocation.target
        });
    let format = image_target
        .and_then(|target| storage.string(target.format))
        .map(|format| target_format(format, swapchain_format))
        .transpose()?
        .unwrap_or(swapchain_format);
    // When the plan already recorded a non-zero authored allocation (e.g.
    // image-local multipass from authored_source_extent), honor it. Overriding
    // with projected object_mesh_pixel_extent expands WE's texture-space domain
    // (e.g. 2318×1794) into screen AABB size (e.g. 2542×1968) and breaks the
    // multipass resource/state stream. Alpha scissor already uses allocation
    // extents for local coverage; keep CreateImage/BeginRendering consistent.
    let (width, height) = if allocation.width != 0 && allocation.height != 0 {
        (allocation.width, allocation.height)
    } else {
        image_target
            .map(|target| scaled_extent(swapchain_extent, target))
            .unwrap_or((
                swapchain_extent.width.max(1),
                swapchain_extent.height.max(1),
            ))
    };
    Ok(SceneEffectTargetImagePlan {
        physical_slot: allocation.physical_slot,
        graph_index: allocation.graph_index,
        target: allocation.target,
        target_name: allocation.target_name,
        format,
        width,
        height,
        batch_field_count: 1,
        batch_atlas_columns: 1,
        batch_atlas_rows: 1,
        persistent_across_frames: matches!(
            allocation.target,
            SceneRenderTargetKind::NamedFbo | SceneRenderTargetKind::FirstClassEffectTarget
        ),
        aliased_logical_target_count: 1,
        input_attachment_required: graph.sampled_bindings.iter().any(|binding| {
            binding.access == SceneRenderingDeviceImageAccess::InputAttachment
                && binding.logical_target()
                    == Some((
                        allocation.graph_index,
                        allocation.target,
                        allocation.target_name,
                    ))
        }),
    })
}

impl LogicalEffectTargetKey {
    pub(super) fn from_pass_target(pass: &SceneRenderingDevicePassNode) -> Option<Self> {
        Self::from_target(pass.graph_index, pass.target, pass.target_name)
    }

    pub(super) fn from_target(
        graph_index: u32,
        target: SceneRenderTargetKind,
        name: SceneStringId,
    ) -> Option<Self> {
        effect_target_kind_is_recordable(target).then_some(Self {
            graph_index,
            target,
            name,
        })
    }
}

pub(super) fn logical_target_references(
    allocations: &[SceneRenderingDeviceTargetAllocation],
) -> Vec<LogicalEffectTargetReference> {
    allocations
        .iter()
        .filter_map(|allocation| {
            LogicalEffectTargetKey::from_target(
                allocation.graph_index,
                allocation.target,
                allocation.target_name,
            )
            .map(|key| LogicalEffectTargetReference {
                key,
                physical_slot: allocation.physical_slot,
            })
        })
        .collect()
}

pub(super) fn command_source_key(
    storage: &SceneStorage,
    pass: &SceneRenderingDevicePassNode,
) -> Option<SceneEffectTargetCommandSource> {
    let start = pass.binding_start as usize;
    let end = start.saturating_add(pass.binding_count as usize);
    storage
        .document()
        .render_bindings
        .get(start..end)?
        .iter()
        .find_map(|binding| {
            if binding.target == SceneRenderTargetKind::SceneColor {
                Some(SceneEffectTargetCommandSource::SceneColor)
            } else {
                LogicalEffectTargetKey::from_target(pass.graph_index, binding.target, binding.name)
                    .map(SceneEffectTargetCommandSource::LogicalTarget)
            }
        })
}

pub(super) fn record_copy_command(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    scene_color_image: vk::Image,
    scene_color_extent: vk::Extent2D,
    command: SceneEffectTargetCommand,
    resources: &[SceneEffectTargetImageResource],
    references: &[LogicalEffectTargetReference],
    draw_commands: &[SceneGpuDrawCommand],
) -> Result<(), String> {
    let source = command
        .source
        .ok_or_else(|| "scene effect copy command has no source target binding".to_owned())?;
    let target = resource_for_key(resources, references, command.target)?;
    match source {
        SceneEffectTargetCommandSource::LogicalTarget(source_key) => {
            let source = resource_for_key(resources, references, source_key)?;
            record_effect_target_copy(device, command_buffer, source, target)
        }
        SceneEffectTargetCommandSource::SceneColor => record_scene_color_copy(
            device,
            command_buffer,
            scene_color_image,
            scene_color_extent,
            target,
            command.scene_color_copy_coverage,
            draw_commands,
        ),
    }
}

pub(super) fn record_effect_target_copy(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    source: &SceneEffectTargetImageResource,
    target: &SceneEffectTargetImageResource,
) -> Result<(), String> {
    if source.plan.physical_slot == target.plan.physical_slot {
        return Ok(());
    }

    record_effect_target_barrier(
        device,
        command_buffer,
        source.image.image,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        vk::PipelineStageFlags2::FRAGMENT_SHADER,
        vk::PipelineStageFlags2::ALL_TRANSFER,
        vk::AccessFlags2::SHADER_SAMPLED_READ,
        vk::AccessFlags2::TRANSFER_READ,
    );
    record_effect_target_barrier(
        device,
        command_buffer,
        target.image.image,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::PipelineStageFlags2::FRAGMENT_SHADER,
        vk::PipelineStageFlags2::ALL_TRANSFER,
        vk::AccessFlags2::SHADER_SAMPLED_READ,
        vk::AccessFlags2::TRANSFER_WRITE,
    );
    let copy_region = vk::ImageCopy::builder()
        .src_subresource(color_subresource_layers())
        .dst_subresource(color_subresource_layers())
        .extent(copy_extent(source, target))
        .build();
    unsafe {
        device.cmd_copy_image(
            command_buffer,
            source.image.image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            target.image.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[copy_region],
        );
    }
    record_effect_target_barrier(
        device,
        command_buffer,
        source.image.image,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        vk::PipelineStageFlags2::ALL_TRANSFER,
        vk::PipelineStageFlags2::FRAGMENT_SHADER,
        vk::AccessFlags2::TRANSFER_READ,
        vk::AccessFlags2::SHADER_SAMPLED_READ,
    );
    record_effect_target_barrier(
        device,
        command_buffer,
        target.image.image,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        vk::PipelineStageFlags2::ALL_TRANSFER,
        vk::PipelineStageFlags2::FRAGMENT_SHADER,
        vk::AccessFlags2::TRANSFER_WRITE,
        vk::AccessFlags2::SHADER_SAMPLED_READ,
    );
    Ok(())
}

pub(super) fn record_scene_color_copy(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    scene_color_image: vk::Image,
    scene_color_extent: vk::Extent2D,
    target: &SceneEffectTargetImageResource,
    coverage: SceneColorCopyCoverage,
    draw_commands: &[SceneGpuDrawCommand],
) -> Result<(), String> {
    let Some(copy_region) = scene_color_copy_region(
        scene_color_extent,
        vk::Extent2D {
            width: target.plan.width,
            height: target.plan.height,
        },
        coverage,
        draw_commands,
    )?
    else {
        return Ok(());
    };
    record_effect_target_barrier(
        device,
        command_buffer,
        scene_color_image,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        vk::PipelineStageFlags2::ALL_TRANSFER,
        vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        vk::AccessFlags2::TRANSFER_READ,
    );
    record_effect_target_barrier(
        device,
        command_buffer,
        target.image.image,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::PipelineStageFlags2::FRAGMENT_SHADER,
        vk::PipelineStageFlags2::ALL_TRANSFER,
        vk::AccessFlags2::SHADER_SAMPLED_READ,
        vk::AccessFlags2::TRANSFER_WRITE,
    );
    unsafe {
        device.cmd_copy_image(
            command_buffer,
            scene_color_image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            target.image.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[copy_region],
        );
    }
    record_effect_target_barrier(
        device,
        command_buffer,
        scene_color_image,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        vk::PipelineStageFlags2::ALL_TRANSFER,
        vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        vk::AccessFlags2::TRANSFER_READ,
        vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
    );
    record_effect_target_barrier(
        device,
        command_buffer,
        target.image.image,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        vk::PipelineStageFlags2::ALL_TRANSFER,
        vk::PipelineStageFlags2::FRAGMENT_SHADER,
        vk::AccessFlags2::TRANSFER_WRITE,
        vk::AccessFlags2::SHADER_SAMPLED_READ,
    );
    Ok(())
}

pub(super) fn swap_logical_references(
    command: SceneEffectTargetCommand,
    references: &mut [LogicalEffectTargetReference],
) -> Result<(), String> {
    let source_key = command
        .source
        .and_then(|source| match source {
            SceneEffectTargetCommandSource::LogicalTarget(key) => Some(key),
            SceneEffectTargetCommandSource::SceneColor => None,
        })
        .ok_or_else(|| "scene effect swap command has no logical source target".to_owned())?;
    let target_key = command.target;
    let source_index = reference_index(references, source_key)
        .ok_or_else(|| "scene effect swap command source target is not allocated".to_owned())?;
    let target_index = reference_index(references, target_key)
        .ok_or_else(|| "scene effect swap command target is not allocated".to_owned())?;
    references.swap(source_index, target_index);
    references[source_index].key = source_key;
    references[target_index].key = target_key;
    Ok(())
}

pub(super) fn resource_for_key<'resources>(
    resources: &'resources [SceneEffectTargetImageResource],
    references: &[LogicalEffectTargetReference],
    key: LogicalEffectTargetKey,
) -> Result<&'resources SceneEffectTargetImageResource, String> {
    let reference = references
        .iter()
        .find(|reference| reference.key == key)
        .ok_or_else(|| "scene effect target is not allocated in graph".to_owned())?;
    resources
        .iter()
        .find(|resource| resource.plan.physical_slot == reference.physical_slot)
        .ok_or_else(|| {
            format!(
                "scene effect target physical slot {} has no image",
                reference.physical_slot
            )
        })
}

pub(super) fn reference_index(
    references: &[LogicalEffectTargetReference],
    key: LogicalEffectTargetKey,
) -> Option<usize> {
    references.iter().position(|reference| reference.key == key)
}

pub(super) fn copy_extent(
    source: &SceneEffectTargetImageResource,
    target: &SceneEffectTargetImageResource,
) -> vk::Extent3D {
    vk::Extent3D {
        width: source.plan.width.min(target.plan.width).max(1),
        height: source.plan.height.min(target.plan.height).max(1),
        depth: 1,
    }
}

pub(super) fn color_subresource_layers() -> vk::ImageSubresourceLayers {
    vk::ImageSubresourceLayers::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .mip_level(0)
        .base_array_layer(0)
        .layer_count(1)
        .build()
}

pub(super) fn effect_target_kind_is_recordable(target: SceneRenderTargetKind) -> bool {
    matches!(
        target,
        SceneRenderTargetKind::ImageLocalMain
            | SceneRenderTargetKind::ImageLocalSub
            | SceneRenderTargetKind::NamedFbo
            | SceneRenderTargetKind::FirstClassEffectTarget
            | SceneRenderTargetKind::Temporary
    )
}

pub(super) fn target_format(
    format: &str,
    swapchain_format: vk::Format,
) -> Result<vk::Format, String> {
    match format {
        "r8" | "r8_unorm" => Ok(vk::Format::R8_UNORM),
        "r16f" | "r16_float" => Ok(vk::Format::R16_SFLOAT),
        "rg1616f" | "rg16f" | "rg16_float" => Ok(vk::Format::R16G16_SFLOAT),
        "rgba8" | "rgba8_unorm" | "rgba8888" | "rgba" => Ok(vk::Format::R8G8B8A8_UNORM),
        "rgba16f" | "rgba16_float" | "rgba16161616f" => Ok(vk::Format::R16G16B16A16_SFLOAT),
        "rgba_backbuffer" | "rgb_backbuffer" | "" => Ok(swapchain_format),
        _ => Err(format!(
            "scene effect target format {format:?} is not supported by the Vulkan format map"
        )),
    }
}

pub(super) fn scaled_extent(extent: vk::Extent2D, target: &SceneImageTargetRecord) -> (u32, u32) {
    (
        divided_axis(extent.width, target.width_divisor_milli),
        divided_axis(extent.height, target.height_divisor_milli),
    )
}

/// Scale one axis by `divisor_milli / 1000` with **integer floor** division.
///
/// Wallpaper Engine source targets store
/// `max(2, full_extent / scale_divisor)` (`source_target_factory_0x1400d2a20`),
/// where `/` is unsigned integer division. Gilder encodes the same scale as
/// milli (`scale * 1000`), so this must be `floor(value * 1000 / milli)`, not
/// `div_ceil`. Odd surfaces (e.g. 2199 with half-scale 2000) otherwise become
/// 1100 instead of WE's 1099.
pub(super) fn divided_axis(value: u32, divisor_milli: u32) -> u32 {
    let numerator = (value.max(1) as u64).saturating_mul(1000);
    let divisor = divisor_milli.max(1) as u64;
    ((numerator / divisor).min(u32::MAX as u64) as u32).max(2)
}

pub(super) fn record_effect_target_barrier(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    src_stage: vk::PipelineStageFlags2,
    dst_stage: vk::PipelineStageFlags2,
    src_access: vk::AccessFlags2,
    dst_access: vk::AccessFlags2,
) {
    record_effect_target_barrier_layers(
        device,
        command_buffer,
        image,
        old_layout,
        new_layout,
        src_stage,
        dst_stage,
        src_access,
        dst_access,
        1,
    );
}

pub(super) fn record_effect_target_barrier_layers(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    src_stage: vk::PipelineStageFlags2,
    dst_stage: vk::PipelineStageFlags2,
    src_access: vk::AccessFlags2,
    dst_access: vk::AccessFlags2,
    array_layers: u32,
) {
    let barrier = vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(src_stage)
        .src_access_mask(src_access)
        .dst_stage_mask(dst_stage)
        .dst_access_mask(dst_access)
        .old_layout(old_layout)
        .new_layout(new_layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(effect_target_subresource_range(0, array_layers))
        .build();
    let barriers = [barrier];
    let dependency = vk::DependencyInfo::builder()
        .image_memory_barriers(&barriers)
        .build();
    unsafe {
        device.cmd_pipeline_barrier2(command_buffer, &dependency);
    }
}

pub(super) fn effect_target_subresource_range(
    base_array_layer: u32,
    array_layers: u32,
) -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(base_array_layer)
        .layer_count(array_layers.max(1))
        .build()
}

pub(super) fn record_dynamic_rendering_pass(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    resource: &SceneEffectTargetImageResource,
    command: SceneEffectTargetCommand,
    load_op: vk::AttachmentLoadOp,
    record_draws: &mut impl FnMut(u32, u32, vk::Extent2D) -> Result<(), String>,
) -> Result<(), String> {
    let clear_value = vk::ClearValue {
        color: vk::ClearColorValue {
            float32: [0.0, 0.0, 0.0, 0.0],
        },
    };
    let attachment = vk::RenderingAttachmentInfo::builder()
        .image_view(resource.image.view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(load_op)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(clear_value)
        .build();
    let attachments = [attachment];
    let render_area = vk::Rect2D::builder()
        .offset(vk::Offset2D { x: 0, y: 0 })
        .extent(vk::Extent2D {
            width: resource.plan.width,
            height: resource.plan.height,
        })
        .build();
    let rendering = vk::RenderingInfo::builder()
        .render_area(render_area)
        .layer_count(1)
        .color_attachments(&attachments)
        .build();
    unsafe {
        device.cmd_begin_rendering(command_buffer, &rendering);
    }
    super::super::draw_recording::record_scene_draw_extent(
        device,
        command_buffer,
        vk::Extent2D {
            width: resource.plan.width,
            height: resource.plan.height,
        },
    );
    let draw_result = if command.mesh_draw_count == 0 {
        Ok(())
    } else {
        record_draws(
            command.mesh_draw_start,
            command.mesh_draw_count,
            vk::Extent2D {
                width: resource.plan.width,
                height: resource.plan.height,
            },
        )
    };
    unsafe {
        device.cmd_end_rendering(command_buffer);
    }
    draw_result
}
