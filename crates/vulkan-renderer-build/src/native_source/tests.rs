use super::*;
use std::fs;

#[test]
fn lowers_a_graphics_pair_to_native_slang_without_frontend_syntax() {
    let source = r#"#version 450
layout(location = 0) in vec2 a_Position;
layout(location = 1) out vec2 v_Uv;
layout(set = 0, binding = 2) uniform Draw { vec4 rows[4]; } u_Draw;
void main() { v_Uv = a_Position; gl_Position = vec4(a_Position + u_Draw.rows[0].xy, 0.0, 1.0); }"#;
    let lowered = lower_generated_stage_to_native_slang(source, ShaderStage::Vertex).unwrap();

    assert!(lowered.contains("[[shader(\"vertex\")]]"));
    assert!(lowered.contains("cbuffer Draw_Buffer : register(b2)"));
    assert!(lowered.contains("[[vk::location(0)]] vec2 a_Position"));
    assert!(lowered.contains("vec2 v_Uv : TEXCOORD1"));
    assert!(!lowered.contains("#version"));
    assert!(!lowered.contains("layout("));
    assert!(!lowered.contains("gl_Position"));
}

#[test]
fn lowers_fixed_array_stage_io_and_preserves_contiguous_locations() {
    let source = r#"layout(location = 0) in vec3 a_Position;
layout(location = 0) out vec4 audioValue[16];
layout(location = 16) out vec2 v_TexCoord;
void main() {
    audioValue[0] = vec4(1.0);
    audioValue[15] = vec4(0.0);
    v_TexCoord = a_Position.xy;
    gl_Position = vec4(a_Position, 1.0);
}"#;
    let lowered = lower_generated_stage_to_native_slang(source, ShaderStage::Vertex)
        .expect("fixed array stage-I/O lowering");

    assert!(lowered.contains("static vec4 audioValue[16];"));
    assert!(lowered.contains("[[vk::location(0)]] vec4 audioValue[16] : TEXCOORD0;"));
    assert!(lowered.contains("[[vk::location(16)]] vec2 v_TexCoord : TEXCOORD16;"));
    assert!(lowered.contains("output.audioValue[0] = audioValue[0];"));
    assert!(lowered.contains("output.audioValue[15] = audioValue[15];"));
}

#[test]
fn production_spirv_reflects_fixed_array_stage_io_location_span() {
    let direct = lower_generated_stage_to_native_slang(
        r#"layout(location = 0) in vec3 a_Position;
layout(location = 0) out vec4 audioValue[16];
layout(location = 16) out vec2 v_TexCoord;
layout(location = 17) out vec3 v_PerspCoord;
layout(location = 18) out vec3 v_ViewCoord;
uniform float g_Time;
void main() {
    audioValue[0] = vec4(g_Time);
    audioValue[15] = vec4(0.0);
    v_TexCoord = a_Position.xy;
    v_PerspCoord = vec3(a_Position.xy, 1.0);
    v_ViewCoord = a_Position;
    gl_Position = vec4(a_Position, 1.0);
}"#,
        ShaderStage::Vertex,
    )
    .expect("fixed array direct native Slang");
    let heap = crate::lower_slang_bindings_to_descriptor_heap(&direct, "main")
        .expect("fixed array descriptor-heap lowering");
    let base = std::env::temp_dir().join(format!(
        "vulkan-renderer-build-array-stage-io-{}",
        std::process::id()
    ));
    let source_path = base.with_extension("slang");
    let output_path = base.with_extension("spv");
    fs::write(&source_path, heap.source).unwrap();
    let report = crate::SlangCompiler::from_environment()
        .compile(&crate::ShaderCompileRequest {
            source: source_path.clone(),
            entry_point: "main".to_owned(),
            stage: ShaderStage::Vertex,
            output: output_path.clone(),
            contract: crate::ShaderContract::descriptor_heap(u64::from(heap.push_constant_bytes)),
        })
        .expect("fixed array production O2 compile");
    let interface =
        crate::reflect_shader_interface(&report.reflection, "main", ShaderStage::Vertex)
            .expect("fixed array typed reflection");
    let outputs = interface
        .stage_io
        .iter()
        .filter(|item| item.direction == crate::ShaderIoDirection::Output)
        .map(|item| (item.name.as_str(), item.location, item.location_count))
        .collect::<Vec<_>>();

    assert_eq!(
        outputs,
        vec![
            ("audioValue", 0, 16),
            ("v_TexCoord", 16, 1),
            ("v_PerspCoord", 17, 1),
            ("v_ViewCoord", 18, 1),
        ]
    );

    let _ = fs::remove_file(source_path);
    let _ = fs::remove_file(output_path);
}

#[test]
fn rejects_non_fixed_or_nested_interface_array_declarators() {
    for declarator in ["values[]", "values[0]", "values[COUNT]", "values[2][2]"] {
        let source = format!("layout(location = 0) out vec4 {declarator};\nvoid main() {{}}");
        let error = lower_generated_stage_to_native_slang(&source, ShaderStage::Vertex)
            .expect_err("invalid interface array must fail strictly");
        assert!(
            error.contains("invalid generated interface array declarator"),
            "unexpected error for {declarator}: {error}"
        );
    }
}

