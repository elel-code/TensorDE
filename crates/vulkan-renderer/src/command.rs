use std::fmt;
use std::sync::Arc;

use vulkanalia::{
    prelude::v1_4::*,
    vk::{self, ExtDescriptorHeapExtensionDeviceCommands},
};

use crate::backend::DeviceOwner;
use crate::{
    BarrierBatch, Buffer, BufferUsages, DescriptorHeap, DescriptorHeapKind, Error, Image, Result,
    SubmissionLease, SubmissionResource, TextureLayout, TextureUsages, TimestampQuery,
    TimestampQuerySet, TimestampWriteStage,
};

mod compute;
mod rendering;
mod transfer;

pub use compute::{ComputeEncoder, ComputePassDescriptor};
pub use rendering::{
    AttachmentView, ColorAttachment, DepthAttachment, IndexFormat, LoadOp, RenderingDescriptor,
    RenderingEncoder, RenderingLocalReadMapping, RenderingLocalReadMappingDescriptor,
    RenderingLocalReadMappingKind, ResolveMode, StencilAttachment, StoreOp,
};
pub use transfer::{
    BufferCopy, BufferImageCopy, ColorBufferImageCopy, ColorImageBufferCopy, ColorImageCopy,
    ImageBlit, ImageBlitFilter, ImageCopy,
};

/// Describes one primary command encoder.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommandEncoderDescriptor {
    /// Shared diagnostic label retained for object inspection.
    pub label: Option<Arc<str>>,
}

