use vulkanalia::{prelude::v1_4::*, vk};

use super::CommandEncoder;
use crate::{Buffer, ComputePipeline, DescriptorHeap, Error, Result};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ComputePassDescriptor<'a> {
    pub label: Option<&'a str>,
}

/// Borrowed compute recording scope with mandatory pipeline validation.
pub struct ComputeEncoder<'encoder> {
    encoder: &'encoder mut CommandEncoder,
    label: Option<String>,
    pipeline_bound: bool,
}

impl std::fmt::Debug for ComputeEncoder<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ComputeEncoder")
            .field("label", &self.label)
            .field("pipeline_bound", &self.pipeline_bound)
            .finish_non_exhaustive()
    }
}

impl CommandEncoder {
    pub fn begin_compute<'encoder>(
        &'encoder mut self,
        descriptor: &ComputePassDescriptor<'_>,
    ) -> ComputeEncoder<'encoder> {
        ComputeEncoder {
            encoder: self,
            label: descriptor.label.map(str::to_owned),
            pipeline_bound: false,
        }
    }
}

impl ComputeEncoder<'_> {
    /// Copies a small shader payload using the descriptor-heap push-data path.
    pub fn push_data(&mut self, offset: u32, data: &[u8]) -> Result<()> {
        self.encoder.push_data(offset, data)
    }

    /// Binds and retains a descriptor-heap compute pipeline.
    pub fn bind_pipeline(&mut self, pipeline: &ComputePipeline) -> Result<()> {
        if !pipeline.belongs_to(&self.encoder.owner) {
            return Err(Error::Validation(
                "compute pipeline was created by a different Device".into(),
            ));
        }
        unsafe {
            self.encoder.owner.device.cmd_bind_pipeline(
                self.encoder.raw(),
                vk::PipelineBindPoint::COMPUTE,
                pipeline.raw(),
            )
        };
        self.encoder.retain_resource(pipeline);
        self.pipeline_bound = true;
        Ok(())
    }

    /// Binds a resource or sampler descriptor heap for compute commands.
    ///
    /// # Safety
    ///
    /// The heap and referenced resources must remain live and unmodified until
    /// submission completes.
    pub unsafe fn bind_descriptor_heap(&mut self, heap: &DescriptorHeap) -> Result<()> {
        unsafe { self.encoder.bind_descriptor_heap(heap) }
    }

    /// Dispatches compute work after a pipeline has been bound.
    ///
    /// # Safety
    ///
    /// Bound descriptors and shader-addressed ranges must be valid for every
    /// invocation.
    pub unsafe fn dispatch(&mut self, x: u32, y: u32, z: u32) -> Result<()> {
        self.validate_pipeline()?;
        if x == 0 || y == 0 || z == 0 {
            return Err(Error::Validation(
                "compute dispatch group counts must be non-zero".into(),
            ));
        }
        unsafe {
            self.encoder
                .owner
                .device
                .cmd_dispatch(self.encoder.raw(), x, y, z)
        };
        Ok(())
    }

    /// Dispatches using one `VkDispatchIndirectCommand` in `buffer`.
    ///
    /// # Safety
    ///
    /// The indirect command and all shader resources must remain live and
    /// valid until submission completes.
    pub unsafe fn dispatch_indirect(&mut self, buffer: &Buffer, offset: u64) -> Result<()> {
        self.validate_pipeline()?;
        validate_indirect_buffer(self.encoder, buffer, offset, 12)?;
        unsafe {
            self.encoder.owner.device.cmd_dispatch_indirect(
                self.encoder.raw(),
                buffer.raw(),
                offset,
            )
        };
        Ok(())
    }

    fn validate_pipeline(&self) -> Result<()> {
        if !self.pipeline_bound {
            return Err(Error::Validation(
                "compute dispatch requires a bound pipeline".into(),
            ));
        }
        Ok(())
    }
}

fn validate_indirect_buffer(
    encoder: &CommandEncoder,
    buffer: &Buffer,
    offset: u64,
    command_size: u64,
) -> Result<()> {
    if !buffer.belongs_to(&encoder.owner) {
        return Err(Error::Validation(
            "indirect buffer was created by a different Device".into(),
        ));
    }
    if !buffer
        .usage()
        .contains(vk::BufferUsageFlags::INDIRECT_BUFFER)
    {
        return Err(Error::Validation(
            "indirect buffer is missing INDIRECT_BUFFER usage".into(),
        ));
    }
    if !offset.is_multiple_of(4)
        || offset
            .checked_add(command_size)
            .is_none_or(|end| end > buffer.size())
    {
        return Err(Error::Validation(
            "indirect command offset is misaligned or outside the buffer".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_pass_descriptor_preserves_diagnostic_label() {
        let descriptor = ComputePassDescriptor {
            label: Some("particles"),
        };
        assert_eq!(descriptor.label, Some("particles"));
    }
}
