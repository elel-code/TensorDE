use std::{fmt, ops::Range, os::fd::OwnedFd, sync::Arc};

use serde::de::DeserializeOwned;
use zvariant::{
    Endian, Structure, StructureBuilder, Type,
    serialized::{Context, Data},
};

use super::{MessageKind, read_u32};
use crate::{DynamicBody, Error, MethodCallFlags, Result};

#[derive(Clone)]
pub struct Message {
    pub(super) kind: MessageKind,
    pub(super) flags: u8,
    pub(super) serial: u32,
    pub(super) reply_serial: Option<u32>,
    pub(super) path: Option<Range<usize>>,
    pub(super) interface: Option<Range<usize>>,
    pub(super) member: Option<Range<usize>>,
    pub(super) error_name: Option<Range<usize>>,
    pub(super) destination: Option<Range<usize>>,
    pub(super) sender: Option<Range<usize>>,
    pub(super) declared_unix_fds: Option<u32>,
    pub(super) signature: Option<Range<usize>>,
    pub(super) frame: Arc<Vec<u8>>,
    pub(super) body_range: Range<usize>,
    pub(super) unix_fds: Arc<[OwnedFd]>,
    pub(super) body_position: usize,
    pub(super) endian: Endian,
}

impl Message {
    pub const fn kind(&self) -> MessageKind {
        self.kind
    }

    pub const fn serial(&self) -> u32 {
        self.serial
    }

    pub const fn flags(&self) -> u8 {
        self.flags
    }

    pub const fn reply_serial(&self) -> Option<u32> {
        self.reply_serial
    }

    pub fn path(&self) -> Option<&str> {
        self.header(self.path.as_ref())
    }

    pub fn interface(&self) -> Option<&str> {
        self.header(self.interface.as_ref())
    }

    pub fn member(&self) -> Option<&str> {
        self.header(self.member.as_ref())
    }

    pub fn sender(&self) -> Option<&str> {
        self.header(self.sender.as_ref())
    }

    pub fn error_name(&self) -> Option<&str> {
        self.header(self.error_name.as_ref())
    }

    pub fn destination(&self) -> Option<&str> {
        self.header(self.destination.as_ref())
    }

    pub fn signature(&self) -> &str {
        self.header(self.signature.as_ref()).unwrap_or_default()
    }

    pub fn unix_fd_count(&self) -> usize {
        self.unix_fds.len()
    }

    pub fn wire_len(&self) -> usize {
        self.frame.len()
    }

    pub fn expects_reply(&self) -> bool {
        self.kind == MessageKind::MethodCall && self.flags & 1 == 0
    }

    pub fn method_call_flags(&self) -> Option<MethodCallFlags> {
        (self.kind == MessageKind::MethodCall)
            .then_some(MethodCallFlags::from_bits_truncate(self.flags))
    }

    pub fn body<T>(&self) -> Result<T>
    where
        T: DeserializeOwned + Type,
    {
        let signature = self.signature();
        let expected = T::SIGNATURE.to_string_no_parens();
        if expected != signature {
            return Err(Error::InvalidMessage(format!(
                "body signature `{signature}` does not match requested type `{expected}`"
            )));
        }
        let data = Data::new_borrowed_fds(
            self.body_bytes(),
            Context::new_dbus(self.endian, self.body_position),
            self.unix_fds.iter(),
        );
        let (value, consumed) = if signature.is_empty() {
            data.deserialize().map_err(Error::Body)?
        } else {
            data.deserialize_for_signature(signature)
                .map_err(Error::Body)?
        };
        if consumed != self.body_bytes().len() {
            return Err(Error::InvalidMessage(format!(
                "body decoder consumed {consumed} of {} bytes",
                self.body_bytes().len()
            )));
        }
        Ok(value)
    }

    /// Decodes the body using its runtime signature into owned top-level fields.
    pub fn body_dynamic(&self) -> Result<DynamicBody> {
        let signature = self.signature();
        if signature.is_empty() {
            return Ok(DynamicBody::Empty);
        }
        let signature = zvariant::Signature::try_from(signature)
            .map_err(|error| Error::InvalidMessage(format!("invalid body signature: {error}")))?;
        let data = Data::new_borrowed_fds(
            self.body_bytes(),
            Context::new_dbus(self.endian, self.body_position),
            self.unix_fds.iter(),
        );
        let (structure, consumed): (Structure<'_>, usize) = data
            .deserialize_for_dynamic_signature(signature)
            .map_err(Error::Body)?;
        if consumed != self.body_bytes().len() {
            return Err(Error::InvalidMessage(format!(
                "dynamic body decoder consumed {consumed} of {} bytes",
                self.body_bytes().len()
            )));
        }
        let mut owned = StructureBuilder::new();
        for field in structure.into_fields() {
            owned.push_value(field.try_into_owned()?.into());
        }
        Ok(DynamicBody::from_structure(owned.build()?))
    }

    pub(crate) fn inspect_body_structure<R>(
        &self,
        inspect: impl FnOnce(&Structure<'_>) -> R,
    ) -> Result<Option<R>> {
        let signature = self.signature();
        if signature.is_empty() {
            return Ok(None);
        }
        let signature = zvariant::Signature::try_from(signature)
            .map_err(|error| Error::InvalidMessage(format!("invalid body signature: {error}")))?;
        let data = Data::new_borrowed_fds(
            self.body_bytes(),
            Context::new_dbus(self.endian, self.body_position),
            self.unix_fds.iter(),
        );
        let (structure, consumed): (Structure<'_>, usize) = data
            .deserialize_for_dynamic_signature(signature)
            .map_err(Error::Body)?;
        if consumed != self.body_bytes().len() {
            return Err(Error::InvalidMessage(format!(
                "dynamic body decoder consumed {consumed} of {} bytes",
                self.body_bytes().len()
            )));
        }
        Ok(Some(inspect(&structure)))
    }

    pub(crate) fn method_error(&self) -> Error {
        Error::Method {
            name: self
                .error_name()
                .unwrap_or("org.freedesktop.DBus.Error.Failed")
                .to_owned(),
            message: self.error_message().unwrap_or_default(),
        }
    }

    pub(super) fn body_bytes(&self) -> &[u8] {
        &self.frame[self.body_range.clone()]
    }

    fn header(&self, range: Option<&Range<usize>>) -> Option<&str> {
        let bytes = &self.frame[range?.clone()];
        Some(std::str::from_utf8(bytes).expect("header strings are validated during decoding"))
    }

    fn error_message(&self) -> Option<String> {
        if !self.signature().starts_with('s') {
            return None;
        }
        let body = self.body_bytes();
        let len = read_u32(body.get(..4)?, self.endian) as usize;
        let end = 4_usize.checked_add(len)?;
        let value = body.get(4..end)?;
        if body.get(end) != Some(&0) {
            return None;
        }
        std::str::from_utf8(value).ok().map(str::to_owned)
    }
}

impl fmt::Debug for Message {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Message")
            .field("kind", &self.kind)
            .field("flags", &self.flags)
            .field("serial", &self.serial)
            .field("reply_serial", &self.reply_serial)
            .field("path", &self.path())
            .field("interface", &self.interface())
            .field("member", &self.member())
            .field("error_name", &self.error_name())
            .field("destination", &self.destination())
            .field("sender", &self.sender())
            .field("signature", &self.signature())
            .field("unix_fd_count", &self.unix_fd_count())
            .field("wire_len", &self.wire_len())
            .field("endian", &self.endian)
            .finish()
    }
}
