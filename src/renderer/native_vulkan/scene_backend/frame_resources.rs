//! Retained scene frame resource state for Vulkan runtime wiring.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `references/godot/servers/rendering/storage/`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    RenderingDeviceCommand, SceneGeometryId, ScenePuppetId, SceneResource,
    SceneResourceResidencyPlan,
};

use super::pipeline::{
    NativeVulkanScenePipelineBinding, NativeVulkanScenePipelineCacheKey,
    NativeVulkanScenePipelineResources, NativeVulkanScenePipelineStore,
};
use super::pipeline_factory::{
    NativeVulkanSceneMeshPipelineLayoutSpec, NativeVulkanSceneMeshPipelineShaders,
    native_vulkan_create_scene_mesh_pipeline_resources,
};
use super::resource_buffers::{
    NativeVulkanSceneGpuBufferStore, NativeVulkanSceneGpuBufferSyncAction,
    NativeVulkanSceneMeshDrawBuffers, NativeVulkanScenePuppetStorageBuffers,
};
use super::resource_storage::NativeVulkanSceneResourceStorage;
use super::resource_upload::NativeVulkanSceneGpuUploadPlan;

pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneFrameResources {
    resource_storage: NativeVulkanSceneResourceStorage,
    gpu_buffers: NativeVulkanSceneGpuBufferStore,
    pipelines: NativeVulkanScenePipelineStore,
}

impl NativeVulkanSceneFrameResources {
    pub(in crate::renderer::native_vulkan) fn new() -> Self {
        Self {
            resource_storage: NativeVulkanSceneResourceStorage::default(),
            gpu_buffers: NativeVulkanSceneGpuBufferStore::default(),
            pipelines: NativeVulkanScenePipelineStore::default(),
        }
    }

    pub(in crate::renderer::native_vulkan) fn resource_storage(
        &self,
    ) -> &NativeVulkanSceneResourceStorage {
        &self.resource_storage
    }

    pub(in crate::renderer::native_vulkan) fn sync_residency_plan(
        &mut self,
        residency: &SceneResourceResidencyPlan,
    ) -> Vec<RenderingDeviceCommand> {
        self.resource_storage.sync_residency_plan(residency)
    }

    pub(in crate::renderer::native_vulkan) fn gpu_upload_plan(
        &self,
        resources: &[SceneResource],
    ) -> Result<NativeVulkanSceneGpuUploadPlan, String> {
        NativeVulkanSceneGpuUploadPlan::from_resident_resources(&self.resource_storage, resources)
            .map_err(|err| err.to_string())
    }

    pub(in crate::renderer::native_vulkan) fn sync_gpu_uploads(
        &mut self,
        device: &Device,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        resources: &[SceneResource],
    ) -> Result<&[NativeVulkanSceneGpuBufferSyncAction], String> {
        let upload_plan = self.gpu_upload_plan(resources)?;
        self.gpu_buffers.sync_upload_plan(
            device,
            memory_properties,
            command_pool,
            queue,
            upload_plan,
        )
    }

    pub(in crate::renderer::native_vulkan) fn last_gpu_buffer_actions(
        &self,
    ) -> &[NativeVulkanSceneGpuBufferSyncAction] {
        self.gpu_buffers.last_actions()
    }

    pub(in crate::renderer::native_vulkan) fn mesh_draw_buffers(
        &self,
        geometry: SceneGeometryId,
    ) -> Result<NativeVulkanSceneMeshDrawBuffers, String> {
        self.gpu_buffers.mesh_draw_buffers(geometry)
    }

    pub(in crate::renderer::native_vulkan) fn puppet_storage_buffers(
        &self,
        puppet: ScenePuppetId,
    ) -> NativeVulkanScenePuppetStorageBuffers {
        self.gpu_buffers.puppet_storage_buffers(puppet)
    }

    pub(in crate::renderer::native_vulkan) fn resolve_pipeline<CreatePipeline>(
        &mut self,
        key: NativeVulkanScenePipelineCacheKey,
        create_pipeline: CreatePipeline,
    ) -> Result<NativeVulkanScenePipelineBinding, String>
    where
        CreatePipeline: FnOnce(
            &NativeVulkanScenePipelineCacheKey,
        ) -> Result<NativeVulkanScenePipelineResources, String>,
    {
        self.pipelines.resolve_pipeline(key, create_pipeline)
    }

    pub(in crate::renderer::native_vulkan) fn resolve_mesh_pipeline(
        &mut self,
        device: &Device,
        key: NativeVulkanScenePipelineCacheKey,
        shaders: NativeVulkanSceneMeshPipelineShaders<'_>,
        layout: NativeVulkanSceneMeshPipelineLayoutSpec<'_>,
    ) -> Result<NativeVulkanScenePipelineBinding, String> {
        self.pipelines.resolve_pipeline(key, |key| {
            native_vulkan_create_scene_mesh_pipeline_resources(device, key, shaders, layout)
        })
    }

    pub(in crate::renderer::native_vulkan) fn cached_mesh_pipeline(
        &self,
        key: &NativeVulkanScenePipelineCacheKey,
    ) -> Result<NativeVulkanScenePipelineBinding, String> {
        self.pipelines.cached_pipeline(key).ok_or_else(|| {
            format!(
                "missing warmed scene mesh pipeline for shader '{}' format {:?}",
                key.shader, key.target_format
            )
        })
    }

