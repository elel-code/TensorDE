
fn native_vulkan_vulkanalia_submit_decoded_image_present_command_buffer2(
    device: &Device,
    queue: vk::Queue,
    command_buffer: vk::CommandBuffer,
    image_available: vk::Semaphore,
    render_finished: vk::Semaphore,
    fence: vk::Fence,
    decode_waits: &[VulkanaliaDecodedImagePresentDecodeWait],
) -> Result<(), String> {
    // Wait for the swapchain image at color output. FFmpeg mirrors AVVkFrame
    // semaphore values as frame dependencies (references/gilder/ffmpeg/libavcodec/
    // vulkan_decode.c:575-586); the decode submit signals at video-decode
    // completion, while this present submit waits before any graphics command
    // mutates the decoded image layout.
    let image_available_wait = vk::SemaphoreSubmitInfo::builder()
        .semaphore(image_available)
        .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .build();
    let mut wait_infos = Vec::with_capacity(decode_waits.len().saturating_add(1));
    wait_infos.push(image_available_wait);
    wait_infos.extend(
        decode_waits
            .iter()
            .filter(|wait| wait.semaphore != vk::Semaphore::null())
            .map(|wait| {
                vk::SemaphoreSubmitInfo::builder()
                    .semaphore(wait.semaphore)
                    .value(wait.value)
                    .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                    .build()
            }),
    );
    let command_buffer_info = vk::CommandBufferSubmitInfo::builder()
        .command_buffer(command_buffer)
        .build();
    let command_buffer_infos = [command_buffer_info];
    let signal = vk::SemaphoreSubmitInfo::builder()
        .semaphore(render_finished)
        .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .build();
    let signals = [signal];
    let mut submit_builder = vk::SubmitInfo2::builder()
        .command_buffer_infos(&command_buffer_infos)
        .signal_semaphore_infos(&signals);
    submit_builder = submit_builder.wait_semaphore_infos(&wait_infos);
    let submit_info = submit_builder.build();

    unsafe {
        device
            .queue_submit2(queue, &[submit_info], fence)
            .map_err(|err| format!("vkQueueSubmit2(vulkanalia decoded image present): {err:?}"))?;
    }

    Ok(())
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_decoded_image_present_command_order(
    same_queue_family: bool,
    present_id_mode: &'static str,
    present_wait_mode: &'static str,
    scene_video_layer_draw_enabled: bool,
    scene_overlay_draw_enabled: bool,
) -> Vec<&'static str> {
    let fullscreen_bind_steps = [
        "cmd_bind_resource_heap_ext",
        "cmd_bind_sampler_heap_ext",
        "cmd_push_data_ext",
        "draw_with_native_descriptor_heap_plane_array_sampler",
    ];
    let video_layer_bind_steps = [
        "cmd_bind_resource_heap_ext",
        "cmd_bind_sampler_heap_ext",
        "cmd_bind_scene_video_layer_pipeline",
        "cmd_draw_scene_video_layers_inside_video_rendering",
    ];
    let mut order = if same_queue_family {
        let mut order = vec![
            "queue_submit2_decode",
            "cmd_pipeline_barrier2_shader_read",
            "cmd_begin_rendering",
        ];
        if scene_video_layer_draw_enabled {
            order.extend(video_layer_bind_steps);
        } else {
            order.extend(fullscreen_bind_steps);
        }
        if scene_overlay_draw_enabled {
            order.extend([
                "cmd_bind_scene_overlay_pipeline",
                "cmd_draw_scene_overlay_inside_video_rendering",
            ]);
        }
        order.extend([
            "cmd_end_rendering",
            "cmd_pipeline_barrier2_decoded_restore",
            "cmd_pipeline_barrier2_present",
            "queue_submit2_present",
            "defer_frame_slot_reuse_until_render_fence",
            "queue_present_khr",
            "no_queue_wait_idle_after_present",
        ]);
        order
    } else {
        let mut order = vec![
            "queue_submit2_decode",
            "cmd_pipeline_barrier2_video_release",
            "cmd_pipeline_barrier2_graphics_acquire_shader_read",
            "cmd_begin_rendering",
        ];
        if scene_video_layer_draw_enabled {
            order.extend(video_layer_bind_steps);
        } else {
            order.extend(fullscreen_bind_steps);
        }
        if scene_overlay_draw_enabled {
            order.extend([
                "cmd_bind_scene_overlay_pipeline",
                "cmd_draw_scene_overlay_inside_video_rendering",
            ]);
        }
        order.extend([
            "cmd_end_rendering",
            "cmd_pipeline_barrier2_decoded_restore",
            "cmd_pipeline_barrier2_present",
            "queue_submit2_present",
            "defer_frame_slot_reuse_until_render_fence",
            "queue_present_khr",
            "no_queue_wait_idle_after_present",
        ]);
        order
    };
    match present_id_mode {
        "present-id2-khr" => order.insert(order.len().saturating_sub(2), "present_id2_khr"),
        _ => {}
    }
    match present_wait_mode {
        "present-wait2-khr" => order.insert(order.len().saturating_sub(1), "wait_for_present2_khr"),
        _ => {}
    }
    order
}

