use super::*;

#[test]
fn rt_method8_geometry_source_closes_wrapper_arguments() {
    let plan = native_vulkan_scene_layer_alpha_mask_rt_method8_geometry_source_plan();

    assert_eq!(plan.creation_site, "0x14020b15e");
    assert_eq!(plan.stored_receiver_field, "[layer+0x490]");
    assert_eq!(plan.wrapper_create_method_vma, "0x14009a880");
    assert_eq!(plan.created_rt_vtable, "0x140486f38");
    assert_eq!(plan.rt_draw_method_vma, "0x1400eacd0");
    assert_eq!(plan.wrapper_argument_count, 9);

    assert_eq!(plan.wrapper_arguments[0].argument, "rcx");
    assert_eq!(
        plan.wrapper_arguments[0].value_source,
        "r11 = [[layer+0xc8]+0x1518]"
    );
    assert_eq!(plan.wrapper_arguments[1].argument, "edx");
    assert_eq!(plan.wrapper_arguments[1].value_source, "ebp");
    assert!(
        plan.wrapper_arguments[1]
            .semantic
            .contains("0x1400ea5b0(edx)")
    );
    assert_eq!(plan.wrapper_arguments[2].argument, "r8");
    assert_eq!(plan.wrapper_arguments[2].value_source, "rbx, normally r15");
    assert_eq!(plan.wrapper_arguments[3].argument, "r9d");
    assert_eq!(plan.wrapper_arguments[3].value_source, "r14d");
    assert_eq!(plan.wrapper_arguments[4].argument, "stack arg 5");
    assert_eq!(plan.wrapper_arguments[4].value_source, "[caller rsp+0x58]");
    assert_eq!(plan.wrapper_arguments[5].argument, "stack arg 6");
    assert_eq!(plan.wrapper_arguments[5].value_source, "[caller rsp+0xe0]");
    assert_eq!(plan.wrapper_arguments[6].argument, "stack arg 7");
    assert_eq!(plan.wrapper_arguments[6].value_source, "0");
    assert!(plan.wrapper_arguments[6].semantic.contains("R16_UINT"));
    assert_eq!(plan.wrapper_arguments[7].argument, "stack arg 8");
    assert_eq!(plan.wrapper_arguments[7].value_source, "0");
    assert!(plan.wrapper_arguments[7].semantic.contains("triangle list"));
    assert_eq!(plan.wrapper_arguments[8].argument, "stack arg 9");
    assert_eq!(
        plan.wrapper_arguments[8].value_source,
        "((([layer+0x4b8]+0x18)->0x18 >> 3) & 1) * 2"
    );
}

#[test]
fn rt_method8_geometry_source_closes_created_fields_and_sibling_targets() {
    let plan = native_vulkan_scene_layer_alpha_mask_rt_method8_geometry_source_plan();

    assert_eq!(plan.created_field_count, 7);
    assert_eq!(plan.created_fields[0].field, "+0x10");
    assert_eq!(plan.created_fields[0].semantic, "vertex buffer");
    assert_eq!(plan.created_fields[1].field, "+0x18");
    assert_eq!(plan.created_fields[1].semantic, "index buffer");
    assert_eq!(plan.created_fields[2].field, "+0x20");
    assert!(plan.created_fields[2].source.contains("0x39 R16"));
    assert_eq!(plan.created_fields[3].field, "+0x24");
    assert_eq!(plan.created_fields[4].field, "+0x28");
    assert_eq!(plan.created_fields[5].field, "+0x2c");
    assert_eq!(plan.created_fields[6].field, "+0x30");

    assert_eq!(plan.usage_selector.stack_argument, "stack arg 9");
    assert_eq!(
        plan.usage_selector.source,
        "((([layer+0x4b8]+0x18)->0x18 >> 3) & 1) * 2"
    );
    assert_eq!(
        plan.usage_selector.bit_one_semantic,
        "dynamic index-buffer creation"
    );

    assert_eq!(plan.sibling_creation_site_count, 2);
    assert_eq!(plan.sibling_creation_sites[0].creation_site, "0x14020b1e8");
    assert_eq!(
        plan.sibling_creation_sites[0].stored_target,
        "[layer+0x4b8]+0x3f8"
    );
    assert_eq!(plan.sibling_creation_sites[1].creation_site, "0x14020a4ff");
    assert_eq!(
        plan.sibling_creation_sites[1].stored_target,
        "[layer+0x4b8]+0x400"
    );
    assert_eq!(plan.remaining_payload_region, "0x14020aa80..0x14020b102");
    assert!(
        plan.remaining_payload_fact
            .contains("retained Vulkan buffer binding")
    );
}
