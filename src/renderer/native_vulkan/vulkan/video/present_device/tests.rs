    use super::*;

    #[test]
    fn queue_family_indices_are_deduped_for_single_family_device() {
        assert_eq!(video_present_queue_family_indices(3, 3), vec![3]);
        assert_eq!(video_present_queue_family_indices(3, 0), vec![3, 0]);
    }

    #[test]
    fn same_family_queue_indices_split_when_driver_exposes_multiple_queues() {
        assert_eq!(video_present_queue_indices(true, 1), (0, 0));
        assert_eq!(video_present_queue_indices(true, 2), (0, 1));
        assert_eq!(video_present_queue_indices(false, 2), (0, 0));
    }

    #[test]
    fn extension_union_keeps_first_order_and_dedupes() {
        let extensions = dedup_static_extensions([
            "VK_KHR_video_queue",
            "VK_KHR_swapchain",
            "VK_KHR_video_queue",
            "VK_KHR_present_wait2",
        ]);

        assert_eq!(
            extensions,
            vec![
                "VK_KHR_video_queue",
                "VK_KHR_swapchain",
                "VK_KHR_present_wait2"
            ]
        );
    }

    #[test]
    fn resource_sharing_model_names_the_real_boundary() {
        assert_eq!(
            decoded_image_resource_sharing_model(false),
            "concurrent-image-sharing-or-explicit-ownership-transfer-between-video-and-present-queue-families"
        );
        assert_eq!(
            video_present_queue_family_model(true, true),
            "single-video-graphics-present-queue-family-single-queue"
        );
        assert_eq!(
            video_present_queue_family_model(true, false),
            "single-video-graphics-present-queue-family-split-queue-indices"
        );
    }
