use vulkanalia::vk::{DeviceV1_0, DeviceV1_2, DeviceV1_3, Handle, HasBuilder};
use vulkanalia::{Device, vk};

const COMMAND_BUFFER_COUNT: usize = 3;

pub(super) struct VulkanFrameExecutor {
    command_pool: vk::CommandPool,
    command_buffers: [vk::CommandBuffer; COMMAND_BUFFER_COUNT],
    retire_values: [u64; COMMAND_BUFFER_COUNT],
    timeline: vk::Semaphore,
}

impl VulkanFrameExecutor {
    pub(super) fn new(device: &Device, graphics_queue_family: u32) -> Result<Self, vk::ErrorCode> {
        let pool_info = vk::CommandPoolCreateInfo::builder()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(graphics_queue_family);
        let command_pool = unsafe { device.create_command_pool(&pool_info, None) }?;
        let allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(COMMAND_BUFFER_COUNT as u32);
        let command_buffers = match unsafe { device.allocate_command_buffers(&allocate_info) } {
            Ok(buffers) if buffers.len() == COMMAND_BUFFER_COUNT => {
                [buffers[0], buffers[1], buffers[2]]
            }
            Ok(buffers) => {
                unsafe {
                    device.free_command_buffers(command_pool, &buffers);
                    device.destroy_command_pool(command_pool, None);
                }
                return Err(vk::ErrorCode::INITIALIZATION_FAILED);
            }
            Err(error) => {
                unsafe { device.destroy_command_pool(command_pool, None) };
                return Err(error);
            }
        };
        let mut timeline_info = vk::SemaphoreTypeCreateInfo::builder()
            .semaphore_type(vk::SemaphoreType::TIMELINE)
            .initial_value(0)
            .build();
        let semaphore_info = vk::SemaphoreCreateInfo::builder().push_next(&mut timeline_info);
        let timeline = match unsafe { device.create_semaphore(&semaphore_info, None) } {
            Ok(semaphore) => semaphore,
            Err(error) => {
                unsafe {
                    device.free_command_buffers(command_pool, &command_buffers);
                    device.destroy_command_pool(command_pool, None);
                }
                return Err(error);
            }
        };
        Ok(Self {
            command_pool,
            command_buffers,
            retire_values: [0; COMMAND_BUFFER_COUNT],
            timeline,
        })
    }

    pub(super) fn completed(&self, device: &Device) -> Result<u64, vk::ErrorCode> {
        unsafe { device.get_semaphore_counter_value(self.timeline) }
    }

    pub(super) fn submit(
        &mut self,
        device: &Device,
        queue: vk::Queue,
        timeline_value: u64,
        completed_value: u64,
    ) -> Result<(), VulkanFrameError> {
        let Some((slot, command_buffer)) = self
            .command_buffers
            .iter()
            .enumerate()
            .find(|(slot, _)| self.retire_values[*slot] <= completed_value)
        else {
            return Err(VulkanFrameError::NoCommandBuffer);
        };
        let begin = vk::CommandBufferBeginInfo::builder();
        unsafe {
            device
                .reset_command_buffer(*command_buffer, vk::CommandBufferResetFlags::empty())
                .map_err(VulkanFrameError::Vulkan)?;
            device
                .begin_command_buffer(*command_buffer, &begin)
                .map_err(VulkanFrameError::Vulkan)?;
            device
                .end_command_buffer(*command_buffer)
                .map_err(VulkanFrameError::Vulkan)?;
        }

        let command_info = vk::CommandBufferSubmitInfo::builder()
            .command_buffer(*command_buffer)
            .build();
        let signal_info = vk::SemaphoreSubmitInfo::builder()
            .semaphore(self.timeline)
            .value(timeline_value)
            .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .build();
        let submit_info = vk::SubmitInfo2::builder()
            .command_buffer_infos(std::slice::from_ref(&command_info))
            .signal_semaphore_infos(std::slice::from_ref(&signal_info));
        unsafe {
            device
                .queue_submit2(queue, std::slice::from_ref(&submit_info), vk::Fence::null())
                .map_err(VulkanFrameError::Vulkan)?;
        }
        self.retire_values[slot] = timeline_value;
        Ok(())
    }

    pub(super) unsafe fn destroy(&mut self, device: &Device) {
        unsafe {
            device.destroy_semaphore(self.timeline, None);
            device.free_command_buffers(self.command_pool, &self.command_buffers);
            device.destroy_command_pool(self.command_pool, None);
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum VulkanFrameError {
    NoCommandBuffer,
    Vulkan(vk::ErrorCode),
}
