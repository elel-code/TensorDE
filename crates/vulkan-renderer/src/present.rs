use vulkanalia::vk;

use crate::Features;

/// Backend-neutral present modes with a one-to-one Vulkan mapping.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PresentMode {
    Immediate,
    Mailbox,
    Fifo,
    FifoRelaxed,
    /// `VK_PRESENT_MODE_FIFO_LATEST_READY`. This mode is usable only when
    /// the device extension and feature were enabled *and* the target surface
    /// advertises the mode.
    FifoLatestReady,
}

impl PresentMode {
    pub const fn as_vk(self) -> vk::PresentModeKHR {
        match self {
            Self::Immediate => vk::PresentModeKHR::IMMEDIATE,
            Self::Mailbox => vk::PresentModeKHR::MAILBOX,
            Self::Fifo => vk::PresentModeKHR::FIFO,
            Self::FifoRelaxed => vk::PresentModeKHR::FIFO_RELAXED,
            Self::FifoLatestReady => vk::PresentModeKHR::FIFO_LATEST_READY,
        }
    }

    fn from_vk(mode: vk::PresentModeKHR) -> Option<Self> {
        match mode {
            vk::PresentModeKHR::IMMEDIATE => Some(Self::Immediate),
            vk::PresentModeKHR::MAILBOX => Some(Self::Mailbox),
            vk::PresentModeKHR::FIFO => Some(Self::Fifo),
            vk::PresentModeKHR::FIFO_RELAXED => Some(Self::FifoRelaxed),
            vk::PresentModeKHR::FIFO_LATEST_READY => Some(Self::FifoLatestReady),
            _ => None,
        }
    }
}

/// Present modes reported for one concrete Vulkan surface.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SurfacePresentCapabilities {
    modes: Vec<PresentMode>,
}

impl SurfacePresentCapabilities {
    /// Converts `vkGetPhysicalDeviceSurfacePresentModesKHR` output without
    /// synthesizing modes the surface did not report.
    pub fn from_vk(modes: &[vk::PresentModeKHR]) -> Self {
        let mut modes = modes
            .iter()
            .copied()
            .filter_map(PresentMode::from_vk)
            .collect::<Vec<_>>();
        modes.sort_by_key(|mode| *mode as u8);
        modes.dedup();
        Self { modes }
    }

    pub fn modes(&self) -> &[PresentMode] {
        &self.modes
    }

    /// Checks both surface support and the device feature gate. In particular,
    /// extension availability alone never makes FIFO latest-ready usable.
    pub fn supports(&self, mode: PresentMode, enabled_features: Features) -> bool {
        self.modes.contains(&mode)
            && (mode != PresentMode::FifoLatestReady
                || enabled_features.contains(Features::FIFO_LATEST_READY))
    }

    /// Returns the first fully usable preference. There is no implicit
    /// downgrade; callers include `Fifo` explicitly when it is acceptable.
    pub fn choose(
        &self,
        preferences: &[PresentMode],
        enabled_features: Features,
    ) -> Option<PresentMode> {
        preferences
            .iter()
            .copied()
            .find(|mode| self.supports(*mode, enabled_features))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifo_latest_ready_requires_device_feature_and_surface_mode() {
        let surface = SurfacePresentCapabilities::from_vk(&[
            vk::PresentModeKHR::FIFO,
            vk::PresentModeKHR::FIFO_LATEST_READY,
        ]);
        assert!(!surface.supports(PresentMode::FifoLatestReady, Features::empty()));
        assert!(surface.supports(PresentMode::FifoLatestReady, Features::FIFO_LATEST_READY));

        let fifo_only = SurfacePresentCapabilities::from_vk(&[vk::PresentModeKHR::FIFO]);
        assert!(!fifo_only.supports(PresentMode::FifoLatestReady, Features::FIFO_LATEST_READY));
    }

    #[test]
    fn selection_does_not_hide_a_present_mode_fallback() {
        let surface = SurfacePresentCapabilities::from_vk(&[vk::PresentModeKHR::FIFO]);
        assert_eq!(
            surface.choose(
                &[PresentMode::FifoLatestReady, PresentMode::Fifo],
                Features::FIFO_LATEST_READY,
            ),
            Some(PresentMode::Fifo)
        );
    }
}
