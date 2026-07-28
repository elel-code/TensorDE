use std::fmt;
use std::sync::Arc;

use vulkanalia::{
    prelude::v1_4::*,
    vk::{self, ExtDescriptorHeapExtensionDeviceCommands},
};

use crate::backend::DeviceOwner;
use crate::{
    BarrierBatch, DescriptorHeap, DescriptorHeapKind, Error, Result, SubmissionLease,
    SubmissionResource,
};

mod compute;
mod rendering;
mod transfer;

pub use compute::{ComputeEncoder, ComputePassDescriptor};
pub use rendering::{
    AttachmentView, ColorAttachment, DepthAttachment, IndexFormat, LoadOp, RenderingDescriptor,
    RenderingEncoder, StencilAttachment, StoreOp,
};
pub use transfer::{BufferCopy, BufferImageCopy, ImageCopy};

/// Describes one primary command encoder.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommandEncoderDescriptor {
    /// Diagnostic label retained for object inspection.
    pub label: Option<String>,
}

/// A recording primary command buffer.
///
/// `finish` consumes the encoder, so a command buffer cannot be ended twice or
/// recorded after it has entered the executable state.
pub struct CommandEncoder {
    owner: Arc<DeviceOwner>,
    handle: Option<vk::CommandBuffer>,
    label: Option<String>,
    submission_leases: Vec<SubmissionLease>,
}

impl fmt::Debug for CommandEncoder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CommandEncoder")
            .field("label", &self.label)
            .field("recording", &self.handle.is_some())
            .field("submission_leases", &self.submission_leases.len())
            .finish_non_exhaustive()
    }
}

impl CommandEncoder {
    pub(crate) fn new(
        owner: Arc<DeviceOwner>,
        descriptor: &CommandEncoderDescriptor,
    ) -> Result<Self> {
        let handle = owner.allocate_primary_command_buffer()?;
        let begin = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        if let Err(source) = unsafe { owner.device.begin_command_buffer(handle, &begin) } {
            owner.free_command_buffers(&[handle]);
            return Err(Error::vulkan("vkBeginCommandBuffer", source));
        }
        Ok(Self {
            owner,
            handle: Some(handle),
            label: descriptor.label.clone(),
            submission_leases: Vec::new(),
        })
    }

    /// Raw handle for Vulkan commands recorded by higher-level encoders.
    pub fn raw(&self) -> vk::CommandBuffer {
        self.handle
            .expect("a live CommandEncoder always owns a command buffer")
    }

    /// Retains arbitrary shared ownership through this command buffer's
    /// eventual submission timeline.
    ///
    /// Dropping an unfinished or unsubmitted command buffer releases the
    /// value immediately. A successful managed queue submission transfers it
    /// to queue retirement automatically.
    pub fn retain<T>(&mut self, value: Arc<T>)
    where
        T: std::any::Any + Send + Sync,
    {
        self.retain_lease(SubmissionLease::new(value));
    }

    /// Attaches an already type-erased submission lease to this command
    /// buffer.
    pub fn retain_lease(&mut self, lease: SubmissionLease) {
        self.submission_leases.push(lease);
    }

    /// Retains a renderer resource through this command buffer's eventual
    /// submission using its standardized ownership token.
    pub fn retain_resource<R>(&mut self, resource: &R)
    where
        R: SubmissionResource + ?Sized,
    {
        self.retain_lease(resource.submission_lease());
    }

    /// Copies a small descriptor-heap shader payload into command-buffer
    /// state using `vkCmdPushDataEXT`.
    pub fn push_data(&mut self, offset: u32, data: &[u8]) -> Result<()> {
        validate_push_data(offset, data.len(), self.owner.max_push_data_size)?;
        let range = vk::HostAddressRangeConstEXT::builder().address(data);
        let info = vk::PushDataInfoEXT::builder().offset(offset).data(range);
        unsafe { self.owner.device.cmd_push_data_ext(self.raw(), &info) };
        Ok(())
    }

    /// Records a compiled render-graph barrier batch.
    ///
    /// # Safety
    ///
    /// Every raw resource in `barriers` must be live, compatible with this
    /// device, and in the graph-declared state and queue family.
    pub unsafe fn pipeline_barrier(&mut self, barriers: &BarrierBatch) {
        unsafe {
            barriers.record(&self.owner.device, self.raw());
        }
    }

