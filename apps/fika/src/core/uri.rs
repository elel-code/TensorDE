use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::ffi::OsString;
#[cfg(unix)]
use std::os::unix::ffi::{OsStrExt, OsStringExt};

use super::network::network_uri_from_path;

pub fn path_uri_from_path(path: &Path) -> String {
    network_uri_from_path(path).unwrap_or_else(|| file_uri_from_path(path))
}

pub fn file_uri_from_path(path: &Path) -> String {
    let bytes = path_bytes(path);
    let mut uri = String::with_capacity(bytes.len() + "file://".len());
    uri.push_str("file://");
    for byte in bytes {
        if uri_path_byte_is_unreserved(byte) || byte == b'/' {
            uri.push(byte as char);
        } else {
            uri.push('%');
            uri.push(hex_digit(byte >> 4));
            uri.push(hex_digit(byte & 0x0f));
        }
    }
    uri
}

pub fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    percent_decode_file_uri_path(uri.strip_prefix("file://")?)
}

fn percent_decode_file_uri_path(text: &str) -> Option<PathBuf> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = hex_value(*bytes.get(index + 1)?)?;
            let low = hex_value(*bytes.get(index + 2)?)?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    path_buf_from_bytes(decoded)
}

#[cfg(unix)]
fn path_bytes(path: &Path) -> Vec<u8> {
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
fn path_bytes(path: &Path) -> Vec<u8> {
    path.to_string_lossy().as_bytes().to_vec()
}

#[cfg(unix)]
fn path_buf_from_bytes(bytes: Vec<u8>) -> Option<PathBuf> {
    Some(PathBuf::from(OsString::from_vec(bytes)))
}

#[cfg(not(unix))]
fn path_buf_from_bytes(bytes: Vec<u8>) -> Option<PathBuf> {
    String::from_utf8(bytes).ok().map(PathBuf::from)
}

fn uri_path_byte_is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'A' + (value - 10)) as char,
        _ => unreachable!("hex digit nibble is always 0..=15"),
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn file_uri_percent_encodes_local_paths_without_gio() {
        assert_eq!(
            file_uri_from_path(Path::new("/tmp/Fika Test/value#1.txt")),
            "file:///tmp/Fika%20Test/value%231.txt"
        );
    }

    #[test]
    fn file_uri_round_trips_percent_encoded_path() {
        let path = PathBuf::from("/tmp/unicode-文档.txt");
        let uri = file_uri_from_path(&path);

        assert_eq!(file_uri_to_path(&uri), Some(path));
    }

    #[test]
    fn path_uri_preserves_network_uris() {
        assert_eq!(
            path_uri_from_path(Path::new("smb://server/share/report%202026.txt")),
            "smb://server/share/report%202026.txt"
        );
    }
}
