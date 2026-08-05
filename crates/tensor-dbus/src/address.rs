use std::{env, os::unix::ffi::OsStringExt, path::PathBuf};

use crate::{Error, Guid, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BusKind {
    Session,
    System,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BusAddress {
    endpoints: Vec<BusEndpoint>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BusEndpoint {
    path: PathBuf,
    guid: Option<Guid>,
}

impl BusEndpoint {
    pub(crate) fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub(crate) const fn guid(&self) -> Option<Guid> {
        self.guid
    }
}

impl BusAddress {
    pub fn session() -> Result<Self> {
        let value = env::var("DBUS_SESSION_BUS_ADDRESS")
            .map_err(|_| Error::AddressUnavailable("DBUS_SESSION_BUS_ADDRESS"))?;
        Self::parse(&value)
    }

    pub fn system() -> Result<Self> {
        match env::var("DBUS_SYSTEM_BUS_ADDRESS") {
            Ok(value) => Self::parse(&value),
            Err(_) => Ok(Self {
                endpoints: vec![BusEndpoint {
                    path: PathBuf::from("/run/dbus/system_bus_socket"),
                    guid: None,
                }],
            }),
        }
    }

    pub fn for_kind(kind: BusKind) -> Result<Self> {
        match kind {
            BusKind::Session => Self::session(),
            BusKind::System => Self::system(),
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        let mut unsupported = None;
        let mut endpoints = Vec::new();
        for candidate in value.split(';').filter(|part| !part.is_empty()) {
            let Some(options) = candidate.strip_prefix("unix:") else {
                unsupported.get_or_insert_with(|| {
                    candidate.split(':').next().unwrap_or(candidate).to_owned()
                });
                continue;
            };
            let mut path = None;
            let mut guid = None;
            for option in options.split(',') {
                if let Some(encoded) = option.strip_prefix("path=") {
                    if path.is_some() {
                        return Err(Error::InvalidAddress(value.to_owned()));
                    }
                    let decoded = percent_decode(encoded)?;
                    if decoded.is_empty() || decoded.contains(&0) {
                        return Err(Error::InvalidAddress(value.to_owned()));
                    }
                    path = Some(PathBuf::from(std::ffi::OsString::from_vec(decoded)));
                }
                if let Some(encoded) = option.strip_prefix("abstract=") {
                    if path.is_some() {
                        return Err(Error::InvalidAddress(value.to_owned()));
                    }
                    let name = percent_decode(encoded)?;
                    if name.is_empty() || name.contains(&0) {
                        return Err(Error::InvalidAddress(value.to_owned()));
                    }
                    let mut address = Vec::with_capacity(name.len() + 1);
                    address.push(0);
                    address.extend_from_slice(&name);
                    path = Some(PathBuf::from(std::ffi::OsString::from_vec(address)));
                }
                if let Some(encoded) = option.strip_prefix("guid=") {
                    if guid.is_some() {
                        return Err(Error::InvalidAddress(value.to_owned()));
                    }
                    let decoded = percent_decode(encoded)?;
                    guid = Some(
                        Guid::parse_bytes(&decoded)
                            .ok_or_else(|| Error::InvalidAddress(value.to_owned()))?,
                    );
                }
            }
            if let Some(path) = path {
                endpoints.push(BusEndpoint { path, guid });
            }
        }
        if !endpoints.is_empty() {
            Ok(Self { endpoints })
        } else if let Some(transport) = unsupported {
            Err(Error::UnsupportedTransport(transport))
        } else {
            Err(Error::InvalidAddress(value.to_owned()))
        }
    }

    pub fn path(&self) -> &std::path::Path {
        self.endpoints[0].path()
    }

    pub(crate) fn endpoints(&self) -> &[BusEndpoint] {
        &self.endpoints
    }
}

fn percent_decode(value: &str) -> Result<Vec<u8>> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        let encoded = bytes
            .get(index + 1..index + 3)
            .ok_or_else(|| Error::InvalidAddress(value.to_owned()))?;
        let high = hex(encoded[0]).ok_or_else(|| Error::InvalidAddress(value.to_owned()))?;
        let low = hex(encoded[1]).ok_or_else(|| Error::InvalidAddress(value.to_owned()))?;
        decoded.push((high << 4) | low);
        index += 3;
    }
    Ok(decoded)
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
    use std::os::unix::ffi::OsStrExt;

    use super::*;

    #[test]
    fn parses_percent_encoded_unix_path() {
        let address = BusAddress::parse(
            "unix:path=/run/user/1000/bus%2dsocket,guid=0123456789abcdef0123456789ABCDEF",
        )
        .unwrap();
        assert_eq!(
            address.path(),
            std::path::Path::new("/run/user/1000/bus-socket")
        );
        assert_eq!(
            address.endpoints()[0].guid().unwrap().to_string(),
            "0123456789abcdef0123456789abcdef"
        );
    }

    #[test]
    fn selects_supported_address_from_a_list() {
        let address = BusAddress::parse("tcp:host=localhost;unix:path=/tmp/test-bus").unwrap();
        assert_eq!(address.path(), std::path::Path::new("/tmp/test-bus"));
    }

    #[test]
    fn preserves_unix_address_fallback_order() {
        let address = BusAddress::parse(
            "unix:path=/tmp/missing-bus;unix:abstract=tensor%2dfallback;tcp:host=localhost",
        )
        .unwrap();
        assert_eq!(address.endpoints().len(), 2);
        assert_eq!(
            address.endpoints()[0].path(),
            std::path::Path::new("/tmp/missing-bus")
        );
        assert_eq!(
            address.endpoints()[1].path().as_os_str().as_bytes(),
            b"\0tensor-fallback"
        );
    }

    #[test]
    fn parses_percent_encoded_abstract_unix_address() {
        let address =
            BusAddress::parse("unix:abstract=tensor%2dbus,guid=0123456789abcdef0123456789abcdef")
                .unwrap();
        assert_eq!(address.path().as_os_str().as_bytes(), b"\0tensor-bus");
    }

    #[test]
    fn rejects_empty_or_nul_containing_path() {
        assert!(BusAddress::parse("unix:path=").is_err());
        assert!(BusAddress::parse("unix:path=/tmp/test%00bus").is_err());
        assert!(BusAddress::parse("unix:abstract=test%00bus").is_err());
        assert!(BusAddress::parse("unix:path=/tmp/test,guid=short").is_err());
        assert!(
            BusAddress::parse("unix:path=/tmp/test,guid=0123456789abcdef0123456789abcdeg").is_err()
        );
    }

    #[test]
    fn rejects_ambiguous_or_duplicate_unix_options() {
        let guid = "0123456789abcdef0123456789abcdef";
        assert!(BusAddress::parse("unix:path=/tmp/one,path=/tmp/two").is_err());
        assert!(BusAddress::parse("unix:path=/tmp/test,abstract=tensor").is_err());
        assert!(
            BusAddress::parse(&format!("unix:path=/tmp/test,guid={guid},guid={guid}")).is_err()
        );
    }
}
