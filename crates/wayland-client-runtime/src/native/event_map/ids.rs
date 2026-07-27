//! Surface id map and mapping focus state.

use std::collections::HashMap;

use crate::native::shell::NativeSurfaceId;
use crate::surface::SurfaceId;

/// Bidirectional id map for native ↔ public surface identifiers.
#[derive(Clone, Debug, Default)]
pub struct SurfaceIdMap {
    native_to_public: HashMap<NativeSurfaceId, SurfaceId>,
    next_public: u64,
}

impl SurfaceIdMap {
    pub fn new() -> Self {
        Self {
            native_to_public: HashMap::new(),
            next_public: 1,
        }
    }

    /// Allocate or reuse a public [`SurfaceId`] for a native surface.
    pub fn intern(&mut self, native: NativeSurfaceId) -> SurfaceId {
        *self.native_to_public.entry(native).or_insert_with(|| {
            let id = SurfaceId(self.next_public);
            self.next_public = self.next_public.saturating_add(1);
            id
        })
    }

    pub fn get(&self, native: NativeSurfaceId) -> Option<SurfaceId> {
        self.native_to_public.get(&native).copied()
    }

    pub fn remove(&mut self, native: NativeSurfaceId) -> Option<SurfaceId> {
        self.native_to_public.remove(&native)
    }
}

/// Mutable mapping context (seat + focus) for serial-bearing events.
#[derive(Clone, Debug, Default)]
pub struct NativeEventMapState {
    pub keyboard_focus: Option<NativeSurfaceId>,
    pub pointer_focus: Option<NativeSurfaceId>,
    pub pointer_pos: (f64, f64),
    pub gesture_surface: Option<NativeSurfaceId>,
    pub dnd_surface: Option<NativeSurfaceId>,
    /// Latest input serial from the native shell (updated by drain helpers).
    pub last_serial: u32,
}
