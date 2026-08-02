use vulkanalia::{prelude::v1_4::*, vk};

use super::RenderingEncoder;
use crate::{Error, MachineCodeGraphicsPipeline, Result};

impl RenderingEncoder<'_> {
    /// Binds and retains a machine-code graphics pipeline created from the
    /// shared typed descriptor path.
    pub fn bind_machine_code_pipeline(
        &mut self,
        pipeline: &MachineCodeGraphicsPipeline,
    ) -> Result<()> {
        if !pipeline.belongs_to(&self.encoder.owner) {
            return Err(Error::Validation(
                "machine-code graphics pipeline was created by a different Device".into(),
            ));
        }
        let facts = pipeline.graphics_facts().ok_or_else(|| {
            Error::Validation(
                "machine-code graphics pipeline was not created from a typed graphics descriptor"
                    .into(),
            )
        })?;
        if facts.color_formats != self.color_formats
            || facts.depth_format != self.depth_format
            || facts.stencil_format != self.stencil_format
            || facts.sample_count.to_vk() != self.sample_count
        {
            return Err(Error::Validation(
                "machine-code graphics pipeline attachment formats or sample count do not match the rendering scope"
                    .into(),
            ));
        }
        unsafe {
            self.encoder.owner.device.cmd_bind_pipeline(
                self.encoder.raw(),
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.raw(),
            );
        }
        self.encoder.retain_resource(pipeline);
        self.pipeline_bound = true;
        self.required_vertex_buffers = facts.vertex_buffer_slots.clone();
        Ok(())
    }
}
