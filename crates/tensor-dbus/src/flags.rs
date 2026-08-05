use std::ops::{BitOr, BitOrAssign};

/// Standard flags carried by a D-Bus method call.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MethodCallFlags(u8);

impl MethodCallFlags {
    const KNOWN_BITS: u8 = 0x7;

    pub const NO_REPLY_EXPECTED: Self = Self(0x1);
    pub const NO_AUTO_START: Self = Self(0x2);
    pub const ALLOW_INTERACTIVE_AUTH: Self = Self(0x4);

    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn contains(self, flag: Self) -> bool {
        self.0 & flag.0 == flag.0
    }

    pub(crate) const fn without(self, flag: Self) -> Self {
        Self(self.0 & !flag.0)
    }

    pub(crate) const fn from_bits_truncate(bits: u8) -> Self {
        Self(bits & Self::KNOWN_BITS)
    }
}

impl BitOr for MethodCallFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for MethodCallFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_call_flags_compose_without_unknown_bits() {
        let flags = MethodCallFlags::NO_AUTO_START | MethodCallFlags::ALLOW_INTERACTIVE_AUTH;
        assert_eq!(flags.bits(), 0x6);
        assert!(flags.contains(MethodCallFlags::NO_AUTO_START));
        assert!(!flags.contains(MethodCallFlags::NO_REPLY_EXPECTED));
        assert_eq!(MethodCallFlags::from_bits_truncate(0xff).bits(), 0x7);
    }
}
