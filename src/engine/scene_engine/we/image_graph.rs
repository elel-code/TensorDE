//! WE image pass graph.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`

use crate::engine::scene_engine::SceneBlendContract;
use serde::Serialize;

use super::{WeEffectKind, WePassRole, WeTarget};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WeImageGraphStep {
    pub role: WePassRole,
    pub effect: Option<WeEffectKind>,
    pub input: WeTarget,
    pub output: WeTarget,
    pub blend: SceneBlendContract,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct WeImageGraph {
    pub steps: Vec<WeImageGraphStep>,
}

impl WeImageGraph {
    pub fn push(&mut self, step: WeImageGraphStep) {
        self.steps.push(step);
    }
}
