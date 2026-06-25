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
        }
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
        if self.present_wait2_enabled && (self.present_id2_enabled || self.present_id_enabled) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn present_timing_prefers_present_id2() {
        let timing = VulkanaliaPresentTimingConfig::new(true, true, true, true);

        assert_eq!(timing.present_id(0), Some(1));
        assert_eq!(timing.present_id(41), Some(42));
        assert_eq!(timing.present_id_mode(), "present-id2-khr");
        assert_eq!(timing.present_wait_mode(), "present-wait2-khr");
    }

    #[test]
    fn present_timing_can_fall_back_to_present_id() {
        let timing = VulkanaliaPresentTimingConfig::new(true, false, true, false);

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
        let timing = VulkanaliaPresentTimingConfig::new(false, false, true, true);

        assert_eq!(timing.present_wait_mode(), "disabled");
    }
}
