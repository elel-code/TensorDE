use crate::core::SceneBlendMode;

const SCENE_BLEND_MODE_COUNT: usize = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaScenePipelineBlendUsage {
    enabled: [bool; SCENE_BLEND_MODE_COUNT],
}

impl VulkanaliaScenePipelineBlendUsage {
    pub(in crate::renderer::native_vulkan::vulkan) fn all() -> Self {
        Self {
            enabled: [true; SCENE_BLEND_MODE_COUNT],
        }
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn from_modes(
        modes: impl IntoIterator<Item = SceneBlendMode>,
    ) -> Self {
        let mut usage = Self {
            enabled: [false; SCENE_BLEND_MODE_COUNT],
        };
        for mode in modes {
            usage.enabled[scene_blend_mode_index(mode)] = true;
        }
        usage
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn contains(self, mode: SceneBlendMode) -> bool {
        self.enabled[scene_blend_mode_index(mode)]
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn enabled_count(self) -> u32 {
        self.enabled.iter().filter(|enabled| **enabled).count() as u32
    }
}

fn scene_blend_mode_index(mode: SceneBlendMode) -> usize {
    match mode {
        SceneBlendMode::Alpha => 0,
        SceneBlendMode::Normal => 1,
        SceneBlendMode::Additive => 2,
        SceneBlendMode::Multiply => 3,
        SceneBlendMode::Screen => 4,
        SceneBlendMode::Max => 5,
        SceneBlendMode::Modulate => 6,
        SceneBlendMode::HslColor => 7,
        SceneBlendMode::AlphaToCoverage => 8,
    }
}
