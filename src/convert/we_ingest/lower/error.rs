use crate::engine::scene::SceneRenderTargetKind;

#[derive(Debug)]
pub enum WeLowerError {
    MissingResourcePayload(u32),
    InvalidTextureMipRange(u32),
    MissingPreviousGraphTarget {
        graph_index: usize,
        pass_id: u32,
        slot: u32,
    },
    IncompatibleImageTargetSpec {
        role: SceneRenderTargetKind,
        name: String,
    },
}

impl std::fmt::Display for WeLowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingResourcePayload(handle) => {
                write!(f, "IR resource {handle} has no payload range")
            }
            Self::InvalidTextureMipRange(resource) => {
                write!(
                    f,
                    "IR texture resource {resource} has an invalid mip payload range"
                )
            }
            Self::MissingPreviousGraphTarget {
                graph_index,
                pass_id,
                slot,
            } => write!(
                f,
                "IR render graph {graph_index} pass {pass_id} samples previous target in slot {slot}, but no previous pass exists"
            ),
            Self::IncompatibleImageTargetSpec { role, name } => write!(
                f,
                "IR image target {role:?}:{name} has conflicting format or scale declarations"
            ),
        }
    }
}

impl std::error::Error for WeLowerError {}
