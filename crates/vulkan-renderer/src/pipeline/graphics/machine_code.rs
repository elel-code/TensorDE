use vulkanalia::vk::{self, Handle};

use super::{
    GraphicsPipelineDescriptor, create_pipeline_with_device, graphics_pipeline_facts,
    validate_graphics_pipeline_descriptor, with_graphics_pipeline_create_info,
};
use crate::{
    Backend, Error, MachineCodeGraphicsPipeline, PipelineBinaryArchiveCache, Result,
    create_graphics_pipeline_machine_code, graphics_pipeline_binary_key,
};

/// Typed graphics state plus the persistent archive selected for strict
/// machine-code materialization.
///
/// The renderer derives the implementation key, performs archive lookup, and
/// captures or recreates the pipeline. Products never construct a raw Vulkan
/// pipeline create-info or receive pipeline-binary handles.
#[derive(Clone, Copy, Debug)]
pub struct MachineCodeGraphicsPipelineDescriptor<'a> {
    pub pipeline: GraphicsPipelineDescriptor<'a>,
    pub archive_cache: &'a PipelineBinaryArchiveCache,
}

impl Backend {
    /// Creates a retained graphics pipeline from usable device machine code.
    ///
    /// A cache miss compiles once with capture enabled, persists the complete
    /// ordered binary archive, destroys the provisional pipeline, and creates
    /// the returned pipeline from those binaries. A cache hit never permits a
    /// shader compilation fallback.
    pub fn create_machine_code_graphics_pipeline(
        &self,
        descriptor: &MachineCodeGraphicsPipelineDescriptor<'_>,
    ) -> Result<MachineCodeGraphicsPipeline> {
        validate_graphics_pipeline_descriptor(self, &descriptor.pipeline)?;
        if descriptor.pipeline.cache.is_some() {
            return Err(Error::Validation(
                "machine-code graphics pipelines use PipelineBinaryArchiveCache, not PipelineCache"
                    .into(),
            ));
        }

        let pipeline_key = with_graphics_pipeline_create_info(
            &descriptor.pipeline,
            vk::PipelineCreateFlags2::empty(),
            None,
            |info| unsafe { graphics_pipeline_binary_key(self, &info) },
        )?;
        let cached_archive = descriptor.archive_cache.load(&pipeline_key)?;
        let cache_hit = cached_archive.is_some();
        let mut pipeline = unsafe {
            create_graphics_pipeline_machine_code(
                self,
                cached_archive.as_ref(),
                |device, creation| {
                    with_graphics_pipeline_create_info(
                        &descriptor.pipeline,
                        creation.flags(),
                        creation.ready_binaries(),
                        |info| create_pipeline_with_device(device, vk::PipelineCache::null(), info),
                    )
                },
            )
        }?;
        pipeline.mark_graphics(graphics_pipeline_facts(&descriptor.pipeline));
        if !cache_hit {
            descriptor
                .archive_cache
                .store(&pipeline_key, pipeline.archive())?;
        }
        Ok(pipeline)
    }
}
