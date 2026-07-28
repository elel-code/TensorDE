use std::collections::BTreeMap;
use std::fmt;

use vulkanalia::{Device, prelude::v1_4::*, vk};

use crate::{CompiledGraph, PassId, ResourceId, ResourceKind};

mod external;
mod semaphore;

pub use external::{ExternalTimelineSemaphoreDescriptor, RetainedExternalTimelineSemaphore};
pub use semaphore::{BinarySemaphore, BinarySemaphoreDescriptor};

/// Raw resource handle associated with a render-graph resource ID while
/// recording one command buffer.
#[derive(Clone, Copy, Debug)]
pub enum ResourceBinding {
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
    const fn kind(self) -> ResourceKind {
        match self {
            Self::Buffer { .. } => ResourceKind::Buffer,
            Self::Image { .. } => ResourceKind::Image,
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
    pub fn buffer_barriers(&self) -> &[vk::BufferMemoryBarrier2] {
        &self.buffer_barriers
    }

    pub fn image_barriers(&self) -> &[vk::ImageMemoryBarrier2] {
        &self.image_barriers
    }

    pub const fn is_empty(&self) -> bool {
        self.buffer_barriers.is_empty() && self.image_barriers.is_empty()
    }

    /// Records one `vkCmdPipelineBarrier2` call. Empty batches are skipped.
    ///
    /// # Safety
    ///
    /// The command buffer must be recording on `device`; every raw binding
    /// must be live and owned by the queue family described by the graph state.
    pub unsafe fn record(&self, device: &Device, command_buffer: vk::CommandBuffer) {
        if self.is_empty() {
            return;
        }
        let dependency = vk::DependencyInfo::builder()
            .buffer_memory_barriers(&self.buffer_barriers)
            .image_memory_barriers(&self.image_barriers);
        unsafe { device.cmd_pipeline_barrier2(command_buffer, &dependency) };
    }
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
            let (source_queue, destination_queue) = queue_family_transfer(
                barrier.source.queue_family,
                barrier.destination.queue_family,
            );
            match binding {
                ResourceBinding::Buffer {
                    buffer,
                    offset,
                    size,
                } => batch.buffer_barriers.push(
                    vk::BufferMemoryBarrier2::builder()
                        .src_stage_mask(barrier.source.stages)
                        .src_access_mask(barrier.source.access)
                        .dst_stage_mask(barrier.destination.stages)
                        .dst_access_mask(barrier.destination.access)
                        .src_queue_family_index(source_queue)
                        .dst_queue_family_index(destination_queue)
                        .buffer(buffer)
                        .offset(offset)
                        .size(size)
                        .build(),
                ),
                ResourceBinding::Image {
                    image,
                    subresource_range,
                } => batch.image_barriers.push(
                    vk::ImageMemoryBarrier2::builder()
                        .src_stage_mask(barrier.source.stages)
                        .src_access_mask(barrier.source.access)
                        .dst_stage_mask(barrier.destination.stages)
                        .dst_access_mask(barrier.destination.access)
                        .old_layout(barrier.source.layout)
                        .new_layout(barrier.destination.layout)
                        .src_queue_family_index(source_queue)
                        .dst_queue_family_index(destination_queue)
                        .image(image)
                        .subresource_range(subresource_range)
                        .build(),
                ),
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
}

impl fmt::Display for RenderGraphSyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for RenderGraphSyncError {}

#[cfg(test)]
mod tests {
    use crate::{AccessKind, RenderGraph, RenderPass, ResourceState, ResourceUse};

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
                state: ResourceState::image(
                    vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    2,
                ),
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
                state: ResourceState::image(
                    vk::PipelineStageFlags2::FRAGMENT_SHADER,
                    vk::AccessFlags2::SHADER_SAMPLED_READ,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    3,
                ),
            }],
        });
        let compiled = graph.compile().unwrap();
        let bindings = BTreeMap::from([(
            resource,
            ResourceBinding::Image {
                image: vk::Image::from_raw(7),
                subresource_range: vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                },
            },
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
            ResourceState::foreign_image(vk::ImageLayout::UNDEFINED),
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
                    vk::PipelineStageFlags2::FRAGMENT_SHADER,
                    vk::AccessFlags2::SHADER_SAMPLED_READ,
                    vk::ImageLayout::GENERAL,
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
                state: ResourceState::foreign_image(vk::ImageLayout::GENERAL),
            }],
        });
        let compiled = graph.compile().unwrap();
        let binding = BTreeMap::from([(
            resource,
            ResourceBinding::Image {
                image: vk::Image::from_raw(12),
                subresource_range: range,
            },
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
}
