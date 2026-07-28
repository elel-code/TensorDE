//! Godot-style frame slots for overlapping CPU preparation with queued GPU work.

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

pub(super) const DEFAULT_SCENE_FRAME_SLOT_COUNT: usize = 1;
const MAX_SCENE_FRAME_SLOT_COUNT: usize = 3;

#[derive(Debug, Clone, Copy)]
pub(super) struct ScenePresentFrameContext {
    pub command_buffer: vk::CommandBuffer,
    pub image_available: vk::Semaphore,
    pub fence: vk::Fence,
}

pub(super) fn scene_frame_slot_count(gpu_timing_enabled: bool) -> usize {
    if gpu_timing_enabled {
        return 1;
    }
    std::env::var("GILDER_NATIVE_VULKAN_SCENE_FRAME_SLOT_COUNT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|count| (1..=MAX_SCENE_FRAME_SLOT_COUNT).contains(count))
        .unwrap_or(DEFAULT_SCENE_FRAME_SLOT_COUNT)
}

pub(super) fn create_scene_present_frame_contexts(
    device: &Device,
    command_buffers: &[vk::CommandBuffer],
) -> Result<Vec<ScenePresentFrameContext>, String> {
    let semaphore_info = vk::SemaphoreCreateInfo::builder();
    let fence_info = vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);
    let mut contexts = Vec::with_capacity(command_buffers.len());
    for (slot, command_buffer) in command_buffers.iter().copied().enumerate() {
        let image_available = match unsafe { device.create_semaphore(&semaphore_info, None) } {
            Ok(semaphore) => semaphore,
            Err(err) => {
                destroy_scene_present_frame_contexts(device, contexts);
                return Err(format!(
                    "vkCreateSemaphore(scene frame slot {slot} image available): {err:?}"
                ));
            }
        };
        let fence = match unsafe { device.create_fence(&fence_info, None) } {
            Ok(fence) => fence,
            Err(err) => {
                unsafe {
                    device.destroy_semaphore(image_available, None);
                }
                destroy_scene_present_frame_contexts(device, contexts);
                return Err(format!("vkCreateFence(scene frame slot {slot}): {err:?}"));
            }
        };
        contexts.push(ScenePresentFrameContext {
            command_buffer,
            image_available,
            fence,
        });
    }
    Ok(contexts)
}

pub(super) fn destroy_scene_present_frame_contexts(
    device: &Device,
    contexts: Vec<ScenePresentFrameContext>,
) {
    unsafe {
        for context in contexts {
            device.destroy_fence(context.fence, None);
            device.destroy_semaphore(context.image_available, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_pool_gpu_timing_forces_one_frame_slot() {
        assert_eq!(scene_frame_slot_count(true), 1);
    }
}
