use serde::{Deserialize, Serialize};

use super::target::RenderTargetRole;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum TextureBindingRole {
    SourceTexture,
    TextureSlot {
        slot: u32,
    },
    AlphaTextureSlot {
        slot: u32,
    },
    PreviousGraphTarget,
    GraphTarget {
        role: RenderTargetRole,
        name: Option<String>,
    },
    NamedFboBind {
        name: String,
    },
    EffectTarget {
        name: String,
    },
    VideoFrame {
        media_instance: u32,
    },
    AudioUniform,
    SystemUniform,
    PassConstant {
        name: String,
    },
}
