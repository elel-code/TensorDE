use vulkanalia::vk::{self, Handle};

use super::{
    ComputePipelineDescriptor, create_compute_pipeline_with_device,
    validate_compute_pipeline_descriptor, with_compute_pipeline_create_info,
};
use crate::{
    Backend, Error, MachineCodeComputePipeline, PipelineBinaryArchiveCache, Result,
    compute_pipeline_binary_key, create_compute_pipeline_machine_code,
};

#[derive(Clone, Copy, Debug)]
pub struct MachineCodeComputePipelineDescriptor<'a> {
    pub pipeline: ComputePipelineDescriptor<'a>,
    pub archive_cache: &'a PipelineBinaryArchiveCache,
}

impl Backend {
    pub fn create_machine_code_compute_pipeline(
        &self,
        descriptor: &MachineCodeComputePipelineDescriptor<'_>,
    ) -> Result<MachineCodeComputePipeline> {
        validate_compute_pipeline_descriptor(self, &descriptor.pipeline)?;
        if descriptor.pipeline.cache.is_some() {
            return Err(Error::Validation(
                "machine-code compute pipelines use PipelineBinaryArchiveCache, not PipelineCache"
                    .into(),
            ));
        }
        let pipeline_key = with_compute_pipeline_create_info(
            &descriptor.pipeline,
            vk::PipelineCreateFlags2::empty(),
            None,
            |info| unsafe { compute_pipeline_binary_key(self, &info) },
        )?;
        let cached_archive = descriptor.archive_cache.load(&pipeline_key)?;
        let cache_hit = cached_archive.is_some();
        let mut pipeline = unsafe {
            create_compute_pipeline_machine_code(
                self,
                cached_archive.as_ref(),
                |device, creation| {
                    with_compute_pipeline_create_info(
                        &descriptor.pipeline,
                        creation.flags(),
                        creation.ready_binaries(),
                        |info| {
                            create_compute_pipeline_with_device(
                                device,
                                vk::PipelineCache::null(),
                                info,
                            )
                        },
                    )
                },
            )
        }?;
        pipeline.mark_compute();
        if !cache_hit {
            descriptor
                .archive_cache
                .store(&pipeline_key, pipeline.archive())?;
        }
        Ok(pipeline)
    }
}
