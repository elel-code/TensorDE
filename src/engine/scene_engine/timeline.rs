//! Timeline sampling contracts.
//!
//! References:
//! - `reverse-engineered/docs/exe/model-and-animation.md`
//! - `reverse-engineered/docs/exe/formulas.md`

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SceneSampleClock {
    pub time_ms: u64,
    pub frame_rate: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SceneTimelineSample {
    pub opacity: f32,
    pub transform_x: [f32; 4],
    pub transform_y: [f32; 4],
}