    /// Binds a resource or sampler descriptor heap for subsequent commands.
    ///
    /// # Safety
    ///
    /// `heap` and every resource referenced by its descriptors must remain live
    /// and unmodified until this command buffer's submission completes.
    pub unsafe fn bind_descriptor_heap(&mut self, heap: &DescriptorHeap) -> Result<()> {
        if !heap.belongs_to(&self.owner) {
            return Err(Error::Validation(
                "descriptor heap was created by a different Device".into(),
            ));
        }
        let binding = heap.bind_info();
        unsafe {
            match heap.kind() {
                DescriptorHeapKind::Resource => self
                    .owner
                    .device
                    .cmd_bind_resource_heap_ext(self.raw(), &binding),
                DescriptorHeapKind::Sampler => self
                    .owner
                    .device
                    .cmd_bind_sampler_heap_ext(self.raw(), &binding),
            }
        }
        Ok(())
    }

    /// Ends recording and returns an executable, single-submission command
    /// buffer. If ending fails, the Vulkan command buffer is freed on drop.
    pub fn finish(mut self) -> Result<CommandBuffer> {
        let handle = self.raw();
        unsafe { self.owner.device.end_command_buffer(handle) }
            .map_err(|source| Error::vulkan("vkEndCommandBuffer", source))?;
        self.handle = None;
        Ok(CommandBuffer {
            owner: Arc::clone(&self.owner),
            handle: Some(handle),
            label: self.label.take(),
            submission_leases: std::mem::take(&mut self.submission_leases),
        })
    }
}

fn validate_push_data(offset: u32, size: usize, maximum: u64) -> Result<()> {
    if size == 0 {
        return Err(Error::Validation("push data must be non-empty".into()));
    }
    let size =
        u64::try_from(size).map_err(|_| Error::Validation("push data size exceeds u64".into()))?;
    if !offset.is_multiple_of(4) || !size.is_multiple_of(4) {
        return Err(Error::Validation(
            "push data offset and size must be multiples of four".into(),
        ));
    }
    if u64::from(offset)
        .checked_add(size)
        .is_none_or(|end| end > maximum)
    {
        return Err(Error::Validation(format!(
            "push data range exceeds maxPushDataSize {maximum}"
        )));
    }
    Ok(())
}

impl Drop for CommandEncoder {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            self.owner.free_command_buffers(&[handle]);
        }
    }
}

/// An executable primary command buffer with exclusive submission ownership.
///
/// Dropping an unsubmitted buffer frees it immediately. Successful queue
/// submission consumes the handle and retires it only after the device
/// timeline reaches that submission.
pub struct CommandBuffer {
    owner: Arc<DeviceOwner>,
    handle: Option<vk::CommandBuffer>,
    label: Option<String>,
    submission_leases: Vec<SubmissionLease>,
}

impl fmt::Debug for CommandBuffer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CommandBuffer")
            .field("label", &self.label)
            .field("submitted", &self.handle.is_none())
            .field("submission_leases", &self.submission_leases.len())
            .finish_non_exhaustive()
    }
}

impl CommandBuffer {
    pub fn raw(&self) -> vk::CommandBuffer {
        self.handle
            .expect("an unconsumed CommandBuffer always owns a Vulkan handle")
    }

    pub(crate) fn belongs_to(&self, owner: &Arc<DeviceOwner>) -> bool {
        Arc::ptr_eq(&self.owner, owner)
    }

    pub(crate) fn take_for_submission(&mut self) -> Vec<SubmissionLease> {
        self.handle
            .take()
            .expect("Queue submission consumes each CommandBuffer only once");
        std::mem::take(&mut self.submission_leases)
    }
}

impl Drop for CommandBuffer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            self.owner.free_command_buffers(&[handle]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::validate_push_data;

    #[test]
    fn push_data_range_obeys_alignment_and_device_limit() {
        assert!(validate_push_data(0, 16, 128).is_ok());
        assert!(validate_push_data(2, 16, 128).is_err());
        assert!(validate_push_data(0, 6, 128).is_err());
        assert!(validate_push_data(120, 12, 128).is_err());
    }
}
