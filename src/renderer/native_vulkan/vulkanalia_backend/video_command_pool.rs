#![allow(dead_code)]

use serde::Serialize;
use vulkanalia::prelude::v1_4::{Device, DeviceV1_0};
use vulkanalia::vk::{self, Handle, HasBuilder};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(super) struct NativeVulkanVulkanaliaDecodeCommandBufferSnapshot {
    pub queue_family_index: u32,
    pub command_pool_created: bool,
    pub command_buffer_allocated: bool,
    pub submit_fence_created: bool,
    pub transient_pool: bool,
    pub reset_command_buffer_enabled: bool,
    pub command_buffer_level: &'static str,
    pub submit_sync_model: &'static str,
}

pub(super) struct VulkanaliaDecodeCommandBuffer {
    pub(super) command_pool: vk::CommandPool,
    pub(super) command_buffer: vk::CommandBuffer,
    pub(super) submit_fence: vk::Fence,
    pub(super) snapshot: NativeVulkanVulkanaliaDecodeCommandBufferSnapshot,
}

pub(super) fn native_vulkan_vulkanalia_create_decode_command_buffer(
    device: &Device,
    queue_family_index: u32,
) -> Result<VulkanaliaDecodeCommandBuffer, String> {
    let pool_flags =
        vk::CommandPoolCreateFlags::TRANSIENT | vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER;
    let command_pool_info = vk::CommandPoolCreateInfo::builder()
        .flags(pool_flags)
        .queue_family_index(queue_family_index)
        .build();
    let command_pool = unsafe { device.create_command_pool(&command_pool_info, None) }
        .map_err(|err| format!("vkCreateCommandPool(vulkanalia decode): {err:?}"))?;
    let mut submit_fence = vk::Fence::null();

    let result = (|| -> Result<VulkanaliaDecodeCommandBuffer, String> {
        let command_buffer_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1)
            .build();
        let command_buffer = unsafe { device.allocate_command_buffers(&command_buffer_info) }
            .map_err(|err| format!("vkAllocateCommandBuffers(vulkanalia decode): {err:?}"))?
            .into_iter()
            .next()
            .ok_or_else(|| {
                "vkAllocateCommandBuffers(vulkanalia decode) returned none".to_owned()
            })?;
        let fence_info = vk::FenceCreateInfo::builder();
        submit_fence = unsafe { device.create_fence(&fence_info, None) }
            .map_err(|err| format!("vkCreateFence(vulkanalia decode submit): {err:?}"))?;

        Ok(VulkanaliaDecodeCommandBuffer {
            command_pool,
            command_buffer,
            submit_fence,
            snapshot: NativeVulkanVulkanaliaDecodeCommandBufferSnapshot {
                queue_family_index,
                command_pool_created: true,
                command_buffer_allocated: true,
                submit_fence_created: true,
                transient_pool: pool_flags.contains(vk::CommandPoolCreateFlags::TRANSIENT),
                reset_command_buffer_enabled: pool_flags
                    .contains(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                command_buffer_level: "primary",
                submit_sync_model: "queue_submit2 + submit fence wait/reset; no queue_wait_idle",
            },
        })
    })();

    if result.is_err() {
        unsafe {
            if submit_fence != vk::Fence::null() {
                device.destroy_fence(submit_fence, None);
            }
            device.destroy_command_pool(command_pool, None);
        }
    }

    result
}

pub(super) fn native_vulkan_vulkanalia_destroy_decode_command_buffer(
    device: &Device,
    command_buffer: VulkanaliaDecodeCommandBuffer,
) {
    unsafe {
        if command_buffer.submit_fence != vk::Fence::null() {
            device.destroy_fence(command_buffer.submit_fence, None);
        }
        device.free_command_buffers(
            command_buffer.command_pool,
            &[command_buffer.command_buffer],
        );
        device.destroy_command_pool(command_buffer.command_pool, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_buffer_snapshot_keeps_pool_policy_explicit() {
        let snapshot = NativeVulkanVulkanaliaDecodeCommandBufferSnapshot {
            queue_family_index: 3,
            command_pool_created: true,
            command_buffer_allocated: true,
            submit_fence_created: true,
            transient_pool: true,
            reset_command_buffer_enabled: true,
            command_buffer_level: "primary",
            submit_sync_model: "queue_submit2 + submit fence wait/reset; no queue_wait_idle",
        };

        assert_eq!(snapshot.queue_family_index, 3);
        assert!(snapshot.transient_pool);
        assert!(snapshot.reset_command_buffer_enabled);
        assert_eq!(snapshot.command_buffer_level, "primary");
        assert!(snapshot.submit_fence_created);
        assert!(snapshot.submit_sync_model.contains("no queue_wait_idle"));
    }
}
