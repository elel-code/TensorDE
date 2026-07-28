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
    PreviousGraphTarget {
        slot: u32,
    },
    GraphTarget {
        slot: u32,
        role: RenderTargetRole,
        name: Option<String>,
    },
    NamedFboBind {
        slot: u32,
        name: String,
    },
    EffectTarget {
        slot: u32,
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
