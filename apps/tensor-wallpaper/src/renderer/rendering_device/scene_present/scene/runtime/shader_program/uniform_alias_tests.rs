use super::*;
use crate::engine::scene::{
    SceneBinaryDocument, SceneShaderBindingRecord, SceneShaderUniformBufferRecord,
    SceneShaderUniformMemberRecord,
};

#[test]
fn rounded_mask_uses_all_authored_aliases_and_pipeline_global_push_offsets() {
    let storage = rounded_mask_storage();
    let vertex = storage
        .shader_program(SceneStringId(0), SceneShaderStage::Vertex)
        .expect("vertex");
    let fragment = storage
        .shader_program(SceneStringId(0), SceneShaderStage::Fragment)
        .expect("fragment");
    let vertex = scene_owned_stage_resource_plan(&storage, vertex).expect("vertex plan");
    let fragment = scene_owned_stage_resource_plan(&storage, fragment).expect("fragment plan");

    assert_eq!(vertex.push_constant_bytes, 4);
    assert_eq!(fragment.push_constant_bytes, 16);
    assert_eq!(
        fragment
            .bindings
            .iter()
            .map(|binding| binding.push_offset)
            .collect::<Vec<_>>(),
        [4, 8, 12]
    );
    assert_eq!(
        vertex
            .uniform_buffers
            .iter()
            .flat_map(|buffer| buffer.members.iter().map(|member| member.source))
            .collect::<Vec<_>>(),
        [
            SceneOwnedUniformSource::ModelViewProjectionMatrix,
            SceneOwnedUniformSource::LayerModelMatrix,
            material("Size"),
            material("offset"),
            material("scale"),
            material("angle"),
            SceneOwnedUniformSource::SampledTextureResolution { slot: 0 },
        ]
    );
    assert_eq!(
        fragment
            .uniform_buffers
            .iter()
            .flat_map(|buffer| buffer.members.iter().map(|member| member.source))
            .collect::<Vec<_>>(),
        [
            material("Color"),
            SceneOwnedUniformSource::SampledTextureResolution { slot: 0 },
            material("Radius"),
            material("Border width"),
            material("Softness"),
            material("ui_editor_properties_opacity"),
        ]
    );
}

fn material(name: &str) -> SceneOwnedUniformSource<'_> {
    SceneOwnedUniformSource::MaterialParameter {
        authored_name: name,
    }
}

fn rounded_mask_storage() -> SceneStorage {
    let spirv = descriptor_heap_spirv();
    SceneStorage::from_document(SceneBinaryDocument {
        strings: strings(),
        shader_programs: vec![
            program(SceneShaderStage::Vertex, 0, 1, 0, 4, spirv.len()),
            program(SceneShaderStage::Fragment, 1, 3, 1, 16, spirv.len()),
        ],
        shader_bindings: vec![
            binding(SceneShaderBindingKind::UniformBuffer, 0, 0),
            binding(SceneShaderBindingKind::SampledImage, 0, 4),
            binding(SceneShaderBindingKind::Sampler, 0, 8),
            binding(SceneShaderBindingKind::UniformBuffer, 0, 12),
        ],
        shader_uniform_buffers: vec![
            SceneShaderUniformBufferRecord {
                name: SceneStringId(2),
                register: 0,
                byte_size: 176,
                member_start: 0,
                member_count: 7,
            },
            SceneShaderUniformBufferRecord {
                name: SceneStringId(2),
                register: 0,
                byte_size: 48,
                member_start: 7,
                member_count: 6,
            },
        ],
        shader_uniform_members: uniform_members(),
        shader_spirv: spirv,
        ..SceneBinaryDocument::default()
    })
    .expect("rounded-mask storage")
}

