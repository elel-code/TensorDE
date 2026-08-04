use super::*;
use crate::engine::scene::{
    SceneBinaryDocument, SceneColorWriteMask, SceneCompositeBlend, SceneCullMode, SceneDepthTest,
    SceneMaterialHandle, SceneObjectHandle, SceneRenderEffectVisibilityPolicy,
    SceneRenderGraphActivationPolicy, SceneRenderPassDrawPrimitive, SceneRenderPassKind,
    SceneRenderPassRecord, SceneRenderTargetKind, SceneRenderingDeviceGraphPlan,
    SceneRenderingDevicePassNode, SceneShaderBindingKind, SceneShaderBindingRecord,
    SceneShaderContractRecord, SceneShaderProgramRecord, SceneShaderScalarType,
    SceneShaderUniformBufferRecord, SceneShaderUniformMemberRecord,
};

#[test]
fn owned_program_reserves_typed_uniforms_without_catalog_lookup() {
    let storage = owned_storage();
    let graph = graph();

    let layout = scene_pipeline_descriptor_layout(&storage, &graph).expect("owned layout");

    assert!(!layout.material_uniform_enabled);
    assert_eq!(layout.scene_owned_uniform_count, 2);
    assert_eq!(layout.scene_owned_uniform_resource_offset(), 1);
    assert_eq!(layout.sampled_resource_offset(), 3);
    assert_eq!(layout.per_draw_resource_count(), 4);
}

