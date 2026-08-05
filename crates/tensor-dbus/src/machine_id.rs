use std::{fmt, str::FromStr};

use crate::{Error, Result};

/// A D-Bus machine identity encoded as 32 hexadecimal digits.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MachineId([u8; 16]);

impl MachineId {
    pub fn parse_bytes(bytes: &[u8]) -> Result<Self> {
        let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
        if bytes.len() != 32 {
            return Err(invalid(bytes));
        }
        let mut value = [0_u8; 16];
        for (output, pair) in value.iter_mut().zip(bytes.chunks_exact(2)) {
            *output = (hex(pair[0]).ok_or_else(|| invalid(bytes))? << 4)
                | hex(pair[1]).ok_or_else(|| invalid(bytes))?;
        }
        Ok(Self(value))
    }

    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl FromStr for MachineId {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        Self::parse_bytes(value.as_bytes())
    }
}

impl fmt::Display for MachineId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for MachineId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("MachineId")
            .field(&self.to_string())
            .finish()
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

fn invalid(bytes: &[u8]) -> Error {
    Error::InvalidMachineId(String::from_utf8_lossy(bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_ids_parse_normalize_and_allow_file_newline() {
        let id = MachineId::parse_bytes(b"0123456789ABCDEF0123456789abcdef\n").unwrap();
        assert_eq!(id.to_string(), "0123456789abcdef0123456789abcdef");
        assert_eq!(id.as_bytes()[0], 0x01);
        assert!("short".parse::<MachineId>().is_err());
        assert!(
            "0123456789abcdef0123456789abcdeg"
                .parse::<MachineId>()
                .is_err()
        );
    }
}
