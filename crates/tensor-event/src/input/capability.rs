//! Device capability bits independent of libinput object lifetimes.

/// Opaque device identity from the adapter (stable for the session).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct DeviceId(pub u64);

impl DeviceId {
    #[inline]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    #[inline]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Session-local identity for a libinput physical device group.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct DeviceGroupId(pub u64);

impl DeviceGroupId {
    #[inline]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    #[inline]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Session-local identity assigned to one libinput tablet tool.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct TabletToolId(pub u64);

impl TabletToolId {
    #[inline]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    #[inline]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Which seat roles a physical device can feed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DeviceCapabilities {
    pub keyboard: bool,
    pub pointer: bool,
    pub touch: bool,
    pub tablet: bool,
}

impl DeviceCapabilities {
    #[inline]
    pub const fn empty() -> Self {
        Self {
            keyboard: false,
            pointer: false,
            touch: false,
            tablet: false,
        }
    }

    #[inline]
    pub const fn any(self) -> bool {
        self.keyboard || self.pointer || self.touch || self.tablet
    }

    /// Merge two capability sets (OR). Used when reconciling seat globals.
    #[inline]
    pub const fn union(self, other: Self) -> Self {
        Self {
            keyboard: self.keyboard || other.keyboard,
            pointer: self.pointer || other.pointer,
            touch: self.touch || other.touch,
            tablet: self.tablet || other.tablet,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_ors_bits() {
        let a = DeviceCapabilities {
            keyboard: true,
            ..DeviceCapabilities::empty()
        };
        let b = DeviceCapabilities {
            pointer: true,
            ..DeviceCapabilities::empty()
        };
        let u = a.union(b);
        assert!(u.keyboard && u.pointer && !u.touch);
    }
}
