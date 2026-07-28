//! Internal scene semantic entity handles.
//!
//! References:
//! - `docs/gilder/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/gilder/docs/scene-format.md`

use super::super::abi::SceneObjectHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SemanticEntity(u32);

impl SemanticEntity {
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticEntityRecord {
    pub entity: SemanticEntity,
    pub object: SceneObjectHandle,
    pub object_index: u32,
}
