use super::*;
use crate::engine::scene_engine::{
    SceneLayerAlphaMaskRtMethod8MdlvGeometry, SceneLayerAlphaMaskRtMethod8MdlvSourceRecord,
    SceneLayerAlphaMaskRtMethod8MdlvSubdraw, SceneObjectId,
};

#[test]
fn rt_method8_payload_plan_closes_mdlv_entry_geometry_fields() {
    let plan = native_vulkan_scene_layer_alpha_mask_rt_method8_payload_plan();

    assert_eq!(plan.rebuild_function_vma, "0x14020ae00");
    assert!(plan.entry_source.contains("[[layer+0x4b8]+0x18]"));
    assert_eq!(plan.entry_field_count, 9);
    assert_eq!(plan.entry_fields[0].field, "+0x18");
    assert!(plan.entry_fields[0].consumer.contains("stack arg 9"));
    assert_eq!(plan.entry_fields[1].field, "+0x38");
    assert_eq!(plan.entry_fields[1].consumer, "wrapper [8] edx");
    assert_eq!(plan.entry_fields[2].field, "+0x3c");
    assert_eq!(plan.entry_fields[3].field, "+0x40");
    assert!(plan.entry_fields[3].consumer.contains("+0x40 / +0x3c"));
    assert_eq!(plan.entry_fields[4].field, "+0x48");
    assert!(plan.entry_fields[4].consumer.contains("wrapper [8] r8"));
    assert_eq!(plan.entry_fields[5].field, "+0x50");
    assert!(plan.entry_fields[5].consumer.contains("+0x50 / 2"));
    assert_eq!(plan.entry_fields[6].field, "+0x58");
    assert_eq!(plan.entry_fields[7].field, "+0xa0");
    assert_eq!(plan.entry_fields[8].field, "+0xa8");
}

#[test]
fn rt_method8_payload_plan_closes_copy_scale_gates_and_local_eye_case() {
    let plan = native_vulkan_scene_layer_alpha_mask_rt_method8_payload_plan();

    assert_eq!(plan.copy_scale_region, "0x14020af31..0x14020b102");
    assert_eq!(plan.copy_scale_gate_count, 4);
    assert_eq!(plan.copy_scale_gates[0].branch_site, "0x14020af31");
    assert_eq!(plan.copy_scale_gates[0].condition, "[layer+0x320] > 0");
    assert!(
        plan.copy_scale_gates[0]
            .outcome_when_true
            .contains("original MDLV vertex payload")
    );
    assert_eq!(plan.copy_scale_gates[1].branch_site, "0x14020af3e");
    assert_eq!(
        plan.copy_scale_gates[2].condition,
        "first_pass_entry+0x1c bit 0x4 is set"
    );
    assert_eq!(
        plan.copy_scale_gates[3].condition,
        "active_entry_bool is true"
    );
    assert!(plan.copy_scale_operation.contains("multiply x by xmm7"));
    assert!(plan.copy_scale_operation.contains("multiply y by xmm6"));

    assert_eq!(
        plan.local_reference.object,
        "3742497499 object 1530 eye puppet"
    );
    assert!(
        plan.local_reference
            .skip_copy_reason
            .contains("0x14020af31")
    );
    assert_eq!(plan.local_reference.file_layout_mask, "0x1800009");
    assert_eq!(plan.local_reference.entry_layout_key, "0x180000f");
    assert_eq!(plan.local_reference.vertex_stride_bytes, 80);
    assert_eq!(plan.local_reference.vertex_count, 4106);
    assert_eq!(plan.local_reference.index_count, 23988);
    assert_eq!(plan.local_reference.entry_flags, "0x4");
    assert_eq!(plan.local_reference.wrapper_usage_arg9, "0");
}

#[test]
fn rt_method8_payload_plan_closes_aux_298_record_layout() {
    let plan = native_vulkan_scene_layer_alpha_mask_rt_method8_payload_plan();

    assert_eq!(plan.aux_payload_region, "0x14020b214..0x14020b66f");
    assert_eq!(plan.aux_vector_target, "aux+0x298");
    assert_eq!(plan.aux_record_size_bytes, 0x1c);
    assert_eq!(
        plan.aux_record_count_source,
        "[entry+0xa0] records from entry+0xa8"
    );
    assert_eq!(plan.aux_record_fields[0].record_offset, "+0x00");
    assert_eq!(plan.aux_record_fields[0].value, "0");
    assert_eq!(plan.aux_record_fields[1].record_offset, "+0x04");
    assert!(plan.aux_record_fields[1].value.contains("aux+0x1f0"));
    assert_eq!(plan.aux_record_fields[2].record_offset, "+0x08");
    assert_eq!(plan.aux_record_fields[6].record_offset, "+0x18");
    assert_eq!(
        plan.aux_record_fields[6].value,
        "flags built from subdraw index lists"
    );

    assert_eq!(plan.aux_flag_source_count, 3);
    assert_eq!(plan.aux_flag_sources[0].flag, "OR +0x18 with 0x1");
    assert_eq!(plan.aux_flag_sources[1].flag, "OR +0x18 with 0x8");
    assert_eq!(plan.aux_flag_sources[2].flag, "OR +0x18 with 0x4");
}

