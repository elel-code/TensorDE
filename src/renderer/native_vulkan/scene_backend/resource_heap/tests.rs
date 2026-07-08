use std::collections::BTreeMap;

use super::super::material_uniforms::{
    NativeVulkanSceneMaterialUniformGpuBufferBinding, NativeVulkanSceneMaterialUniformKey,
    NativeVulkanSceneMaterialUniformUploadPlan,
};
use super::super::offscreen_targets::NativeVulkanSceneOffscreenTargetBinding;
use super::super::texture_descriptors::{
    NativeVulkanSceneTargetInputTextureDescriptor, NativeVulkanSceneTextureDescriptorFramePlan,
    NativeVulkanSceneTextureDescriptorSource, NativeVulkanSceneTextureDescriptorVkFormat,
};
use super::super::texture_images::NativeVulkanSceneTextureImageBinding;
use super::frame_plan::{
    NativeVulkanSceneResourceHeapEntryRole, NativeVulkanSceneResourceHeapFramePlan,
};
use crate::engine::scene_engine::{
    SceneBlendContract, SceneGeometryId, SceneGraph, SceneGraphDraw, SceneGraphPass,
    SceneGraphPipelineClass, SceneGraphResourceBinding, SceneGraphResourceRole, SceneGraphTarget,
    SceneMaterialKey, SceneObjectId, SceneResourceId, SceneShaderUniformFramePlan,
    SceneTextureFormat, SceneTextureResidency,
};
use crate::renderer::native_vulkan::vulkan::{
    NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
};
use vulkanalia::vk;
use vulkanalia::vk::Handle;

#[test]
fn resource_heap_plan_places_material_uniform_before_textures() {
    let graph = mesh_graph(vec![mesh_draw(
        SceneObjectId(7),
        vec![
            SceneGraphResourceBinding {
                slot: 0,
                role: SceneGraphResourceRole::shader_texture(0),
                resource: SceneResourceId(3),
            },
            SceneGraphResourceBinding {
                slot: 4,
                role: SceneGraphResourceRole::shader_texture(4),
                resource: SceneResourceId(5),
            },
        ],
    )]);
    let material_bindings = material_bindings(&graph);
    let texture_plan = texture_plan(&graph);

    let plan = NativeVulkanSceneResourceHeapFramePlan::from_graph(
        &graph,
        &texture_plan,
        descriptor_heap_properties(),
        |key| material_binding(&material_bindings, key),
        texture_binding,
        target_binding,
    )
    .expect("resource heap frame plan");

    assert_eq!(plan.draw_count, 1);
    assert_eq!(plan.heap_slice_count, 1);
    assert_eq!(plan.resource_descriptor_count, 3);
    assert_eq!(plan.sampler_descriptor_count, 2);
    assert_eq!(
        plan.descriptor_heap_plan.resource_descriptor_kinds,
        vec![
            NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
            NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
            NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
        ]
    );
    assert_eq!(
        plan.entries
            .iter()
            .map(|entry| entry.resource_heap_offset)
            .collect::<Vec<_>>(),
        vec![0, 32, 64]
    );
    assert!(matches!(
        plan.entries[0].role,
        NativeVulkanSceneResourceHeapEntryRole::WePsMaterialConstantsSlot3 { .. }
    ));
    assert!(matches!(
        plan.entries[1].role,
        NativeVulkanSceneResourceHeapEntryRole::WeSampledTexture {
            slot: 0,
            image_handle,
            ..
        } if image_handle == 0x3003
    ));
    assert_eq!(plan.entries[1].sampler_heap_offset, Some(0));
    assert_eq!(plan.entries[2].sampler_heap_offset, Some(16));
    assert_eq!(plan.draw_bindings[0].base_resource_heap_offset, 0);
    assert_eq!(plan.draw_bindings[0].base_sampler_heap_offset, Some(0));
    assert_eq!(plan.draw_bindings[0].resource_descriptor_count, 3);
    assert_eq!(plan.draw_bindings[0].texture_count, 2);
    assert_eq!(plan.bindings.len(), 3);
}

