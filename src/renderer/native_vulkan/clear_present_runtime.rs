use std::time::Duration;

use super::{
    NativeVulkanError, NativeVulkanOptions, NativeVulkanVulkanaliaClearPresentOptions,
    NativeVulkanVulkanaliaClearPresentSnapshot, run_native_vulkan_vulkanalia_clear_present,
};

pub fn run_clear(
    options: NativeVulkanOptions,
    duration: Duration,
) -> Result<NativeVulkanVulkanaliaClearPresentSnapshot, NativeVulkanError> {
    run_native_vulkan_vulkanalia_clear_present(native_vulkan_clear_present_options(
        options, duration,
    ))
    .map_err(NativeVulkanError::Clear)
}

fn native_vulkan_clear_present_options(
    options: NativeVulkanOptions,
    duration: Duration,
) -> NativeVulkanVulkanaliaClearPresentOptions {
    NativeVulkanVulkanaliaClearPresentOptions {
        host: options.host,
        wait_configure_roundtrips: options.wait_configure_roundtrips,
        duration,
        target_max_fps: options.target_max_fps,
        clear_color: options.clear_color,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::native_wayland::NativeWaylandHostOptions;

    #[test]
    fn clear_present_options_keep_vulkanalia_runtime_inputs() {
        let options = native_vulkan_clear_present_options(
            NativeVulkanOptions {
                host: NativeWaylandHostOptions {
                    output_name: Some("HDMI-A-1".to_owned()),
                    ..Default::default()
                },
                target_max_fps: Some(240),
                ..Default::default()
            },
            Duration::ZERO,
        );

        assert_eq!(options.host.output_name.as_deref(), Some("HDMI-A-1"));
        assert_eq!(options.duration, Duration::ZERO);
        assert_eq!(options.target_max_fps, Some(240));
    }
}
