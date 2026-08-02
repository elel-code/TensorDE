use vulkanalia::{
    prelude::v1_4::*,
    vk::{self, ExtDescriptorHeapExtensionDeviceCommands},
};

use super::{DescriptorHeap, DescriptorHeapKind, DescriptorHeapMemory, DescriptorHeapUploadRange};
use crate::{CommandEncoder, Error, Result};

/// Reusable scratch storage for device-local descriptor-heap copies.
///
/// Keep one batch per recording stream. Its storage grows only when a frame
/// actually needs more disjoint upload ranges, so steady-state recording does
/// not allocate while still issuing one `vkCmdCopyBuffer` for the batch.
#[derive(Debug, Default)]
pub struct DescriptorHeapUploadBatch {
    copies: Vec<vk::BufferCopy>,
}

impl DescriptorHeapUploadBatch {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            copies: Vec::with_capacity(capacity),
        }
    }

    fn prepare(&mut self, ranges: &[DescriptorHeapUploadRange]) {
        self.copies.clear();
        self.copies.reserve(ranges.len());
        self.copies.extend(ranges.iter().map(|range| {
            vk::BufferCopy::builder()
                .src_offset(range.offset)
                .dst_offset(range.offset)
                .size(range.size)
                .build()
        }));
        // Callers can compose ranges from independent table writers. Sort and
        // merge touching/overlapping ranges so this remains one compact copy
        // command instead of reproducing descriptor-table fragmentation in
        // the GPU command stream. `validate_upload_ranges` checked every end
        // before this private helper is reached.
        self.copies.sort_unstable_by_key(|copy| copy.src_offset);
        let mut compacted = 0usize;
        for index in 0..self.copies.len() {
            let copy = self.copies[index];
            if compacted > 0 {
                let previous = &mut self.copies[compacted - 1];
                let previous_end = previous.src_offset.saturating_add(previous.size);
                let copy_end = copy.src_offset.saturating_add(copy.size);
                if copy.src_offset <= previous_end {
                    previous.size = copy_end.max(previous_end) - previous.src_offset;
                    continue;
                }
            }
            self.copies[compacted] = copy;
            compacted += 1;
        }
        self.copies.truncate(compacted);
    }
}

impl DescriptorHeap {
    /// Records the visibility transition, optional staging copies, and direct
    /// heap bind needed before shaders access this heap.
    ///
    /// `upload_ranges` must refer only to currently allocated application
    /// ranges whose descriptor bytes were fully written and flushed through
    /// this heap. For a host-visible heap no copy occurs; the host-write
    /// barrier still makes those writes visible to direct descriptor access.
    ///
    /// # Safety
    ///
    /// Callers must not rewrite an uploaded allocation until the submission
    /// that uses it has retired.
    pub unsafe fn record_upload_and_bind(
        &self,
        encoder: &mut CommandEncoder,
        upload_ranges: &[DescriptorHeapUploadRange],
        batch: &mut DescriptorHeapUploadBatch,
    ) -> Result<()> {
        if !encoder.belongs_to(&self.owner) {
            return Err(Error::Validation(
                "descriptor heap and command encoder belong to different Devices".into(),
            ));
        }
        self.validate_upload_ranges(upload_ranges)?;
        let command_buffer = encoder.raw();
        let device = &self.owner.device;
        let descriptor_access = match self.kind {
            DescriptorHeapKind::Resource => vk::AccessFlags2::RESOURCE_HEAP_READ_EXT,
            DescriptorHeapKind::Sampler => vk::AccessFlags2::SAMPLER_HEAP_READ_EXT,
        };
        if !upload_ranges.is_empty() {
            let host_barrier = vk::MemoryBarrier2::builder()
                .src_stage_mask(vk::PipelineStageFlags2::HOST)
                .src_access_mask(vk::AccessFlags2::HOST_WRITE)
                .dst_stage_mask(match self.memory {
                    DescriptorHeapMemory::HostVisible => vk::PipelineStageFlags2::ALL_COMMANDS,
                    DescriptorHeapMemory::DeviceLocal => vk::PipelineStageFlags2::ALL_TRANSFER,
                })
                .dst_access_mask(match self.memory {
                    DescriptorHeapMemory::HostVisible => descriptor_access,
                    DescriptorHeapMemory::DeviceLocal => vk::AccessFlags2::TRANSFER_READ,
                })
                .build();
            unsafe {
                device.cmd_pipeline_barrier2(
                    command_buffer,
                    &vk::DependencyInfo::builder()
                        .memory_barriers(&[host_barrier])
                        .build(),
                );
            }

            if self.memory == DescriptorHeapMemory::DeviceLocal {
                let staging_buffer = self.staging_buffer.ok_or_else(|| {
                    Error::Validation(
                        "device-local descriptor heap is missing staging storage".into(),
                    )
                })?;
                batch.prepare(upload_ranges);
                unsafe {
                    device.cmd_copy_buffer(
                        command_buffer,
                        staging_buffer,
                        self.buffer,
                        &batch.copies,
                    );
                }
                let transfer_barrier = vk::MemoryBarrier2::builder()
                    .src_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
                    .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                    .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                    .dst_access_mask(descriptor_access)
                    .build();
                unsafe {
                    device.cmd_pipeline_barrier2(
                        command_buffer,
                        &vk::DependencyInfo::builder()
                            .memory_barriers(&[transfer_barrier])
                            .build(),
                    );
                }
            }
        }
        unsafe {
            match self.kind {
                DescriptorHeapKind::Resource => {
                    device.cmd_bind_resource_heap_ext(command_buffer, &self.bind_info())
                }
                DescriptorHeapKind::Sampler => {
                    device.cmd_bind_sampler_heap_ext(command_buffer, &self.bind_info())
                }
            }
        }
        Ok(())
    }

    fn validate_upload_ranges(&self, ranges: &[DescriptorHeapUploadRange]) -> Result<()> {
        for range in ranges {
            if range.size == 0
                || range
                    .offset
                    .checked_add(range.size)
                    .is_none_or(|end| end > self.application_capacity())
            {
                return Err(Error::Validation(
                    "descriptor upload range is empty or exceeds application heap storage".into(),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_batch_compacts_out_of_order_touching_ranges() {
        let mut batch = DescriptorHeapUploadBatch::with_capacity(5);
        batch.prepare(&[
            DescriptorHeapUploadRange {
                offset: 32,
                size: 16,
            },
            DescriptorHeapUploadRange {
                offset: 0,
                size: 16,
            },
            DescriptorHeapUploadRange {
                offset: 16,
                size: 16,
            },
            DescriptorHeapUploadRange {
                offset: 40,
                size: 16,
            },
            DescriptorHeapUploadRange {
                offset: 96,
                size: 8,
            },
        ]);

        assert_eq!(batch.copies.len(), 2);
        assert_eq!(batch.copies[0].src_offset, 0);
        assert_eq!(batch.copies[0].dst_offset, 0);
        assert_eq!(batch.copies[0].size, 56);
        assert_eq!(batch.copies[1].src_offset, 96);
        assert_eq!(batch.copies[1].dst_offset, 96);
        assert_eq!(batch.copies[1].size, 8);
    }
}
