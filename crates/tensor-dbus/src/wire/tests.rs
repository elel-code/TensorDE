use super::*;
use crate::DynamicBody;
use std::os::fd::{FromRawFd, IntoRawFd};

fn test_message(kind: MessageKind, fields: Vec<u8>, body: &[u8]) -> Vec<u8> {
    let header_len = align(FIXED_HEADER_LEN + fields.len(), 8);
    let mut bytes = Vec::with_capacity(header_len + body.len());
    bytes.extend_from_slice(&[b'l', kind as u8, 0, 1]);
    bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&(fields.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&fields);
    bytes.resize(header_len, 0);
    bytes.extend_from_slice(body);
    bytes
}

fn push_big_endian_string_field(fields: &mut Vec<u8>, code: u8, signature: u8, value: &str) {
    align_vec(fields, FIXED_HEADER_LEN, 8);
    fields.push(code);
    fields.extend_from_slice(&[1, signature, 0]);
    align_vec(fields, FIXED_HEADER_LEN, 4);
    fields.extend_from_slice(&(value.len() as u32).to_be_bytes());
    fields.extend_from_slice(value.as_bytes());
    fields.push(0);
}

fn push_big_endian_signature_field(fields: &mut Vec<u8>, value: &str) {
    align_vec(fields, FIXED_HEADER_LEN, 8);
    fields.push(8);
    fields.extend_from_slice(&[1, b'g', 0, value.len() as u8]);
    fields.extend_from_slice(value.as_bytes());
    fields.push(0);
}

