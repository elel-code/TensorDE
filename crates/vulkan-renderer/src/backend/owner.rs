use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use vulkanalia::{Device, Entry, Instance, prelude::v1_4::*, vk};

use super::DeviceQueues;
use crate::capabilities::{DeviceProperties, Features, Limits};
use crate::{Error, FrameToken, Result};

const RETAINED_COMMAND_BUFFER_LIMIT: usize = 64;

pub(crate) struct InstanceOwner {
    pub(super) entry: Entry,
    pub(crate) instance: Instance,
    #[cfg(feature = "ffmpeg-vulkan-decode")]
    pub(crate) enabled_extensions: Vec<String>,
}

impl Drop for InstanceOwner {
    fn drop(&mut self) {
        unsafe { self.instance.destroy_instance(None) };
        let _ = &self.entry;
    }
}

pub(crate) struct DeviceOwner {
    pub(crate) device: Device,
    pub(super) instance: Arc<InstanceOwner>,
    pub(super) physical_device: vk::PhysicalDevice,
    pub(super) queues: DeviceQueues,
    #[cfg(feature = "ffmpeg-vulkan-decode")]
    pub(super) graphics_queue_family: u32,
    pub(crate) enabled_features: Features,
    pub(crate) properties: DeviceProperties,
    pub(crate) limits: Limits,
    #[cfg(feature = "ffmpeg-vulkan-decode")]
    pub(crate) enabled_device_extensions: Vec<String>,
    pub(crate) max_push_data_size: u64,
    pub(super) command_pool: vk::CommandPool,
    pub(super) timeline: vk::Semaphore,
    pub(super) next_timeline: AtomicU64,
    pub(super) completed_timeline: AtomicU64,
    pub(super) submit_lock: Mutex<()>,
    pub(super) command_pool_lock: Mutex<()>,
    pub(super) retained_command_buffers: Mutex<Vec<vk::CommandBuffer>>,
    pub(super) pending_command_buffers: Mutex<Vec<(u64, Vec<vk::CommandBuffer>)>>,
}

impl DeviceOwner {
    pub(crate) fn instance_owner(&self) -> &Arc<InstanceOwner> {
        &self.instance
    }

    pub(crate) fn physical_device(&self) -> vk::PhysicalDevice {
        self.physical_device
    }

    pub(crate) fn timeline(&self) -> vk::Semaphore {
        self.timeline
    }

    #[cfg(feature = "ffmpeg-vulkan-decode")]
    pub(crate) const fn graphics_queue_family(&self) -> u32 {
        self.graphics_queue_family
    }

    pub(super) fn allocate_frame(&self) -> Result<FrameToken> {
        self.next_timeline
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |next| {
                next.checked_add(1)
            })
            .map(FrameToken::from_value)
            .map_err(|_| Error::TimelineExhausted)
    }

    pub(super) fn retire_timeline(&self, completed: u64) {
        self.completed_timeline
            .fetch_max(completed, Ordering::AcqRel);
    }

    pub(crate) fn allocate_primary_command_buffer(&self) -> Result<vk::CommandBuffer> {
        let _pool_guard = self
            .command_pool_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(command_buffer) = self
            .retained_command_buffers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop()
        {
            if let Err(source) = unsafe {
                self.device
                    .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
            } {
                unsafe {
                    self.device
                        .free_command_buffers(self.command_pool, &[command_buffer]);
                }
                return Err(Error::vulkan("vkResetCommandBuffer", source));
            }
            return Ok(command_buffer);
        }
        let info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        unsafe { self.device.allocate_command_buffers(&info) }
            .map_err(|source| Error::vulkan("vkAllocateCommandBuffers", source))?
            .into_iter()
            .next()
            .ok_or_else(|| Error::Validation("Vulkan returned no command buffer".into()))
    }

    pub(crate) fn free_command_buffers(&self, command_buffers: &[vk::CommandBuffer]) {
        if command_buffers.is_empty() {
            return;
        }
        let _pool_guard = self
            .command_pool_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let overflow = {
            let mut retained = self
                .retained_command_buffers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let keep = RETAINED_COMMAND_BUFFER_LIMIT
                .saturating_sub(retained.len())
                .min(command_buffers.len());
            retained.extend_from_slice(&command_buffers[..keep]);
            command_buffers[keep..].to_vec()
        };
        if !overflow.is_empty() {
            unsafe {
                self.device
                    .free_command_buffers(self.command_pool, &overflow)
            };
        }
    }

    pub(super) fn retire_command_buffers_after(
        &self,
        frame: FrameToken,
        command_buffers: Vec<vk::CommandBuffer>,
    ) {
        if command_buffers.is_empty() {
            return;
        }
        self.pending_command_buffers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((frame.value(), command_buffers));
        // Completion can race installation of this retirement entry.
        let completed = self.completed_timeline.load(Ordering::Acquire);
        if completed >= frame.value() {
            self.retire_completed_command_buffers(completed);
        }
    }

    pub(super) fn retire_completed_command_buffers(&self, completed: u64) {
        let retired = {
            let mut pending = self
                .pending_command_buffers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut retired = Vec::new();
            let mut still_pending = Vec::with_capacity(pending.len());
            for (timeline, command_buffers) in pending.drain(..) {
                if timeline <= completed {
                    retired.extend(command_buffers);
                } else {
                    still_pending.push((timeline, command_buffers));
                }
            }
            *pending = still_pending;
            retired
        };
        self.free_command_buffers(&retired);
    }
}

impl Drop for DeviceOwner {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_semaphore(self.timeline, None);
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
        }
        let _ = &self.instance;
    }
}
