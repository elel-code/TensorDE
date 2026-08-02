// Synthetic MDLV0023 fixtures shared by the puppet-ingest contract tests above.

fn test_mdlv0023() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"MDLV0023\0");
    push_u32(&mut bytes, 0x0180_0009);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 1);
    bytes.extend_from_slice(b"materials/puppet.json\0");
    push_u32(&mut bytes, 0);
    for value in [0.0_f32, 0.0, 0.0, 1.0, 1.0, 0.0] {
        push_f32(&mut bytes, value);
    }
    push_u32(&mut bytes, 0x0180_000f);
    let mut vertices = Vec::new();
    push_mdl_vertex(&mut vertices, [0.0, 0.0, 0.0], [0.0, 1.0]);
    push_mdl_vertex(&mut vertices, [1.0, 0.0, 0.0], [1.0, 1.0]);
    push_mdl_vertex(&mut vertices, [1.0, 1.0, 0.0], [1.0, 1.0]);
    push_u32(&mut bytes, vertices.len() as u32);
    bytes.extend_from_slice(&vertices);
    push_u32(&mut bytes, 6);
    for index in [0_u16, 1, 2] {
        bytes.extend_from_slice(&index.to_le_bytes());
    }
    bytes.extend_from_slice(b"MDLS0004");
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 1);
    bytes.extend_from_slice(b"eye-bone\0");
    bytes.extend_from_slice(&0_i32.to_le_bytes());
    push_u32(&mut bytes, u32::MAX);
    push_u32(&mut bytes, 64);
    for index in 0..16 {
        let value = if index == 0 || index == 5 || index == 10 || index == 15 {
            1.0
        } else {
            0.0
        };
        push_f32(&mut bytes, value);
    }
    bytes.extend_from_slice(b"{}\0");
    bytes.extend_from_slice(b"MDLA0006");
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 475);
    push_u32(&mut bytes, 0);
    bytes.extend_from_slice(b"blink\0");
    bytes.extend_from_slice(b"loop\0");
    push_f32(&mut bytes, 30.0);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 72);
    push_transform_sample(
        &mut bytes,
        [1.0, 2.0, 3.0],
        [0.0, 0.0, 0.0],
        [1.0, 1.0, 1.0],
    );
    push_transform_sample(
        &mut bytes,
        [4.0, 5.0, 6.0],
        [0.0, 0.0, 1.0],
        [2.0, 2.0, 2.0],
    );
    bytes.extend_from_slice(b"MDAT0001\0");
    push_u32(&mut bytes, 0);
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(b"eye\0");
    for index in 0..16 {
        let value = if index == 0 || index == 5 || index == 10 || index == 15 {
            1.0
        } else {
            0.0
        };
        push_f32(&mut bytes, value);
    }
    bytes
}

fn test_clipped_mdlv0023() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"MDLV0023\0");
    push_u32(&mut bytes, 0x0180_0009);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 1);
    bytes.extend_from_slice(b"materials/clipped.json\0");
    push_u32(&mut bytes, 0);
    for value in [0.0_f32, 0.0, 0.0, 1.0, 1.0, 0.0] {
        push_f32(&mut bytes, value);
    }
    push_u32(&mut bytes, 0x0180_000f);
    let mut vertices = Vec::new();
    push_mdl_skinned_vertex(&mut vertices, [0.0, 0.0, 0.0], [0.0, 1.0]);
    push_mdl_skinned_vertex(&mut vertices, [1.0, 0.0, 0.0], [1.0, 1.0]);
    push_mdl_skinned_vertex(&mut vertices, [1.0, 1.0, 0.0], [1.0, 0.0]);
    push_u32(&mut bytes, vertices.len() as u32);
    bytes.extend_from_slice(&vertices);
    let indices = [0_u16, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2];
    push_u32(
        &mut bytes,
        (indices.len() * std::mem::size_of::<u16>()) as u32,
    );
    for index in indices {
        bytes.extend_from_slice(&index.to_le_bytes());
    }

    bytes.push(0);
    bytes.push(1);
    push_u32(&mut bytes, 6 * 16);
    for (source_index, index_start) in [(10, 0), (11, 3), (12, 6), (13, 9), (14, 12), (15, 15)] {
        push_u32(&mut bytes, source_index);
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, index_start);
        push_u32(&mut bytes, 3);
    }
    push_u32(&mut bytes, 2);
    bytes.extend_from_slice(&7_u64.to_le_bytes());
    bytes.extend_from_slice(b"masks/eye-clip\0");
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 2);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 1);
    bytes.extend_from_slice(&8_u64.to_le_bytes());
    bytes.extend_from_slice(b"masks/eye-clip\0");
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 4);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 3);

    bytes.extend_from_slice(b"MDLS0004");
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 1);
    bytes.extend_from_slice(b"root\0");
    bytes.extend_from_slice(&0_i32.to_le_bytes());
    push_u32(&mut bytes, u32::MAX);
    push_u32(&mut bytes, 64);
    for index in 0..16 {
        push_f32(
            &mut bytes,
            f32::from(index == 0 || index == 5 || index == 10 || index == 15),
        );
    }
    bytes.extend_from_slice(b"{}\0");
    bytes
}

fn push_mdl_vertex(out: &mut Vec<u8>, position: [f32; 3], uv: [f32; 2]) {
    for value in position {
        push_f32(out, value);
    }
    out.resize(out.len() + 60, 0);
    push_f32(out, uv[0]);
    push_f32(out, uv[1]);
}

fn push_mdl_skinned_vertex(out: &mut Vec<u8>, position: [f32; 3], uv: [f32; 2]) {
    let start = out.len();
    for value in position {
        push_f32(out, value);
    }
    out.resize(start + 40, 0);
    for blend_index in [0_u32; 4] {
        push_u32(out, blend_index);
    }
    for blend_weight in [1.0_f32, 0.0, 0.0, 0.0] {
        push_f32(out, blend_weight);
    }
    push_f32(out, uv[0]);
    push_f32(out, uv[1]);
}

fn push_transform_sample(
    out: &mut Vec<u8>,
    translation: [f32; 3],
    rotation: [f32; 3],
    scale: [f32; 3],
) {
    for value in translation.into_iter().chain(rotation).chain(scale) {
        push_f32(out, value);
    }
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}
