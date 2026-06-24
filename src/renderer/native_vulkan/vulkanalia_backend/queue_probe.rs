use vulkanalia::prelude::v1_4::*;

pub(super) fn native_vulkan_vulkanalia_video_decode_queue_family_indices(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
) -> Vec<u32> {
    let queue_families =
        unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
    video_decode_queue_family_indices_from_flags(
        &queue_families
            .iter()
            .map(|queue| queue.queue_flags)
            .collect::<Vec<_>>(),
    )
}

fn video_decode_queue_family_indices_from_flags(queue_flags: &[vk::QueueFlags]) -> Vec<u32> {
    queue_flags
        .iter()
        .enumerate()
        .filter_map(|(queue_family_index, flags)| {
            flags
                .contains(vk::QueueFlags::VIDEO_DECODE_KHR)
                .then_some(queue_family_index as u32)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_decode_queue_flag_is_the_backend_gate() {
        assert!(vk::QueueFlags::VIDEO_DECODE_KHR.contains(vk::QueueFlags::VIDEO_DECODE_KHR));
        assert!(!vk::QueueFlags::GRAPHICS.contains(vk::QueueFlags::VIDEO_DECODE_KHR));
    }

    #[test]
    fn video_decode_queue_indices_keep_actual_family_numbers() {
        let queue_flags = [
            vk::QueueFlags::GRAPHICS,
            vk::QueueFlags::VIDEO_DECODE_KHR,
            vk::QueueFlags::TRANSFER | vk::QueueFlags::VIDEO_DECODE_KHR,
        ];

        assert_eq!(
            video_decode_queue_family_indices_from_flags(&queue_flags),
            vec![1, 2]
        );
    }
}
