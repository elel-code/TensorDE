use serde::{Deserialize, Serialize};

/// Stable compositor identity for a protocol surface.
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
/// This value is never a file descriptor or native renderer handle.
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
        assert_eq!(serde_json::to_string(&SurfaceId::new(11)).unwrap(), "11");
        assert_eq!(
            serde_json::to_string(&SurfaceBufferId::new(13)).unwrap(),
            "13"
        );
    }
}