#[test]
fn rejects_location_overlap_within_an_interface_array_span() {
    let source = r#"layout(location = 0) out vec4 audioValue[16];
layout(location = 15) out vec2 v_TexCoord;
void main() { gl_Position = vec4(0.0); }"#;
    let error = lower_generated_stage_to_native_slang(source, ShaderStage::Vertex)
        .expect_err("array location overlap must fail strictly");

    assert_eq!(error, "duplicate generated output location 15");
}

#[test]
fn lowers_combined_samplers_and_fragment_outputs() {
    let source = r#"#version 450
layout(location = 0) in vec2 v_Uv;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
void main() { o_Color = texture2D(g_Texture0, v_Uv); }"#;
    let lowered = lower_generated_stage_to_native_slang(source, ShaderStage::Fragment).unwrap();

    assert!(lowered.contains("Texture2D<float4> g_Texture0_texture : register(t0)"));
    assert!(lowered.contains("SamplerState g_Texture0_sampler : register(s0)"));
    assert!(lowered.contains("[[shader(\"fragment\")]]"));
    assert!(!lowered.contains("sampler2D"));
    assert!(lowered.contains("S ## _texture.Sample(S ## _sampler, UV)"));
    assert!(lowered.contains("int2 gilderTextureSize_g_Texture0(uint mip)"));
    assert!(lowered.contains("g_Texture0_texture.GetDimensions"));

    let heap = crate::lower_slang_bindings_to_descriptor_heap(&lowered, "main").unwrap();
    assert!(
        heap.source
            .contains("gilderHeap_ ## S ## _texture().Sample(gilderHeap_ ## S ## _sampler(), UV)")
    );
    assert!(
        heap.source
            .contains("gilderHeap_g_Texture0_texture().GetDimensions")
    );
}

#[test]
fn packs_active_implicit_combined_samplers_by_declaration_order() {
    let source = r#"layout(location = 0) in vec2 v_Uv;
layout(location = 0) out vec4 o_Color;
uniform sampler2D g_Texture0;
uniform sampler2D g_Texture1;
uniform sampler2D g_Texture2;
void main() { o_Color = texture2D(g_Texture0, v_Uv) + texture2D(g_Texture2, v_Uv); }"#;
    let lowered = lower_generated_stage_to_native_slang(source, ShaderStage::Fragment).unwrap();

    assert!(lowered.contains("g_Texture0_texture : register(t0)"));
    assert!(lowered.contains("g_Texture2_texture : register(t1)"));
    assert!(!lowered.contains("g_Texture1_texture"));
    assert!(!lowered.contains("g_Texture2_texture : register(t2)"));
}

#[test]
fn preserves_authored_material_binding_thirty_five() {
    let source = r#"layout(location = 0) in vec2 v_Uv;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 35) uniform Material { vec4 color; } u_Material;
void main() { o_Color = u_Material.color; }"#;
    let lowered = lower_generated_stage_to_native_slang(source, ShaderStage::Fragment).unwrap();

    assert!(lowered.contains("cbuffer Material_Buffer : register(b35)"));
}

#[test]
fn lowers_early_main_return_after_writing_fragment_output() {
    let source = r#"layout(location = 0) out vec4 o_Color;
void main() {
    o_Color = vec4(0.0);
    return;
}"#;
    let lowered = lower_generated_stage_to_native_slang(source, ShaderStage::Fragment).unwrap();

    assert!(lowered.contains("GilderFragmentOutput output;"));
    assert!(lowered.contains("output.o_Color = o_Color;\n    return output;"));
}

#[test]
fn lowers_we_row_vector_matrix_mul_to_uploaded_row_dot_semantics() {
    let source = r#"layout(location = 0) in vec3 a_Position;
layout(set = 0, binding = 0) uniform mat4 g_ModelViewProjectionMatrix;
void main() {
    gl_Position = mul(vec4(a_Position, 1.0), g_ModelViewProjectionMatrix);
}"#;
    let lowered = lower_generated_stage_to_native_slang(source, ShaderStage::Vertex).unwrap();

    assert!(lowered.contains("mul(g_ModelViewProjectionMatrix, vec4(a_Position, 1.0))"));
    assert!(!lowered.contains("mul(vec4(a_Position, 1.0), g_ModelViewProjectionMatrix)"));
}