fn strings() -> Vec<String> {
    [
        "workshop/example/effects/rounded_mask__SLOTS_1",
        "main",
        "GlobalParams",
        "g_ModelViewProjectionMatrix",
        "g_LayerModelMatrix",
        "u_Size",
        "g_Offset",
        "g_Scale",
        "g_Direction",
        "g_Texture0Resolution",
        "u_Color",
        "u_Radius",
        "u_BorderWidth",
        "u_Softness",
        "u_Alpha",
        "Size",
        "offset",
        "scale",
        "angle",
        "Color",
        "Radius",
        "Border width",
        "Softness",
        "ui_editor_properties_opacity",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn uniform_members() -> Vec<SceneShaderUniformMemberRecord> {
    vec![
        member(3, None, 0, 64, 4, 4, 16),
        member(4, None, 64, 64, 4, 4, 16),
        member(5, Some(15), 128, 8, 2, 1, 0),
        member(6, Some(16), 136, 8, 2, 1, 0),
        member(7, Some(17), 144, 8, 2, 1, 0),
        member(8, Some(18), 152, 4, 1, 1, 0),
        member(9, None, 160, 16, 4, 1, 0),
        member(10, Some(19), 0, 12, 3, 1, 0),
        member(9, None, 16, 16, 4, 1, 0),
        member(11, Some(20), 32, 4, 1, 1, 0),
        member(12, Some(21), 36, 4, 1, 1, 0),
        member(13, Some(22), 40, 4, 1, 1, 0),
        member(14, Some(23), 44, 4, 1, 1, 0),
    ]
}

fn member(
    name: u32,
    material_parameter: Option<u32>,
    byte_offset: u32,
    byte_size: u32,
    rows: u32,
    columns: u32,
    matrix_stride: u32,
) -> SceneShaderUniformMemberRecord {
    SceneShaderUniformMemberRecord {
        name: SceneStringId(name),
        material_parameter: material_parameter.map_or(SceneStringId::NONE, SceneStringId),
        byte_offset,
        byte_size,
        scalar_type: SceneShaderScalarType::F32,
        rows,
        columns,
        array_count: 1,
        array_stride: 0,
        matrix_stride,
    }
}

fn program(
    stage: SceneShaderStage,
    binding_start: u32,
    binding_count: u32,
    uniform_buffer_start: u32,
    push_constant_bytes: u32,
    spirv_count: usize,
) -> SceneShaderProgramRecord {
    SceneShaderProgramRecord {
        program_key: SceneStringId(0),
        stage,
        entry_point: SceneStringId(1),
        spirv_start: 0,
        spirv_count: spirv_count as u32,
        binding_start,
        binding_count,
        stage_io_start: 0,
        stage_io_count: 0,
        uniform_buffer_start,
        uniform_buffer_count: 1,
        push_constant_bytes,
    }
}

fn binding(
    kind: SceneShaderBindingKind,
    register: u32,
    push_offset: u32,
) -> SceneShaderBindingRecord {
    SceneShaderBindingRecord {
        kind,
        register,
        descriptor_count: 1,
        push_offset,
    }
}

fn descriptor_heap_spirv() -> Vec<u32> {
    let mut words = vec![0x0723_0203, 0x0001_0600, 0, 2, 0];
    words.extend(spirv_instruction(17, &[5_128]));
    words.extend(spirv_string_instruction(10, "SPV_EXT_descriptor_heap"));
    words
}

fn spirv_instruction(opcode: u32, operands: &[u32]) -> Vec<u32> {
    let word_count = u32::try_from(operands.len() + 1).expect("instruction word count");
    std::iter::once((word_count << 16) | opcode)
        .chain(operands.iter().copied())
        .collect()
}

fn spirv_string_instruction(opcode: u32, value: &str) -> Vec<u32> {
    let mut bytes = value.as_bytes().to_vec();
    bytes.push(0);
    bytes.resize(bytes.len().next_multiple_of(4), 0);
    let operands = bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes(chunk.try_into().expect("SPIR-V word")))
        .collect::<Vec<_>>();
    spirv_instruction(opcode, &operands)
}
