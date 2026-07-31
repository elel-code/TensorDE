use std::fmt;
use std::sync::{Arc, Mutex};

use vulkanalia::{prelude::v1_4::*, vk};

use crate::backend::DeviceOwner;
use crate::{Backend, Error, Result};

mod binary;
mod binary_cache;
mod compute;
mod graphics;
mod mapping;

pub use binary::{
    MachineCodeComputePipeline, MachineCodeGraphicsPipeline, MachineCodePipeline,
    PipelineBinaryArchive, PipelineBinaryBlob, PipelineBinaryCreation, compute_pipeline_binary_key,
    create_compute_pipeline_machine_code, create_graphics_pipeline_machine_code,
    graphics_pipeline_binary_key,
};
pub use binary_cache::{PipelineBinaryArchiveCache, PipelineBinaryCacheIdentity};
pub use compute::{ComputePipeline, ComputePipelineDescriptor};

pub use graphics::{
    BlendComponent, BlendState, ColorTargetState, DepthBiasState, DepthState, DepthStencilState,
    FragmentState, GraphicsPipeline, GraphicsPipelineDescriptor, MultisampleState, PrimitiveState,
    ProgrammableStage, StencilState, VertexAttribute, VertexBufferLayout, VertexState,
    VertexStepMode,
};
pub(crate) use graphics::{format_has_depth, format_has_stencil};
pub use mapping::{
    ConstantOffsetMapping, IndirectIndexMapping, PushIndexMapping, ShaderBindingMap,
    ShaderBindingMapError, ShaderBindingMapping, ShaderBindingSource,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PipelineCacheDescriptor {
    pub label: Option<String>,
    /// Opaque bytes previously returned by [`PipelineCache::data`].
    pub initial_data: Vec<u8>,
}

/// Host-synchronized Vulkan pipeline cache with independently retained device
/// ownership.
pub struct PipelineCache {
    owner: Arc<DeviceOwner>,
    raw: vk::PipelineCache,
    label: Option<String>,
    host_lock: Mutex<()>,
}

impl fmt::Debug for PipelineCache {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PipelineCache")
            .field("raw", &self.raw)
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

impl Backend {
    pub fn create_pipeline_cache(
        &self,
        descriptor: &PipelineCacheDescriptor,
    ) -> Result<PipelineCache> {
        let create = vk::PipelineCacheCreateInfo::builder().initial_data(&descriptor.initial_data);
        let owner = self.shared_owner();
        let raw = unsafe { owner.device.create_pipeline_cache(&create, None) }
            .map_err(|source| Error::vulkan("vkCreatePipelineCache", source))?;
        Ok(PipelineCache {
            owner,
            raw,
            label: descriptor.label.clone(),
            host_lock: Mutex::new(()),
        })
    }
}

impl PipelineCache {
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Serializes host access while a pipeline-creation call uses this cache.
    pub fn with_raw<T>(&self, operation: impl FnOnce(vk::PipelineCache) -> T) -> T {
        let _guard = self
            .host_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        operation(self.raw)
    }

    /// Returns implementation-owned opaque cache bytes suitable for a later
    /// `PipelineCacheDescriptor::initial_data` on a compatible device/driver.
    pub fn data(&self) -> Result<Vec<u8>> {
        self.with_raw(|raw| unsafe { self.owner.device.get_pipeline_cache_data(raw) })
            .map_err(|source| Error::vulkan("vkGetPipelineCacheData", source))
    }

    pub(crate) fn belongs_to(&self, owner: &Arc<DeviceOwner>) -> bool {
        Arc::ptr_eq(&self.owner, owner)
    }
}

impl Drop for PipelineCache {
    fn drop(&mut self) {
        unsafe { self.owner.device.destroy_pipeline_cache(self.raw, None) };
    }
}
