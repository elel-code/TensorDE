use std::ops::{BitOr, BitOrAssign};

use crate::{Error, Result};

const MAX_NAME_LEN: usize = 255;

pub(crate) fn validate_bus_name(value: &str, kind: &'static str) -> Result<()> {
    if is_unique_name(value) || is_well_known_name(value) {
        Ok(())
    } else {
        Err(invalid_name(kind, value))
    }
}

pub(crate) fn validate_unique_name(value: &str, kind: &'static str) -> Result<()> {
    if is_unique_name(value) {
        Ok(())
    } else {
        Err(invalid_name(kind, value))
    }
}

pub(crate) fn validate_well_known_name(value: &str, kind: &'static str) -> Result<()> {
    if is_well_known_name(value) {
        Ok(())
    } else {
        Err(invalid_name(kind, value))
    }
}

pub(crate) fn validate_interface_name(value: &str, kind: &'static str) -> Result<()> {
    if is_interface_name(value) {
        Ok(())
    } else {
        Err(invalid_name(kind, value))
    }
}

pub(crate) fn validate_error_name(value: &str, kind: &'static str) -> Result<()> {
    validate_interface_name(value, kind)
}

pub(crate) fn validate_member_name(value: &str, kind: &'static str) -> Result<()> {
    let bytes = value.as_bytes();
    if bytes.len() <= MAX_NAME_LEN
        && bytes
            .first()
            .is_some_and(|byte| is_alpha_or_underscore(*byte))
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        Ok(())
    } else {
        Err(invalid_name(kind, value))
    }
}

fn is_unique_name(value: &str) -> bool {
    if value == "org.freedesktop.DBus" {
        return true;
    }
    let bytes = value.as_bytes();
    bytes.len() <= MAX_NAME_LEN
        && bytes
            .strip_prefix(b":")
            .is_some_and(|name| validate_dotted(name, is_bus_name_byte, is_bus_name_byte))
}

fn is_well_known_name(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() <= MAX_NAME_LEN && validate_dotted(bytes, is_well_known_start, is_bus_name_byte)
}

fn is_interface_name(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() <= MAX_NAME_LEN
        && validate_dotted(bytes, is_alpha_or_underscore, |byte| {
            byte.is_ascii_alphanumeric() || byte == b'_'
        })
}

fn validate_dotted(
    bytes: &[u8],
    valid_start: impl Fn(u8) -> bool,
    valid_byte: impl Fn(u8) -> bool,
) -> bool {
    bytes.contains(&b'.')
        && bytes.split(|byte| *byte == b'.').all(|element| {
            element.first().is_some_and(|byte| valid_start(*byte))
                && element[1..].iter().all(|byte| valid_byte(*byte))
        })
}

fn is_well_known_start(byte: u8) -> bool {
    is_alpha_or_underscore(byte) || byte == b'-'
}

fn is_alpha_or_underscore(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_bus_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

fn invalid_name(kind: &'static str, value: &str) -> Error {
    Error::InvalidName {
        kind,
        value: value.to_owned(),
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RequestNameFlags(u32);

impl RequestNameFlags {
    pub const ALLOW_REPLACEMENT: Self = Self(1);
    pub const REPLACE_EXISTING: Self = Self(2);
    pub const DO_NOT_QUEUE: Self = Self(4);

    pub const fn bits(self) -> u32 {
        self.0
    }
}

impl BitOr for RequestNameFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for RequestNameFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestNameReply {
    PrimaryOwner,
    InQueue,
    Exists,
    AlreadyOwner,
}

impl TryFrom<u32> for RequestNameReply {
    type Error = Error;

    fn try_from(value: u32) -> Result<Self> {
        match value {
            1 => Ok(Self::PrimaryOwner),
            2 => Ok(Self::InQueue),
            3 => Ok(Self::Exists),
            4 => Ok(Self::AlreadyOwner),
            _ => Err(Error::InvalidMessage(format!(
                "unknown RequestName reply {value}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseNameReply {
    Released,
    NonExistent,
    NotOwner,
}

impl TryFrom<u32> for ReleaseNameReply {
    type Error = Error;

    fn try_from(value: u32) -> Result<Self> {
        match value {
            1 => Ok(Self::Released),
            2 => Ok(Self::NonExistent),
            3 => Ok(Self::NotOwner),
            _ => Err(Error::InvalidMessage(format!(
                "unknown ReleaseName reply {value}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bus_names_follow_well_known_and_unique_rules() {
        for valid in [
            "org.tensor.Service-for_you",
            "_org.-service",
            ":1.42",
            ":org.tensor.Service-for_you",
            "org.freedesktop.DBus",
        ] {
            validate_bus_name(valid, "bus name").unwrap();
        }
        for invalid in [
            "",
            "no-dots",
            ".leading.dot",
            "trailing.dot.",
            "double..dots",
            "1st.element",
            "org.2nd",
            ":no-dots",
            ":double..dots",
            "org.tensor.non_ascii_\u{e9}",
        ] {
            assert!(validate_bus_name(invalid, "bus name").is_err(), "{invalid}");
        }
    }

    #[test]
    fn interface_error_and_member_names_are_stricter_than_bus_names() {
        for valid in ["org.tensor.Interface_1", "_org.tensor"] {
            validate_interface_name(valid, "interface name").unwrap();
            validate_error_name(valid, "error name").unwrap();
        }
        for invalid in [
            "org.tensor-with-dash",
            "org.2nd",
            "no_dot",
            "org.tensor.\u{e9}",
        ] {
            assert!(
                validate_interface_name(invalid, "interface name").is_err(),
                "{invalid}"
            );
        }
        for valid in ["Ping", "_private_1"] {
            validate_member_name(valid, "member name").unwrap();
        }
        for invalid in ["", "1Ping", "has.dot", "has-dash", "non_ascii_\u{e9}"] {
            assert!(
                validate_member_name(invalid, "member name").is_err(),
                "{invalid}"
            );
        }
    }

    #[test]
    fn every_name_kind_enforces_the_255_byte_limit() {
        let valid_bus = format!("a.{}", "b".repeat(MAX_NAME_LEN - 2));
        let long_bus = format!("a.{}", "b".repeat(MAX_NAME_LEN - 1));
        validate_bus_name(&valid_bus, "bus name").unwrap();
        assert!(validate_bus_name(&long_bus, "bus name").is_err());

        let valid_member = format!("M{}", "a".repeat(MAX_NAME_LEN - 1));
        let long_member = format!("M{}", "a".repeat(MAX_NAME_LEN));
        validate_member_name(&valid_member, "member name").unwrap();
        assert!(validate_member_name(&long_member, "member name").is_err());
    }
}
