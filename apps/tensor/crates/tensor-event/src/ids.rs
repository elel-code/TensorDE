//! Stable identity handles used on the event bus.
//!
//! These are opaque to this crate. The compositor maps them to ECS / protocol
//! IDs at the adapter boundary so neither side holds the other's types.

/// Output / CRTC identity on the event bus.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct OutputId(pub u64);

/// Wayland surface identity (protocol object id or compositor-stable key).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct SurfaceId(pub u64);

/// ECS / scene view identity on the event bus.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct ViewId(pub u64);

impl OutputId {
    #[inline]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    #[inline]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl SurfaceId {
    #[inline]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    #[inline]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl ViewId {
    #[inline]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    #[inline]
    pub const fn get(self) -> u64 {
        self.0
    }
}
