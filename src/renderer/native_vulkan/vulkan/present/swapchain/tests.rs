    use super::*;

    #[test]
    fn present_device_extensions_keep_swapchain_required() {
        let disabled = NativeVulkanVulkanaliaPresentFeatureSelection {
            core_features: NativeVulkanVulkanaliaCoreFeatureSnapshot::default(),
            vulkan_1_4_properties: NativeVulkanVulkanaliaVulkan14PropertySnapshot::default(),
            descriptor_heap_properties:
                NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default(),
            synchronization2_enabled: false,
            dynamic_rendering_enabled: false,
            present_id2_enabled: false,
            present_wait2_enabled: false,
            swapchain_maintenance1_enabled: false,
            present_mode_fifo_latest_ready_enabled: false,
            blend_operation_advanced_enabled: false,
            blend_operation_advanced_coherent_operations: false,
            maintenance7_enabled: false,
            maintenance8_enabled: false,
            maintenance9_enabled: false,
            maintenance10_enabled: false,
        };
        let enabled = NativeVulkanVulkanaliaPresentFeatureSelection {
            swapchain_maintenance1_enabled: true,
            ..disabled
        };
        let enabled2 = NativeVulkanVulkanaliaPresentFeatureSelection {
            present_id2_enabled: true,
            present_wait2_enabled: true,
            swapchain_maintenance1_enabled: true,
            ..disabled
        };
        let descriptor_heap_enabled = NativeVulkanVulkanaliaPresentFeatureSelection {
            core_features: NativeVulkanVulkanaliaCoreFeatureSnapshot {
                descriptor_heap: true,
                ..NativeVulkanVulkanaliaCoreFeatureSnapshot::default()
            },
            ..disabled
        };
        let fifo_latest_ready_enabled = NativeVulkanVulkanaliaPresentFeatureSelection {
            present_mode_fifo_latest_ready_enabled: true,
            ..disabled
        };
        let advanced_blend_enabled = NativeVulkanVulkanaliaPresentFeatureSelection {
            blend_operation_advanced_enabled: true,
            blend_operation_advanced_coherent_operations: true,
            ..disabled
        };
        let maintenance_roadmap_enabled = NativeVulkanVulkanaliaPresentFeatureSelection {
            maintenance7_enabled: true,
            maintenance8_enabled: true,
            maintenance9_enabled: true,
            maintenance10_enabled: true,
            ..disabled
        };

        assert_eq!(
            enabled_present_device_extensions(&disabled),
            vec!["VK_KHR_swapchain"]
        );
        assert_eq!(
            enabled_present_device_extensions(&enabled),
            vec!["VK_KHR_swapchain", SWAPCHAIN_MAINTENANCE1_EXTENSION_NAME,]
        );
        assert_eq!(
            enabled_present_device_extensions(&enabled2),
            vec![
                "VK_KHR_swapchain",
                PRESENT_ID2_EXTENSION_NAME,
                PRESENT_WAIT2_EXTENSION_NAME,
                SWAPCHAIN_MAINTENANCE1_EXTENSION_NAME,
            ]
        );
        assert_eq!(
            enabled_present_device_extensions(&descriptor_heap_enabled),
            vec!["VK_KHR_swapchain", DESCRIPTOR_HEAP_EXTENSION_NAME]
        );
        assert_eq!(
            enabled_present_device_extensions(&fifo_latest_ready_enabled),
            vec![
                "VK_KHR_swapchain",
                PRESENT_MODE_FIFO_LATEST_READY_EXTENSION_NAME,
            ]
        );
        assert_eq!(
            enabled_present_device_extensions(&advanced_blend_enabled),
            vec!["VK_KHR_swapchain", BLEND_OPERATION_ADVANCED_EXTENSION_NAME]
        );
        assert_eq!(
            enabled_present_device_extensions(&maintenance_roadmap_enabled),
            vec![
                "VK_KHR_swapchain",
                MAINTENANCE7_EXTENSION_NAME,
                MAINTENANCE8_EXTENSION_NAME,
                MAINTENANCE9_EXTENSION_NAME,
                MAINTENANCE10_EXTENSION_NAME,
            ]
        );
    }

    #[test]
    fn swapchain_create_flags_report_present_id2_and_wait2() {
        let disabled = swapchain_create_flags(false, false);
        let id2 = swapchain_create_flags(true, false);
        let wait2 = swapchain_create_flags(true, true);

        assert!(disabled.is_empty());
        assert_eq!(swapchain_create_flag_labels(disabled), Vec::<&str>::new());
        assert_eq!(swapchain_create_flag_labels(id2), vec!["present-id2"]);
        assert_eq!(
            swapchain_create_flag_labels(wait2),
            vec!["present-id2", "present-wait2"]
        );
    }

    #[test]
    fn present_mode_requires_fifo_latest_ready_policy_without_mailbox_or_immediate_fallback() {
        assert_eq!(
            choose_present_mode(
                &[
                    vk::PresentModeKHR::FIFO,
                    vk::PresentModeKHR::MAILBOX,
                    vk::PresentModeKHR::FIFO_LATEST_READY,
                ],
                true,
            )
            .expect("fifo latest ready present mode"),
            vk::PresentModeKHR::FIFO_LATEST_READY
        );
        assert!(
            choose_present_mode(
                &[
                    vk::PresentModeKHR::FIFO,
                    vk::PresentModeKHR::MAILBOX,
                    vk::PresentModeKHR::FIFO_LATEST_READY,
                ],
                false,
            )
            .expect_err("fifo latest ready feature is mandatory")
            .contains("requires VK_KHR_present_mode_fifo_latest_ready")
        );
        assert!(
            choose_present_mode(&[vk::PresentModeKHR::MAILBOX], true)
                .expect_err("mailbox-only present surface is forbidden")
                .contains("VK_PRESENT_MODE_FIFO_LATEST_READY_KHR")
        );
        assert!(
            choose_present_mode(
                &[vk::PresentModeKHR::FIFO, vk::PresentModeKHR::FIFO_RELAXED,],
                true,
            )
            .expect_err("fifo relaxed fallback is forbidden")
            .contains("VK_PRESENT_MODE_FIFO_LATEST_READY_KHR")
        );
        assert!(
            choose_present_mode(&[vk::PresentModeKHR::FIFO], true)
                .expect_err("fifo fallback is forbidden")
                .contains("VK_PRESENT_MODE_FIFO_LATEST_READY_KHR")
        );
        assert_eq!(
            choose_present_mode(
                &[
                    vk::PresentModeKHR::FIFO,
                    vk::PresentModeKHR::FIFO_LATEST_READY,
                ],
                true,
            )
            .expect("fifo latest ready present mode"),
            vk::PresentModeKHR::FIFO_LATEST_READY
        );
        assert!(
            choose_present_mode(
                &[
                    vk::PresentModeKHR::FIFO,
                    vk::PresentModeKHR::FIFO_LATEST_READY,
                ],
                false,
            )
            .expect_err("fifo latest ready feature is mandatory")
            .contains("requires VK_KHR_present_mode_fifo_latest_ready")
        );
    }

    #[test]
    fn swapchain_image_count_uses_surface_minimum_for_wallpaper_present() {
        let mut capabilities = vk::SurfaceCapabilitiesKHR::default();
        capabilities.min_image_count = 2;
        capabilities.max_image_count = 0;
        assert_eq!(swapchain_image_count(&capabilities), 4);

        capabilities.min_image_count = 3;
        capabilities.max_image_count = 2;
        assert_eq!(swapchain_image_count(&capabilities), 2);
    }

    #[test]
    fn composite_alpha_prefers_we_premultiplied_handoff() {
        let both = vk::CompositeAlphaFlagsKHR::OPAQUE | vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED;
        assert_eq!(
            choose_composite_alpha(both),
            vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED
        );
        assert_eq!(
            choose_composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE),
            vk::CompositeAlphaFlagsKHR::OPAQUE
        );
    }

    #[test]
    fn swapchain_extent_selection_prefers_surface_current_extent_like_godot() {
        let mut capabilities = vk::SurfaceCapabilitiesKHR::default();
        capabilities.current_extent = vk::Extent2D {
            width: 2561,
            height: 1601,
        };
        capabilities.min_image_extent = vk::Extent2D {
            width: 64,
            height: 64,
        };
        capabilities.max_image_extent = vk::Extent2D {
            width: 8192,
            height: 8192,
        };

        let (extent, selection) = choose_swapchain_extent(&capabilities, (2560, 1600)).unwrap();

        assert_eq!(extent.width, 2561);
        assert_eq!(extent.height, 1601);
        assert_eq!(selection.source, "surface-current-extent");
        assert_eq!(selection.requested_wayland_buffer_size, (2560, 1600));
        assert_eq!(selection.surface_current_extent, Some((2561, 1601)));
    }

    #[test]
    fn swapchain_extent_selection_clamps_wayland_buffer_when_surface_extent_is_unknown() {
        let mut capabilities = vk::SurfaceCapabilitiesKHR::default();
        capabilities.current_extent = vk::Extent2D {
            width: u32::MAX,
            height: u32::MAX,
        };
        capabilities.min_image_extent = vk::Extent2D {
            width: 100,
            height: 100,
        };
        capabilities.max_image_extent = vk::Extent2D {
            width: 2000,
            height: 1200,
        };

        let (extent, selection) = choose_swapchain_extent(&capabilities, (2560, 900)).unwrap();

        assert_eq!(extent.width, 2000);
        assert_eq!(extent.height, 900);
        assert_eq!(
            selection.source,
            "wayland-buffer-size-clamped-to-surface-capabilities"
        );
        assert_eq!(selection.requested_wayland_buffer_size, (2560, 900));
        assert_eq!(selection.surface_current_extent, None);
    }

    #[test]
    fn unknown_surface_extent_is_none() {
        assert_eq!(
            extent_tuple(vk::Extent2D {
                width: u32::MAX,
                height: 1080,
            }),
            None
        );
        assert_eq!(
            extent_tuple(vk::Extent2D {
                width: 1920,
                height: 1080,
            }),
            Some((1920, 1080))
        );
    }
