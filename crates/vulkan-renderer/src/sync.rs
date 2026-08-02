use std::collections::BTreeMap;
use std::fmt;

use vulkanalia::{Device, prelude::v1_4::*, vk};

use crate::{
    Buffer, CompiledGraph, Image, PassId, ResourceId, ResourceKind, ResourceState, TextureAspects,
};

mod external;
mod semaphore;

#[cfg(feature = "ffmpeg-vulkan-decode")]
pub(crate) use external::retain_external_timeline_semaphore_for_owner;
pub use external::{ExternalTimelineSemaphoreDescriptor, RetainedExternalTimelineSemaphore};
pub use semaphore::{BinarySemaphore, BinarySemaphoreDescriptor};

/// A renderer-owned resource associated with a render-graph resource ID while
/// recording one command buffer.
///
/// Products construct bindings from retained renderer resources or from the
/// matching external/surface ownership token. Raw Vulkan handles never cross
/// the public render-graph boundary.
#[derive(Clone, Copy, Debug)]
pub struct ResourceBinding {
    inner: ResourceBindingInner,
}

#[derive(Clone, Copy, Debug)]
enum ResourceBindingInner {
    Buffer {
        buffer: vk::Buffer,
        offset: vk::DeviceSize,
        size: vk::DeviceSize,
    },
    Image {
        image: vk::Image,
        subresource_range: vk::ImageSubresourceRange,
    },
}

impl ResourceBinding {
    /// Binds the complete range of one renderer-owned buffer.
    pub fn whole_buffer(buffer: &Buffer) -> Self {
        Self::raw_buffer(buffer.raw(), 0, buffer.size())
    }

    /// Binds every mip and layer of one renderer-owned color image.
    pub fn whole_color_image(image: &Image) -> Self {
        Self::raw_image(
            image.raw(),
            image.full_subresource_range(TextureAspects::COLOR).to_vk(),
        )
    }

    pub(crate) const fn raw_buffer(
        buffer: vk::Buffer,
        offset: vk::DeviceSize,
        size: vk::DeviceSize,
    ) -> Self {
        Self {
            inner: ResourceBindingInner::Buffer {
                buffer,
                offset,
                size,
            },
        }
    }

    pub(crate) const fn raw_image(
        image: vk::Image,
        subresource_range: vk::ImageSubresourceRange,
    ) -> Self {
        Self {
            inner: ResourceBindingInner::Image {
                image,
                subresource_range,
            },
        }
    }

    const fn kind(self) -> ResourceKind {
        match self.inner {
            ResourceBindingInner::Buffer { .. } => ResourceKind::Buffer,
            ResourceBindingInner::Image { .. } => ResourceKind::Image,
        }
    }
}

/// Owned synchronization2 barriers for one pass boundary.
#[derive(Clone, Debug, Default)]
pub struct BarrierBatch {
    buffer_barriers: Vec<vk::BufferMemoryBarrier2>,
    image_barriers: Vec<vk::ImageMemoryBarrier2>,
}

impl BarrierBatch {
    /// Creates reusable synchronization scratch with retained vector capacity.
    /// Call [`Self::clear`] before rebuilding it for the next frame.
    pub fn with_capacity(buffer_capacity: usize, image_capacity: usize) -> Self {
        Self {
            buffer_barriers: Vec::with_capacity(buffer_capacity),
            image_barriers: Vec::with_capacity(image_capacity),
        }
    }

    #[cfg(test)]
    pub(crate) fn image_barriers(&self) -> &[vk::ImageMemoryBarrier2] {
        &self.image_barriers
    }

    pub const fn is_empty(&self) -> bool {
        self.buffer_barriers.is_empty() && self.image_barriers.is_empty()
    }

    /// Clears recorded barriers while preserving the batch's allocation for
    /// the next command buffer.
    pub fn clear(&mut self) {
        self.buffer_barriers.clear();
        self.image_barriers.clear();
    }

    /// Appends one semantic color-image transition without exposing Vulkan
    /// synchronization flags or queue-family sentinels to the caller.
    ///
    /// Both states must describe images. Imported and exported dma-bufs use
    /// [`crate::ResourceState::foreign_image`] on the host-owned side and an
    /// ordinary image state while the renderer owns the image.
    pub fn add_image_transition(
        &mut self,
        binding: ResourceBinding,
        source: ResourceState,
        destination: ResourceState,
    ) -> Result<(), RenderGraphSyncError> {
        self.image_barriers
            .push(lower_image_transition(binding, source, destination)?);
        Ok(())
    }

