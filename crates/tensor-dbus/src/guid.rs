use std::{fmt, str::FromStr};

use crate::{Error, Result};

/// A 128-bit D-Bus server identity encoded as 32 hexadecimal digits.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct Guid([u8; 16]);

impl Guid {
    pub fn generate() -> Result<Self> {
        let mut bytes = [0_u8; 16];
        rustix::rand::getrandom(&mut bytes, rustix::rand::GetRandomFlags::empty())
            .map_err(|error| Error::Io(error.into()))?;
        Ok(Self(bytes))
    }

    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    pub(crate) fn parse_bytes(value: &[u8]) -> Option<Self> {
        if value.len() != 32 {
            return None;
        }
        let mut bytes = [0_u8; 16];
        for (index, pair) in value.chunks_exact(2).enumerate() {
            bytes[index] = (hex(pair[0])? << 4) | hex(pair[1])?;
        }
        Some(Self(bytes))
    }
}

impl FromStr for Guid {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        Self::parse_bytes(value.as_bytes()).ok_or_else(|| Error::InvalidGuid(value.to_owned()))
    }
}

impl fmt::Display for Guid {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Guid {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

const fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guid_round_trips_and_normalizes_hex() {
        let guid: Guid = "0123456789abcdef0123456789ABCDEF".parse().unwrap();
        assert_eq!(guid.to_string(), "0123456789abcdef0123456789abcdef");
        assert!("short".parse::<Guid>().is_err());
        assert!("0123456789abcdef0123456789abcdeg".parse::<Guid>().is_err());
    }

    #[test]
    fn generated_guids_are_not_constant() {
        assert_ne!(Guid::generate().unwrap(), Guid::generate().unwrap());
    }
}