#[test]
fn resource_heap_plan_dedupes_identical_draw_heap_slices() {
    let graph = mesh_graph(vec![
        mesh_draw(
            SceneObjectId(7),
            vec![SceneGraphResourceBinding {
                slot: 0,
                role: SceneGraphResourceRole::shader_texture(0),
                resource: SceneResourceId(3),
            }],
        ),
        mesh_draw(
            SceneObjectId(7),
            vec![SceneGraphResourceBinding {
                slot: 0,
                role: SceneGraphResourceRole::shader_texture(0),
                resource: SceneResourceId(3),
            }],
        ),
    ]);
    let material_bindings = material_bindings(&graph);
    let texture_plan = texture_plan(&graph);

    let plan = NativeVulkanSceneResourceHeapFramePlan::from_graph(
        &graph,
        &texture_plan,
        descriptor_heap_properties(),
        |key| material_binding(&material_bindings, key),
        texture_binding,
        target_binding,
    )
    .expect("resource heap frame plan");

    assert_eq!(plan.draw_binding_count, 2);
    assert_eq!(plan.heap_slice_count, 1);
    assert_eq!(plan.resource_descriptor_count, 2);
    assert_eq!(plan.draw_bindings[0].heap_slice_index, 0);
    assert_eq!(plan.draw_bindings[1].heap_slice_index, 0);
}

#[test]
fn resource_heap_plan_rejects_mismatched_draw_count() {
    let graph = mesh_graph(vec![mesh_draw(
        SceneObjectId(7),
        vec![SceneGraphResourceBinding {
            slot: 0,
            role: SceneGraphResourceRole::shader_texture(0),
            resource: SceneResourceId(3),
        }],
    )]);
    let material_bindings = material_bindings(&graph);
    let mut texture_plan = texture_plan(&graph);
    texture_plan.draw_count = 0;

    let err = NativeVulkanSceneResourceHeapFramePlan::from_graph(
        &graph,
        &texture_plan,
        descriptor_heap_properties(),
        |key| material_binding(&material_bindings, key),
        texture_binding,
        target_binding,
    )
    .expect_err("draw count mismatch must fail");

    assert!(err.contains("exceeds draw count"));
}

#[test]
fn resource_heap_plan_requires_retained_material_uniform_gpu_binding() {
    let graph = mesh_graph(vec![mesh_draw(
        SceneObjectId(7),
        vec![SceneGraphResourceBinding {
            slot: 0,
            role: SceneGraphResourceRole::shader_texture(0),
            resource: SceneResourceId(3),
        }],
    )]);
    let texture_plan = texture_plan(&graph);

    let err = NativeVulkanSceneResourceHeapFramePlan::from_graph(
        &graph,
        &texture_plan,
        descriptor_heap_properties(),
        |key| {
            Err(format!(
                "missing retained scene material uniform GPU buffer for {key:?}"
            ))
        },
        texture_binding,
        target_binding,
    )
    .expect_err("missing retained material uniform GPU binding must fail");

    assert!(err.contains("missing retained scene material uniform GPU buffer"));
}

#[test]
fn resource_heap_plan_rejects_zero_material_uniform_device_address() {
    let graph = mesh_graph(vec![mesh_draw(
        SceneObjectId(7),
        vec![SceneGraphResourceBinding {
            slot: 0,
            role: SceneGraphResourceRole::shader_texture(0),
            resource: SceneResourceId(3),
        }],
    )]);
    let mut material_bindings = material_bindings(&graph);
    material_bindings
        .values_mut()
        .next()
        .expect("fake material binding")
        .device_address = 0;
    let texture_plan = texture_plan(&graph);

    let err = NativeVulkanSceneResourceHeapFramePlan::from_graph(
        &graph,
        &texture_plan,
        descriptor_heap_properties(),
        |key| material_binding(&material_bindings, key),
        texture_binding,
        target_binding,
    )
    .expect_err("zero material uniform device address must fail");

    assert!(err.contains("zero device address"));
}

#[test]
fn resource_heap_plan_rejects_texture_image_metadata_mismatch() {
    let graph = mesh_graph(vec![mesh_draw(
        SceneObjectId(7),
        vec![SceneGraphResourceBinding {
            slot: 0,
            role: SceneGraphResourceRole::shader_texture(0),
            resource: SceneResourceId(3),
        }],
    )]);
    let material_bindings = material_bindings(&graph);
    let texture_plan = texture_plan(&graph);

    let err = NativeVulkanSceneResourceHeapFramePlan::from_graph(
        &graph,
        &texture_plan,
        descriptor_heap_properties(),
        |key| material_binding(&material_bindings, key),
        |resource| {
            let mut binding = texture_binding(resource)?;
            binding.width = 2048;
            Ok(binding)
        },
        target_binding,
    )
    .expect_err("texture metadata mismatch must fail");

    assert!(err.contains("descriptor width"));
}

