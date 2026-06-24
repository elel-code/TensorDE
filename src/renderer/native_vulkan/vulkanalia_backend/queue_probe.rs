use vulkanalia::prelude::v1_4::*;

pub(super) fn native_vulkan_vulkanalia_has_video_decode_queue_family(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
) -> bool {
    let queue_families =
        unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
    queue_families
        .iter()
        .any(|queue| queue.queue_flags.contains(vk::QueueFlags::VIDEO_DECODE_KHR))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_decode_queue_flag_is_the_backend_gate() {
        assert!(vk::QueueFlags::VIDEO_DECODE_KHR.contains(vk::QueueFlags::VIDEO_DECODE_KHR));
        assert!(!vk::QueueFlags::GRAPHICS.contains(vk::QueueFlags::VIDEO_DECODE_KHR));
    }
}
