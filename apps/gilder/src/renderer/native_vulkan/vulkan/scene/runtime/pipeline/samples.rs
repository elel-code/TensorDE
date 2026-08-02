//! Rasterization sample policy for scene pipelines.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScenePipelineSamples {
    Single,
    SceneColor4x,
}

impl ScenePipelineSamples {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Single => "1x",
            Self::SceneColor4x => "4x",
        }
    }
}
