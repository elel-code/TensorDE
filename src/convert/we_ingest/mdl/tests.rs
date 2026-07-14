use super::*;

#[test]
fn parses_mdlv0023_material_and_mesh_blocks() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"MDLV0023\0");
    push_u32(&mut bytes, 0x0180_0009);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 1);
    bytes.extend_from_slice(b"materials/puppet.json\0");
    push_u32(&mut bytes, 0);
    for value in [-1.0_f32, -2.0, 0.0, 3.0, 4.0, 0.0] {
        push_f32(&mut bytes, value);
    }
    push_u32(&mut bytes, 0x0180_000f);
    let mut vertices = Vec::new();
    push_vertex(
        &mut vertices,
        [-1.0, 0.0, 0.0],
        [0.25, 0.75],
        [3, 2, 1, 0],
        [0.5, 0.25, 0.125, 0.125],
    );
    push_vertex(
        &mut vertices,
        [1.0, 2.0, 0.0],
        [0.5, 0.125],
        [0, 0, 0, 0],
        [1.0, 0.0, 0.0, 0.0],
    );
    push_u32(&mut bytes, vertices.len() as u32);
    bytes.extend_from_slice(&vertices);
    push_u32(&mut bytes, 4);
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());

    let model = parse_mdl_model(&bytes).expect("mdl");

    assert_eq!(model.version, 23);
    assert_eq!(model.material_paths, ["materials/puppet.json"]);
    assert_eq!(model.entries.len(), 1);
    assert_eq!(model.entries[0].entry_layout_mask, 0x0180_000f);
    assert_eq!(model.entries[0].vertices.len(), 2);
    assert_eq!(model.entries[0].vertices[0].position.x, -1.0);
    assert_eq!(model.entries[0].vertices[0].uv, [0.25, 0.75]);
    assert_eq!(model.entries[0].vertices[0].blend_indices, [3, 2, 1, 0]);
    assert_eq!(
        model.entries[0].vertices[0].blend_weights,
        [0.5, 0.25, 0.125, 0.125]
    );
    assert_eq!(model.entries[0].indices, [0, 1]);
}

#[test]
fn parses_mdat_attachment_records() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"MDLV0023\0");
    push_u32(&mut bytes, 0x0180_0009);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 0);
    bytes.extend_from_slice(b"MDAT0001\0");
    push_u32(&mut bytes, 0);
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&41_u16.to_le_bytes());
    bytes.extend_from_slice("eye".as_bytes());
    bytes.push(0);
    for index in 0..16 {
        let value = if index == 0 || index == 5 || index == 10 || index == 15 {
            1.0
        } else {
            0.0
        };
        push_f32(&mut bytes, value);
    }

    let model = parse_mdl_model(&bytes).expect("mdl");

    assert_eq!(model.attachments.len(), 1);
    assert_eq!(model.attachments[0].bone_index, 41);
    assert_eq!(model.attachments[0].name, "eye");
    assert_eq!(model.attachments[0].local_matrix[15], 1.0);
}

#[test]
fn parses_mdls_skeleton_bone_records() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"MDLV0023\0");
    push_u32(&mut bytes, 0x0180_0009);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 0);
    bytes.extend_from_slice(b"MDLS0004");
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 1);
    bytes.extend_from_slice(b"eye-bone\0");
    bytes.extend_from_slice(&3_i32.to_le_bytes());
    push_u32(&mut bytes, u32::MAX);
    push_u32(&mut bytes, 64);
    for index in 0..16 {
        let value = if index == 12 { 17.0 } else { 0.0 };
        push_f32(&mut bytes, value);
    }
    bytes.extend_from_slice(b"{}\0");

    let model = parse_mdl_model(&bytes).expect("mdl");

    assert_eq!(model.bones.len(), 1);
    assert_eq!(model.bones[0].bone_index, 0);
    assert_eq!(model.bones[0].name, "eye-bone");
    assert_eq!(model.bones[0].simulation_type, 3);
    assert_eq!(model.bones[0].parent_index, -1);
    assert_eq!(model.bones[0].local_bind_matrix[12], 17.0);
    assert_eq!(model.bones[0].simulation_json, "{}");
}

#[test]
fn rejects_mdls_bone_count_beyond_remaining_payload() {
    let mut bytes = Vec::new();
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, u32::MAX);

    assert_eq!(
        parse_mdls_bones_from(&bytes, 0),
        Err(MdlParseError::UnexpectedEof("mdls_bone_records"))
    );
}

