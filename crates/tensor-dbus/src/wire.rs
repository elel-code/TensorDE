use std::{
    io::{Cursor, Seek},
    ops::Range,
    os::fd::OwnedFd,
    sync::Arc,
};

use serde::Serialize;
use zvariant::{
    DynamicType, Endian, LE, Signature, Value,
    serialized::{Context, Data},
};

use crate::{
    Error, MethodCallFlags, Result,
    name::{
        validate_bus_name, validate_error_name, validate_interface_name, validate_member_name,
        validate_unique_name,
    },
};

mod message;

pub use message::Message;

pub(crate) const FIXED_HEADER_LEN: usize = 16;
pub(crate) const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;
pub(crate) const MAX_UNIX_FDS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum MessageKind {
    MethodCall = 1,
    MethodReturn = 2,
    Error = 3,
    Signal = 4,
}

impl TryFrom<u8> for MessageKind {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            1 => Ok(Self::MethodCall),
            2 => Ok(Self::MethodReturn),
            3 => Ok(Self::Error),
            4 => Ok(Self::Signal),
            _ => Err(Error::InvalidMessage(format!(
                "unknown message type {value}"
            ))),
        }
    }
}

pub(crate) struct MethodCall<'a> {
    pub serial: u32,
    pub flags: MethodCallFlags,
    pub destination: Option<&'a str>,
    pub path: &'a str,
    pub interface: Option<&'a str>,
    pub member: &'a str,
}

pub(crate) struct Outgoing<'a> {
    pub kind: MessageKind,
    pub flags: u8,
    pub serial: u32,
    pub reply_serial: Option<u32>,
    pub path: Option<&'a str>,
    pub interface: Option<&'a str>,
    pub member: Option<&'a str>,
    pub error_name: Option<&'a str>,
    pub destination: Option<&'a str>,
}

pub(crate) struct EncodedMessage {
    pub bytes: Vec<u8>,
    pub unix_fds: Vec<zvariant::OwnedFd>,
}