fn native_vulkan_vulkanalia_create_shader_module(
    device: &Device,
    code: &[u32],
    label: &'static str,
) -> Result<vk::ShaderModule, String> {
    if code.first().copied() != Some(0x0723_0203) {
        return Err(format!(
            "decoded present {label} shader is not valid SPIR-V bytecode"
        ));
    }
    let create_info = vk::ShaderModuleCreateInfo::builder()
        .code(code)
        .code_size(native_vulkan_vulkanalia_shader_code_size_bytes(code));
    unsafe { device.create_shader_module(&create_info, None) }
        .map_err(|err| format!("vkCreateShaderModule(vulkanalia {label}): {err:?}"))
}

fn native_vulkan_vulkanalia_shader_code_size_bytes(code: &[u32]) -> usize {
    std::mem::size_of_val(code)
}

fn native_vulkan_vulkanalia_color_subresource_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
        .build()
}

fn native_vulkan_vulkanalia_decoded_image_layer_subresource_range(
    sampled_array_layer: u32,
) -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(sampled_array_layer)
        .layer_count(1)
        .build()
}

fn native_vulkan_vulkanalia_decoded_image_present_queue_family_barrier_indices(
    source: VulkanaliaDecodedImagePresentImageSource,
    present_queue_family_index: u32,
) -> Result<(u32, u32), String> {
    if source.queue_family_index == vk::QUEUE_FAMILY_IGNORED {
        return Ok((vk::QUEUE_FAMILY_IGNORED, vk::QUEUE_FAMILY_IGNORED));
    }
    if source.queue_family_index == present_queue_family_index {
        return Ok((vk::QUEUE_FAMILY_IGNORED, vk::QUEUE_FAMILY_IGNORED));
    }
    Err(format!(
        "decoded image present source queue family {} differs from present queue family {}; FFmpeg AVVkFrame split-family handoff requires FFmpeg-created concurrent images or an explicit video-queue release",
        source.queue_family_index, present_queue_family_index
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecodedImageDescriptorKind {
    SampledImage,
    Sampler,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DecodedImageDescriptorBinding {
    kind: DecodedImageDescriptorKind,
    register: u32,
    push_offset: u32,
}

include!(concat!(env!("OUT_DIR"), "/gilder_video_present_shaders.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    const OP_CAPABILITY: u16 = 17;
    const OP_DECORATE: u16 = 71;
    const CAPABILITY_DESCRIPTOR_HEAP_EXT: u32 = 5_128;
    const DECORATION_BUILT_IN: u32 = 11;
    const DECORATION_BINDING: u32 = 33;
    const DECORATION_DESCRIPTOR_SET: u32 = 34;
    const BUILT_IN_RESOURCE_HEAP_EXT: u32 = 5_123;
    const BUILT_IN_SAMPLER_HEAP_EXT: u32 = 5_122;

    fn instructions(words: &[u32]) -> Vec<&[u32]> {
        let mut result = Vec::new();
        let mut cursor = 5;
        while cursor < words.len() {
            let count = (words[cursor] >> 16) as usize;
            assert!(count > 0 && cursor + count <= words.len());
            result.push(&words[cursor..cursor + count]);
            cursor += count;
        }
        result
    }

    fn assert_native_image_sampler_heap(words: &[u32]) {
        let instructions = instructions(words);
        assert!(instructions.iter().any(|instruction| {
            (instruction[0] & 0xffff) as u16 == OP_CAPABILITY
                && instruction.get(1) == Some(&CAPABILITY_DESCRIPTOR_HEAP_EXT)
        }));
        for built_in in [BUILT_IN_RESOURCE_HEAP_EXT, BUILT_IN_SAMPLER_HEAP_EXT] {
            assert!(instructions.iter().any(|instruction| {
                (instruction[0] & 0xffff) as u16 == OP_DECORATE
                    && instruction.get(2) == Some(&DECORATION_BUILT_IN)
                    && instruction.get(3) == Some(&built_in)
            }));
        }
        assert!(!instructions.iter().any(|instruction| {
            (instruction[0] & 0xffff) as u16 == OP_DECORATE
                && matches!(
                    instruction.get(2),
                    Some(&DECORATION_BINDING) | Some(&DECORATION_DESCRIPTOR_SET)
                )
        }));
    }

    #[test]
    fn decoded_image_present_order_keeps_queue_ownership_explicit() {
        let split = native_vulkan_vulkanalia_decoded_image_present_command_order(
            false, "disabled", "disabled", false, false,
        );
        assert!(split.contains(&"cmd_pipeline_barrier2_video_release"));
        assert!(split.contains(&"cmd_pipeline_barrier2_graphics_acquire_shader_read"));
        assert!(split.contains(&"cmd_begin_rendering"));
        assert!(split.contains(&"cmd_bind_resource_heap_ext"));
        assert!(split.contains(&"cmd_bind_sampler_heap_ext"));
        assert!(split.contains(&"cmd_push_data_ext"));
        assert!(split.contains(&"draw_with_native_descriptor_heap_plane_array_sampler"));
        assert!(split.contains(&"cmd_pipeline_barrier2_decoded_restore"));
        assert!(split.contains(&"queue_submit2_present"));
        assert!(split.contains(&"defer_frame_slot_reuse_until_render_fence"));
        assert!(split.contains(&"no_queue_wait_idle_after_present"));

        let same = native_vulkan_vulkanalia_decoded_image_present_command_order(
            true, "disabled", "disabled", false, false,
        );
        assert!(!same.contains(&"cmd_pipeline_barrier2_video_release"));
        assert!(same.contains(&"cmd_bind_resource_heap_ext"));
        assert!(same.contains(&"cmd_bind_sampler_heap_ext"));
        assert!(same.contains(&"draw_with_native_descriptor_heap_plane_array_sampler"));
        assert!(same.contains(&"defer_frame_slot_reuse_until_render_fence"));

        let video_layer = native_vulkan_vulkanalia_decoded_image_present_command_order(
            true, "disabled", "disabled", true, false,
        );
        assert!(video_layer.contains(&"cmd_bind_scene_video_layer_pipeline"));
        assert!(video_layer.contains(&"cmd_draw_scene_video_layers_inside_video_rendering"));
        assert!(!video_layer.contains(&"draw_with_native_descriptor_heap_plane_array_sampler"));

        let present_id2 = native_vulkan_vulkanalia_decoded_image_present_command_order(
            true,
            "present-id2-khr",
            "present-wait2-khr",
            false,
            false,
        );
        assert!(present_id2.contains(&"present_id2_khr"));
        assert!(present_id2.contains(&"wait_for_present2_khr"));
        assert!(present_id2.windows(3).any(|triple| triple
            == [
                "present_id2_khr",
                "queue_present_khr",
                "wait_for_present2_khr"
            ]));
    }

    #[test]
    fn shader_module_code_size_uses_bytes_not_words() {
        assert_eq!(
            native_vulkan_vulkanalia_shader_code_size_bytes(
                DECODED_PRESENT_VERTEX_SPIRV
            ),
            DECODED_PRESENT_VERTEX_SPIRV.len() * 4
        );
        assert_eq!(DECODED_SCENE_VIDEO_VERTEX_SPIRV[0], 0x07230203);
        assert_eq!(DECODED_SCENE_VIDEO_FRAGMENT_SPIRV[0], 0x07230203);
        assert_eq!(
            native_vulkan_vulkanalia_shader_code_size_bytes(DECODED_SCENE_VIDEO_VERTEX_SPIRV),
            DECODED_SCENE_VIDEO_VERTEX_SPIRV.len() * 4
        );
        assert_eq!(
            native_vulkan_vulkanalia_shader_code_size_bytes(DECODED_SCENE_VIDEO_FRAGMENT_SPIRV),
            DECODED_SCENE_VIDEO_FRAGMENT_SPIRV.len() * 4
        );
        assert_eq!(DECODED_PRESENT_PUSH_BYTES, 16);
        assert_eq!(DECODED_SCENE_VIDEO_PUSH_BYTES, 24);
        assert_eq!(DECODED_PRESENT_BINDINGS.len(), 4);
        assert_eq!(DECODED_SCENE_VIDEO_BINDINGS.len(), 4);
        assert_native_image_sampler_heap(DECODED_PRESENT_FRAGMENT_SPIRV);
        assert_native_image_sampler_heap(DECODED_SCENE_VIDEO_FRAGMENT_SPIRV);
    }
}