#[test]
fn resource_heap_plan_binds_graph_target_input_as_sampled_image() {
    let graph = SceneGraph {
        passes: vec![
            SceneGraphPass {
                name: "effect-write".to_owned(),
                input: None,
                output: SceneGraphTarget::EffectTarget(0),
                draws: vec![mesh_draw(
                    SceneObjectId(7),
                    vec![SceneGraphResourceBinding {
                        slot: 0,
                        role: SceneGraphResourceRole::shader_texture(0),
                        resource: SceneResourceId(3),
                    }],
                )],
            },
            SceneGraphPass {
                name: "effect-resolve".to_owned(),
                input: Some(SceneGraphTarget::EffectTarget(0)),
                output: SceneGraphTarget::Swapchain,
                draws: vec![mesh_draw(SceneObjectId(8), Vec::new())],
            },
        ],
    };
    let material_bindings = material_bindings(&graph);
    let texture_plan = NativeVulkanSceneTextureDescriptorFramePlan::from_graph_with_target_inputs(
        &graph,
        |resource| {
            Some(SceneTextureResidency {
                id: resource,
                width: Some(1024),
                height: Some(512),
                format: Some(SceneTextureFormat::R8G8B8A8Unorm),
                mip_count: Some(10),
                payload_bytes: Some(2_796_204),
            })
        },
        |target| {
            Ok(NativeVulkanSceneTargetInputTextureDescriptor {
                target,
                width: 3840,
                height: 2160,
                format: NativeVulkanSceneTextureDescriptorVkFormat::R16G16B16A16Sfloat,
            })
        },
    )
    .expect("target input texture descriptor plan");

    let plan = NativeVulkanSceneResourceHeapFramePlan::from_graph(
        &graph,
        &texture_plan,
        descriptor_heap_properties(),
        |key| material_binding(&material_bindings, key),
        texture_binding,
        target_binding,
    )
    .expect("resource heap frame plan with target input");

    assert_eq!(plan.draw_count, 2);
    assert_eq!(plan.heap_slice_count, 2);
    assert_eq!(plan.resource_descriptor_count, 4);
    assert_eq!(plan.sampler_descriptor_count, 2);
    assert_eq!(plan.draw_bindings[1].texture_count, 1);
    assert!(matches!(
        plan.entries[3].role,
        NativeVulkanSceneResourceHeapEntryRole::WeSampledTexture {
            source: NativeVulkanSceneTextureDescriptorSource::GraphTarget(
                SceneGraphTarget::EffectTarget(0)
            ),
            slot: 0,
            image_handle,
            ..
        } if image_handle == 0x9400
    ));
}

#[test]
fn resource_heap_plan_binds_object_final_input_as_texture0_graph_target() {
    let object = SceneObjectId(42);
    let graph = SceneGraph {
        passes: vec![SceneGraphPass {
            name: "scene-object-final-42".to_owned(),
            input: Some(SceneGraphTarget::ObjectFinal(object)),
            output: SceneGraphTarget::Swapchain,
            draws: vec![mesh_draw(object, Vec::new())],
        }],
    };
    let material_bindings = material_bindings(&graph);
    let texture_plan = NativeVulkanSceneTextureDescriptorFramePlan::from_graph_with_target_inputs(
        &graph,
        |_| None,
        |target| {
            Ok(NativeVulkanSceneTargetInputTextureDescriptor {
                target,
                width: 3840,
                height: 2160,
                format: NativeVulkanSceneTextureDescriptorVkFormat::R16G16B16A16Sfloat,
            })
        },
    )
    .expect("object-final input texture descriptor plan");

    let plan = NativeVulkanSceneResourceHeapFramePlan::from_graph(
        &graph,
        &texture_plan,
        descriptor_heap_properties(),
        |key| material_binding(&material_bindings, key),
        texture_binding,
        target_binding,
    )
    .expect("resource heap frame plan with object-final input");

    assert_eq!(plan.draw_count, 1);
    assert_eq!(plan.draw_bindings[0].texture_count, 1);
    assert_eq!(plan.draw_bindings[0].texture_set.slot_mask(), 1);
    assert_eq!(
        plan.draw_bindings[0].shader_mappings,
        vec![
            "WE PSSetConstantBuffers(slot=3) -> draw-heap-slice-offset0".to_owned(),
            "we.texture_slot0.g_Texture0 -> draw-heap-slice-offset1".to_owned()
        ]
    );
    assert!(matches!(
        plan.entries[1].role,
        NativeVulkanSceneResourceHeapEntryRole::WeSampledTexture {
            source: NativeVulkanSceneTextureDescriptorSource::GraphTarget(
                SceneGraphTarget::ObjectFinal(SceneObjectId(42))
            ),
            slot: 0,
            image_handle,
            ..
        } if image_handle == 0x9500 + 42
    ));
}

