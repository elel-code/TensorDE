use super::super::shader_program::{
    SceneOwnedDescriptorBindingPlan, SceneOwnedStageResourcePlan, SceneOwnedUniformBufferPlan,
};
use super::shared::{DenseHeapPlan, builtin_pipeline_push, scene_owned_pipeline_push};
use super::*;
use crate::engine::scene::SceneRenderingDeviceDrawPrimitive;
use vulkan_renderer::DescriptorSlotKind;

#[test]
fn scene_owned_graphics_push_uses_one_pipeline_global_address_space() {
    let layout = ScenePipelineDescriptorLayout {
        sampled_slots: vec![0],
        input_attachment_slots: Vec::new(),
        material_uniform_enabled: false,
        skinning_storage_enabled: false,
        scene_owned_uniform_count: 2,
    };
    let resource_kinds = [
        DescriptorSlotKind::UniformBuffer,
        DescriptorSlotKind::UniformBuffer,
        DescriptorSlotKind::UniformBuffer,
        DescriptorSlotKind::SampledImage,
    ];
    let plan = dense_plan(&resource_kinds, 1);
    let draw = draw_command();
    let vertex = owned_stage(
        crate::engine::scene::SceneShaderStage::Vertex,
        4,
        vec![owned_binding(
            crate::engine::scene::SceneShaderBindingKind::UniformBuffer,
            0,
            0,
        )],
    );
    let fragment = owned_stage(
        crate::engine::scene::SceneShaderStage::Fragment,
        16,
        vec![
            owned_binding(
                crate::engine::scene::SceneShaderBindingKind::SampledImage,
                0,
                4,
            ),
            owned_binding(crate::engine::scene::SceneShaderBindingKind::Sampler, 0, 8),
            owned_binding(
                crate::engine::scene::SceneShaderBindingKind::UniformBuffer,
                0,
                12,
            ),
        ],
    );

    let push =
        scene_owned_pipeline_push(&vertex, &fragment, &layout, &plan, &draw).expect("owned push");
    assert_eq!(push_words(&push), [1, 3, 0, 2]);
}

#[test]
fn built_in_fragment_push_uses_shared_dense_heap_indices() {
    let layout = ScenePipelineDescriptorLayout {
        sampled_slots: vec![0],
        input_attachment_slots: Vec::new(),
        material_uniform_enabled: true,
        skinning_storage_enabled: false,
        scene_owned_uniform_count: 0,
    };
    let resource_kinds = [
        DescriptorSlotKind::UniformBuffer,
        DescriptorSlotKind::UniformBuffer,
        DescriptorSlotKind::SampledImage,
    ];
    let plan = dense_plan(&resource_kinds, 1);
    let shader = crate::renderer::native_vulkan::scene::native_vulkan_scene_shader_for_key(
        "gilder/dynamic-text",
    )
    .expect("dynamic-text built-in shader");
    let mut draw = draw_command();
    draw.material_resource_descriptor = Some(1);
    draw.sampled_resource_descriptor_base = 2;

    let push = builtin_pipeline_push(shader, shader.vertex, &layout, &plan, &draw)
        .expect("built-in shared pipeline push");
    assert_eq!(push_words(&push), [2, 0, 1, 0]);
}

#[test]
fn built_in_clipping_push_uses_authored_slot_identity_not_declaration_order() {
    let layout = ScenePipelineDescriptorLayout {
        sampled_slots: (0..=8).collect(),
        input_attachment_slots: Vec::new(),
        material_uniform_enabled: true,
        skinning_storage_enabled: true,
        scene_owned_uniform_count: 0,
    };
    let resource_kinds = [
        DescriptorSlotKind::UniformBuffer,
        DescriptorSlotKind::UniformBuffer,
        DescriptorSlotKind::StorageBuffer,
        DescriptorSlotKind::SampledImage,
        DescriptorSlotKind::SampledImage,
        DescriptorSlotKind::SampledImage,
        DescriptorSlotKind::SampledImage,
        DescriptorSlotKind::SampledImage,
        DescriptorSlotKind::SampledImage,
        DescriptorSlotKind::SampledImage,
        DescriptorSlotKind::SampledImage,
        DescriptorSlotKind::SampledImage,
    ];
    let plan = dense_plan(&resource_kinds, 9);
    let shader = crate::renderer::native_vulkan::scene::native_vulkan_scene_shader_for_key(
        "we/puppet-iris-waterripple-clipping-final",
    )
    .expect("puppet iris clipping shader");
    let mut draw = draw_command();
    draw.material_resource_descriptor = Some(1);
    draw.skinning_resource_descriptor = Some(2);
    draw.sampled_resource_descriptor_base = 3;

    let push = builtin_pipeline_push(shader, shader.vertex, &layout, &plan, &draw)
        .expect("built-in clipping push");
    assert_eq!(
        push_words(&push),
        [3, 4, 5, 11, 6, 0, 1, 2, 8, 3, 1, 0, 2]
    );
}

