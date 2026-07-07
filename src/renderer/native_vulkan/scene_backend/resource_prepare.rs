//! Scene mesh resource prepare command recording.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{SceneFramePlan, SceneGraphDrawFamilyPlan, SceneResource};
use crate::renderer::native_vulkan::vulkan::NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot;

use super::draw_family::{
    NativeVulkanSceneDrawFamilyExecutorPlan, native_vulkan_require_scene_mesh_executor_families,
};
use super::frame_completion::NativeVulkanSceneFrameSubmission;
use super::frame_resources::NativeVulkanSceneFrameResources;
use super::resource_heap::NativeVulkanSceneResourceHeapFramePlan;
use super::texture_descriptors::NativeVulkanSceneTextureDescriptorFramePlan;

pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMeshResourcePrepareContext<'a> {
    pub device: &'a Device,
    pub memory_properties: &'a vk::PhysicalDeviceMemoryProperties,
    pub descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    pub command_buffer: vk::CommandBuffer,
    pub frame_submission: NativeVulkanSceneFrameSubmission,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMeshResourcePreparePlan {
    pub draw_family_executor: NativeVulkanSceneDrawFamilyExecutorPlan,
    pub residency_command_count: usize,
    pub material_uniform_gpu_buffer_action_count: usize,
    pub texture_descriptors: NativeVulkanSceneTextureDescriptorFramePlan,
    pub resource_heap: NativeVulkanSceneResourceHeapFramePlan,
    pub resource_heap_action_count: usize,
    pub texture_image_action_count: usize,
    pub gpu_buffer_action_count: usize,
    pub command_order: [&'static str; 7],
}

impl NativeVulkanSceneMeshResourcePreparePlan {
    pub(in crate::renderer::native_vulkan) fn from_parts(
        draw_family_executor: NativeVulkanSceneDrawFamilyExecutorPlan,
        residency_command_count: usize,
        material_uniform_gpu_buffer_action_count: usize,
        texture_descriptors: NativeVulkanSceneTextureDescriptorFramePlan,
        resource_heap: NativeVulkanSceneResourceHeapFramePlan,
        resource_heap_action_count: usize,
        texture_image_action_count: usize,
        gpu_buffer_action_count: usize,
    ) -> Self {
        Self {
            draw_family_executor,
            residency_command_count,
            material_uniform_gpu_buffer_action_count,
            texture_descriptors,
            resource_heap,
            resource_heap_action_count,
            texture_image_action_count,
            gpu_buffer_action_count,
            command_order: [
                "select_scene_draw_family_executors",
                "sync_residency",
                "record_material_uniform_buffer_uploads",
                "prepare_texture_descriptors",
                "record_texture_image_uploads",
                "sync_draw_resource_heap",
                "record_gpu_buffer_uploads",
            ],
        }
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_mesh_resource_prepare_frame(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: NativeVulkanSceneMeshResourcePrepareContext<'_>,
    resources: &[SceneResource],
    frame: &SceneFramePlan,
) -> Result<NativeVulkanSceneMeshResourcePreparePlan, String> {
    let draw_family_executor = native_vulkan_require_scene_mesh_executor_families(
        &SceneGraphDrawFamilyPlan::from_graph(&frame.graph),
    )?;
    let residency_command_count = frame_resources.sync_residency_plan(&frame.residency).len();
    let material_uniform_gpu_buffer_action_count = frame_resources
        .sync_material_uniform_gpu_buffers_recorded(
            context.device,
            context.memory_properties,
            context.command_buffer,
            context.frame_submission,
            &frame.graph,
        )?
        .len();
    let texture_descriptors = frame_resources.texture_descriptor_frame_plan(&frame.graph)?;
    let texture_image_action_count = frame_resources
        .sync_texture_images_recorded(
            context.device,
            context.memory_properties,
            context.command_buffer,
            context.frame_submission,
            resources,
        )?
        .len();
    let resource_heap_action_count = frame_resources
        .sync_draw_resource_heap(
            context.device,
            context.memory_properties,
            &frame.graph,
            &texture_descriptors,
            context.descriptor_heap_properties,
        )?
        .len();
    let resource_heap = frame_resources
        .current_resource_heap_frame_plan()
        .ok_or_else(|| {
            "scene mesh resource prepare missing draw resource heap frame plan".to_owned()
        })?
        .clone();
    let gpu_buffer_action_count = frame_resources
        .sync_gpu_uploads_recorded(
            context.device,
            context.memory_properties,
            context.command_buffer,
            context.frame_submission,
            resources,
        )?
        .len();

    Ok(NativeVulkanSceneMeshResourcePreparePlan::from_parts(
        draw_family_executor,
        residency_command_count,
        material_uniform_gpu_buffer_action_count,
        texture_descriptors,
        resource_heap,
        resource_heap_action_count,
        texture_image_action_count,
        gpu_buffer_action_count,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGraph, SceneGraphDraw, SceneGraphDrawFamilyPlan, SceneGraphPass,
        SceneGraphPipelineClass, SceneGraphResourceBinding, SceneGraphResourceRole,
        SceneGraphTarget, SceneMaterialKey, SceneObjectId, SceneResourceId,
    };
    use crate::renderer::native_vulkan::scene_backend::material_uniforms::{
        NativeVulkanSceneMaterialUniformGpuBufferBinding, NativeVulkanSceneMaterialUniformKey,
    };
    use crate::renderer::native_vulkan::scene_backend::offscreen_targets::NativeVulkanSceneOffscreenTargetBinding;
    use crate::renderer::native_vulkan::scene_backend::resource_heap::NativeVulkanSceneResourceHeapFramePlan;
    use crate::renderer::native_vulkan::scene_backend::texture_descriptors::{
        NativeVulkanSceneTextureDescriptorBinding, NativeVulkanSceneTextureDescriptorFormat,
        NativeVulkanSceneTextureDescriptorFramePlan, NativeVulkanSceneTextureDescriptorSource,
    };
    use crate::renderer::native_vulkan::scene_backend::texture_images::NativeVulkanSceneTextureImageBinding;
    use crate::renderer::native_vulkan::vulkan::NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot;

    #[test]
    fn resource_prepare_plan_preserves_upload_before_heap_before_gpu_buffer_order() {
        let graph = mesh_graph();
        let texture_descriptors = texture_descriptor_plan();
        let resource_heap = NativeVulkanSceneResourceHeapFramePlan::from_graph(
            &graph,
            &texture_descriptors,
            descriptor_heap_properties(),
            material_binding,
            texture_binding,
            target_binding,
        )
        .expect("resource heap frame plan");

        let plan = NativeVulkanSceneMeshResourcePreparePlan::from_parts(
            native_vulkan_require_scene_mesh_executor_families(
                &SceneGraphDrawFamilyPlan::from_graph(&graph),
            )
            .expect("mesh family executor"),
            2,
            3,
            texture_descriptors,
            resource_heap,
            4,
            5,
            6,
        );

        assert_eq!(
            plan.command_order,
            [
                "select_scene_draw_family_executors",
                "sync_residency",
                "record_material_uniform_buffer_uploads",
                "prepare_texture_descriptors",
                "record_texture_image_uploads",
                "sync_draw_resource_heap",
                "record_gpu_buffer_uploads"
            ]
        );
        assert_eq!(plan.draw_family_executor.missing_executor_draw_count, 0);
        assert_eq!(plan.residency_command_count, 2);
        assert_eq!(plan.material_uniform_gpu_buffer_action_count, 3);
        assert_eq!(plan.resource_heap_action_count, 4);
        assert_eq!(plan.texture_image_action_count, 5);
        assert_eq!(plan.gpu_buffer_action_count, 6);
    }

    fn mesh_graph() -> SceneGraph {
        SceneGraph {
            passes: vec![SceneGraphPass {
                name: "scene-main".to_owned(),
                input: None,
                output: SceneGraphTarget::Swapchain,
                draws: vec![SceneGraphDraw {
                    object: SceneObjectId(1),
                    pipeline: SceneGraphPipelineClass::Mesh,
                    material: SceneMaterialKey {
                        shader: "we/genericimage4".to_owned(),
                        blend: SceneBlendContract::TranslucentAlpha,
                        render_state:
                            crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(),
                    },
                    geometry: None,
                    puppet: None,
                    resources: vec![SceneGraphResourceBinding {
                        slot: 0,
                        role: SceneGraphResourceRole::shader_texture(0),
                        resource: SceneResourceId(7),
                    }],
                    index_count: 6,
                }],
            }],
        }
    }

    fn texture_descriptor_plan() -> NativeVulkanSceneTextureDescriptorFramePlan {
        NativeVulkanSceneTextureDescriptorFramePlan {
            draw_count: 1,
            binding_count: 1,
            bindings: vec![NativeVulkanSceneTextureDescriptorBinding {
                draw_index: 0,
                object: SceneObjectId(1),
                slot: 0,
                role: SceneGraphResourceRole::shader_texture(0),
                source: NativeVulkanSceneTextureDescriptorSource::ResidentTexture(SceneResourceId(
                    7,
                )),
                width: 64,
                height: 64,
                format: NativeVulkanSceneTextureDescriptorFormat::SceneTexture(
                    crate::engine::scene_engine::SceneTextureFormat::R8G8B8A8Unorm,
                ),
                mip_count: 1,
                payload_bytes: Some(16_384),
                shader_mapping: "set0.binding0.g_Texture0".to_owned(),
            }],
            descriptor_model: "VK_EXT_descriptor_heap",
            command_order: [
                "resolve_resident_texture_descriptors",
                "bind_descriptor_heap_texture_mapping",
            ],
        }
    }

    fn descriptor_heap_properties() -> NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
        NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
            resource_heap_alignment: 64,
            sampler_heap_alignment: 32,
            max_resource_heap_size: 4096,
            min_resource_heap_reserved_range: 96,
            max_sampler_heap_size: 4096,
            min_sampler_heap_reserved_range: 48,
            image_descriptor_size: 24,
            image_descriptor_alignment: 32,
            buffer_descriptor_size: 16,
            buffer_descriptor_alignment: 16,
            sampler_descriptor_size: 12,
            sampler_descriptor_alignment: 16,
            ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
        }
    }

    fn material_binding(
        key: &NativeVulkanSceneMaterialUniformKey,
    ) -> Result<NativeVulkanSceneMaterialUniformGpuBufferBinding, String> {
        Ok(NativeVulkanSceneMaterialUniformGpuBufferBinding {
            key: key.clone(),
            buffer: vk::Buffer::from_raw(11),
            device_address: 0x1000,
            record_index: 0,
            bytes: 48,
            payload_hash: 99,
        })
    }

    fn texture_binding(
        resource: SceneResourceId,
    ) -> Result<NativeVulkanSceneTextureImageBinding, String> {
        Ok(NativeVulkanSceneTextureImageBinding {
            resource,
            image: vk::Image::from_raw(21),
            view: vk::ImageView::from_raw(22),
            sampler: vk::Sampler::from_raw(23),
            format: vk::Format::R8G8B8A8_UNORM,
            width: 64,
            height: 64,
            mip_count: 1,
        })
    }

    fn target_binding(
        target: SceneGraphTarget,
    ) -> Result<NativeVulkanSceneOffscreenTargetBinding, String> {
        Err(format!("unexpected graph target input {target:?}"))
    }
}
