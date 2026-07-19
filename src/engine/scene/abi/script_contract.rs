//! Typed semantic contracts lowered from supported SceneScript property bindings.

use serde::{Deserialize, Serialize};

use super::SceneObjectHandle;
use super::SceneStringId;

/// Runtime event classes which may dirty a SceneScript module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneScriptSubscriptions(pub u32);

impl SceneScriptSubscriptions {
    pub const NONE: Self = Self(0);
    pub const INITIALIZE: Self = Self(1 << 0);
    pub const FRAME: Self = Self(1 << 1);
    pub const AUDIO: Self = Self(1 << 2);
    pub const POINTER: Self = Self(1 << 3);
    pub const LOCAL_TIME: Self = Self(1 << 4);
    pub const MEDIA: Self = Self(1 << 5);
    pub const USER_PROPERTY: Self = Self(1 << 6);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Property owned by semantic ECS and writable by one script delta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneScriptTarget {
    Origin,
    Angles,
    Scale,
    Color,
    Alpha,
    Visible,
    Text,
    TechCircleSectorWidth,
}

impl SceneScriptTarget {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Origin => 1,
            Self::Angles => 2,
            Self::Scale => 3,
            Self::Color => 4,
            Self::Alpha => 5,
            Self::Visible => 6,
            Self::Text => 7,
            Self::TechCircleSectorWidth => 8,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Origin),
            2 => Some(Self::Angles),
            3 => Some(Self::Scale),
            4 => Some(Self::Color),
            5 => Some(Self::Alpha),
            6 => Some(Self::Visible),
            7 => Some(Self::Text),
            8 => Some(Self::TechCircleSectorWidth),
            _ => None,
        }
    }

    pub const fn is_vector(self) -> bool {
        matches!(
            self,
            Self::Origin | Self::Angles | Self::Scale | Self::Color
        )
    }
}

/// Canonical source module and host binding persisted in `.gscene`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneScriptProgramRecord {
    pub object: SceneObjectHandle,
    pub target: SceneScriptTarget,
    pub source: SceneStringId,
    pub properties_json: SceneStringId,
    pub initial_text: SceneStringId,
    pub subscriptions: SceneScriptSubscriptions,
    pub initial_numeric: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneAudioBandMaterialTarget {
    TechCircleSectorWidth,
}