    /// Records one `vkCmdPipelineBarrier2` call. Empty batches are skipped.
    pub(crate) unsafe fn record(&self, device: &Device, command_buffer: vk::CommandBuffer) {
        if self.is_empty() {
            return;
        }
        let dependency = vk::DependencyInfo::builder()
            .buffer_memory_barriers(&self.buffer_barriers)
            .image_memory_barriers(&self.image_barriers);
        unsafe { device.cmd_pipeline_barrier2(command_buffer, &dependency) };
    }
}

fn lower_image_transition(
    binding: ResourceBinding,
    source: ResourceState,
    destination: ResourceState,
) -> Result<vk::ImageMemoryBarrier2, RenderGraphSyncError> {
    let ResourceBindingInner::Image {
        image,
        subresource_range,
    } = binding.inner
    else {
        return Err(RenderGraphSyncError::InvalidImageTransition {
            binding: binding.kind(),
            source: source.resource_kind(),
            destination: destination.resource_kind(),
        });
    };
    if source.resource_kind() != ResourceKind::Image
        || destination.resource_kind() != ResourceKind::Image
    {
        return Err(RenderGraphSyncError::InvalidImageTransition {
            binding: ResourceKind::Image,
            source: source.resource_kind(),
            destination: destination.resource_kind(),
        });
    }
    let (source_stages, source_access, source_layout, source_family) = source.synchronization();
    let (destination_stages, destination_access, destination_layout, destination_family) =
        destination.synchronization();
    let (source_queue, destination_queue) =
        queue_family_transfer(source_family, destination_family);
    Ok(vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(source_stages)
        .src_access_mask(source_access)
        .dst_stage_mask(destination_stages)
        .dst_access_mask(destination_access)
        .old_layout(source_layout)
        .new_layout(destination_layout)
        .src_queue_family_index(source_queue)
        .dst_queue_family_index(destination_queue)
        .image(image)
        .subresource_range(subresource_range)
        .build())
}

impl CompiledGraph {
    /// Resolves abstract graph barriers into Vulkan synchronization2 barriers
    /// which must execute immediately before `pass`.
    pub fn barrier_batch_before(
        &self,
        pass: PassId,
        bindings: &BTreeMap<ResourceId, ResourceBinding>,
    ) -> Result<BarrierBatch, RenderGraphSyncError> {
        let mut batch = BarrierBatch::default();
        for barrier in self.barriers.iter().filter(|barrier| barrier.after == pass) {
            let binding = bindings
                .get(&barrier.resource)
                .copied()
                .ok_or(RenderGraphSyncError::MissingBinding(barrier.resource))?;
            if binding.kind() != barrier.kind {
                return Err(RenderGraphSyncError::KindMismatch {
                    resource: barrier.resource,
                    graph: barrier.kind,
                    binding: binding.kind(),
                });
            }
            match binding.inner {
                ResourceBindingInner::Buffer {
                    buffer,
                    offset,
                    size,
                } => {
                    let (source_stages, source_access, _, source_family) =
                        barrier.source.synchronization();
                    let (destination_stages, destination_access, _, destination_family) =
                        barrier.destination.synchronization();
                    let (source_queue, destination_queue) =
                        queue_family_transfer(source_family, destination_family);
                    batch.buffer_barriers.push(
                        vk::BufferMemoryBarrier2::builder()
                            .src_stage_mask(source_stages)
                            .src_access_mask(source_access)
                            .dst_stage_mask(destination_stages)
                            .dst_access_mask(destination_access)
                            .src_queue_family_index(source_queue)
                            .dst_queue_family_index(destination_queue)
                            .buffer(buffer)
                            .offset(offset)
                            .size(size)
                            .build(),
                    );
                }
                ResourceBindingInner::Image {
                    image: _,
                    subresource_range: _,
                } => batch.image_barriers.push(lower_image_transition(
                    binding,
                    barrier.source,
                    barrier.destination,
                )?),
            }
        }
        Ok(batch)
    }
}

