use super::*;

#[test]
fn scene_binary_round_trip_preserves_only_spirv_and_compact_shader_abi() {
    let document = SceneBinaryDocument {
        strings: vec![
            "workshop/example/effects/wave__SLOTS_1".to_owned(),
            "main".to_owned(),
            "v_TexCoord".to_owned(),
            "fragColor".to_owned(),
            "GlobalParams".to_owned(),
            "g_Time".to_owned(),
            "time".to_owned(),
        ],
        shader_programs: vec![SceneShaderProgramRecord {
            program_key: SceneStringId(0),
            stage: SceneShaderStage::Fragment,
            entry_point: SceneStringId(1),
            spirv_start: 0,
            spirv_count: 6,
            binding_start: 0,
            binding_count: 1,
            stage_io_start: 0,
            stage_io_count: 2,
            uniform_buffer_start: 0,
            uniform_buffer_count: 1,
            push_constant_bytes: 4,
        }],
        shader_bindings: vec![SceneShaderBindingRecord {
            kind: SceneShaderBindingKind::UniformBuffer,
            register: 0,
            descriptor_count: 1,
            push_offset: 0,
        }],
        shader_stage_io: vec![
            SceneShaderStageIoRecord {
                name: SceneStringId(2),
                direction: SceneShaderIoDirection::Input,
                location: 0,
                scalar_type: SceneShaderScalarType::F32,
                rows: 2,
                columns: 1,
                location_count: 1,
            },
            SceneShaderStageIoRecord {
                name: SceneStringId(3),
                direction: SceneShaderIoDirection::Output,
                location: 0,
                scalar_type: SceneShaderScalarType::F32,
                rows: 4,
                columns: 1,
                location_count: 1,
            },
        ],
        shader_uniform_buffers: vec![SceneShaderUniformBufferRecord {
            name: SceneStringId(4),
            register: 0,
            byte_size: 16,
            member_start: 0,
            member_count: 1,
        }],
        shader_uniform_members: vec![SceneShaderUniformMemberRecord {
            name: SceneStringId(5),
            material_parameter: SceneStringId(6),
            byte_offset: 0,
            byte_size: 4,
            scalar_type: SceneShaderScalarType::F32,
            rows: 1,
            columns: 1,
            array_count: 1,
            array_stride: 0,
            matrix_stride: 0,
        }],
        shader_spirv: vec![0x0723_0203, 0x0001_0600, 0, 2, 0, 0x0001_0000],
        ..SceneBinaryDocument::default()
    };
    let mut bytes = Vec::new();
    write_scene_binary(&document, &mut bytes).expect("write scene");
    let decoded = read_scene_binary_bytes(&bytes).expect("read scene");

    assert_eq!(decoded.shader_programs, document.shader_programs);
    assert_eq!(decoded.shader_bindings, document.shader_bindings);
    assert_eq!(decoded.shader_stage_io, document.shader_stage_io);
    assert_eq!(
        decoded.shader_uniform_buffers,
        document.shader_uniform_buffers
    );
    assert_eq!(
        decoded.shader_uniform_members,
        document.shader_uniform_members
    );
    assert_eq!(decoded.shader_spirv, document.shader_spirv);
    assert!(!bytes.windows(4).any(|bytes| bytes == b"GLSL"));
    assert!(!bytes.windows(5).any(|bytes| bytes == b"Slang"));
}
