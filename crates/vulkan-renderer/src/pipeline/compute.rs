use std::fmt;
use std::sync::Arc;

use vulkanalia::{prelude::v1_4::*, vk};

use super::{PipelineCache, ProgrammableStage};
use crate::backend::DeviceOwner;
use crate::{Backend, Error, Result};

mod machine_code;

pub use machine_code::MachineCodeComputePipelineDescriptor;

/// Descriptor-heap compute pipeline contract.
#[derive(Clone, Copy, Debug)]
pub struct ComputePipelineDescriptor<'a> {
    pub label: Option<&'a str>,
    pub stage: ProgrammableStage<'a>,
    pub cache: Option<&'a PipelineCache>,
}

/// Compute pipeline using a null pipeline layout and
/// `VK_PIPELINE_CREATE_2_DESCRIPTOR_HEAP_BIT_EXT`.
#[derive(Clone)]
pub struct ComputePipeline {
    inner: Arc<ComputePipelineInner>,
}

struct ComputePipelineInner {
    owner: Arc<DeviceOwner>,
    raw: vk::Pipeline,
    label: Option<String>,
}

impl fmt::Debug for ComputePipeline {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ComputePipeline")
            .field("raw", &self.inner.raw)
            .field("label", &self.inner.label)
            .finish_non_exhaustive()
    }
}

impl ComputePipeline {
    pub fn raw(&self) -> vk::Pipeline {
        self.inner.raw
    }

    pub fn label(&self) -> Option<&str> {
        self.inner.label.as_deref()
    }

    pub(crate) fn belongs_to(&self, owner: &Arc<DeviceOwner>) -> bool {
        Arc::ptr_eq(&self.inner.owner, owner)
    }
}

impl crate::SubmissionResource for ComputePipeline {
    fn submission_lease(&self) -> crate::SubmissionLease {
        crate::SubmissionLease::new(Arc::clone(&self.inner))
    }
}

impl Drop for ComputePipelineInner {
    fn drop(&mut self) {
        unsafe { self.owner.device.destroy_pipeline(self.raw, None) };
    }
}

impl Backend {
    pub fn create_compute_pipeline(
        &self,
        descriptor: &ComputePipelineDescriptor<'_>,
    ) -> Result<ComputePipeline> {
        validate_compute_pipeline_descriptor(self, descriptor)?;
        let owner = self.shared_owner();
        let raw = with_compute_pipeline_create_info(
            descriptor,
            vk::PipelineCreateFlags2::empty(),
            None,
            |info| create_compute_pipeline_with_cache(&owner, descriptor.cache, info),
        )?;
        Ok(ComputePipeline {
            inner: Arc::new(ComputePipelineInner {
                owner,
                raw,
                label: descriptor.label.map(str::to_owned),
            }),
        })
    }
}

pub(super) fn validate_compute_pipeline_descriptor(
    backend: &Backend,
    descriptor: &ComputePipelineDescriptor<'_>,
) -> Result<()> {
    if !backend
        .features()
        .contains(crate::Features::DESCRIPTOR_HEAP)
    {
        return Err(Error::Validation(
            "compute pipelines require enabled Features::DESCRIPTOR_HEAP".into(),
        ));
    }
    descriptor
        .stage
        .bindings
        .validate_for_device(backend.device_info().limits.descriptor_heap)
        .map_err(|error| Error::Validation(format!("compute shader binding map: {error}")))?;
    let owner = backend.shared_owner();
    if !descriptor.stage.module.belongs_to(&owner) {
        return Err(Error::Validation(
            "compute shader module was created by a different Device".into(),
        ));
    }
    if descriptor
        .cache
        .is_some_and(|cache| !cache.belongs_to(&owner))
    {
        return Err(Error::Validation(
            "compute pipeline cache was created by a different Device".into(),
        ));
    }
    Ok(())
}

pub(super) fn with_compute_pipeline_create_info<T>(
    descriptor: &ComputePipelineDescriptor<'_>,
    additional_flags: vk::PipelineCreateFlags2,
    ready_binaries: Option<&[vk::PipelineBinaryKHR]>,
    use_info: impl FnOnce(vk::ComputePipelineCreateInfo) -> Result<T>,
) -> Result<T> {
    descriptor
        .stage
        .bindings
        .with_stage_create_info(
            vk::ShaderStageFlags::COMPUTE,
            descriptor.stage.module,
            descriptor.stage.entry_point,
            |stage| {
                let mut flags = vk::PipelineCreateFlags2CreateInfo::builder()
                    .flags(vk::PipelineCreateFlags2::DESCRIPTOR_HEAP_EXT | additional_flags)
                    .build();
                let mut binary_info = ready_binaries.map(|binaries| {
                    vk::PipelineBinaryInfoKHR::builder()
                        .pipeline_binaries(binaries)
                        .build()
                });
                let mut info = vk::ComputePipelineCreateInfo::builder()
                    .stage(*stage)
                    .layout(vk::PipelineLayout::null())
                    .push_next(&mut flags);
                if let Some(binary_info) = binary_info.as_mut() {
                    info = info.push_next(binary_info);
                }
                use_info(info.build())
            },
        )
        .map_err(|error| Error::Validation(error.to_string()))?
}

fn create_compute_pipeline_with_cache(
    owner: &Arc<DeviceOwner>,
    cache: Option<&PipelineCache>,
    info: vk::ComputePipelineCreateInfo,
) -> Result<vk::Pipeline> {
    match cache {
        Some(cache) => {
            cache.with_raw(|cache| create_compute_pipeline_with_device(&owner.device, cache, info))
        }
        None => create_compute_pipeline_with_device(&owner.device, vk::PipelineCache::null(), info),
    }
}

pub(super) fn create_compute_pipeline_with_device(
    device: &vulkanalia::Device,
    cache: vk::PipelineCache,
    info: vk::ComputePipelineCreateInfo,
) -> Result<vk::Pipeline> {
    let (mut pipelines, status) = unsafe { device.create_compute_pipelines(cache, &[info], None) }
        .map_err(|source| Error::vulkan("vkCreateComputePipelines", source))?;
    if status != vk::SuccessCode::SUCCESS || pipelines.len() != 1 {
        for pipeline in pipelines {
            unsafe { device.destroy_pipeline(pipeline, None) };
        }
        return Err(Error::Validation(format!(
            "vkCreateComputePipelines did not return exactly one ready pipeline: status={status:?}"
        )));
    }
    Ok(pipelines.remove(0))
}