fn owned_storage() -> SceneStorage {
    let spirv = descriptor_heap_spirv();
    SceneStorage::from_document(SceneBinaryDocument {
        strings: [
            "workshop/example/effects/owned__SLOTS_1",
            "pipeline",
            "main",
            "GlobalParams",
            "vertexValue",
            "fragmentValue",
            "vertex material",
            "fragment material",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        shader_contracts: vec![SceneShaderContractRecord {
            shader_key: SceneStringId(0),
            pipeline_key: SceneStringId(1),
            texture_slot_mask: 1,
            input_attachment_slot_mask: 0,
            constant_start: 0,
            constant_count: 0,
            resource_heap_count: 2,
            sampler_heap_count: 1,
        }],
        render_passes: vec![render_pass()],
        shader_programs: vec![
            program(SceneShaderStage::Vertex, 0, 0, spirv.len()),
            program(SceneShaderStage::Fragment, 1, 1, spirv.len()),
        ],
        shader_bindings: vec![
            SceneShaderBindingRecord {
                kind: SceneShaderBindingKind::UniformBuffer,
                register: 0,
                descriptor_count: 1,
                push_offset: 0,
            },
            SceneShaderBindingRecord {
                kind: SceneShaderBindingKind::UniformBuffer,
                register: 0,
                descriptor_count: 1,
                push_offset: 4,
            },
        ],
        shader_uniform_buffers: vec![uniform_buffer(0), uniform_buffer(1)],
        shader_uniform_members: vec![uniform_member(4, 6), uniform_member(5, 7)],
        shader_spirv: spirv,
        ..SceneBinaryDocument::default()
    })
    .expect("owned storage")
}

fn program(
    stage: SceneShaderStage,
    binding_start: u32,
    uniform_buffer_start: u32,
    spirv_count: usize,
) -> SceneShaderProgramRecord {
    SceneShaderProgramRecord {
        program_key: SceneStringId(0),
        stage,
        entry_point: SceneStringId(2),
        spirv_start: 0,
        spirv_count: spirv_count as u32,
        binding_start,
        binding_count: 1,
        stage_io_start: 0,
        stage_io_count: 0,
        uniform_buffer_start,
        uniform_buffer_count: 1,
        push_constant_bytes: (binding_start + 1) * 4,
    }
}

fn uniform_buffer(member_start: u32) -> SceneShaderUniformBufferRecord {
    SceneShaderUniformBufferRecord {
        name: SceneStringId(3),
        register: 0,
        byte_size: 16,
        member_start,
        member_count: 1,
    }
}

fn uniform_member(name: u32, material: u32) -> SceneShaderUniformMemberRecord {
    SceneShaderUniformMemberRecord {
        name: SceneStringId(name),
        material_parameter: SceneStringId(material),
        byte_offset: 0,
        byte_size: 4,
        scalar_type: SceneShaderScalarType::F32,
        rows: 1,
        columns: 1,
        array_count: 1,
        array_stride: 0,
        matrix_stride: 0,
    }
}

fn render_pass() -> SceneRenderPassRecord {
    SceneRenderPassRecord {
        id: 0,
        role: SceneRenderPassKind::EffectMaterial,
        draw_primitive: SceneRenderPassDrawPrimitive::FullscreenTriangle,
        object: SceneObjectHandle(crate::engine::scene::INVALID_OBJECT_ID),
        material: SceneMaterialHandle(crate::engine::scene::INVALID_MATERIAL_ID),
        pass_index: 0,
        shader_key: SceneStringId(0),
        target: SceneRenderTargetKind::SceneColor,
        target_name: SceneStringId::NONE,
        binding_start: 0,
        binding_count: 0,
        effect_binding_start: u32::MAX,
        effect_binding_count: 0,
        effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
        pipeline_blend: crate::engine::scene::ScenePipelineBlend::Normal,
        scene_blend: SceneCompositeBlend::Alpha,
        depth_test: SceneDepthTest::Disabled,
        depth_write: false,
        cull_mode: SceneCullMode::None,
        color_write_mask: SceneColorWriteMask::Rgba,
        clear_target: false,
    }
}

fn graph() -> SceneRenderingDeviceGraphPlan {
    SceneRenderingDeviceGraphPlan {
        pass_nodes: vec![SceneRenderingDevicePassNode {
            graph_index: 0,
            graph_activation_policy: SceneRenderGraphActivationPolicy::Always,
            pass_record_index: 0,
            pass_id: 0,
            role: SceneRenderPassKind::EffectMaterial,
            target: SceneRenderTargetKind::SceneColor,
            target_name: SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            effect_binding_start: u32::MAX,
            effect_binding_count: 0,
            effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
            mesh_draw_start: 0,
            mesh_draw_count: 1,
        }],
        mesh_draws: Vec::new(),
        target_allocations: Vec::new(),
        effect_batches: Vec::new(),
        effect_batch_instances: Vec::new(),
        sampled_bindings: Vec::new(),
        material_sampled_bindings: Vec::new(),
        puppet_bone_palettes: Vec::new(),
        puppet_bone_matrices: Vec::new(),
        particle_gpu_emitters: Vec::new(),
        resolved_object_count: 0,
        resolved_visible_object_count: 0,
        resolved_attachment_link_count: 0,
        resolved_visible_effect_instance_count: 0,
        resolved_visible_effect_pass_count: 0,
        resolved_visible_effect_fbo_count: 0,
        descriptor_heap_required: true,
        descriptor_heap_resource_count: 0,
        descriptor_heap_sampled_image_count: 0,
        descriptor_heap_uniform_buffer_count: 0,
        descriptor_heap_storage_buffer_count: 0,
        descriptor_heap_sampler_count: 0,
        graph_physical_target_count: 0,
        graph_aliased_target_count: 0,
        fifo_latest_ready_present_required: true,
    }
}

fn descriptor_heap_spirv() -> Vec<u32> {
    let mut words = vec![0x0723_0203, 0x0001_0600, 0, 2, 0];
    words.extend(spirv_instruction(17, &[5_128]));
    words.extend(spirv_string_instruction(10, "SPV_EXT_descriptor_heap"));
    words
}

fn spirv_instruction(opcode: u32, operands: &[u32]) -> Vec<u32> {
    let count = u32::try_from(operands.len() + 1).unwrap();
    std::iter::once((count << 16) | opcode)
        .chain(operands.iter().copied())
        .collect()
}

fn spirv_string_instruction(opcode: u32, value: &str) -> Vec<u32> {
    let mut bytes = value.as_bytes().to_vec();
    bytes.push(0);
    bytes.resize(bytes.len().next_multiple_of(4), 0);
    let operands = bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
        .collect::<Vec<_>>();
    spirv_instruction(opcode, &operands)
}
