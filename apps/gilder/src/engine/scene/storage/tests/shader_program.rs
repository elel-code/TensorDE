use super::*;

#[test]
fn accepts_native_heap_spirv_and_exposes_borrowed_program_slices() {
    let document = native_uniform_document();
    let spirv = document.shader_spirv.clone();

    let storage = SceneStorage::from_document(document).expect("native heap SPIR-V storage");
    let program = &storage.shader_programs()[0];
    assert_eq!(
        storage.shader_program(SceneStringId(0), SceneShaderStage::Fragment),
        Some(program)
    );
    assert_eq!(
        storage.shader_program(SceneStringId(0), SceneShaderStage::Vertex),
        None
    );
    assert_eq!(storage.shader_program_spirv(program), spirv);
    assert_eq!(storage.shader_program_bindings(program).len(), 1);
    let buffers = storage.shader_program_uniform_buffers(program);
    assert_eq!(buffers.len(), 1);
    assert_eq!(storage.shader_uniform_buffer_members(&buffers[0]).len(), 1);
}

#[test]
fn rejects_uniform_binding_without_typed_buffer() {
    let mut document = native_uniform_document();
    document.shader_programs[0].uniform_buffer_count = 0;

    assert!(matches!(
        SceneStorage::from_document(document),
        Err(SceneStorageError::InvalidShaderProgram {
            reason: "shader uniform-buffer bindings do not match typed reflection",
            ..
        })
    ));
}

#[test]
fn rejects_uniform_member_outside_buffer() {
    let mut document = native_uniform_document();
    document.shader_uniform_buffers[0].byte_size = 2;

    assert!(matches!(
        SceneStorage::from_document(document),
        Err(SceneStorageError::InvalidShaderProgram {
            reason: "shader uniform member exceeds its buffer",
            ..
        })
    ));
}

#[test]
fn rejects_empty_uniform_material_parameter() {
    let mut document = native_uniform_document();
    document.strings.push(String::new());
    document.shader_uniform_members[0].material_parameter = SceneStringId(4);

    assert!(matches!(
        SceneStorage::from_document(document),
        Err(SceneStorageError::InvalidShaderProgram {
            reason: "shader uniform material parameter is empty",
            ..
        })
    ));
}

#[test]
fn rejects_incompatible_fragment_input() {
    let spirv = vec![0x0723_0203, 0x0001_0600, 0, 2, 0];
    let document = SceneBinaryDocument {
        strings: vec!["program".to_owned(), "main".to_owned(), "uv".to_owned()],
        shader_programs: vec![
            program_record(SceneShaderStage::Vertex, 0, spirv.len()),
            program_record(SceneShaderStage::Fragment, 1, spirv.len()),
        ],
        shader_stage_io: vec![
            stage_io(SceneShaderIoDirection::Output, 2),
            stage_io(SceneShaderIoDirection::Input, 3),
        ],
        shader_spirv: spirv,
        ..SceneBinaryDocument::default()
    };

    let error = SceneStorage::from_document(document).expect_err("stage linkage must be strict");
    assert!(matches!(
        &error,
        SceneStorageError::InvalidShaderProgram {
            reason: "fragment input has no compatible vertex output",
            ..
        }
    ));
    let diagnostic = error.to_string();
    assert!(diagnostic.contains("scene shader program 0 (program)"));
    assert!(diagnostic.contains("fragment input uv at location 0 (F32, rows=3"));
    assert!(diagnostic.contains("available vertex outputs: [uv at location 0 (F32, rows=2"));
}

#[test]
fn accepts_fixed_array_stage_io_location_span() {
    let spirv = vec![0x0723_0203, 0x0001_0600, 0, 2, 0];
    let mut item = stage_io(SceneShaderIoDirection::Output, 4);
    item.location_count = 16;
    let document = SceneBinaryDocument {
        strings: vec![
            "program".to_owned(),
            "main".to_owned(),
            "audioValue".to_owned(),
        ],
        shader_programs: vec![program_record(SceneShaderStage::Vertex, 0, spirv.len())],
        shader_stage_io: vec![item],
        shader_spirv: spirv,
        ..SceneBinaryDocument::default()
    };

    SceneStorage::from_document(document).expect("fixed array stage-I/O span");
}

#[test]
fn rejects_stage_io_span_that_is_not_a_whole_array_of_columns() {
    let spirv = vec![0x0723_0203, 0x0001_0600, 0, 2, 0];
    let mut item = stage_io(SceneShaderIoDirection::Output, 4);
    item.columns = 2;
    item.location_count = 3;
    let document = SceneBinaryDocument {
        strings: vec!["program".to_owned(), "main".to_owned(), "values".to_owned()],
        shader_programs: vec![program_record(SceneShaderStage::Vertex, 0, spirv.len())],
        shader_stage_io: vec![item],
        shader_spirv: spirv,
        ..SceneBinaryDocument::default()
    };

    assert!(matches!(
        SceneStorage::from_document(document),
        Err(SceneStorageError::InvalidShaderProgram {
            reason: "shader stage-I/O shape is invalid",
            ..
        })
    ));
}

