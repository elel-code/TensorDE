use super::*;

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
