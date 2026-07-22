use serde::{Deserialize, Serialize};

use super::{SceneObjectHandle, SceneStringId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneUserPropertyTarget {
    Visible,
}

impl SceneUserPropertyTarget {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Visible => 1,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Visible),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneUserPropertyPredicate {
    BooleanEquals(bool),
    StringEquals(SceneStringId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneUserPropertyBindingRecord {
    pub object: SceneObjectHandle,
    pub property: SceneStringId,
    pub target: SceneUserPropertyTarget,
    pub predicate: SceneUserPropertyPredicate,
}