fn material_bindings(
    graph: &SceneGraph,
) -> BTreeMap<NativeVulkanSceneMaterialUniformKey, NativeVulkanSceneMaterialUniformGpuBufferBinding>
{
    let frame_plan = SceneShaderUniformFramePlan::from_graph(graph).unwrap();
    NativeVulkanSceneMaterialUniformUploadPlan::from_shader_uniform_frame_plan(&frame_plan)
        .unwrap()
        .uploads()
        .iter()
        .enumerate()
        .map(|(index, upload)| {
            (
                upload.key.clone(),
                NativeVulkanSceneMaterialUniformGpuBufferBinding {
                    key: upload.key.clone(),
                    buffer: vk::Buffer::from_raw(0x1000 + index as u64),
                    device_address: 0x0100_0000 + (index as u64 * 0x100),
                    record_index: upload.record_index,
                    bytes: upload.payload.len() as u64,
                    payload_hash: stable_hash(&upload.payload),
                },
            )
        })
        .collect()
}

fn material_binding(
    bindings: &BTreeMap<
        NativeVulkanSceneMaterialUniformKey,
        NativeVulkanSceneMaterialUniformGpuBufferBinding,
    >,
    key: &NativeVulkanSceneMaterialUniformKey,
) -> Result<NativeVulkanSceneMaterialUniformGpuBufferBinding, String> {
    bindings
        .get(key)
        .cloned()
        .ok_or_else(|| format!("missing fake material uniform binding for {key:?}"))
}

fn texture_plan(graph: &SceneGraph) -> NativeVulkanSceneTextureDescriptorFramePlan {
    NativeVulkanSceneTextureDescriptorFramePlan::from_graph(graph, |resource| {
        Some(SceneTextureResidency {
            id: resource,
            width: Some(1024),
            height: Some(512),
            format: Some(SceneTextureFormat::R8G8B8A8Unorm),
            mip_count: Some(10),
            payload_bytes: Some(2_796_204),
        })
    })
    .unwrap()
}

fn texture_binding(
    resource: SceneResourceId,
) -> Result<NativeVulkanSceneTextureImageBinding, String> {
    Ok(NativeVulkanSceneTextureImageBinding {
        resource,
        image: vk::Image::from_raw(0x3000 + u64::from(resource.0)),
        view: vk::ImageView::from_raw(0x4000 + u64::from(resource.0)),
        sampler: vk::Sampler::from_raw(0x5000 + u64::from(resource.0)),
        format: vk::Format::R8G8B8A8_UNORM,
        width: 1024,
        height: 512,
        mip_count: 10,
    })
}

fn target_binding(
    target: SceneGraphTarget,
) -> Result<NativeVulkanSceneOffscreenTargetBinding, String> {
    Ok(NativeVulkanSceneOffscreenTargetBinding {
        target,
        image: vk::Image::from_raw(0x9000 + target_binding_ordinal(target)),
        view: vk::ImageView::from_raw(0xa000 + target_binding_ordinal(target)),
        sampler: vk::Sampler::from_raw(0xb000 + target_binding_ordinal(target)),
        format: vk::Format::R16G16B16A16_SFLOAT,
        width: 3840,
        height: 2160,
        current_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
    })
}

fn target_binding_ordinal(target: SceneGraphTarget) -> u64 {
    match target {
        SceneGraphTarget::Swapchain => 0,
        SceneGraphTarget::ImageLocalMain(index) => 0x100 + u64::from(index),
        SceneGraphTarget::ImageLocalSub(index) => 0x200 + u64::from(index),
        SceneGraphTarget::NamedFbo(index) => 0x300 + u64::from(index),
        SceneGraphTarget::EffectTarget(index) => 0x400 + u64::from(index),
        SceneGraphTarget::ObjectFinal(object) => 0x500 + u64::from(object.0),
        SceneGraphTarget::FullAlphaMask => 0x600,
        SceneGraphTarget::FullAlphaMaskIntermediate => 0x601,
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

fn mesh_graph(draws: Vec<SceneGraphDraw>) -> SceneGraph {
    SceneGraph {
        passes: vec![SceneGraphPass {
            name: "scene-main".to_owned(),
            input: None,
            output: SceneGraphTarget::Swapchain,
            draws,
        }],
    }
}

fn mesh_draw(object: SceneObjectId, resources: Vec<SceneGraphResourceBinding>) -> SceneGraphDraw {
    SceneGraphDraw {
        object,
        pipeline: SceneGraphPipelineClass::Mesh,
        material: SceneMaterialKey {
            shader: "we/genericimage4".to_owned(),
            blend: SceneBlendContract::TranslucentAlpha,
            render_state: crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(),
        },
        geometry: Some(SceneGeometryId(object.0)),
        puppet: None,
        resources,
        index_count: 6,
    }
}

fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}