    pub(in crate::renderer::native_vulkan) fn destroy_all(&mut self, device: &Device) {
        self.gpu_buffers.destroy_all(device);
        self.pipelines.destroy_all(device);
    }
}

impl Default for NativeVulkanSceneFrameResources {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::scene::SceneMeshVertex;
    use crate::engine::scene_engine::{
        RenderingDeviceCommand, SceneBlendContract, SceneGeometryId, SceneGraphPipelineClass,
        SceneResource, SceneResourceResidencyPlan,
    };
    use vulkanalia::vk;

    #[test]
    fn frame_resources_sync_residency_before_upload_plan() {
        let resources = vec![mesh_resource(SceneGeometryId(4))];
        let residency = SceneResourceResidencyPlan::from_resources(&resources);
        let mut frame_resources = NativeVulkanSceneFrameResources::new();

        let commands = frame_resources.sync_residency_plan(&residency);
        let upload_plan = frame_resources
            .gpu_upload_plan(&resources)
            .expect("resident mesh upload plan");

        assert!(matches!(
            commands.as_slice(),
            [RenderingDeviceCommand::EnsureMeshGeometryResident {
                geometry: SceneGeometryId(4),
                ..
            }]
        ));
        assert_eq!(upload_plan.uploads().len(), 2);
        assert!(
            frame_resources
                .resource_storage()
                .mesh_geometry(SceneGeometryId(4))
                .is_some()
        );
    }

    #[test]
    fn frame_resources_reuse_unchanged_residency_metadata() {
        let resources = vec![mesh_resource(SceneGeometryId(4))];
        let residency = SceneResourceResidencyPlan::from_resources(&resources);
        let mut frame_resources = NativeVulkanSceneFrameResources::new();

        frame_resources.sync_residency_plan(&residency);
        let commands = frame_resources.sync_residency_plan(&residency);

        assert!(commands.is_empty());
    }

    #[test]
    fn frame_resources_upload_plan_requires_resident_payload() {
        let resources = vec![mesh_resource(SceneGeometryId(4))];
        let residency = SceneResourceResidencyPlan::from_resources(&resources);
        let mut frame_resources = NativeVulkanSceneFrameResources::new();
        frame_resources.sync_residency_plan(&residency);

        let err = frame_resources
            .gpu_upload_plan(&[])
            .expect_err("missing resident payload must fail");

        assert!(err.contains("missing resident scene GPU payload"));
    }

    #[test]
    fn frame_resources_resolve_pipeline_reuses_cached_pipeline() {
        let mut frame_resources = NativeVulkanSceneFrameResources::new();
        let key = pipeline_key();
        let mut creates = 0usize;

        let first = frame_resources
            .resolve_pipeline(key.clone(), |_| {
                creates += 1;
                Ok(pipeline_resources(11, 12))
            })
            .expect("create pipeline");
        let second = frame_resources
            .resolve_pipeline(key, |_| {
                creates += 1;
                Ok(pipeline_resources(21, 22))
            })
            .expect("reuse pipeline");

        assert_eq!(creates, 1);
        assert_eq!(first.pipeline, second.pipeline);
    }

    #[test]
    fn frame_resources_reads_cached_pipeline_after_warmup() {
        let mut frame_resources = NativeVulkanSceneFrameResources::new();
        let key = pipeline_key();
        frame_resources
            .resolve_pipeline(key.clone(), |_| Ok(pipeline_resources(11, 12)))
            .expect("warm pipeline");

        let binding = frame_resources
            .cached_mesh_pipeline(&key)
            .expect("cached mesh pipeline");

        assert_eq!(binding.pipeline, vk::Pipeline::from_raw(11));
        assert_eq!(binding.pipeline_layout, vk::PipelineLayout::from_raw(12));
    }

    fn mesh_resource(geometry: SceneGeometryId) -> SceneResource {
        SceneResource::MeshGeometry {
            id: geometry,
            source_record: 12,
            vertices: vec![SceneMeshVertex::default(); 2],
            indices: vec![0, 1, 0],
        }
    }

    fn pipeline_key() -> NativeVulkanScenePipelineCacheKey {
        NativeVulkanScenePipelineCacheKey {
            shader: "we/genericimage4".to_owned(),
            blend: SceneBlendContract::TranslucentAlpha,
            writes_depth: false,
            tests_depth: false,
            pipeline_class: SceneGraphPipelineClass::Mesh,
            vertex_layout:
                super::super::pipeline::NativeVulkanScenePipelineVertexLayout::SceneMeshV0,
            target_format: vk::Format::B8G8R8A8_UNORM,
        }
    }

    fn pipeline_resources(
        pipeline: u64,
        pipeline_layout: u64,
    ) -> NativeVulkanScenePipelineResources {
        NativeVulkanScenePipelineResources {
            pipeline: vk::Pipeline::from_raw(pipeline),
            pipeline_layout: vk::PipelineLayout::from_raw(pipeline_layout),
        }
    }
}
