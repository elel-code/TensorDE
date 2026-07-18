//! Typed semantic contracts lowered from supported SceneScript property bindings.

use serde::{Deserialize, Serialize};

use super::SceneObjectHandle;
use super::SceneStringId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneTextProviderKind {
    ChineseLunarCalendar,
    ChineseWeekday,
    ChineseSolarTerm,
    ChineseMonthDay,
    ChineseYear,
    ChineseClock,
}

impl SceneTextProviderKind {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::ChineseLunarCalendar => 1,
            Self::ChineseWeekday => 2,
            Self::ChineseSolarTerm => 3,
            Self::ChineseMonthDay => 4,
            Self::ChineseYear => 5,
            Self::ChineseClock => 6,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::ChineseLunarCalendar),
            2 => Some(Self::ChineseWeekday),
            3 => Some(Self::ChineseSolarTerm),
            4 => Some(Self::ChineseMonthDay),
            5 => Some(Self::ChineseYear),
            6 => Some(Self::ChineseClock),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneTextProviderRecord {
    pub object: SceneObjectHandle,
    pub kind: SceneTextProviderKind,
    pub initial_text: SceneStringId,
    pub source_data: SceneStringId,
    pub update_interval_seconds: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneAudioBandMaterialTarget {
    TechCircleSectorWidth,
    ObjectUniformScale,
}

impl SceneAudioBandMaterialTarget {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::TechCircleSectorWidth => 1,
            Self::ObjectUniformScale => 2,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::TechCircleSectorWidth),
            2 => Some(Self::ObjectUniformScale),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneAudioBandMaterialBindingRecord {
    pub object: SceneObjectHandle,
    pub target: SceneAudioBandMaterialTarget,
    pub spectrum_resolution: u32,
    pub band_index: u32,
    pub smoothing: f32,
    pub minimum_multiplier: f32,
    pub maximum_multiplier: f32,
    pub initial_value: f32,
}
