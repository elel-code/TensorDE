    use super::*;

    #[test]
    fn image_sampler_plan_aligns_offsets_and_heap_ranges() {
        let snapshot = native_vulkan_vulkanalia_descriptor_heap_image_sampler_plan(
            NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanInput {
                image_count: 3,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
                    resource_heap_alignment: 64,
                    sampler_heap_alignment: 32,
                    max_resource_heap_size: 4096,
                    min_resource_heap_reserved_range: 96,
                    max_sampler_heap_size: 2048,
                    min_sampler_heap_reserved_range: 48,
                    image_descriptor_size: 24,
                    sampler_descriptor_size: 16,
                    image_descriptor_alignment: 32,
                    sampler_descriptor_alignment: 16,
                    ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
                },
            },
        );

        assert!(snapshot.backend_ready);
        assert_eq!(snapshot.descriptor_model, "VK_EXT_descriptor_heap");
        assert_eq!(snapshot.image_descriptor_stride, 32);
        assert_eq!(snapshot.sampler_descriptor_stride, 16);
        assert_eq!(snapshot.resource_heap_reserved_range_offset, 128);
        assert_eq!(snapshot.resource_heap_reserved_range_size, 128);
        assert_eq!(snapshot.sampler_heap_reserved_range_offset, 64);
        assert_eq!(snapshot.sampler_heap_reserved_range_size, 64);
        assert_eq!(snapshot.resource_heap_bytes, 256);
        assert_eq!(snapshot.sampler_heap_bytes, 128);
        assert_eq!(snapshot.image_descriptor_offsets, vec![0, 32, 64]);
        assert_eq!(snapshot.sampler_descriptor_offsets, vec![0, 16, 32]);
        assert!(
            snapshot
                .command_order
                .contains(&"cmd_bind_resource_heap_ext")
        );
        assert!(
            snapshot
                .command_order
                .contains(&"cmd_bind_sampler_heap_ext")
        );
    }

    #[test]
    fn image_sampler_plan_blocks_when_descriptor_sizes_are_missing() {
        let snapshot = native_vulkan_vulkanalia_descriptor_heap_image_sampler_plan(
            NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanInput {
                image_count: 1,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default(),
            },
        );

        assert!(!snapshot.backend_ready);
        assert_eq!(
            snapshot.blocking_reason,
            Some("descriptor-heap-descriptor-sizes-unavailable")
        );
        assert_eq!(
            snapshot.command_order,
            vec!["wait_for_descriptor_heap_capabilities"]
        );
    }

    #[test]
    fn video_present_plane_plan_uses_one_descriptor_pair_per_plane() {
        let snapshot = native_vulkan_vulkanalia_descriptor_heap_image_sampler_plan(
            NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanInput {
                image_count: 2,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
                    resource_heap_alignment: 64,
                    sampler_heap_alignment: 64,
                    max_resource_heap_size: 4096,
                    min_resource_heap_reserved_range: 0,
                    max_sampler_heap_size: 4096,
                    image_descriptor_size: 32,
                    sampler_descriptor_size: 16,
                    image_descriptor_alignment: 32,
                    sampler_descriptor_alignment: 16,
                    ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
                },
            },
        );

        assert!(snapshot.backend_ready);
        assert_eq!(snapshot.image_count, 2);
        assert_eq!(snapshot.image_descriptor_offsets, vec![0, 32]);
        assert_eq!(snapshot.sampler_descriptor_offsets, vec![0, 16]);
        assert!(snapshot.resource_heap_bytes >= snapshot.image_descriptor_size);
        assert!(snapshot.sampler_heap_bytes >= snapshot.sampler_descriptor_size);
        assert!(
            snapshot
                .primary_reference
                .contains("FFmpeg-style retained frame lifetime")
        );
    }

    #[test]
    fn combined_image_sampler_mapping_uses_constant_heap_offsets() {
        let snapshot = native_vulkan_vulkanalia_descriptor_heap_image_sampler_plan(
            NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanInput {
                image_count: 2,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
                    resource_heap_alignment: 64,
                    sampler_heap_alignment: 64,
                    max_resource_heap_size: 4096,
                    min_resource_heap_reserved_range: 0,
                    max_sampler_heap_size: 4096,
                    image_descriptor_size: 32,
                    sampler_descriptor_size: 16,
                    image_descriptor_alignment: 32,
                    sampler_descriptor_alignment: 16,
                    ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
                },
            },
        );

        let mapping =
            native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_mapping(&snapshot, 1)
                .expect("mapping should fit u32 offsets");

        assert_eq!(mapping.heap_table, 0);
        assert_eq!(mapping.first_binding, 0);
        assert_eq!(mapping.binding_count, 1);
        assert_eq!(
            mapping.resource_mask,
            vk::SpirvResourceTypeFlagsEXT::COMBINED_SAMPLED_IMAGE
        );
        assert_eq!(
            mapping.source,
            vk::DescriptorMappingSourceEXT::HEAP_WITH_CONSTANT_OFFSET
        );
        unsafe {
            assert_eq!(mapping.source_data.constant_offset.heap_offset, 32);
            assert_eq!(mapping.source_data.constant_offset.sampler_heap_offset, 16);
        }
    }

    #[test]
    fn uniform_buffer_plan_aligns_offsets_and_resource_heap_range() {
        let snapshot = native_vulkan_vulkanalia_descriptor_heap_uniform_buffer_plan(
            NativeVulkanVulkanaliaDescriptorHeapUniformBufferPlanInput {
                buffer_count: 3,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
                    resource_heap_alignment: 64,
                    max_resource_heap_size: 4096,
                    min_resource_heap_reserved_range: 96,
                    buffer_descriptor_size: 24,
                    buffer_descriptor_alignment: 32,
                    ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
                },
            },
        );

        assert!(snapshot.backend_ready);
        assert_eq!(snapshot.descriptor_model, "VK_EXT_descriptor_heap");
        assert_eq!(snapshot.buffer_descriptor_stride, 32);
        assert_eq!(snapshot.resource_heap_reserved_range_offset, 128);
        assert_eq!(snapshot.resource_heap_reserved_range_size, 128);
        assert_eq!(snapshot.resource_heap_bytes, 256);
        assert_eq!(snapshot.buffer_descriptor_offsets, vec![0, 32, 64]);
        assert!(
            snapshot
                .command_order
                .contains(&"write_uniform_buffer_descriptors_into_resource_heap")
        );
    }

    #[test]
    fn uniform_buffer_plan_blocks_when_buffer_descriptor_size_is_missing() {
        let snapshot = native_vulkan_vulkanalia_descriptor_heap_uniform_buffer_plan(
            NativeVulkanVulkanaliaDescriptorHeapUniformBufferPlanInput {
                buffer_count: 1,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default(),
            },
        );

        assert!(!snapshot.backend_ready);
        assert_eq!(
            snapshot.blocking_reason,
            Some("descriptor-heap-buffer-descriptor-size-unavailable")
        );
        assert_eq!(
            snapshot.command_order,
            vec!["wait_for_descriptor_heap_capabilities"]
        );
    }

    #[test]
    fn uniform_buffer_mapping_uses_constant_heap_offsets() {
        let snapshot = native_vulkan_vulkanalia_descriptor_heap_uniform_buffer_plan(
            NativeVulkanVulkanaliaDescriptorHeapUniformBufferPlanInput {
                buffer_count: 2,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
                    resource_heap_alignment: 64,
                    max_resource_heap_size: 4096,
                    min_resource_heap_reserved_range: 0,
                    buffer_descriptor_size: 32,
                    buffer_descriptor_alignment: 32,
                    ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
                },
            },
        );

        let mapping = native_vulkan_vulkanalia_descriptor_heap_uniform_buffer_binding_mapping(
            &snapshot, 3, 1,
        )
        .expect("mapping should fit u32 offsets");

        assert_eq!(mapping.heap_table, 0);
        assert_eq!(mapping.first_binding, 3);
        assert_eq!(mapping.binding_count, 1);
        assert_eq!(
            mapping.resource_mask,
            vk::SpirvResourceTypeFlagsEXT::UNIFORM_BUFFER
        );
        assert_eq!(
            mapping.source,
            vk::DescriptorMappingSourceEXT::HEAP_WITH_CONSTANT_OFFSET
        );
        unsafe {
            assert_eq!(mapping.source_data.constant_offset.heap_offset, 32);
            assert_eq!(mapping.source_data.constant_offset.heap_array_stride, 32);
            assert_eq!(mapping.source_data.constant_offset.sampler_heap_offset, 0);
            assert_eq!(
                mapping
                    .source_data
                    .constant_offset
                    .sampler_heap_array_stride,
                0
            );
        }
    }

    #[test]
    fn mixed_resource_plan_co_packs_uniform_buffers_and_sampled_images() {
        let snapshot = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                ],
                sampler_count: 3,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
                    resource_heap_alignment: 64,
                    sampler_heap_alignment: 32,
                    max_resource_heap_size: 4096,
                    min_resource_heap_reserved_range: 96,
                    max_sampler_heap_size: 4096,
                    min_sampler_heap_reserved_range: 48,
                    image_descriptor_size: 24,
                    image_descriptor_alignment: 32,
                    buffer_descriptor_size: 16,
                    buffer_descriptor_alignment: 16,
                    sampler_descriptor_size: 12,
                    sampler_descriptor_alignment: 16,
                    ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
                },
            },
        );

        assert!(snapshot.backend_ready);
        assert_eq!(snapshot.sampled_image_count, 3);
        assert_eq!(snapshot.uniform_buffer_count, 2);
        assert_eq!(
            snapshot.resource_descriptor_offsets,
            vec![0, 32, 64, 96, 128]
        );
        assert_eq!(snapshot.sampler_descriptor_offsets, vec![0, 16, 32]);
        assert_eq!(snapshot.resource_heap_reserved_range_offset, 192);
        assert_eq!(snapshot.resource_heap_reserved_range_size, 128);
        assert_eq!(snapshot.sampler_heap_reserved_range_offset, 64);
        assert_eq!(snapshot.sampler_heap_reserved_range_size, 64);
        assert!(
            snapshot
                .command_order
                .contains(&"write_uniform_buffer_descriptors_into_resource_heap")
        );
        assert!(
            snapshot
                .command_order
                .contains(&"write_sampled_image_and_input_attachment_descriptors_into_same_resource_heap")
        );
    }

    #[test]
    fn mixed_resource_plan_maps_input_attachments_as_read_only_images_without_samplers() {
        let snapshot = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::InputAttachment,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                ],
                sampler_count: 1,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
                    resource_heap_alignment: 64,
                    sampler_heap_alignment: 32,
                    max_resource_heap_size: 4096,
                    min_resource_heap_reserved_range: 0,
                    max_sampler_heap_size: 4096,
                    min_sampler_heap_reserved_range: 0,
                    image_descriptor_size: 24,
                    image_descriptor_alignment: 32,
                    buffer_descriptor_size: 16,
                    buffer_descriptor_alignment: 16,
                    sampler_descriptor_size: 12,
                    sampler_descriptor_alignment: 16,
                    ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
                },
            },
        );

        assert!(snapshot.backend_ready);
        assert_eq!(snapshot.sampled_image_count, 1);
        assert_eq!(snapshot.input_attachment_count, 1);
        assert_eq!(snapshot.sampler_count, 1);
        assert_eq!(snapshot.resource_descriptor_offsets, vec![0, 32, 64]);
        let mapping =
            native_vulkan_vulkanalia_descriptor_heap_resource_input_attachment_binding_mapping(
                &snapshot, 36, 1,
            )
            .expect("input attachment mapping");
        assert_eq!(mapping.first_binding, 36);
        assert_eq!(
            mapping.resource_mask,
            vk::SpirvResourceTypeFlagsEXT::READ_ONLY_IMAGE
        );
        unsafe {
            assert_eq!(mapping.source_data.constant_offset.heap_offset, 32);
            assert_eq!(mapping.source_data.constant_offset.heap_array_stride, 32);
            assert_eq!(mapping.source_data.constant_offset.sampler_heap_offset, 0);
            assert_eq!(
                mapping.source_data.constant_offset.sampler_heap_array_stride,
                0
            );
        }
    }

    #[test]
    fn mixed_resource_binding_mappings_use_heap_slice_relative_offsets() {
        let snapshot = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                ],
                sampler_count: 2,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
                    resource_heap_alignment: 64,
                    sampler_heap_alignment: 32,
                    max_resource_heap_size: 4096,
                    min_resource_heap_reserved_range: 0,
                    max_sampler_heap_size: 4096,
                    min_sampler_heap_reserved_range: 0,
                    image_descriptor_size: 24,
                    image_descriptor_alignment: 32,
                    buffer_descriptor_size: 16,
                    buffer_descriptor_alignment: 16,
                    sampler_descriptor_size: 12,
                    sampler_descriptor_alignment: 16,
                    ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
                },
            },
        );

        let uniform =
            native_vulkan_vulkanalia_descriptor_heap_resource_relative_uniform_buffer_binding_mapping(
                &snapshot, 3, 0, 0,
            )
            .expect("relative uniform mapping");
        let texture =
            native_vulkan_vulkanalia_descriptor_heap_resource_combined_image_sampler_binding_mapping(
                &snapshot, 4, 2, 1,
            )
            .expect("texture mapping");

        assert_eq!(uniform.first_binding, 3);
        assert_eq!(
            uniform.resource_mask,
            vk::SpirvResourceTypeFlagsEXT::UNIFORM_BUFFER
        );
        assert_eq!(texture.first_binding, 4);
        assert_eq!(
            texture.resource_mask,
            vk::SpirvResourceTypeFlagsEXT::COMBINED_SAMPLED_IMAGE
        );
        unsafe {
            assert_eq!(uniform.source_data.constant_offset.heap_offset, 0);
            assert_eq!(uniform.source_data.constant_offset.heap_array_stride, 16);
            assert_eq!(texture.source_data.constant_offset.heap_offset, 64);
            assert_eq!(texture.source_data.constant_offset.sampler_heap_offset, 16);
        }
    }

    #[test]
    fn descriptor_heap_indexed_bind_info_aligns_heap_range_base_down() {
        let heap = test_descriptor_heap_buffer(0x1000, 256);

        let bind = descriptor_heap_indexed_bind_info(&heap, 192, 64, 80, 32, "test")
            .expect("aligned bind info");

        assert_eq!(bind.heap_range.address, 0x1040);
        assert_eq!(bind.heap_range.size, 192);
        assert_eq!(bind.reserved_range_offset, 128);
        assert_eq!(bind.reserved_range_size, 64);
    }

    #[test]
    fn mixed_resource_relative_mapping_uses_aligned_heap_slice_base() {
        let snapshot = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                ],
                sampler_count: 2,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
                    resource_heap_alignment: 64,
                    sampler_heap_alignment: 32,
                    max_resource_heap_size: 4096,
                    min_resource_heap_reserved_range: 0,
                    max_sampler_heap_size: 4096,
                    min_sampler_heap_reserved_range: 0,
                    image_descriptor_size: 24,
                    image_descriptor_alignment: 32,
                    buffer_descriptor_size: 16,
                    buffer_descriptor_alignment: 16,
                    sampler_descriptor_size: 12,
                    sampler_descriptor_alignment: 16,
                    ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
                },
            },
        );

        assert_eq!(snapshot.resource_descriptor_offsets, vec![0, 32, 64]);
        let uniform =
            native_vulkan_vulkanalia_descriptor_heap_resource_relative_uniform_buffer_binding_mapping(
                &snapshot, 3, 1, 1,
            )
            .expect("relative uniform mapping");
        let texture =
            native_vulkan_vulkanalia_descriptor_heap_resource_relative_combined_image_sampler_binding_mapping(
                &snapshot, 4, 1, 2, 1, 1,
            )
            .expect("relative image mapping");

        unsafe {
            assert_eq!(uniform.source_data.constant_offset.heap_offset, 32);
            assert_eq!(texture.source_data.constant_offset.heap_offset, 64);
            assert_eq!(texture.source_data.constant_offset.sampler_heap_offset, 16);
        }
    }

    #[test]
    fn mixed_resource_relative_uniform_mapping_rejects_non_uniform_base() {
        let snapshot = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                ],
                sampler_count: 1,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
                    resource_heap_alignment: 64,
                    sampler_heap_alignment: 32,
                    max_resource_heap_size: 4096,
                    min_resource_heap_reserved_range: 0,
                    max_sampler_heap_size: 4096,
                    min_sampler_heap_reserved_range: 0,
                    image_descriptor_size: 24,
                    image_descriptor_alignment: 32,
                    buffer_descriptor_size: 16,
                    buffer_descriptor_alignment: 16,
                    sampler_descriptor_size: 12,
                    sampler_descriptor_alignment: 16,
                    ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
                },
            },
        );

        let err =
            native_vulkan_vulkanalia_descriptor_heap_resource_relative_uniform_buffer_binding_mapping(
                &snapshot, 3, 0, 1,
            )
            .expect_err("sampled image base cannot anchor a relative uniform mapping");

        assert!(err.contains("expected UniformBuffer"));
    }

    #[test]
    fn mixed_resource_binding_mapping_rejects_wrong_descriptor_kind() {
        let snapshot = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                ],
                sampler_count: 1,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
                    resource_heap_alignment: 64,
                    sampler_heap_alignment: 32,
                    max_resource_heap_size: 4096,
                    min_resource_heap_reserved_range: 0,
                    max_sampler_heap_size: 4096,
                    min_sampler_heap_reserved_range: 0,
                    image_descriptor_size: 24,
                    image_descriptor_alignment: 32,
                    buffer_descriptor_size: 16,
                    buffer_descriptor_alignment: 16,
                    sampler_descriptor_size: 12,
                    sampler_descriptor_alignment: 16,
                    ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
                },
            },
        );

        let err = native_vulkan_vulkanalia_descriptor_heap_resource_uniform_buffer_binding_mapping(
            &snapshot, 3, 1,
        )
        .expect_err("sampled image descriptor cannot map as uniform buffer");

        assert!(err.contains("expected UniformBuffer"));
    }

    #[test]
    fn mixed_resource_plan_requires_sampler_per_sampled_image() {
        let snapshot = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                ],
                sampler_count: 0,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
                    resource_heap_alignment: 64,
                    sampler_heap_alignment: 64,
                    max_resource_heap_size: 4096,
                    min_resource_heap_reserved_range: 0,
                    max_sampler_heap_size: 4096,
                    image_descriptor_size: 32,
                    image_descriptor_alignment: 32,
                    buffer_descriptor_size: 32,
                    buffer_descriptor_alignment: 32,
                    sampler_descriptor_size: 16,
                    sampler_descriptor_alignment: 16,
                    ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
                },
            },
        );

        assert!(!snapshot.backend_ready);
        assert_eq!(
            snapshot.blocking_reason,
            Some("sampler-count-must-match-sampled-image-count")
        );
    }

    fn test_descriptor_heap_buffer(
        device_address: vk::DeviceAddress,
        requested_bytes: u64,
    ) -> VulkanaliaDescriptorHeapBuffer {
        VulkanaliaDescriptorHeapBuffer {
            buffer: vk::Buffer::null(),
            memory: vk::DeviceMemory::null(),
            mapped_ptr: std::ptr::null_mut(),
            mapped_size: requested_bytes,
            device_address,
            host_coherent: true,
            snapshot: NativeVulkanVulkanaliaDescriptorHeapBufferSnapshot {
                role: "test",
                buffer_created: true,
                memory_bound: true,
                mapped: true,
                device_address_nonzero: device_address != 0,
                requested_bytes,
                memory_size: requested_bytes,
                memory_alignment: 32,
                memory_type_bits: 1,
                selected_memory_type_index: 0,
                selected_memory_property_flags: vec!["host-visible"],
                usage_flags: vec!["descriptor-buffer"],
                host_coherent: true,
            },
        }
    }