#[test]
fn rejects_stage_io_location_overlap_inside_an_array_span() {
    let spirv = vec![0x0723_0203, 0x0001_0600, 0, 2, 0];
    let mut array = stage_io(SceneShaderIoDirection::Output, 4);
    array.location_count = 16;
    let mut tail = stage_io(SceneShaderIoDirection::Output, 2);
    tail.name = SceneStringId(3);
    tail.location = 15;
    let mut program = program_record(SceneShaderStage::Vertex, 0, spirv.len());
    program.stage_io_count = 2;
    let document = SceneBinaryDocument {
        strings: vec![
            "program".to_owned(),
            "main".to_owned(),
            "audioValue".to_owned(),
            "uv".to_owned(),
        ],
        shader_programs: vec![program],
        shader_stage_io: vec![array, tail],
        shader_spirv: spirv,
        ..SceneBinaryDocument::default()
    };

    assert!(matches!(
        SceneStorage::from_document(document),
        Err(SceneStorageError::InvalidShaderProgram {
            reason: "shader stage-I/O locations overlap",
            ..
        })
    ));
}

#[test]
fn rejects_scene_spirv_with_legacy_descriptor_decorations() {
    let mut spirv = vec![0x0723_0203, 0x0001_0600, 0, 2, 0];
    spirv.extend(spirv_instruction(71, &[1, 33, 0]));
    let document = SceneBinaryDocument {
        strings: vec!["program".to_owned(), "main".to_owned()],
        shader_programs: vec![SceneShaderProgramRecord {
            program_key: SceneStringId(0),
            stage: SceneShaderStage::Fragment,
            entry_point: SceneStringId(1),
            spirv_start: 0,
            spirv_count: spirv.len() as u32,
            binding_start: 0,
            binding_count: 0,
            stage_io_start: 0,
            stage_io_count: 0,
            uniform_buffer_start: 0,
            uniform_buffer_count: 0,
            push_constant_bytes: 0,
        }],
        shader_spirv: spirv,
        ..SceneBinaryDocument::default()
    };

    assert!(matches!(
        SceneStorage::from_document(document),
        Err(SceneStorageError::InvalidShaderProgram {
            reason: "SPIR-V contains legacy descriptor decorations",
            ..
        })
    ));
}

fn program_record(
    stage: SceneShaderStage,
    io_start: u32,
    spirv_words: usize,
) -> SceneShaderProgramRecord {
    SceneShaderProgramRecord {
        program_key: SceneStringId(0),
        stage,
        entry_point: SceneStringId(1),
        spirv_start: 0,
        spirv_count: spirv_words as u32,
        binding_start: 0,
        binding_count: 0,
        stage_io_start: io_start,
        stage_io_count: 1,
        uniform_buffer_start: 0,
        uniform_buffer_count: 0,
        push_constant_bytes: 0,
    }
}

fn stage_io(direction: SceneShaderIoDirection, rows: u32) -> SceneShaderStageIoRecord {
    SceneShaderStageIoRecord {
        name: SceneStringId(2),
        direction,
        location: 0,
        scalar_type: SceneShaderScalarType::F32,
        rows,
        columns: 1,
        location_count: 1,
    }
}

fn native_uniform_document() -> SceneBinaryDocument {
    let mut spirv = vec![0x0723_0203, 0x0001_0600, 0, 2, 0];
    spirv.extend(spirv_instruction(17, &[5_128]));
    spirv.extend(spirv_string_instruction(10, "SPV_EXT_descriptor_heap"));
    SceneBinaryDocument {
        strings: vec![
            "workshop/example/effects/wave__SLOTS_1".to_owned(),
            "main".to_owned(),
            "GlobalParams".to_owned(),
            "g_Time".to_owned(),
        ],
        shader_programs: vec![SceneShaderProgramRecord {
            program_key: SceneStringId(0),
            stage: SceneShaderStage::Fragment,
            entry_point: SceneStringId(1),
            spirv_start: 0,
            spirv_count: spirv.len() as u32,
            binding_start: 0,
            binding_count: 1,
            stage_io_start: 0,
            stage_io_count: 0,
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
        shader_uniform_buffers: vec![SceneShaderUniformBufferRecord {
            name: SceneStringId(2),
            register: 0,
            byte_size: 16,
            member_start: 0,
            member_count: 1,
        }],
        shader_uniform_members: vec![SceneShaderUniformMemberRecord {
            name: SceneStringId(3),
            material_parameter: SceneStringId::NONE,
            byte_offset: 0,
            byte_size: 4,
            scalar_type: SceneShaderScalarType::F32,
            rows: 1,
            columns: 1,
            array_count: 1,
            array_stride: 0,
            matrix_stride: 0,
        }],
        shader_spirv: spirv,
        ..SceneBinaryDocument::default()
    }
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