#[test]
fn production_spirv_uses_every_uploaded_matrix_row_for_clip_xyzw() {
    const OP_VECTOR_TIMES_MATRIX: u32 = 144;
    const OP_MATRIX_TIMES_VECTOR: u32 = 145;

    let direct = lower_generated_stage_to_native_slang(
        r#"layout(location = 0) in vec3 a_Position;
uniform mat4 g_ModelViewProjectionMatrix;
void main() {
    gl_Position = mul(vec4(a_Position, 1.0), g_ModelViewProjectionMatrix);
}"#,
        ShaderStage::Vertex,
    )
    .unwrap();
    let heap = crate::lower_slang_bindings_to_descriptor_heap(&direct, "main").unwrap();
    let base = std::env::temp_dir().join(format!(
        "vulkan-renderer-build-row-dot-matrix-{}",
        std::process::id()
    ));
    let source_path = base.with_extension("slang");
    let output_path = base.with_extension("spv");
    fs::write(&source_path, heap.source).unwrap();
    crate::SlangCompiler::from_environment()
        .compile(&crate::ShaderCompileRequest {
            source: source_path.clone(),
            entry_point: "main".to_owned(),
            stage: ShaderStage::Vertex,
            output: output_path.clone(),
            contract: crate::ShaderContract::descriptor_heap(u64::from(heap.push_constant_bytes)),
        })
        .unwrap();

    let bytes = fs::read(&output_path).unwrap();
    let opcodes = spirv_opcodes(&bytes);
    assert_eq!(
        opcodes
            .iter()
            .filter(|opcode| matches!(**opcode, OP_VECTOR_TIMES_MATRIX | OP_MATRIX_TIMES_VECTOR))
            .copied()
            .collect::<Vec<_>>(),
        vec![OP_VECTOR_TIMES_MATRIX]
    );

    let uploaded_rows = [
        [2.0, 3.0, 5.0, 7.0],
        [11.0, 13.0, 17.0, 19.0],
        [23.0, 29.0, 31.0, 37.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let point = [0.25, -0.5, 2.0, 1.0];
    let clip = evaluate_uploaded_matrix_op(OP_VECTOR_TIMES_MATRIX, uploaded_rows, point);
    assert_eq!(clip[3], 1.0, "translation entered clip.w");
    assert_eq!(clip[0], 16.0);
    assert_eq!(clip[1], 49.25);
    assert_eq!(clip[2], 90.25);
    let old_clip = evaluate_uploaded_matrix_op(OP_MATRIX_TIMES_VECTOR, uploaded_rows, point);
    assert_eq!(old_clip[3], 67.25);
    assert_ne!(old_clip[3], clip[3]);

    let _ = fs::remove_file(source_path);
    let _ = fs::remove_file(output_path);
}

fn spirv_opcodes(bytes: &[u8]) -> Vec<u32> {
    let words = bytes
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
        .collect::<Vec<_>>();
    let mut opcodes = Vec::new();
    let mut offset = 5;
    while offset < words.len() {
        let word_count = (words[offset] >> 16) as usize;
        assert_ne!(word_count, 0);
        opcodes.push(words[offset] & 0xffff);
        offset += word_count;
    }
    assert_eq!(offset, words.len());
    opcodes
}

fn evaluate_uploaded_matrix_op(opcode: u32, rows: [[f32; 4]; 4], point: [f32; 4]) -> [f32; 4] {
    match opcode {
        144 => rows.map(|row| dot4(row, point)),
        145 => std::array::from_fn(|column| {
            dot4(
                [
                    rows[0][column],
                    rows[1][column],
                    rows[2][column],
                    rows[3][column],
                ],
                point,
            )
        }),
        _ => panic!("unexpected matrix-vector opcode {opcode}"),
    }
}

fn dot4(left: [f32; 4], right: [f32; 4]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2] + left[3] * right[3]
}

#[test]
fn wraps_multiple_global_uniforms_as_one_typed_heap_buffer() {
    let source = r#"layout(location = 0) in vec3 a_Position;
uniform mat4 g_ModelViewProjectionMatrix;
uniform float g_Time;
void main() {
    gl_Position = mul(vec4(a_Position, 1.0), g_ModelViewProjectionMatrix);
    gl_Position.x += g_Time;
}"#;
    let direct = lower_generated_stage_to_native_slang(source, ShaderStage::Vertex).unwrap();
    let lowered = crate::lower_slang_bindings_to_descriptor_heap(&direct, "main").unwrap();

    assert!(direct.contains("struct GilderUniforms0Data"));
    assert!(direct.contains("GilderUniforms0Data gilderUniforms0;"));
    assert!(direct.contains(
        "#define g_ModelViewProjectionMatrix gilderUniforms0.g_ModelViewProjectionMatrix"
    ));
    assert!(direct.contains("#define g_Time gilderUniforms0.g_Time"));
    assert!(
        lowered
            .source
            .contains("DescriptorHandle<ConstantBuffer<GilderUniforms0Data>>")
    );
    assert!(
        lowered
            .source
            .contains("#define g_Time gilderHeap_gilderUniforms0().g_Time")
    );
    assert!(!lowered.source.contains(": register("));
}

#[test]
fn prunes_unused_declared_resources_before_native_heap_lowering() {
    let source = r#"layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 2) uniform Draw { vec4 value; } u_Draw;
void main() { o_Color = vec4(1.0); }"#;
    let lowered = lower_generated_stage_to_native_slang(source, ShaderStage::Fragment).unwrap();

    assert!(!lowered.contains(": register("));
}
