//! WE image graph target model.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum WeTarget {
    SourceTexture,
    ImageLocalMain,
    ImageLocalSub,
    NamedFbo(u32),
    FirstClassEffectTarget,
    Scene,
}

impl WeTarget {
    pub fn is_render_target(self) -> bool {
        !matches!(self, Self::SourceTexture)
    }
}