#[test]
fn rt_method8_payload_lowering_builds_aux_records_from_source_and_subdraw_lists() {
    let geometry = test_rt_method8_geometry_with_source_records();

    let plan = native_vulkan_scene_layer_alpha_mask_rt_method8_lower_aux_payload(&geometry)
        .expect("aux payload lowering");

    assert_eq!(plan.source_record_count, 3);
    assert_eq!(plan.subdraw_count, 2);
    assert_eq!(plan.aux_record_count, 3);
    assert_eq!(plan.records[0].payload_index, 0);
    assert_eq!(plan.records[0].source_index, 7);
    assert_eq!(plan.records[0].local_offset, 10);
    assert_eq!(plan.records[0].index_span_offset, 0);
    assert_eq!(plan.records[0].index_span_count, 2);
    assert_eq!(
        plan.records[0].flags,
        LAYER_490_RT_METHOD8_AUX_FLAG_FIRST_LIST
            | LAYER_490_RT_METHOD8_AUX_FLAG_FIRST_LIST_MODIFIER
    );
    assert_eq!(
        plan.records[1].flags,
        LAYER_490_RT_METHOD8_AUX_FLAG_FIRST_LIST
            | LAYER_490_RT_METHOD8_AUX_FLAG_FIRST_LIST_MODIFIER
            | LAYER_490_RT_METHOD8_AUX_FLAG_SECOND_LIST
    );
    assert_eq!(
        plan.records[2].flags,
        LAYER_490_RT_METHOD8_AUX_FLAG_SECOND_LIST
    );
    assert!(
        plan.reference_points
            .iter()
            .any(|reference| reference.contains("0x14020c710/0x14020c850"))
    );
}

#[test]
fn rt_method8_payload_materializes_r16_index_slice_from_aux_spans() {
    let geometry = test_rt_method8_geometry_with_source_records();
    let aux_payload = native_vulkan_scene_layer_alpha_mask_rt_method8_lower_aux_payload(&geometry)
        .expect("aux payload lowering");

    let slice = native_vulkan_scene_layer_alpha_mask_rt_method8_materialize_index_slice(
        &geometry,
        &aux_payload,
        5,
        &[0, 2],
        true,
    )
    .expect("slice plan");

    assert_eq!(slice.helper_vma, "0x14020c850");
    assert_eq!(slice.draw_index, 5);
    assert!(slice.appends_token_zero);
    assert_eq!(slice.payload_indices, vec![0, 2]);
    assert_eq!(slice.index_count, 3);
    assert_eq!(slice.index_payload, vec![0, 0, 1, 0, 4, 0]);
    assert_eq!(slice.copied_spans.len(), 2);
    assert_eq!(slice.copied_spans[0].byte_offset, 0);
    assert_eq!(slice.copied_spans[0].byte_count, 4);
    assert_eq!(slice.copied_spans[1].byte_offset, 8);
    assert_eq!(slice.copied_spans[1].byte_count, 2);

    let no_token = native_vulkan_scene_layer_alpha_mask_rt_method8_materialize_index_slice(
        &geometry,
        &aux_payload,
        6,
        &[1],
        false,
    )
    .expect("no-token slice plan");
    assert_eq!(no_token.helper_vma, "0x14020c710");
    assert!(!no_token.appends_token_zero);
    assert_eq!(no_token.index_payload, vec![2, 0, 3, 0]);
}

#[test]
fn rt_method8_payload_materialize_rejects_out_of_bounds_index_span() {
    let mut geometry = test_rt_method8_geometry_with_source_records();
    geometry.source_records[0].index_span_offset = 6;
    let aux_payload = native_vulkan_scene_layer_alpha_mask_rt_method8_lower_aux_payload(&geometry)
        .expect("aux payload lowering");

    let err = native_vulkan_scene_layer_alpha_mask_rt_method8_materialize_index_slice(
        &geometry,
        &aux_payload,
        0,
        &[0],
        false,
    )
    .expect_err("out-of-bounds R16 span must fail");

    assert!(err.contains("outside MDLV index payload"));
}

fn test_rt_method8_geometry_with_source_records() -> SceneLayerAlphaMaskRtMethod8MdlvGeometry {
    SceneLayerAlphaMaskRtMethod8MdlvGeometry {
        object: SceneObjectId(1530),
        entry_owner_index: 0,
        layout_key: 0x0180_000f,
        vertex_stride_bytes: 80,
        vertex_count: 1,
        index_count: 6,
        vertex_payload: vec![0; 80],
        index_payload: vec![0, 0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0],
        source_records: vec![
            SceneLayerAlphaMaskRtMethod8MdlvSourceRecord {
                source_index: 7,
                local_offset: 10,
                index_span_offset: 0,
                index_span_count: 2,
            },
            SceneLayerAlphaMaskRtMethod8MdlvSourceRecord {
                source_index: 8,
                local_offset: 20,
                index_span_offset: 2,
                index_span_count: 2,
            },
            SceneLayerAlphaMaskRtMethod8MdlvSourceRecord {
                source_index: 9,
                local_offset: 30,
                index_span_offset: 4,
                index_span_count: 1,
            },
        ],
        subdraws: vec![
            SceneLayerAlphaMaskRtMethod8MdlvSubdraw {
                source_qword: 0x690,
                mask_resource: "masks/clipping_mask_eye".to_owned(),
                raw_flags: 0x8,
                first_indices: vec![0, 1],
                second_indices: Vec::new(),
                link: u32::MAX,
            },
            SceneLayerAlphaMaskRtMethod8MdlvSubdraw {
                source_qword: 0x691,
                mask_resource: "masks/clipping_mask_inner".to_owned(),
                raw_flags: 0,
                first_indices: Vec::new(),
                second_indices: vec![1, 2],
                link: 0,
            },
        ],
    }
}
