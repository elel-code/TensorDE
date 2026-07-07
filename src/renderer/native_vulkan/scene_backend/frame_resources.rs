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
    RenderingDeviceCommand, SceneGeometryId, SceneGraph, ScenePuppetId, SceneResource,
    SceneResourceId, SceneResourceResidencyPlan,
};
use crate::renderer::native_vulkan::vulkan::NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot;

use super::frame_completion::{
    NativeVulkanSceneFrameResourceRelease, NativeVulkanSceneFrameSubmission,
};
use super::material_uniforms::{
    NativeVulkanSceneMaterialUniformCatalog, NativeVulkanSceneMaterialUniformSyncAction,
    NativeVulkanSceneMaterialUniformUploadPlan,
};
use super::pipeline::{
    NativeVulkanScenePipelineBinding, NativeVulkanScenePipelineCacheKey,
    NativeVulkanScenePipelineStore,
};
use super::pipeline_factory::{
    NativeVulkanSceneMeshPipelineLayoutSpec, NativeVulkanSceneMeshPipelineShaders,
    native_vulkan_create_scene_mesh_pipeline_resources,
};
use super::resource_buffers::{
    NativeVulkanSceneGpuBufferStore, NativeVulkanSceneGpuBufferSyncAction,
    NativeVulkanSceneMeshDrawBuffers, NativeVulkanScenePuppetStorageBuffers,
};
use super::resource_heap::NativeVulkanSceneResourceHeapFramePlan;
use super::resource_storage::NativeVulkanSceneResourceStorage;
use super::resource_upload::NativeVulkanSceneGpuUploadPlan;
use super::texture_descriptors::NativeVulkanSceneTextureDescriptorFramePlan;
use super::texture_heap::{
    NativeVulkanSceneTextureHeapDrawBindInfo, NativeVulkanSceneTextureHeapFramePlan,
    NativeVulkanSceneTextureHeapStore, NativeVulkanSceneTextureHeapSyncAction,
    NativeVulkanSceneTextureSetKey,
};
use super::texture_images::{
    NativeVulkanSceneTextureImageBinding, NativeVulkanSceneTextureImageStore,
    NativeVulkanSceneTextureImageSyncAction, NativeVulkanSceneTextureUploadPlan,
};

pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneFrameResources {
    resource_storage: NativeVulkanSceneResourceStorage,
    gpu_buffers: NativeVulkanSceneGpuBufferStore,
    texture_images: NativeVulkanSceneTextureImageStore,
    texture_heap: NativeVulkanSceneTextureHeapStore,
    material_uniforms: NativeVulkanSceneMaterialUniformCatalog,
    pipelines: NativeVulkanScenePipelineStore,
}