#[test]
fn built_in_local_read_push_uses_the_input_attachment_heap_lane() {
    let layout = ScenePipelineDescriptorLayout {
        sampled_slots: vec![0],
        input_attachment_slots: vec![0],
        material_uniform_enabled: false,
        skinning_storage_enabled: false,
        scene_owned_uniform_count: 0,
    };
    let resource_kinds = [
        DescriptorSlotKind::UniformBuffer,
        DescriptorSlotKind::SampledImage,
        DescriptorSlotKind::InputAttachment,
    ];
    let plan = dense_plan(&resource_kinds, 1);
    let shader =
        crate::renderer::native_vulkan::scene::native_vulkan_scene_shader_for_key("we/passthrough")
            .expect("passthrough built-in shader");
    let mut draw = draw_command();
    draw.sampled_resource_descriptor_base = 1;
    draw.input_attachment_resource_descriptor_base = 2;

    let push = builtin_pipeline_push(shader, shader.vertex, &layout, &plan, &draw)
        .expect("built-in local-read push");
    assert_eq!(push_words(&push), [1, 0, 2]);
}

#[test]
fn effect_passthrough_pipeline_selects_its_independent_shared_push() {
    let mut draw = draw_command();
    draw.pipeline_index = 3;
    draw.authored_pipeline_index = 3;
    draw.disabled_pipeline_index = Some(4);
    draw.native_descriptor_push = Some(SceneNativeDescriptorPush::EngineBuiltIn(vec![1]));
    draw.disabled_native_descriptor_push = Some(SceneNativeDescriptorPush::EngineBuiltIn(vec![2]));

    assert_eq!(draw.active_native_descriptor_push().unwrap().bytes(), &[1]);
    draw.pipeline_index = 4;
    assert_eq!(draw.active_native_descriptor_push().unwrap().bytes(), &[2]);
}

fn dense_plan(resource_kinds: &[DescriptorSlotKind], sampler_count: usize) -> DenseHeapPlan<'_> {
    DenseHeapPlan {
        resource_kinds,
        sampler_count,
    }
}

fn push_words(push: &SceneNativeDescriptorPush) -> Vec<u32> {
    push.bytes()
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
        .collect()
}

fn owned_stage(
    stage: crate::engine::scene::SceneShaderStage,
    push_constant_bytes: u32,
    bindings: Vec<SceneOwnedDescriptorBindingPlan>,
) -> SceneOwnedStageResourcePlan<'static> {
    SceneOwnedStageResourcePlan {
        stage,
        push_constant_bytes,
        bindings,
        uniform_buffers: vec![SceneOwnedUniformBufferPlan {
            name: "GlobalParams",
            register: 0,
            byte_size: 16,
            members: Vec::new(),
        }],
    }
}

fn owned_binding(
    kind: crate::engine::scene::SceneShaderBindingKind,
    register: u32,
    push_offset: u32,
) -> SceneOwnedDescriptorBindingPlan {
    SceneOwnedDescriptorBindingPlan {
        kind,
        register,
        descriptor_count: 1,
        push_offset,
    }
}

fn draw_command() -> SceneGpuDrawCommand {
    SceneGpuDrawCommand {
        enabled: true,
        primitive: SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
        pipeline_index: 0,
        authored_pipeline_index: 0,
        disabled_pipeline_index: None,
        first_index: 0,
        index_count: 0,
        vertex_offset: 0,
        vertex_buffer_byte_offset: None,
        vertex_count: 3,
        instance_count: 1,
        instance_capacity: 1,
        first_instance: 0,
        dynamic_text: false,
        video_media_instance: None,
        video_vertex_byte_offset: None,
        particle_indirect_index: None,
        resource_descriptor_base: 0,
        material_resource_descriptor: None,
        skinning_resource_descriptor: None,
        scene_owned_uniform_descriptor_base: 1,
        sampled_resource_descriptor_base: 3,
        input_attachment_resource_descriptor_base: 4,
        sampler_descriptor_base: 0,
        native_descriptor_push: None,
        disabled_native_descriptor_push: None,
        skinning_byte_offset: 0,
        skinning_byte_count: 0,
        scissor: None,
    }
}
