use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ViewId(u64);

impl ViewId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct WorkspaceId(u32);

impl WorkspaceId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Stable identity for a Wayland surface owned by the compositor.
///
/// The protocol layer assigns this value when a surface enters the compositor
/// scene.  It is deliberately separate from Smithay's `ObjectId` so scene and
/// ECS data never depend on a live Wayland resource.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct SurfaceId(u64);

impl SurfaceId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Stable identity for an imported client buffer.
///
/// This is not a file descriptor and must not be derived from one.  The
/// renderer owns the Vulkan image associated with the value; ECS and scene
/// snapshots only carry this compact reference.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct SurfaceBufferId(u64);

impl SurfaceBufferId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_have_transparent_wire_representations() {
        assert_eq!(serde_json::to_string(&ViewId::new(7)).unwrap(), "7");
        assert_eq!(serde_json::to_string(&WorkspaceId::new(3)).unwrap(), "3");
        assert_eq!(serde_json::to_string(&SurfaceId::new(11)).unwrap(), "11");
        assert_eq!(
            serde_json::to_string(&SurfaceBufferId::new(13)).unwrap(),
            "13"
        );
    }
}
