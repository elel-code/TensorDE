//! Rasterization sample policy for scene pipelines.

use vulkanalia::vk;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScenePipelineSamples {
    Single,
    SceneColor4x,
}

impl ScenePipelineSamples {
    pub(super) const fn rasterization_samples(self) -> vk::SampleCountFlags {
        match self {
            Self::Single => vk::SampleCountFlags::_1,
            Self::SceneColor4x => vk::SampleCountFlags::_4,
        }
    }
}