impl NativeVulkanSceneFrameResources {
    pub(in crate::renderer::native_vulkan) fn new() -> Self {
        Self {
            resource_storage: NativeVulkanSceneResourceStorage::default(),
            gpu_buffers: NativeVulkanSceneGpuBufferStore::default(),
            texture_images: NativeVulkanSceneTextureImageStore::default(),
            texture_heap: NativeVulkanSceneTextureHeapStore::default(),
            material_uniforms: NativeVulkanSceneMaterialUniformCatalog::default(),
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

    pub(in crate::renderer::native_vulkan) fn sync_gpu_uploads_recorded(
        &mut self,
        device: &Device,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        command_buffer: vk::CommandBuffer,
        frame_submission: NativeVulkanSceneFrameSubmission,
        resources: &[SceneResource],
    ) -> Result<&[NativeVulkanSceneGpuBufferSyncAction], String> {
        let upload_plan = self.gpu_upload_plan(resources)?;
        self.gpu_buffers.sync_upload_plan_recorded(
            device,
            memory_properties,
            command_buffer,
            frame_submission,
            upload_plan,
        )
    }

    pub(in crate::renderer::native_vulkan) fn texture_upload_plan(
        &self,
        resources: &[SceneResource],
    ) -> Result<NativeVulkanSceneTextureUploadPlan, String> {
        NativeVulkanSceneTextureUploadPlan::from_resident_resources(
            &self.resource_storage,
            resources,
        )
    }

    pub(in crate::renderer::native_vulkan) fn sync_texture_images_recorded(
        &mut self,
        device: &Device,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        command_buffer: vk::CommandBuffer,
        frame_submission: NativeVulkanSceneFrameSubmission,
        resources: &[SceneResource],
    ) -> Result<&[NativeVulkanSceneTextureImageSyncAction], String> {
        let upload_plan = self.texture_upload_plan(resources)?;
        self.texture_images.sync_upload_plan_recorded(
            device,
            memory_properties,
            command_buffer,
            frame_submission,
            upload_plan,
        )
    }

    pub(in crate::renderer::native_vulkan) fn release_completed_frame_resources(
        &mut self,
        device: &Device,
        completed_submission: NativeVulkanSceneFrameSubmission,
    ) -> NativeVulkanSceneFrameResourceRelease {
        let gpu_buffers = self
            .gpu_buffers
            .release_completed_uploads(device, completed_submission);
        let texture_release = self
            .texture_images
            .release_completed_uploads(device, completed_submission);
        NativeVulkanSceneFrameResourceRelease {
            completed_submission,
            gpu_buffers,
            texture_images: texture_release.images,
            texture_staging_buffers: texture_release.staging_buffers,
        }
    }

    pub(in crate::renderer::native_vulkan) fn last_texture_image_actions(
        &self,
    ) -> &[NativeVulkanSceneTextureImageSyncAction] {
        self.texture_images.last_actions()
    }

    pub(in crate::renderer::native_vulkan) fn texture_image_binding(
        &self,
        resource: SceneResourceId,
    ) -> Result<NativeVulkanSceneTextureImageBinding, String> {
        self.texture_images.texture_binding(resource)
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

    pub(in crate::renderer::native_vulkan) fn texture_descriptor_frame_plan(
        &self,
        graph: &SceneGraph,
    ) -> Result<NativeVulkanSceneTextureDescriptorFramePlan, String> {
        NativeVulkanSceneTextureDescriptorFramePlan::from_graph(graph, |resource| {
            self.resource_storage.texture(resource).copied()
        })
    }

    pub(in crate::renderer::native_vulkan) fn texture_heap_frame_plan(
        &self,
        descriptors: &NativeVulkanSceneTextureDescriptorFramePlan,
        descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    ) -> Result<NativeVulkanSceneTextureHeapFramePlan, String> {
        NativeVulkanSceneTextureHeapFramePlan::from_texture_descriptors(
            descriptors,
            descriptor_heap_properties,
            |resource| self.texture_image_binding(resource),
        )
    }

    pub(in crate::renderer::native_vulkan) fn sync_texture_descriptor_heap(
        &mut self,
        device: &Device,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
        descriptors: &NativeVulkanSceneTextureDescriptorFramePlan,
    ) -> Result<&[NativeVulkanSceneTextureHeapSyncAction], String> {
        let frame_plan = NativeVulkanSceneTextureHeapFramePlan::from_texture_descriptors(
            descriptors,
            descriptor_heap_properties,
            |resource| self.texture_images.texture_binding(resource),
        )?;
        self.texture_heap
            .sync_frame_plan(device, memory_properties, frame_plan)
    }

    pub(in crate::renderer::native_vulkan) fn current_texture_heap_frame_plan(
        &self,
    ) -> Option<&NativeVulkanSceneTextureHeapFramePlan> {
        self.texture_heap.current_frame_plan()
    }

    pub(in crate::renderer::native_vulkan) fn last_texture_heap_actions(
        &self,
    ) -> &[NativeVulkanSceneTextureHeapSyncAction] {
        self.texture_heap.last_actions()
    }

    pub(in crate::renderer::native_vulkan) fn material_uniform_upload_plan(
        &self,
        graph: &SceneGraph,
    ) -> Result<NativeVulkanSceneMaterialUniformUploadPlan, String> {
        let frame_plan =
            crate::engine::scene_engine::SceneShaderUniformFramePlan::from_graph(graph)?;
        NativeVulkanSceneMaterialUniformUploadPlan::from_shader_uniform_frame_plan(&frame_plan)
            .map_err(|err| err.to_string())
    }

    pub(in crate::renderer::native_vulkan) fn sync_material_uniform_records(
        &mut self,
        graph: &SceneGraph,
    ) -> Result<&[NativeVulkanSceneMaterialUniformSyncAction], String> {
        let upload_plan = self.material_uniform_upload_plan(graph)?;
        self.material_uniforms
            .sync_upload_plan(&upload_plan)
            .map_err(|err| err.to_string())
    }

    pub(in crate::renderer::native_vulkan) fn resource_heap_frame_plan(
        &self,
        graph: &SceneGraph,
        texture_descriptors: &NativeVulkanSceneTextureDescriptorFramePlan,
        descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    ) -> Result<NativeVulkanSceneResourceHeapFramePlan, String> {
        let material_uniforms = self.material_uniform_upload_plan(graph)?;
        NativeVulkanSceneResourceHeapFramePlan::from_graph(
            graph,
            &material_uniforms,
            texture_descriptors,
            descriptor_heap_properties,
        )
    }

    pub(in crate::renderer::native_vulkan) fn last_material_uniform_actions(
        &self,
    ) -> &[NativeVulkanSceneMaterialUniformSyncAction] {
        self.material_uniforms.last_actions()
    }

    pub(in crate::renderer::native_vulkan) fn texture_heap_draw_bind_info_for_set(
        &self,
        texture_set: &NativeVulkanSceneTextureSetKey,
    ) -> Result<NativeVulkanSceneTextureHeapDrawBindInfo, String> {
        self.texture_heap
            .draw_bind_info_for_texture_set(texture_set)
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
        self.texture_heap.destroy_all(device);
        self.texture_images.destroy_all(device);
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
    use super::super::pipeline::NativeVulkanScenePipelineResources;
    use super::*;
    use crate::core::scene::SceneMeshVertex;
    use crate::engine::scene_engine::{
        RenderingDeviceCommand, SceneBlendContract, SceneGeometryId, SceneGraph, SceneGraphDraw,
        SceneGraphPass, SceneGraphPipelineClass, SceneGraphResourceBinding, SceneGraphResourceRole,
        SceneGraphTarget, SceneMaterialKey, SceneObjectId, SceneResource, SceneResourceId,
        SceneResourceResidencyPlan, SceneTextureFormat,
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
            .pipelines
            .resolve_pipeline(key.clone(), |_| {
                creates += 1;
                Ok(pipeline_resources(11, 12))
            })
            .expect("create pipeline");
        let second = frame_resources
            .pipelines
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
            .pipelines
            .resolve_pipeline(key.clone(), |_| Ok(pipeline_resources(11, 12)))
            .expect("warm pipeline");

        let binding = frame_resources
            .cached_mesh_pipeline(&key)
            .expect("cached mesh pipeline");

        assert_eq!(binding.pipeline, vk::Pipeline::from_raw(11));
        assert_eq!(binding.pipeline_layout, vk::PipelineLayout::from_raw(12));
    }

    #[test]
    fn frame_resources_resolves_texture_descriptor_plan_from_residency() {
        let resources = vec![
            mesh_resource(SceneGeometryId(4)),
            SceneResource::Texture {
                id: SceneResourceId(7),
                source: "diffuse.png".into(),
                width: Some(512),
                height: Some(256),
                format: Some(SceneTextureFormat::Bc7UnormBlock),
                mip_count: Some(1),
                payload_bytes: Some(131_072),
            },
        ];
        let residency = SceneResourceResidencyPlan::from_resources(&resources);
        let mut frame_resources = NativeVulkanSceneFrameResources::new();
        frame_resources.sync_residency_plan(&residency);

        let plan = frame_resources
            .texture_descriptor_frame_plan(&texture_graph())
            .expect("texture descriptor frame plan");

        assert_eq!(plan.binding_count, 1);
        assert_eq!(plan.bindings[0].resource, SceneResourceId(7));
        assert_eq!(plan.bindings[0].width, Some(512));
        assert_eq!(
            plan.bindings[0].format,
            Some(SceneTextureFormat::Bc7UnormBlock)
        );
        assert_eq!(plan.bindings[0].payload_bytes, Some(131_072));
        assert_eq!(plan.descriptor_model, "VK_EXT_descriptor_heap");
    }

    #[test]
    fn frame_resources_builds_texture_upload_plan_from_residency() {
        let resources = vec![SceneResource::Texture {
            id: SceneResourceId(7),
            source: "assets/diffuse.gtex".into(),
            width: Some(512),
            height: Some(256),
            format: Some(SceneTextureFormat::Bc7UnormBlock),
            mip_count: Some(1),
            payload_bytes: Some(131_072),
        }];
        let residency = SceneResourceResidencyPlan::from_resources(&resources);
        let mut frame_resources = NativeVulkanSceneFrameResources::new();
        frame_resources.sync_residency_plan(&residency);

        let plan = frame_resources
            .texture_upload_plan(&resources)
            .expect("texture upload plan");

        assert_eq!(plan.uploads().len(), 1);
        assert_eq!(plan.uploads()[0].requirement.resource, SceneResourceId(7));
        assert_eq!(plan.uploads()[0].requirement.mip_count, 1);
        assert_eq!(plan.uploads()[0].requirement.payload_bytes, 131_072);
    }

    fn mesh_resource(geometry: SceneGeometryId) -> SceneResource {
        SceneResource::MeshGeometry {
            id: geometry,
            source_record: 12,
            vertices: vec![SceneMeshVertex::default(); 2],
            indices: vec![0, 1, 0],
        }
    }

    fn texture_graph() -> SceneGraph {
        SceneGraph {
            passes: vec![SceneGraphPass {
                name: "scene-main".to_owned(),
                input: None,
                output: SceneGraphTarget::Swapchain,
                draws: vec![SceneGraphDraw {
                    object: SceneObjectId(4),
                    pipeline: SceneGraphPipelineClass::Mesh,
                    material: SceneMaterialKey {
                        shader: "we/genericimage4".to_owned(),
                        blend: SceneBlendContract::TranslucentAlpha,
                        render_state:
                            crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(),
                    },
                    geometry: Some(SceneGeometryId(4)),
                    puppet: None,
                    resources: vec![SceneGraphResourceBinding {
                        slot: 0,
                        role: SceneGraphResourceRole::shader_texture(0),
                        resource: SceneResourceId(7),
                    }],
                    index_count: 3,
                }],
            }],
        }
    }

    fn pipeline_key() -> NativeVulkanScenePipelineCacheKey {
        NativeVulkanScenePipelineCacheKey {
            shader: "we/genericimage4".to_owned(),
            blend: SceneBlendContract::TranslucentAlpha,
            render_state: crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(),
            pipeline_class: SceneGraphPipelineClass::Mesh,
            vertex_layout:
                super::super::pipeline::NativeVulkanScenePipelineVertexLayout::SceneMeshV0,
            target_format: vk::Format::B8G8R8A8_UNORM,
            texture_slot_mask: 1,
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