const fn queue_family_transfer(source: u32, destination: u32) -> (u32, u32) {
    if source == destination {
        (vk::QUEUE_FAMILY_IGNORED, vk::QUEUE_FAMILY_IGNORED)
    } else {
        (source, destination)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderGraphSyncError {
    MissingBinding(ResourceId),
    KindMismatch {
        resource: ResourceId,
        graph: ResourceKind,
        binding: ResourceKind,
    },
    InvalidImageTransition {
        binding: ResourceKind,
        source: ResourceKind,
        destination: ResourceKind,
    },
}

impl fmt::Display for RenderGraphSyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for RenderGraphSyncError {}

#[cfg(test)]
mod tests {
    use crate::{
        AccessKind, ForeignImageState, RenderGraph, RenderGraphImageState, RenderPass,
        ResourceState, ResourceUse,
    };

    use super::*;

    #[test]
    fn image_transition_becomes_one_sync2_image_barrier() {
        let resource = ResourceId(4);
        let mut graph = RenderGraph::default();
        graph.add_pass(RenderPass {
            id: PassId(1),
            label: "color".into(),
            depends_on: vec![],
            resources: vec![ResourceUse {
                resource,
                kind: ResourceKind::Image,
                access: AccessKind::Write,
                state: ResourceState::image(RenderGraphImageState::ColorAttachmentWrite, 2),
            }],
        });
        graph.add_pass(RenderPass {
            id: PassId(2),
            label: "sample".into(),
            depends_on: vec![],
            resources: vec![ResourceUse {
                resource,
                kind: ResourceKind::Image,
                access: AccessKind::Read,
                state: ResourceState::image(RenderGraphImageState::FragmentSampledRead, 3),
            }],
        });
        let compiled = graph.compile().unwrap();
        let bindings = BTreeMap::from([(
            resource,
            ResourceBinding::raw_image(
                vk::Image::from_raw(7),
                vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                },
            ),
        )]);
        let batch = compiled.barrier_batch_before(PassId(2), &bindings).unwrap();
        assert_eq!(batch.image_barriers().len(), 1);
        let barrier = batch.image_barriers()[0];
        assert_eq!(
            barrier.old_layout,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
        );
        assert_eq!(
            barrier.new_layout,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        );
        assert_eq!(barrier.src_queue_family_index, 2);
        assert_eq!(barrier.dst_queue_family_index, 3);
    }

    #[test]
    fn identical_queue_family_uses_ignored_indices() {
        assert_eq!(
            queue_family_transfer(5, 5),
            (vk::QUEUE_FAMILY_IGNORED, vk::QUEUE_FAMILY_IGNORED)
        );
    }

    #[test]
    fn imported_image_acquires_from_and_releases_to_foreign() {
        let resource = ResourceId(9);
        let graphics = 3;
        let acquire = PassId(1);
        let release = PassId(2);
        let range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };
        let mut graph = RenderGraph::default();
        graph.set_initial_state(
            resource,
            ResourceKind::Image,
            ResourceState::foreign_image(ForeignImageState::Undefined),
        );
        graph.add_pass(RenderPass {
            id: acquire,
            label: "sample imported client image".into(),
            depends_on: vec![],
            resources: vec![ResourceUse {
                resource,
                kind: ResourceKind::Image,
                access: AccessKind::Read,
                state: ResourceState::image(
                    RenderGraphImageState::FragmentSampledReadGeneral,
                    graphics,
                ),
            }],
        });
        graph.add_pass(RenderPass {
            id: release,
            label: "release imported client image".into(),
            depends_on: vec![acquire],
            resources: vec![ResourceUse {
                resource,
                kind: ResourceKind::Image,
                access: AccessKind::Read,
                state: ResourceState::foreign_image(ForeignImageState::General),
            }],
        });
        let compiled = graph.compile().unwrap();
        let binding = BTreeMap::from([(
            resource,
            ResourceBinding::raw_image(vk::Image::from_raw(12), range),
        )]);
        let acquire_barrier = compiled
            .barrier_batch_before(acquire, &binding)
            .unwrap()
            .image_barriers()[0];
        assert_eq!(
            acquire_barrier.src_queue_family_index,
            vk::QUEUE_FAMILY_FOREIGN_EXT
        );
        assert_eq!(acquire_barrier.dst_queue_family_index, graphics);
        assert_eq!(acquire_barrier.old_layout, vk::ImageLayout::UNDEFINED);
        let release_barrier = compiled
            .barrier_batch_before(release, &binding)
            .unwrap()
            .image_barriers()[0];
        assert_eq!(release_barrier.src_queue_family_index, graphics);
        assert_eq!(
            release_barrier.dst_queue_family_index,
            vk::QUEUE_FAMILY_FOREIGN_EXT
        );
        assert_eq!(release_barrier.new_layout, vk::ImageLayout::GENERAL);
    }

    #[test]
    fn reusable_batch_lowers_a_foreign_image_transition_without_raw_sync_flags() {
        let graphics = 4;
        let binding = ResourceBinding::raw_image(
            vk::Image::from_raw(15),
            vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            },
        );
        let mut batch = BarrierBatch::with_capacity(0, 2);
        batch
            .add_image_transition(
                binding,
                ResourceState::foreign_image(ForeignImageState::General),
                ResourceState::image(RenderGraphImageState::ColorAttachmentWrite, graphics),
            )
            .unwrap();
        let barrier = batch.image_barriers()[0];
        assert_eq!(barrier.old_layout, vk::ImageLayout::GENERAL);
        assert_eq!(
            barrier.new_layout,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
        );
        assert_eq!(barrier.src_queue_family_index, vk::QUEUE_FAMILY_FOREIGN_EXT);
        assert_eq!(barrier.dst_queue_family_index, graphics);
        batch.clear();
        assert!(batch.is_empty());
    }
}
