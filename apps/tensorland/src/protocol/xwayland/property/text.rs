//! Bounded 8-bit X11 property replies and legacy string decoding.

use std::io;

use compio::{buf::IoBuf, io::AsyncReadExt, net::UnixStream};

const MAX_TEXT_BYTES: usize = 4096;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct X11WindowMetadata {
    pub(crate) title: String,
    pub(crate) app_id: String,
}

pub(super) async fn read_initial_metadata(
    stream: &mut UnixStream,
    utf8_string: u32,
    string: u32,
) -> io::Result<X11WindowMetadata> {
    let net_title = read_string(stream, utf8_string, TextEncoding::Utf8).await?;
    let legacy_title = read_string(stream, string, TextEncoding::Latin1).await?;
    let class = read_bytes(stream, string).await?;
    Ok(X11WindowMetadata {
        title: net_title.or(legacy_title).unwrap_or_default(),
        app_id: class
            .as_deref()
            .and_then(decode_wm_class)
            .unwrap_or_default(),
    })
}

pub(super) async fn read_title(
    stream: &mut UnixStream,
    utf8_string: u32,
    string: u32,
) -> io::Result<String> {
    let net_title = read_string(stream, utf8_string, TextEncoding::Utf8).await?;
    let legacy_title = read_string(stream, string, TextEncoding::Latin1).await?;
    Ok(net_title.or(legacy_title).unwrap_or_default())
}

pub(super) async fn read_class(stream: &mut UnixStream, string: u32) -> io::Result<String> {
    Ok(read_bytes(stream, string)
        .await?
        .as_deref()
        .and_then(decode_wm_class)
        .unwrap_or_default())
}

#[derive(Clone, Copy)]
enum TextEncoding {
    Utf8,
    Latin1,
}

async fn read_string(
    stream: &mut UnixStream,
    expected_type: u32,
    encoding: TextEncoding,
) -> io::Result<Option<String>> {
    let Some(bytes) = read_bytes(stream, expected_type).await? else {
        return Ok(None);
    };
    let bytes = bytes.split(|byte| *byte == 0).next().unwrap_or_default();
    Ok(match encoding {
        TextEncoding::Utf8 => std::str::from_utf8(bytes).ok().map(str::to_owned),
        TextEncoding::Latin1 => Some(decode_latin1(bytes)),
    })
}

async fn read_bytes(stream: &mut UnixStream, expected_type: u32) -> io::Result<Option<Vec<u8>>> {
    let (result, header) = stream.read_exact([0_u8; 32]).await.into_parts();
    result?;
    if header[0] != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "X11 text property request failed with error code {}",
                header[1]
            ),
        ));
    }

    let format = header[1];
    let property_type = get_u32(&header[8..12]);
    let bytes_after = get_u32(&header[12..16]);
    let value_len = usize::try_from(get_u32(&header[16..20])).unwrap_or(usize::MAX);
    let body_len = usize::try_from(get_u32(&header[4..8]))
        .unwrap_or(usize::MAX)
        .checked_mul(4)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "X11 text length overflow"))?;

    if format == 0 && property_type == 0 && value_len == 0 && body_len == 0 && bytes_after == 0 {
        return Ok(None);
    }
    let padded_len = value_len
        .checked_add(3)
        .map(|len| len & !3)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "X11 text padding overflow"))?;
    if format != 8
        || property_type != expected_type
        || bytes_after != 0
        || value_len > MAX_TEXT_BYTES
        || body_len != padded_len
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "X11 text property exceeded its bounded 8-bit contract",
        ));
    }
    if body_len == 0 {
        return Ok(Some(Vec::new()));
    }
    let body = [0_u8; MAX_TEXT_BYTES].slice(..body_len);
    let (result, body) = stream.read_exact(body).await.into_parts();
    result?;
    Ok(Some(body[..value_len].to_vec()))
}

fn decode_wm_class(bytes: &[u8]) -> Option<String> {
    let mut fields = bytes.split(|byte| *byte == 0);
    let instance = fields.next().unwrap_or_default();
    let class = fields.next().unwrap_or_default();
    let selected = if class.is_empty() { instance } else { class };
    (!selected.is_empty()).then(|| decode_latin1(selected))
}

fn decode_latin1(bytes: &[u8]) -> String {
    bytes.iter().copied().map(char::from).collect()
}

fn get_u32(input: &[u8]) -> u32 {
    u32::from_ne_bytes(input.try_into().expect("four-byte X11 field"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wm_class_prefers_class_and_falls_back_to_instance() {
        assert_eq!(
            decode_wm_class(b"firefox\0Firefox\0").as_deref(),
            Some("Firefox")
        );
        assert_eq!(decode_wm_class(b"xterm\0\0").as_deref(), Some("xterm"));
        assert_eq!(decode_wm_class(b"\0\0"), None);
    }

    #[test]
    fn latin1_decode_preserves_every_legacy_byte() {
        assert_eq!(decode_latin1(&[b'A', 0xe9]), "A\u{e9}");
    }
}