#[test]
fn parses_mdla_animation_transform_tracks() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"MDLV0023\0");
    push_u32(&mut bytes, 0x0180_0009);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 0);
    bytes.extend_from_slice(b"MDLS0004");
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 1);
    bytes.extend_from_slice(b"eye-bone\0");
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
    bytes.extend_from_slice(b"MDLA0006");
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 2);
    push_u32(&mut bytes, 475);
    push_u32(&mut bytes, 3);
    bytes.extend_from_slice(b"blink\0");
    bytes.extend_from_slice(b"loop\0");
    push_f32(&mut bytes, 30.0);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 99);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 7);
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
    bytes.extend_from_slice(&[0; 5]);
    push_u32(&mut bytes, 9);
    push_u32(&mut bytes, 8);
    push_f32(&mut bytes, 0.25);
    push_f32(&mut bytes, 0.75);
    bytes.extend_from_slice(&[0; 11]);
    push_u32(&mut bytes, 549);
    push_u32(&mut bytes, 0);
    bytes.extend_from_slice(b"wave\0");
    bytes.extend_from_slice(b"loop\0");
    push_f32(&mut bytes, 30.0);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 36);
    push_transform_sample(
        &mut bytes,
        [7.0, 8.0, 9.0],
        [0.0, 0.0, 0.0],
        [1.0, 1.0, 1.0],
    );

    let model = parse_mdl_model(&bytes).expect("mdl");

    assert_eq!(model.animations.len(), 2);
    assert_eq!(model.animations[0].clip_id, 475);
    assert_eq!(model.animations[0].name, "blink");
    assert_eq!(model.animations[0].playback, "loop");
    assert_eq!(model.animations[0].fps, 30.0);
    assert_eq!(model.animations[0].tracks.len(), 1);
    assert_eq!(model.animations[0].tracks[0].bone_index, 0);
    assert_eq!(model.animations[0].tracks[0].track_flags, 7);
    assert_eq!(model.animations[0].tracks[0].samples.len(), 2);
    assert_eq!(model.animations[0].tracks[0].samples[1].translation.x, 4.0);
    assert_eq!(model.animations[0].tracks[0].samples[1].scale.z, 2.0);
    assert_eq!(model.animations[0].tracks[0].opacity_flags, 9);
    assert_eq!(model.animations[0].tracks[0].opacity_samples, [0.25, 0.75]);
    assert_eq!(model.animations[1].clip_id, 549);
    assert_eq!(model.animations[1].tracks[0].samples[0].translation.x, 7.0);
    assert!(model.animations[1].tracks[0].opacity_samples.is_empty());
}

#[test]
fn malformed_present_mdla_section_is_not_silently_discarded() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"MDLV0023\0");
    push_u32(&mut bytes, 0x0180_0009);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 0);
    bytes.extend_from_slice(b"MDLA0006");
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 1);

    assert!(matches!(
        parse_mdl_model(&bytes),
        Err(MdlParseError::UnexpectedEof("mdla_clip_id"))
            | Err(MdlParseError::UnexpectedEof("mdla_clip_records"))
            | Err(MdlParseError::UnexpectedEof("mdla_section_metadata"))
    ));
}

#[test]
fn v23_mask_producer_spans_are_typed_separately_from_visible_indices() {
    let entry = MdlMeshEntry {
        entry_flags: 0,
        entry_layout_mask: 0,
        bounds: [0.0; 6],
        vertices: Vec::new(),
        indices: (0..18).collect(),
        source_records: vec![
            MdlSourceRecord {
                source_index: 7,
                local_index_offset: 0,
                index_start: 0,
                index_count: 6,
            },
            MdlSourceRecord {
                source_index: 8,
                local_index_offset: 0,
                index_start: 6,
                index_count: 6,
            },
        ],
        clipping_subdraws: vec![MdlClippingSubdraw {
            source_qword: 0,
            mask_resource: "masks/clip".to_owned(),
            raw_flags: 0,
            target_source_ordinals: vec![1],
            mask_source_ordinals: vec![0],
        }],
    };

    assert_eq!(
        entry.non_producer_indices(),
        [0, 1, 2, 3, 4, 5, 12, 13, 14, 15, 16, 17]
    );
}

fn push_vertex(
    out: &mut Vec<u8>,
    position: [f32; 3],
    uv: [f32; 2],
    blend_indices: [u32; 4],
    blend_weights: [f32; 4],
) {
    let start = out.len();
    for value in position {
        push_f32(out, value);
    }
    out.resize(start + 40, 0);
    for value in blend_indices {
        push_u32(out, value);
    }
    for value in blend_weights {
        push_f32(out, value);
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