pub(crate) fn encode_method_call<T>(call: MethodCall<'_>, body: &T) -> Result<EncodedMessage>
where
    T: ?Sized + Serialize + DynamicType,
{
    encode_outgoing(
        Outgoing {
            kind: MessageKind::MethodCall,
            flags: call.flags.bits(),
            serial: call.serial,
            reply_serial: None,
            path: Some(call.path),
            interface: call.interface,
            member: Some(call.member),
            error_name: None,
            destination: call.destination,
        },
        body,
    )
}

pub(crate) fn encode_outgoing<T>(outgoing: Outgoing<'_>, body: &T) -> Result<EncodedMessage>
where
    T: ?Sized + Serialize + DynamicType,
{
    let signature = body.signature().to_string_no_parens();
    let mut message = Vec::with_capacity(256);
    message.resize(FIXED_HEADER_LEN, 0);
    if let Some(value) = outgoing.path {
        push_string_field_at(&mut message, 0, 1, b'o', value)?;
    }
    if let Some(value) = outgoing.interface {
        push_string_field_at(&mut message, 0, 2, b's', value)?;
    }
    if let Some(value) = outgoing.member {
        push_string_field_at(&mut message, 0, 3, b's', value)?;
    }
    if let Some(value) = outgoing.error_name {
        push_string_field_at(&mut message, 0, 4, b's', value)?;
    }
    if let Some(value) = outgoing.reply_serial {
        push_u32_field_at(&mut message, 0, 5, value);
    }
    if let Some(value) = outgoing.destination {
        push_string_field_at(&mut message, 0, 6, b's', value)?;
    }
    if !signature.is_empty() {
        push_signature_field_at(&mut message, 0, &signature)?;
    }

    let preliminary_size = zvariant::serialized_size(Context::new_dbus(LE, 0), body)?;
    let unix_fd_count = preliminary_size.num_fds() as usize;
    if unix_fd_count > MAX_UNIX_FDS {
        return Err(Error::UnixFdLimit {
            count: unix_fd_count,
            limit: MAX_UNIX_FDS,
        });
    }
    if unix_fd_count != 0 {
        push_u32_field_at(&mut message, 0, 9, preliminary_size.num_fds());
    }

    let fields_len = message.len() - FIXED_HEADER_LEN;
    let header_len = align(message.len(), 8);
    let context = Context::new_dbus(LE, header_len);
    let body_size = zvariant::serialized_size(context, body)?;
    debug_assert_eq!(body_size.num_fds(), preliminary_size.num_fds());
    let body_len = body_size.size();
    let total = header_len
        .checked_add(body_len)
        .ok_or(Error::MessageTooLarge {
            limit: MAX_MESSAGE_SIZE,
        })?;
    if total > MAX_MESSAGE_SIZE {
        return Err(Error::MessageTooLarge {
            limit: MAX_MESSAGE_SIZE,
        });
    }

    message[..4].copy_from_slice(&[b'l', outgoing.kind as u8, outgoing.flags, 1]);
    message[4..8].copy_from_slice(&(body_len as u32).to_le_bytes());
    message[8..12].copy_from_slice(&outgoing.serial.to_le_bytes());
    message[12..16].copy_from_slice(&(fields_len as u32).to_le_bytes());
    message.reserve_exact(total - message.len());
    message.resize(header_len, 0);
    let unix_fds = {
        let mut cursor = Cursor::new(&mut message);
        cursor.seek(std::io::SeekFrom::Start(header_len as u64))?;
        // SAFETY: the returned descriptor owners remain alive through the
        // transport send, while the cursor writes into the final allocation.
        let written = unsafe { zvariant::to_writer(&mut cursor, context, body) }?;
        if written.fds().len() != unix_fd_count || cursor.get_ref().len() != total {
            return Err(Error::InvalidMessage(
                "body changed while the D-Bus message was being serialized".to_owned(),
            ));
        }
        written.into_fds()
    };
    Ok(EncodedMessage {
        bytes: message,
        unix_fds,
    })
}

#[cfg(test)]
fn push_u32_field(fields: &mut Vec<u8>, code: u8, value: u32) {
    push_u32_field_at(fields, FIXED_HEADER_LEN, code, value);
}

fn push_u32_field_at(fields: &mut Vec<u8>, base: usize, code: u8, value: u32) {
    align_vec(fields, base, 8);
    fields.push(code);
    fields.extend_from_slice(&[1, b'u', 0]);
    align_vec(fields, base, 4);
    fields.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn frame_len(fixed: &[u8; FIXED_HEADER_LEN]) -> Result<usize> {
    let endian = parse_endian(fixed[0])?;
    if fixed[3] != 1 {
        return Err(Error::InvalidMessage(format!(
            "unsupported protocol version {}",
            fixed[3]
        )));
    }
    MessageKind::try_from(fixed[1])?;
    let body_len = read_u32(&fixed[4..8], endian) as usize;
    let fields_len = read_u32(&fixed[12..16], endian) as usize;
    let header_len = align(
        FIXED_HEADER_LEN
            .checked_add(fields_len)
            .ok_or(Error::MessageTooLarge {
                limit: MAX_MESSAGE_SIZE,
            })?,
        8,
    );
    let total = header_len
        .checked_add(body_len)
        .ok_or(Error::MessageTooLarge {
            limit: MAX_MESSAGE_SIZE,
        })?;
    if total > MAX_MESSAGE_SIZE {
        return Err(Error::MessageTooLarge {
            limit: MAX_MESSAGE_SIZE,
        });
    }
    Ok(total)
}

pub(crate) fn decode_message(bytes: Vec<u8>, unix_fds: Vec<OwnedFd>) -> Result<Message> {
    let fixed: &[u8; FIXED_HEADER_LEN] = bytes
        .get(..FIXED_HEADER_LEN)
        .ok_or_else(|| Error::InvalidMessage("truncated fixed header".to_owned()))?
        .try_into()
        .unwrap();
    let total = frame_len(fixed)?;
    if bytes.len() != total {
        return Err(Error::InvalidMessage(format!(
            "frame length mismatch: expected {total}, received {}",
            bytes.len()
        )));
    }
    let endian = parse_endian(fixed[0])?;
    let kind = MessageKind::try_from(fixed[1])?;
    let flags = fixed[2];
    let serial = read_u32(&fixed[8..12], endian);
    if serial == 0 {
        return Err(Error::InvalidMessage("message serial is zero".to_owned()));
    }
    let fields_len = read_u32(&fixed[12..16], endian) as usize;
    let fields_end = FIXED_HEADER_LEN + fields_len;
    let body_position = align(fields_end, 8);
    let body_len = read_u32(&fixed[4..8], endian) as usize;
    let frame = Arc::new(bytes);
    let mut message = Message {
        kind,
        flags,
        serial,
        reply_serial: None,
        path: None,
        interface: None,
        member: None,
        error_name: None,
        destination: None,
        sender: None,
        declared_unix_fds: None,
        signature: None,
        frame: Arc::clone(&frame),
        body_range: body_position..body_position + body_len,
        unix_fds: unix_fds.into(),
        body_position,
        endian,
    };
    parse_fields(&frame, fields_end, endian, &mut message)?;
    validate_zero_padding(&frame[fields_end..body_position], "header")?;
    validate_unix_fds(&message)?;
    validate_required_fields(&message)?;
    validate_field_semantics(&message)?;
    validate_field_values(&message)?;
    validate_body_signature(&message)?;
    Ok(message)
}

fn validate_unix_fds(message: &Message) -> Result<()> {
    let declared = message.declared_unix_fds.unwrap_or(0) as usize;
    if declared > MAX_UNIX_FDS {
        return Err(Error::UnixFdLimit {
            count: declared,
            limit: MAX_UNIX_FDS,
        });
    }
    if declared != message.unix_fds.len() {
        return Err(Error::InvalidMessage(format!(
            "header declares {declared} Unix file descriptors but {} were received",
            message.unix_fds.len()
        )));
    }
    Ok(())
}

fn validate_required_fields(message: &Message) -> Result<()> {
    let missing = match message.kind {
        MessageKind::MethodCall if message.path.is_none() => Some("path"),
        MessageKind::MethodCall if message.member.is_none() => Some("member"),
        MessageKind::MethodReturn | MessageKind::Error if message.reply_serial.is_none() => {
            Some("reply serial")
        }
        MessageKind::Error if message.error_name.is_none() => Some("error name"),
        MessageKind::Signal if message.path.is_none() => Some("path"),
        MessageKind::Signal if message.interface.is_none() => Some("interface"),
        MessageKind::Signal if message.member.is_none() => Some("member"),
        _ => None,
    };
    match missing {
        Some(field) => Err(Error::InvalidMessage(format!("missing required {field}"))),
        None => Ok(()),
    }
}

fn validate_field_semantics(message: &Message) -> Result<()> {
    let invalid = match message.kind {
        MessageKind::MethodCall if message.reply_serial.is_some() => Some("reply serial"),
        MessageKind::MethodCall if message.error_name.is_some() => Some("error name"),
        MessageKind::MethodReturn if message.path.is_some() => Some("path"),
        MessageKind::MethodReturn if message.interface.is_some() => Some("interface"),
        MessageKind::MethodReturn if message.member.is_some() => Some("member"),
        MessageKind::MethodReturn if message.error_name.is_some() => Some("error name"),
        MessageKind::Error if message.path.is_some() => Some("path"),
        MessageKind::Error if message.interface.is_some() => Some("interface"),
        MessageKind::Error if message.member.is_some() => Some("member"),
        MessageKind::Signal if message.reply_serial.is_some() => Some("reply serial"),
        MessageKind::Signal if message.error_name.is_some() => Some("error name"),
        _ => None,
    };
    match invalid {
        Some(field) => Err(Error::InvalidMessage(format!(
            "{field} is invalid for a {:?} message",
            message.kind
        ))),
        None => Ok(()),
    }
}

fn validate_field_values(message: &Message) -> Result<()> {
    if let Some(path) = message.path() {
        zvariant::ObjectPath::try_from(path).map_err(|_| invalid_field("object path", path))?;
    }
    if let Some(interface) = message.interface() {
        map_invalid_name(validate_interface_name(interface, "interface name"))?;
    }
    if let Some(member) = message.member() {
        map_invalid_name(validate_member_name(member, "member name"))?;
    }
    if let Some(error_name) = message.error_name() {
        map_invalid_name(validate_error_name(error_name, "error name"))?;
    }
    if let Some(destination) = message.destination() {
        map_invalid_name(validate_bus_name(destination, "destination bus name"))?;
    }
    if let Some(sender) = message.sender() {
        map_invalid_name(validate_unique_name(sender, "sender unique name"))?;
    }
    if message.reply_serial == Some(0) {
        return Err(Error::InvalidMessage("reply serial is zero".to_owned()));
    }
    Ok(())
}

fn validate_body_signature(message: &Message) -> Result<()> {
    Signature::try_from(message.signature())
        .map_err(|error| Error::InvalidMessage(format!("invalid body signature: {error}")))?;
    if message.body_bytes().is_empty() != message.signature().is_empty() {
        return Err(Error::InvalidMessage(
            "body length and signature presence are inconsistent".to_owned(),
        ));
    }
    Ok(())
}

fn map_invalid_name(result: Result<()>) -> Result<()> {
    result.map_err(|error| Error::InvalidMessage(error.to_string()))
}

fn invalid_field(kind: &str, value: &str) -> Error {
    Error::InvalidMessage(format!("invalid {kind} `{value}`"))
}

fn parse_fields(
    bytes: &[u8],
    fields_end: usize,
    endian: Endian,
    message: &mut Message,
) -> Result<()> {
    let mut position = FIXED_HEADER_LEN;
    let mut known_fields = 0_u16;
    while position < fields_end {
        align_field_position(bytes, &mut position, fields_end, 8)?;
        if position == fields_end {
            break;
        }
        let code = take_u8(bytes, &mut position, fields_end)?;
        if code == 0 {
            return Err(Error::InvalidMessage(
                "header field code zero is reserved".to_owned(),
            ));
        }
        let variant_position = position;
        let signature_len = take_u8(bytes, &mut position, fields_end)? as usize;
        let signature = take(bytes, &mut position, signature_len, fields_end)?;
        if take_u8(bytes, &mut position, fields_end)? != 0 {
            return Err(Error::InvalidMessage(
                "header variant signature is not NUL terminated".to_owned(),
            ));
        }
        if let Some(expected) = known_field_signature(code) {
            let bit = 1_u16 << code;
            if known_fields & bit != 0 {
                return Err(Error::InvalidMessage(format!(
                    "duplicate header field {code}"
                )));
            }
            known_fields |= bit;
            if signature != expected {
                return Err(Error::InvalidMessage(format!(
                    "header field {code} has signature `{}`, expected `{}`",
                    String::from_utf8_lossy(signature),
                    String::from_utf8_lossy(expected)
                )));
            }
        }
        match signature {
            b"s" | b"o" => {
                align_field_position(bytes, &mut position, fields_end, 4)?;
                let len = take_u32(bytes, &mut position, fields_end, endian)? as usize;
                let value = take_string_range(bytes, &mut position, len, fields_end)?;
                assign_string_field(message, code, value);
            }
            b"g" => {
                let len = take_u8(bytes, &mut position, fields_end)? as usize;
                let value = take_string_range(bytes, &mut position, len, fields_end)?;
                if code == 8 {
                    message.signature = Some(value);
                }
            }
            b"u" => {
                align_field_position(bytes, &mut position, fields_end, 4)?;
                let value = take_u32(bytes, &mut position, fields_end, endian)?;
                match code {
                    5 => message.reply_serial = Some(value),
                    9 => message.declared_unix_fds = Some(value),
                    _ => {}
                }
            }
            _ => skip_unknown_field(
                bytes,
                &mut position,
                variant_position,
                fields_end,
                endian,
                signature,
                message,
            )?,
        }
    }
    Ok(())
}

fn known_field_signature(code: u8) -> Option<&'static [u8]> {
    match code {
        1 => Some(b"o"),
        2 | 3 | 4 | 6 | 7 => Some(b"s"),
        5 | 9 => Some(b"u"),
        8 => Some(b"g"),
        _ => None,
    }
}

fn skip_unknown_field(
    bytes: &[u8],
    position: &mut usize,
    variant_position: usize,
    fields_end: usize,
    endian: Endian,
    signature_bytes: &[u8],
    message: &Message,
) -> Result<()> {
    let signature_text = std::str::from_utf8(signature_bytes)
        .map_err(|_| Error::InvalidMessage("header field signature is not UTF-8".to_owned()))?;
    let signature = Signature::try_from(signature_text).map_err(|error| {
        Error::InvalidMessage(format!("invalid header field signature: {error}"))
    })?;
    if signature_text.is_empty() || signature.to_string() != signature_text {
        return Err(Error::InvalidMessage(
            "header variant must contain exactly one complete type".to_owned(),
        ));
    }
    let data = Data::new_borrowed_fds(
        &bytes[variant_position..fields_end],
        Context::new_dbus(endian, variant_position),
        message.unix_fds.iter(),
    );
    let (_, consumed): (Value<'_>, usize) = data
        .deserialize()
        .map_err(|error| Error::InvalidMessage(format!("invalid unknown header field: {error}")))?;
    *position = variant_position
        .checked_add(consumed)
        .ok_or_else(|| Error::InvalidMessage("header field overflow".to_owned()))?;
    if *position > fields_end {
        return Err(Error::InvalidMessage("truncated header field".to_owned()));
    }
    Ok(())
}

fn align_field_position(
    bytes: &[u8],
    position: &mut usize,
    end: usize,
    alignment: usize,
) -> Result<()> {
    let aligned = align(*position, alignment);
    let padding = take(bytes, position, aligned - *position, end)?;
    validate_zero_padding(padding, "header field")
}

fn validate_zero_padding(bytes: &[u8], kind: &str) -> Result<()> {
    if bytes.iter().any(|byte| *byte != 0) {
        return Err(Error::InvalidMessage(format!("nonzero {kind} padding")));
    }
    Ok(())
}

fn assign_string_field(message: &mut Message, code: u8, value: Range<usize>) {
    match code {
        1 => message.path = Some(value),
        2 => message.interface = Some(value),
        3 => message.member = Some(value),
        4 => message.error_name = Some(value),
        6 => message.destination = Some(value),
        7 => message.sender = Some(value),
        _ => {}
    }
}

#[cfg(test)]
fn push_string_field(fields: &mut Vec<u8>, code: u8, signature: u8, value: &str) -> Result<()> {
    push_string_field_at(fields, FIXED_HEADER_LEN, code, signature, value)
}

fn push_string_field_at(
    fields: &mut Vec<u8>,
    base: usize,
    code: u8,
    signature: u8,
    value: &str,
) -> Result<()> {
    if fields.len().saturating_add(value.len()).saturating_add(16) > MAX_MESSAGE_SIZE {
        return Err(Error::MessageTooLarge {
            limit: MAX_MESSAGE_SIZE,
        });
    }
    align_vec(fields, base, 8);
    fields.push(code);
    fields.extend_from_slice(&[1, signature, 0]);
    align_vec(fields, base, 4);
    let len = u32::try_from(value.len()).map_err(|_| Error::MessageTooLarge {
        limit: MAX_MESSAGE_SIZE,
    })?;
    fields.extend_from_slice(&len.to_le_bytes());
    fields.extend_from_slice(value.as_bytes());
    fields.push(0);
    Ok(())
}

#[cfg(test)]
fn push_signature_field(fields: &mut Vec<u8>, value: &str) -> Result<()> {
    push_signature_field_at(fields, FIXED_HEADER_LEN, value)
}

fn push_signature_field_at(fields: &mut Vec<u8>, base: usize, value: &str) -> Result<()> {
    let len = u8::try_from(value.len())
        .map_err(|_| Error::InvalidMessage("body signature exceeds 255 bytes".to_owned()))?;
    align_vec(fields, base, 8);
    fields.push(8);
    fields.extend_from_slice(&[1, b'g', 0, len]);
    fields.extend_from_slice(value.as_bytes());
    fields.push(0);
    Ok(())
}

fn parse_endian(byte: u8) -> Result<Endian> {
    match byte {
        b'l' => Ok(Endian::Little),
        b'B' => Ok(Endian::Big),
        _ => Err(Error::InvalidMessage(format!(
            "invalid endian marker {byte:#x}"
        ))),
    }
}

fn read_u32(bytes: &[u8], endian: Endian) -> u32 {
    let bytes: [u8; 4] = bytes.try_into().unwrap();
    match endian {
        Endian::Little => u32::from_le_bytes(bytes),
        Endian::Big => u32::from_be_bytes(bytes),
    }
}

fn take_u32(bytes: &[u8], position: &mut usize, end: usize, endian: Endian) -> Result<u32> {
    let value = take(bytes, position, 4, end)?;
    Ok(read_u32(value, endian))
}

fn take_u8(bytes: &[u8], position: &mut usize, end: usize) -> Result<u8> {
    Ok(take(bytes, position, 1, end)?[0])
}

fn take<'a>(bytes: &'a [u8], position: &mut usize, len: usize, end: usize) -> Result<&'a [u8]> {
    let next = position
        .checked_add(len)
        .ok_or_else(|| Error::InvalidMessage("header field overflow".to_owned()))?;
    if next > end || next > bytes.len() {
        return Err(Error::InvalidMessage("truncated header field".to_owned()));
    }
    let value = &bytes[*position..next];
    *position = next;
    Ok(value)
}

fn take_string_range(
    bytes: &[u8],
    position: &mut usize,
    len: usize,
    end: usize,
) -> Result<Range<usize>> {
    let start = *position;
    let value = take(bytes, position, len, end)?;
    if take_u8(bytes, position, end)? != 0 {
        return Err(Error::InvalidMessage(
            "header string is not NUL terminated".to_owned(),
        ));
    }
    std::str::from_utf8(value)
        .map_err(|_| Error::InvalidMessage("header string is not UTF-8".to_owned()))?;
    Ok(start..start + len)
}

const fn align(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}

fn align_vec(bytes: &mut Vec<u8>, base: usize, alignment: usize) {
    let aligned = align(base + bytes.len(), alignment) - base;
    bytes.resize(aligned, 0);
}

#[cfg(test)]
mod tests;
