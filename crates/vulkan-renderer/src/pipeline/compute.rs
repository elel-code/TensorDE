use std::fmt;
use std::sync::Arc;

use vulkanalia::{prelude::v1_4::*, vk};

use super::{PipelineCache, ProgrammableStage};
use crate::backend::DeviceOwner;
use crate::{Backend, Error, Result};

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
        if !self.features().contains(crate::Features::DESCRIPTOR_HEAP) {
            return Err(Error::Validation(
                "compute pipelines require enabled Features::DESCRIPTOR_HEAP".into(),
            ));
        }
        descriptor
            .stage
            .bindings
            .validate_for_device(self.device_info().limits.descriptor_heap)
            .map_err(|error| Error::Validation(format!("compute shader binding map: {error}")))?;
        let owner = self.shared_owner();
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
        let raw = descriptor
            .stage
            .bindings
            .with_stage_create_info(
                vk::ShaderStageFlags::COMPUTE,
                descriptor.stage.module,
                descriptor.stage.entry_point,
                |stage| create_compute_pipeline(&owner, descriptor.cache, *stage),
            )
            .map_err(|error| Error::Validation(error.to_string()))??;
        Ok(ComputePipeline {
            inner: Arc::new(ComputePipelineInner {
                owner,
                raw,
                label: descriptor.label.map(str::to_owned),
            }),
        })
    }
}

fn create_compute_pipeline(
    owner: &Arc<DeviceOwner>,
    cache: Option<&PipelineCache>,
    stage: vk::PipelineShaderStageCreateInfo,
) -> Result<vk::Pipeline> {
    let mut flags = vk::PipelineCreateFlags2CreateInfo::builder()
        .flags(vk::PipelineCreateFlags2::DESCRIPTOR_HEAP_EXT)
        .build();
    let create = vk::ComputePipelineCreateInfo::builder()
        .stage(stage)
        .layout(vk::PipelineLayout::null())
        .push_next(&mut flags)
        .build();
    let operation = |cache| unsafe {
        owner
            .device
            .create_compute_pipelines(cache, &[create], None)
    };
    let (mut pipelines, _) = match cache {
        Some(cache) => cache.with_raw(operation),
        None => operation(vk::PipelineCache::null()),
    }
    .map_err(|source| Error::vulkan("vkCreateComputePipelines", source))?;
    pipelines
        .pop()
        .ok_or_else(|| Error::Validation("Vulkan returned no compute pipeline".into()))
}
