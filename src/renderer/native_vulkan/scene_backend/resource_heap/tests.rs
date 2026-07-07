use super::super::material_uniforms::NativeVulkanSceneMaterialUniformUploadPlan;
use super::super::texture_descriptors::NativeVulkanSceneTextureDescriptorFramePlan;
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
    let material_plan = material_plan(&graph);
    let texture_plan = texture_plan(&graph);

    let plan = NativeVulkanSceneResourceHeapFramePlan::from_graph(
        &graph,
        &material_plan,
        &texture_plan,
        descriptor_heap_properties(),
    )
    .expect("resource heap frame plan");

    assert_eq!(plan.draw_count, 1);
    assert_eq!(plan.resource_set_count, 1);
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
        NativeVulkanSceneResourceHeapEntryRole::WeSampledTexture { slot: 0, .. }
    ));
    assert_eq!(plan.entries[1].sampler_heap_offset, Some(0));
    assert_eq!(plan.entries[2].sampler_heap_offset, Some(16));
    assert_eq!(plan.draw_bindings[0].base_resource_heap_offset, 0);
    assert_eq!(plan.draw_bindings[0].resource_descriptor_count, 3);
    assert_eq!(plan.draw_bindings[0].texture_count, 2);
}

#[test]
fn resource_heap_plan_dedupes_identical_draw_resource_sets() {
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
    let material_plan = material_plan(&graph);
    let texture_plan = texture_plan(&graph);

    let plan = NativeVulkanSceneResourceHeapFramePlan::from_graph(
        &graph,
        &material_plan,
        &texture_plan,
        descriptor_heap_properties(),
    )
    .expect("resource heap frame plan");

    assert_eq!(plan.draw_binding_count, 2);
    assert_eq!(plan.resource_set_count, 1);
    assert_eq!(plan.resource_descriptor_count, 2);
    assert_eq!(plan.draw_bindings[0].resource_set_index, 0);
    assert_eq!(plan.draw_bindings[1].resource_set_index, 0);
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
    let material_plan = material_plan(&graph);
    let mut texture_plan = texture_plan(&graph);
    texture_plan.draw_count = 0;

    let err = NativeVulkanSceneResourceHeapFramePlan::from_graph(
        &graph,
        &material_plan,
        &texture_plan,
        descriptor_heap_properties(),
    )
    .expect_err("draw count mismatch must fail");

    assert!(err.contains("exceeds draw count"));
}

fn material_plan(graph: &SceneGraph) -> NativeVulkanSceneMaterialUniformUploadPlan {
    let frame_plan = SceneShaderUniformFramePlan::from_graph(graph).unwrap();
    NativeVulkanSceneMaterialUniformUploadPlan::from_shader_uniform_frame_plan(&frame_plan).unwrap()
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
