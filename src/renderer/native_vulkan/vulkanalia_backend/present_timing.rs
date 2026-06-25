use std::env;

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{
    self, HasBuilder, KhrPresentWait2ExtensionDeviceCommands, KhrPresentWaitExtensionDeviceCommands,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct VulkanaliaPresentTimingConfig {
    pub(super) present_id_enabled: bool,
    pub(super) present_id2_enabled: bool,
    pub(super) present_wait_enabled: bool,
    pub(super) present_wait2_enabled: bool,
    pub(super) wait_after_present_enabled: bool,
}

impl VulkanaliaPresentTimingConfig {
    pub(super) fn new(
        present_id_enabled: bool,
        present_id2_enabled: bool,
        present_wait_enabled: bool,
        present_wait2_enabled: bool,
    ) -> Self {
        Self {
            present_id_enabled,
            present_id2_enabled,
            present_wait_enabled,
            present_wait2_enabled,
            wait_after_present_enabled: env_present_wait_after_present_enabled(),
        }
    }

    #[cfg(test)]
    pub(super) fn with_wait_after_present_enabled(mut self, enabled: bool) -> Self {
        self.wait_after_present_enabled = enabled;
        self
    }

    pub(super) fn present_id(self, present_frame_index: u32) -> Option<u64> {
        if self.present_id2_enabled || self.present_id_enabled {
            Some(u64::from(present_frame_index).saturating_add(1))
        } else {
            None
        }
    }

    pub(super) fn present_id_mode(self) -> &'static str {
        if self.present_id2_enabled {
            "present-id2-khr"
        } else if self.present_id_enabled {
            "present-id-khr"
        } else {
            "disabled"
        }
    }

    pub(super) fn present_wait_mode(self) -> &'static str {
        if !self.wait_after_present_enabled {
            "disabled"
        } else if self.present_wait2_enabled
            && (self.present_id2_enabled || self.present_id_enabled)
        {
            "present-wait2-khr"
        } else if self.present_wait_enabled && (self.present_id2_enabled || self.present_id_enabled)
        {
            "present-wait-khr"
        } else {
            "disabled"
        }
    }

    pub(super) fn wait_after_queue_present(
        self,
        device: &Device,
        swapchain: vk::SwapchainKHR,
        present_id: Option<u64>,
        label: &'static str,
    ) -> Result<bool, String> {
        if !self.wait_after_present_enabled {
            return Ok(false);
        }

        let Some(present_id) = present_id else {
            return Ok(false);
        };

        if self.present_wait2_enabled {
            let wait_info = vk::PresentWait2InfoKHR::builder()
                .present_id(present_id)
                .timeout(u64::MAX)
                .build();
            unsafe {
                device
                    .wait_for_present2_khr(swapchain, &wait_info)
                    .map_err(|err| format!("vkWaitForPresent2KHR(vulkanalia {label}): {err:?}"))?;
            }
            Ok(true)
        } else if self.present_wait_enabled {
            unsafe {
                device
                    .wait_for_present_khr(swapchain, present_id, u64::MAX)
                    .map_err(|err| format!("vkWaitForPresentKHR(vulkanalia {label}): {err:?}"))?;
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

fn env_present_wait_after_present_enabled() -> bool {
    env::var("GILDER_VULKAN_PRESENT_WAIT_AFTER_PRESENT")
        .ok()
        .and_then(|value| parse_env_bool(&value))
        .unwrap_or(false)
}

fn parse_env_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn present_timing_prefers_present_id2() {
        let timing = VulkanaliaPresentTimingConfig::new(true, true, true, true)
            .with_wait_after_present_enabled(true);

        assert_eq!(timing.present_id(0), Some(1));
        assert_eq!(timing.present_id(41), Some(42));
        assert_eq!(timing.present_id_mode(), "present-id2-khr");
        assert_eq!(timing.present_wait_mode(), "present-wait2-khr");
    }

    #[test]
    fn present_timing_can_fall_back_to_present_id() {
        let timing = VulkanaliaPresentTimingConfig::new(true, false, true, false)
            .with_wait_after_present_enabled(true);

        assert_eq!(timing.present_id(0), Some(1));
        assert_eq!(timing.present_id_mode(), "present-id-khr");
        assert_eq!(timing.present_wait_mode(), "present-wait-khr");
    }

    #[test]
    fn present_timing_can_be_disabled() {
        let timing = VulkanaliaPresentTimingConfig::new(false, false, false, false);

        assert_eq!(timing.present_id(0), None);
        assert_eq!(timing.present_id_mode(), "disabled");
        assert_eq!(timing.present_wait_mode(), "disabled");
    }

    #[test]
    fn present_wait_requires_a_present_id_source() {
        let timing = VulkanaliaPresentTimingConfig::new(false, false, true, true)
            .with_wait_after_present_enabled(true);

        assert_eq!(timing.present_wait_mode(), "disabled");
    }

    #[test]
    fn present_wait_is_diagnostic_only_by_default() {
        let timing = VulkanaliaPresentTimingConfig::new(true, true, true, true)
            .with_wait_after_present_enabled(false);

        assert_eq!(timing.present_wait_mode(), "disabled");
    }

    #[test]
    fn parse_env_bool_accepts_common_spellings() {
        assert_eq!(parse_env_bool("1"), Some(true));
        assert_eq!(parse_env_bool("on"), Some(true));
        assert_eq!(parse_env_bool("false"), Some(false));
        assert_eq!(parse_env_bool("no"), Some(false));
        assert_eq!(parse_env_bool("maybe"), None);
    }
}