/// A recording primary command buffer.
///
/// `finish` consumes the encoder, so a command buffer cannot be ended twice or
/// recorded after it has entered the executable state.
pub struct CommandEncoder {
    owner: Arc<DeviceOwner>,
    handle: Option<vk::CommandBuffer>,
    label: Option<Arc<str>>,
    submission_leases: Vec<SubmissionLease>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextureState {
    Undefined,
    ColorAttachmentWrite,
    ColorAttachmentReadWrite,
    RenderingLocalRead,
    FragmentSampledRead,
    ComputeSampledRead,
    TransferSource,
    TransferDestination,
    StorageReadWrite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BufferState {
    Undefined,
    TransferSource,
    TransferDestination,
    VertexRead,
    IndexRead,
    UniformRead,
    StorageReadWrite,
    ComputeStorageReadWrite,
    IndirectRead,
}

impl BufferState {
    pub(crate) fn synchronization(
        self,
    ) -> (vk::PipelineStageFlags2, vk::AccessFlags2, BufferUsages) {
        match self {
            Self::Undefined => (
                vk::PipelineStageFlags2::NONE,
                vk::AccessFlags2::NONE,
                BufferUsages::empty(),
            ),
            Self::TransferSource => (
                vk::PipelineStageFlags2::ALL_TRANSFER,
                vk::AccessFlags2::TRANSFER_READ,
                BufferUsages::COPY_SOURCE,
            ),
            Self::TransferDestination => (
                vk::PipelineStageFlags2::ALL_TRANSFER,
                vk::AccessFlags2::TRANSFER_WRITE,
                BufferUsages::COPY_DESTINATION,
            ),
            Self::VertexRead => (
                vk::PipelineStageFlags2::VERTEX_ATTRIBUTE_INPUT,
                vk::AccessFlags2::VERTEX_ATTRIBUTE_READ,
                BufferUsages::VERTEX,
            ),
            Self::IndexRead => (
                vk::PipelineStageFlags2::INDEX_INPUT,
                vk::AccessFlags2::INDEX_READ,
                BufferUsages::INDEX,
            ),
            Self::UniformRead => (
                vk::PipelineStageFlags2::ALL_COMMANDS,
                vk::AccessFlags2::UNIFORM_READ,
                BufferUsages::UNIFORM,
            ),
            Self::StorageReadWrite => (
                vk::PipelineStageFlags2::ALL_COMMANDS,
                vk::AccessFlags2::SHADER_STORAGE_READ | vk::AccessFlags2::SHADER_STORAGE_WRITE,
                BufferUsages::STORAGE,
            ),
            Self::ComputeStorageReadWrite => (
                vk::PipelineStageFlags2::COMPUTE_SHADER,
                vk::AccessFlags2::SHADER_STORAGE_READ | vk::AccessFlags2::SHADER_STORAGE_WRITE,
                BufferUsages::STORAGE,
            ),
            Self::IndirectRead => (
                vk::PipelineStageFlags2::DRAW_INDIRECT,
                vk::AccessFlags2::INDIRECT_COMMAND_READ,
                BufferUsages::INDIRECT,
            ),
        }
    }
}

impl TextureState {
    fn synchronization(
        self,
    ) -> (
        vk::PipelineStageFlags2,
        vk::AccessFlags2,
        TextureLayout,
        TextureUsages,
    ) {
        match self {
            Self::Undefined => (
                vk::PipelineStageFlags2::NONE,
                vk::AccessFlags2::NONE,
                TextureLayout::Undefined,
                TextureUsages::empty(),
            ),
            Self::ColorAttachmentWrite => (
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                TextureLayout::ColorAttachment,
                TextureUsages::COLOR_ATTACHMENT,
            ),
            Self::ColorAttachmentReadWrite => (
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                TextureLayout::ColorAttachment,
                TextureUsages::COLOR_ATTACHMENT,
            ),
            Self::RenderingLocalRead => (
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags2::FRAGMENT_SHADER,
                vk::AccessFlags2::COLOR_ATTACHMENT_READ
                    | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE
                    | vk::AccessFlags2::INPUT_ATTACHMENT_READ,
                TextureLayout::RenderingLocalRead,
                TextureUsages::COLOR_ATTACHMENT | TextureUsages::INPUT_ATTACHMENT,
            ),
            Self::FragmentSampledRead => (
                vk::PipelineStageFlags2::FRAGMENT_SHADER,
                vk::AccessFlags2::SHADER_SAMPLED_READ,
                TextureLayout::ShaderReadOnly,
                TextureUsages::SAMPLED,
            ),
            Self::ComputeSampledRead => (
                vk::PipelineStageFlags2::COMPUTE_SHADER,
                vk::AccessFlags2::SHADER_SAMPLED_READ,
                TextureLayout::ShaderReadOnly,
                TextureUsages::SAMPLED,
            ),
            Self::TransferSource => (
                vk::PipelineStageFlags2::ALL_TRANSFER,
                vk::AccessFlags2::TRANSFER_READ,
                TextureLayout::TransferSource,
                TextureUsages::COPY_SOURCE,
            ),
            Self::TransferDestination => (
                vk::PipelineStageFlags2::ALL_TRANSFER,
                vk::AccessFlags2::TRANSFER_WRITE,
                TextureLayout::TransferDestination,
                TextureUsages::COPY_DESTINATION,
            ),
            Self::StorageReadWrite => (
                vk::PipelineStageFlags2::ALL_COMMANDS,
                vk::AccessFlags2::SHADER_STORAGE_READ | vk::AccessFlags2::SHADER_STORAGE_WRITE,
                TextureLayout::General,
                TextureUsages::STORAGE,
            ),
        }
    }
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
    /// Resets a contiguous range before writing a new timestamp sample.
    pub fn reset_timestamp_queries(
        &mut self,
        queries: &TimestampQuerySet,
        first: TimestampQuery,
        count: u32,
    ) -> Result<()> {
        queries.validate_range(&self.owner, first, count)?;
        unsafe {
            self.owner
                .device
                .cmd_reset_query_pool(self.raw(), queries.raw(), first.index(), count);
        }
        self.retain_resource(queries);
        Ok(())
    }

    /// Writes one timestamp at an explicit graphics/compute pipeline boundary.
    pub fn write_timestamp(
        &mut self,
        queries: &TimestampQuerySet,
        query: TimestampQuery,
        stage: TimestampWriteStage,
    ) -> Result<()> {
        queries.validate_range(&self.owner, query, 1)?;
        unsafe {
            self.owner.device.cmd_write_timestamp2(
                self.raw(),
                stage.to_vk(),
                queries.raw(),
                query.index(),
            );
        }
        self.retain_resource(queries);
        Ok(())
    }

    #[cfg(feature = "ffmpeg-vulkan-decode")]
    pub(crate) fn owner(&self) -> &Arc<DeviceOwner> {
        &self.owner
    }

    #[cfg(feature = "ffmpeg-vulkan-decode")]
    pub(crate) unsafe fn external_image_barrier(&mut self, barrier: vk::ImageMemoryBarrier2) {
        unsafe {
            self.owner.device.cmd_pipeline_barrier2(
                self.raw(),
                &vk::DependencyInfo::builder()
                    .image_memory_barriers(&[barrier])
                    .build(),
            );
        }
    }

    /// Transitions the complete range of one renderer-owned buffer.
    pub fn transition_buffer(
        &mut self,
        buffer: &Buffer,
        old: BufferState,
        new: BufferState,
    ) -> Result<()> {
        if !buffer.belongs_to(&self.owner) {
            return Err(Error::Validation(
                "transition buffer was created by a different Device".into(),
            ));
        }
        let (src_stage, src_access, old_usage) = old.synchronization();
        let (dst_stage, dst_access, new_usage) = new.synchronization();
        if !buffer.usage().contains(old_usage | new_usage) {
            return Err(Error::Validation(
                "transition buffer is missing usage required by its states".into(),
            ));
        }
        let barrier = vk::BufferMemoryBarrier2::builder()
            .src_stage_mask(src_stage)
            .src_access_mask(src_access)
            .dst_stage_mask(dst_stage)
            .dst_access_mask(dst_access)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(buffer.raw())
            .offset(0)
            .size(buffer.size())
            .build();
        unsafe {
            self.owner.device.cmd_pipeline_barrier2(
                self.raw(),
                &vk::DependencyInfo::builder()
                    .buffer_memory_barriers(&[barrier])
                    .build(),
            );
        }
        self.retain_resource(buffer);
        Ok(())
    }

    /// Transitions every color subresource of one renderer-owned image.
    pub fn transition_image(
        &mut self,
        image: &Image,
        old: TextureState,
        new: TextureState,
    ) -> Result<()> {
        if !image.belongs_to(&self.owner) {
            return Err(Error::Validation(
                "transition image was created by a different Device".into(),
            ));
        }
        let (src_stage, src_access, old_layout, old_usage) = old.synchronization();
        let (dst_stage, dst_access, new_layout, new_usage) = new.synchronization();
        let required_usage = old_usage | new_usage;
        if !image.usage().contains(required_usage) {
            return Err(Error::Validation(
                "transition image is missing usage required by its states".into(),
            ));
        }
        let barrier = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(src_stage)
            .src_access_mask(src_access)
            .dst_stage_mask(dst_stage)
            .dst_access_mask(dst_access)
            .old_layout(old_layout.to_vk())
            .new_layout(new_layout.to_vk())
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image.raw())
            .subresource_range(
                image
                    .full_subresource_range(crate::TextureAspects::COLOR)
                    .to_vk(),
            )
            .build();
        unsafe {
            self.owner.device.cmd_pipeline_barrier2(
                self.raw(),
                &vk::DependencyInfo::builder()
                    .image_memory_barriers(&[barrier])
                    .build(),
            );
        }
        self.retain_resource(image);
        Ok(())
    }

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

    pub(crate) fn belongs_to(&self, owner: &Arc<DeviceOwner>) -> bool {
        Arc::ptr_eq(&self.owner, owner)
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

    /// Records one color-image layout transition used by an enclosing shared
    /// presentation transaction.
    ///
    /// # Safety
    ///
    /// `image` must be live on this device and currently have `old_layout`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) unsafe fn transition_color_image(
        &mut self,
        image: vk::Image,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
        source_stages: vk::PipelineStageFlags2,
        source_access: vk::AccessFlags2,
        destination_stages: vk::PipelineStageFlags2,
        destination_access: vk::AccessFlags2,
    ) {
        let barrier = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(source_stages)
            .src_access_mask(source_access)
            .dst_stage_mask(destination_stages)
            .dst_access_mask(destination_access)
            .old_layout(old_layout)
            .new_layout(new_layout)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        let barriers = [barrier.build()];
        let dependency = vk::DependencyInfo::builder().image_memory_barriers(&barriers);
        unsafe {
            self.owner
                .device
                .cmd_pipeline_barrier2(self.raw(), &dependency);
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
    label: Option<Arc<str>>,
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
    use super::{TextureState, validate_push_data};
    use crate::{TextureLayout, TextureUsages};
    use std::sync::Arc;
    use vulkanalia::vk;

    #[test]
    fn command_encoder_descriptor_clone_shares_label_storage() {
        let descriptor = super::CommandEncoderDescriptor {
            label: Some(Arc::from("retained-frame-label")),
        };
        let cloned = descriptor.clone();

        assert!(Arc::ptr_eq(
            descriptor.label.as_ref().unwrap(),
            cloned.label.as_ref().unwrap()
        ));
    }

    #[test]
    fn push_data_range_obeys_alignment_and_device_limit() {
        assert!(validate_push_data(0, 16, 128).is_ok());
        assert!(validate_push_data(2, 16, 128).is_err());
        assert!(validate_push_data(0, 6, 128).is_err());
        assert!(validate_push_data(120, 12, 128).is_err());
    }

    #[test]
    fn local_read_state_keeps_attachment_and_fragment_input_access() {
        let (stages, access, layout, usage) = TextureState::RenderingLocalRead.synchronization();
        assert_eq!(
            stages,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags2::FRAGMENT_SHADER
        );
        assert_eq!(
            access,
            vk::AccessFlags2::COLOR_ATTACHMENT_READ
                | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE
                | vk::AccessFlags2::INPUT_ATTACHMENT_READ
        );
        assert_eq!(layout, TextureLayout::RenderingLocalRead);
        assert_eq!(
            usage,
            TextureUsages::COLOR_ATTACHMENT | TextureUsages::INPUT_ATTACHMENT
        );
    }
}