fn big_endian_message(kind: MessageKind, fields: Vec<u8>, body: &[u8]) -> Vec<u8> {
    let header_len = align(FIXED_HEADER_LEN + fields.len(), 8);
    let mut bytes = Vec::with_capacity(header_len + body.len());
    bytes.extend_from_slice(&[b'B', kind as u8, 0, 1]);
    bytes.extend_from_slice(&(body.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    bytes.extend_from_slice(&(fields.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&fields);
    bytes.resize(header_len, 0);
    bytes.extend_from_slice(body);
    bytes
}

fn required_call_fields() -> Vec<u8> {
    let mut fields = Vec::new();
    push_string_field(&mut fields, 1, b'o', "/org/tensor/Test").unwrap();
    push_string_field(&mut fields, 3, b's', "Ping").unwrap();
    fields
}

fn push_unknown_unix_fd_field(fields: &mut Vec<u8>, code: u8, index: u32) {
    align_vec(fields, FIXED_HEADER_LEN, 8);
    fields.push(code);
    fields.extend_from_slice(&[1, b'h', 0]);
    align_vec(fields, FIXED_HEADER_LEN, 4);
    fields.extend_from_slice(&index.to_le_bytes());
}

#[test]
fn method_call_round_trips_through_wire_decoder() {
    let call = MethodCall {
        serial: 7,
        flags: MethodCallFlags::NO_AUTO_START | MethodCallFlags::ALLOW_INTERACTIVE_AUTH,
        destination: Some("org.freedesktop.DBus"),
        path: "/org/freedesktop/DBus",
        interface: Some("org.freedesktop.DBus"),
        member: "RequestName",
    };
    let encoded = encode_method_call(call, &("org.tensor.Test", 0_u32)).unwrap();
    let message = decode_message(encoded.bytes, Vec::new()).unwrap();

    assert_eq!(message.kind(), MessageKind::MethodCall);
    assert_eq!(message.serial(), 7);
    assert_eq!(message.flags(), 0x6);
    assert_eq!(
        message.method_call_flags(),
        Some(MethodCallFlags::NO_AUTO_START | MethodCallFlags::ALLOW_INTERACTIVE_AUTH)
    );
    assert_eq!(message.destination(), Some("org.freedesktop.DBus"));
    assert_eq!(message.member(), Some("RequestName"));
    assert_eq!(message.signature(), "su");
    assert_eq!(
        message.body::<(String, u32)>().unwrap(),
        ("org.tensor.Test".to_owned(), 0)
    );
}

#[test]
fn dynamic_body_preserves_top_level_fields_and_can_be_reencoded() {
    let encoded = encode_method_call(
        MethodCall {
            serial: 9,
            flags: MethodCallFlags::default(),
            destination: None,
            path: "/org/tensor/Test",
            interface: None,
            member: "Dynamic",
        },
        &(42_u32, "text", zvariant::Value::new(true)),
    )
    .unwrap();
    let message = decode_message(encoded.bytes, Vec::new()).unwrap();
    let dynamic = message.body_dynamic().unwrap();
    assert_eq!(dynamic.signature().to_string_no_parens(), "usv");
    assert_eq!(dynamic.fields().len(), 3);
    assert_eq!(u32::try_from(&dynamic.fields()[0]).unwrap(), 42);
    assert_eq!(<&str>::try_from(&dynamic.fields()[1]).unwrap(), "text");

    let reencoded = encode_method_call(
        MethodCall {
            serial: 10,
            flags: MethodCallFlags::default(),
            destination: None,
            path: "/org/tensor/Test",
            interface: None,
            member: "Dynamic",
        },
        &dynamic,
    )
    .unwrap();
    let message = decode_message(reencoded.bytes, Vec::new()).unwrap();
    assert_eq!(message.signature(), "usv");
    let (number, text, value): (u32, String, zvariant::OwnedValue) = message.body().unwrap();
    assert_eq!(number, 42);
    assert_eq!(text, "text");
    assert!(bool::try_from(value).unwrap());
}

#[test]
fn dynamic_body_models_an_empty_body_without_a_fake_field() {
    let encoded = encode_method_call(
        MethodCall {
            serial: 11,
            flags: MethodCallFlags::default(),
            destination: None,
            path: "/org/tensor/Test",
            interface: None,
            member: "Empty",
        },
        &(),
    )
    .unwrap();
    let message = decode_message(encoded.bytes, Vec::new()).unwrap();
    let dynamic = message.body_dynamic().unwrap();
    assert!(dynamic.is_empty());
    assert!(dynamic.fields().is_empty());
    assert_eq!(dynamic.signature(), zvariant::Signature::Unit);
}

#[test]
fn dynamic_body_can_be_constructed_from_runtime_fields() {
    let dynamic = DynamicBody::from_fields([
        zvariant::Value::new(23_u32).try_into_owned().unwrap(),
        zvariant::Value::new("runtime").try_into_owned().unwrap(),
    ])
    .unwrap();
    assert_eq!(dynamic.signature().to_string_no_parens(), "us");

    let encoded = encode_method_call(
        MethodCall {
            serial: 12,
            flags: MethodCallFlags::default(),
            destination: None,
            path: "/org/tensor/Test",
            interface: None,
            member: "ConstructedDynamic",
        },
        &dynamic,
    )
    .unwrap();
    let message = decode_message(encoded.bytes, Vec::new()).unwrap();
    let decoded: (u32, String) = message.body().unwrap();
    assert_eq!(decoded, (23, "runtime".to_owned()));

    assert!(
        DynamicBody::from_fields(std::iter::empty::<zvariant::Value<'static>>())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn decoded_header_views_borrow_the_retained_frame() {
    let encoded = encode_method_call(
        MethodCall {
            serial: 7,
            flags: MethodCallFlags::default(),
            destination: Some("org.freedesktop.DBus"),
            path: "/org/freedesktop/DBus",
            interface: Some("org.freedesktop.DBus"),
            member: "ListNames",
        },
        &(),
    )
    .unwrap();
    let message = decode_message(encoded.bytes, Vec::new()).unwrap();
    let frame = message.frame.as_ptr_range();

    for value in [
        message.path().unwrap(),
        message.interface().unwrap(),
        message.member().unwrap(),
        message.destination().unwrap(),
    ] {
        assert!(value.as_ptr() >= frame.start && value.as_ptr() < frame.end);
    }
}

#[test]
fn message_debug_does_not_expand_the_retained_body() {
    let payload = vec![0xa5_u8; 1024 * 1024];
    let encoded = encode_outgoing(
        Outgoing {
            kind: MessageKind::Signal,
            flags: 0,
            serial: 9,
            reply_serial: None,
            path: Some("/org/tensor/Test"),
            interface: Some("org.tensor.Test"),
            member: Some("Changed"),
            error_name: None,
            destination: None,
        },
        &payload,
    )
    .unwrap();
    let message = decode_message(encoded.bytes, Vec::new()).unwrap();
    let debug = format!("{message:?}");

    assert!(debug.len() < 512);
    assert!(debug.contains("wire_len: 104"));
    assert!(!debug.contains("165, 165"));
}

#[test]
fn rejects_oversized_frame_before_allocating_it() {
    let mut fixed = [0_u8; FIXED_HEADER_LEN];
    fixed[..4].copy_from_slice(&[b'l', 2, 0, 1]);
    fixed[4..8].copy_from_slice(&(MAX_MESSAGE_SIZE as u32).to_le_bytes());
    fixed[8..12].copy_from_slice(&1_u32.to_le_bytes());
    assert!(matches!(
        frame_len(&fixed),
        Err(Error::MessageTooLarge { .. })
    ));
}

#[test]
fn outgoing_header_and_body_limits_are_checked_before_final_frame_growth() {
    let oversized_path = "a".repeat(MAX_MESSAGE_SIZE);
    assert!(matches!(
        encode_outgoing(
            Outgoing {
                kind: MessageKind::Signal,
                flags: 0,
                serial: 1,
                reply_serial: None,
                path: Some(&oversized_path),
                interface: Some("org.tensor.Test"),
                member: Some("Changed"),
                error_name: None,
                destination: None,
            },
            &(),
        ),
        Err(Error::MessageTooLarge {
            limit: MAX_MESSAGE_SIZE
        })
    ));

    let oversized_body = vec![0_u8; MAX_MESSAGE_SIZE];
    assert!(matches!(
        encode_outgoing(
            Outgoing {
                kind: MessageKind::Signal,
                flags: 0,
                serial: 1,
                reply_serial: None,
                path: Some("/org/tensor/Test"),
                interface: Some("org.tensor.Test"),
                member: Some("Changed"),
                error_name: None,
                destination: None,
            },
            &oversized_body,
        ),
        Err(Error::MessageTooLarge {
            limit: MAX_MESSAGE_SIZE
        })
    ));
}

#[test]
fn rejects_duplicate_and_mistyped_known_header_fields() {
    let mut duplicate = required_call_fields();
    push_string_field(&mut duplicate, 1, b'o', "/org/tensor/Other").unwrap();
    assert!(matches!(
        decode_message(
            test_message(MessageKind::MethodCall, duplicate, &[]),
            Vec::new()
        ),
        Err(Error::InvalidMessage(message)) if message.contains("duplicate header field 1")
    ));

    let mut mistyped = Vec::new();
    push_string_field(&mut mistyped, 1, b's', "/org/tensor/Test").unwrap();
    push_string_field(&mut mistyped, 3, b's', "Ping").unwrap();
    assert!(matches!(
        decode_message(
            test_message(MessageKind::MethodCall, mistyped, &[]),
            Vec::new()
        ),
        Err(Error::InvalidMessage(message)) if message.contains("expected `o`")
    ));
}

#[test]
fn rejects_reserved_zero_header_field_code() {
    let mut fields = Vec::new();
    align_vec(&mut fields, FIXED_HEADER_LEN, 8);
    fields.push(0);
    fields.extend_from_slice(&[1, b'u', 0]);
    align_vec(&mut fields, FIXED_HEADER_LEN, 4);
    fields.extend_from_slice(&1_u32.to_le_bytes());
    fields.extend(required_call_fields());
    assert!(matches!(
        decode_message(
            test_message(MessageKind::MethodCall, fields, &[]),
            Vec::new()
        ),
        Err(Error::InvalidMessage(message)) if message.contains("code zero")
    ));
}

#[test]
fn rejects_zero_reply_serial_and_invalid_incoming_names() {
    let mut reply = Vec::new();
    push_u32_field(&mut reply, 5, 0);
    assert!(matches!(
        decode_message(
            test_message(MessageKind::MethodReturn, reply, &[]),
            Vec::new()
        ),
        Err(Error::InvalidMessage(message)) if message == "reply serial is zero"
    ));

    let mut fields = Vec::new();
    push_string_field(&mut fields, 1, b'o', "not/an/object/path").unwrap();
    push_string_field(&mut fields, 3, b's', "Ping").unwrap();
    assert!(matches!(
        decode_message(
            test_message(MessageKind::MethodCall, fields, &[]),
            Vec::new()
        ),
        Err(Error::InvalidMessage(message)) if message.contains("invalid object path")
    ));
}

#[test]
fn rejects_known_fields_on_incompatible_message_types() {
    let mut call = required_call_fields();
    push_u32_field(&mut call, 5, 7);
    assert!(matches!(
        decode_message(test_message(MessageKind::MethodCall, call, &[]), Vec::new()),
        Err(Error::InvalidMessage(message)) if message.contains("reply serial is invalid")
    ));

    let mut method_return = Vec::new();
    push_u32_field(&mut method_return, 5, 7);
    push_string_field(&mut method_return, 1, b'o', "/org/tensor/Test").unwrap();
    assert!(matches!(
        decode_message(
            test_message(MessageKind::MethodReturn, method_return, &[]),
            Vec::new()
        ),
        Err(Error::InvalidMessage(message)) if message.contains("path is invalid")
    ));

    let mut signal = Vec::new();
    push_string_field(&mut signal, 1, b'o', "/org/tensor/Test").unwrap();
    push_string_field(&mut signal, 2, b's', "org.tensor.Test").unwrap();
    push_string_field(&mut signal, 3, b's', "Changed").unwrap();
    push_u32_field(&mut signal, 5, 7);
    assert!(matches!(
        decode_message(test_message(MessageKind::Signal, signal, &[]), Vec::new()),
        Err(Error::InvalidMessage(message)) if message.contains("reply serial is invalid")
    ));
}

#[test]
fn decodes_typed_big_endian_body() {
    let mut fields = Vec::new();
    push_big_endian_string_field(&mut fields, 1, b'o', "/org/tensor/Test");
    push_big_endian_string_field(&mut fields, 3, b's', "Ping");
    push_big_endian_signature_field(&mut fields, "u");
    let message = decode_message(
        big_endian_message(MessageKind::MethodCall, fields, &42_u32.to_be_bytes()),
        Vec::new(),
    )
    .unwrap();

    assert_eq!(message.body::<u32>().unwrap(), 42);
}

#[test]
fn method_error_preserves_first_string_from_multi_field_body() {
    let encoded = encode_outgoing(
        Outgoing {
            kind: MessageKind::Error,
            flags: 0,
            serial: 9,
            reply_serial: Some(7),
            path: None,
            interface: None,
            member: None,
            error_name: Some("org.tensor.Test.Failed"),
            destination: None,
        },
        &("specific failure", 42_u32),
    )
    .unwrap();
    let message = decode_message(encoded.bytes, Vec::new()).unwrap();

    assert!(matches!(
        message.method_error(),
        Error::Method { name, message }
            if name == "org.tensor.Test.Failed" && message == "specific failure"
    ));
}

#[test]
fn validates_body_signature_presence_and_syntax() {
    assert!(matches!(
        decode_message(
            test_message(MessageKind::MethodCall, required_call_fields(), &[0]),
            Vec::new()
        ),
        Err(Error::InvalidMessage(message))
            if message.contains("body length and signature presence")
    ));

    let mut fields = required_call_fields();
    push_signature_field(&mut fields, "z").unwrap();
    assert!(matches!(
        decode_message(
            test_message(MessageKind::MethodCall, fields, &[0]),
            Vec::new()
        ),
        Err(Error::InvalidMessage(message)) if message.contains("invalid body signature")
    ));
}

#[test]
fn ignores_well_formed_unknown_header_field_types() {
    let mut fields = Vec::new();
    align_vec(&mut fields, FIXED_HEADER_LEN, 8);
    fields.push(42);
    fields.extend_from_slice(&[1, b't', 0]);
    align_vec(&mut fields, FIXED_HEADER_LEN, 8);
    fields.extend_from_slice(&42_u64.to_le_bytes());
    push_string_field(&mut fields, 1, b'o', "/org/tensor/Test").unwrap();
    push_string_field(&mut fields, 3, b's', "Ping").unwrap();

    let message = decode_message(
        test_message(MessageKind::MethodCall, fields, &[]),
        Vec::new(),
    )
    .unwrap();
    assert_eq!(message.path(), Some("/org/tensor/Test"));
}

#[test]
fn rejects_unknown_header_field_referencing_a_missing_unix_fd() {
    let mut fields = Vec::new();
    push_unknown_unix_fd_field(&mut fields, 42, 0);
    fields.extend(required_call_fields());

    assert!(matches!(
        decode_message(
            test_message(MessageKind::MethodCall, fields, &[]),
            Vec::new()
        ),
        Err(Error::InvalidMessage(message))
            if message.contains("invalid unknown header field")
    ));
}

#[test]
fn unknown_header_field_borrows_a_valid_message_owned_unix_fd() {
    let mut fields = Vec::new();
    push_unknown_unix_fd_field(&mut fields, 42, 0);
    push_u32_field(&mut fields, 9, 1);
    fields.extend(required_call_fields());

    let raw = std::fs::File::open("/dev/null").unwrap().into_raw_fd();
    // SAFETY: `into_raw_fd` transferred the only owner to this test and the
    // descriptor is immediately moved into `decode_message`.
    let fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(raw) };
    let message =
        decode_message(test_message(MessageKind::MethodCall, fields, &[]), vec![fd]).unwrap();

    assert_eq!(message.path(), Some("/org/tensor/Test"));
    assert_ne!(unsafe { libc::fcntl(raw, libc::F_GETFD) }, -1);
    drop(message);
    assert_eq!(unsafe { libc::fcntl(raw, libc::F_GETFD) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::EBADF)
    );
}

#[test]
fn deterministic_malformed_frame_corpus_never_panics_or_overallocates() {
    let mut state = 0x6a09_e667_f3bc_c909_u64;
    for index in 0..4096_usize {
        let len = 16 + index % 241;
        let mut bytes = vec![0_u8; len];
        for byte in &mut bytes {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *byte = state as u8;
        }
        if index % 3 == 0 {
            bytes[0] = b'l';
        } else if index % 3 == 1 {
            bytes[0] = b'B';
        }
        if index % 5 == 0 {
            bytes[1] = (index % 6) as u8;
        }
        if index % 7 == 0 {
            bytes[3] = 1;
        }

        let fixed: &[u8; FIXED_HEADER_LEN] = bytes[..FIXED_HEADER_LEN].try_into().unwrap();
        if let Ok(total) = frame_len(fixed)
            && total == bytes.len()
        {
            let _ = decode_message(bytes, Vec::new());
        }
    }
}

#[test]
fn typed_body_decode_requires_exact_signature_and_consumption() {
    let call = MethodCall {
        serial: 7,
        flags: MethodCallFlags::default(),
        destination: Some("org.freedesktop.DBus"),
        path: "/org/freedesktop/DBus",
        interface: Some("org.freedesktop.DBus"),
        member: "GetNameOwner",
    };
    let mut bytes = encode_method_call(call, &"org.tensor.Test").unwrap().bytes;
    assert!(matches!(
        decode_message(bytes.clone(), Vec::new())
            .unwrap()
            .body::<u32>(),
        Err(Error::InvalidMessage(message)) if message.contains("does not match")
    ));

    bytes.push(0);
    let body_len = read_u32(&bytes[4..8], Endian::Little) + 1;
    bytes[4..8].copy_from_slice(&body_len.to_le_bytes());
    assert!(matches!(
        decode_message(bytes, Vec::new()).unwrap().body::<String>(),
        Err(Error::InvalidMessage(message)) if message.contains("body decoder consumed")
    ));
}
