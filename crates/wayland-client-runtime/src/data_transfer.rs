//! MIME payloads and pipes shared by clipboard and drag-and-drop transfers.

use std::io::{self, Read, Write};
use std::sync::Arc;
use std::thread;

use smithay_client_toolkit::data_device_manager::{ReadPipe, WritePipe};

pub const MIME_TEXT_PLAIN_UTF8: &str = "text/plain;charset=utf-8";
pub const MIME_UTF8_STRING: &str = "UTF8_STRING";
pub const MIME_TEXT_PLAIN: &str = "text/plain";
pub const MIME_STRING: &str = "STRING";
pub const MIME_TEXT: &str = "TEXT";

const TEXT_MIME_TYPES: [&str; 5] = [
    MIME_TEXT_PLAIN_UTF8,
    MIME_TEXT_PLAIN,
    MIME_UTF8_STRING,
    MIME_STRING,
    MIME_TEXT,
];

pub fn text_mime_types() -> &'static [&'static str] {
    &TEXT_MIME_TYPES
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum TransferError {
    #[error("transfer content must contain at least one MIME payload")]
    EmptyContent,
    #[error("transfer MIME type must not be empty")]
    EmptyMime,
}

/// Owned bytes advertised under one Wayland MIME type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MimePayload {
    mime: String,
    bytes: Arc<[u8]>,
}

impl MimePayload {
    pub fn new(
        mime: impl Into<String>,
        bytes: impl Into<Arc<[u8]>>,
    ) -> Result<Self, TransferError> {
        let mime = mime.into();
        if mime.is_empty() {
            return Err(TransferError::EmptyMime);
        }
        Ok(Self {
            mime,
            bytes: bytes.into(),
        })
    }

    pub fn mime(&self) -> &str {
        &self.mime
    }

    pub fn bytes(&self) -> &Arc<[u8]> {
        &self.bytes
    }
}

/// The MIME alternatives exposed by one clipboard or drag-and-drop source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferContent {
    payloads: Vec<MimePayload>,
}

impl TransferContent {
    pub fn new(payloads: impl IntoIterator<Item = MimePayload>) -> Result<Self, TransferError> {
        let mut unique = Vec::<MimePayload>::new();
        for payload in payloads {
            if let Some(existing) = unique.iter_mut().find(|item| item.mime == payload.mime) {
                *existing = payload;
            } else {
                unique.push(payload);
            }
        }
        if unique.is_empty() {
            return Err(TransferError::EmptyContent);
        }
        Ok(Self { payloads: unique })
    }

    pub fn text(text: impl AsRef<str>) -> Self {
        let bytes = Arc::<[u8]>::from(text.as_ref().as_bytes());
        Self {
            payloads: TEXT_MIME_TYPES
                .iter()
                .map(|mime| MimePayload {
                    mime: (*mime).to_string(),
                    bytes: bytes.clone(),
                })
                .collect(),
        }
    }

    pub fn payloads(&self) -> &[MimePayload] {
        &self.payloads
    }

    pub(crate) fn mime_types(&self) -> impl Iterator<Item = &str> {
        self.payloads.iter().map(MimePayload::mime)
    }

    pub(crate) fn bytes_for_mime(&self, mime: &str) -> Option<Arc<[u8]>> {
        self.payloads
            .iter()
            .find(|payload| payload.mime == mime)
            .map(|payload| payload.bytes.clone())
    }
}

/// A readable pipe returned by a Wayland data offer.
#[derive(Debug)]
pub struct TransferReadPipe {
    mime: String,
    inner: TransferReadInner,
}

#[derive(Debug)]
enum TransferReadInner {
    /// SCTK / calloop path.
    Sctk(ReadPipe),
    /// Native path: bytes already transferred (or pre-buffered).
    Memory(io::Cursor<Vec<u8>>),
}

impl TransferReadPipe {
    pub(crate) fn new(mime: String, inner: ReadPipe) -> Self {
        Self {
            mime,
            inner: TransferReadInner::Sctk(inner),
        }
    }

    /// Build a pipe from bytes already read on the native data-device path.
    pub(crate) fn from_bytes(mime: String, bytes: Vec<u8>) -> Self {
        Self {
            mime,
            inner: TransferReadInner::Memory(io::Cursor::new(bytes)),
        }
    }

    pub fn mime(&self) -> &str {
        &self.mime
    }

    /// Read a textual offer, accepting non-UTF-8 bytes lossily like common
    /// Wayland clipboard implementations.
    pub fn read_text(mut self) -> io::Result<String> {
        let mut content = Vec::new();
        self.read_to_end(&mut content)?;
        let text = String::from_utf8(content)
            .unwrap_or_else(|error| String::from_utf8_lossy(&error.into_bytes()).into_owned());
        Ok(match self.mime.as_str() {
            MIME_TEXT_PLAIN_UTF8 | MIME_TEXT_PLAIN => {
                text.replace("\r\n", "\n").replace('\r', "\n")
            }
            _ => text,
        })
    }
}

impl Read for TransferReadPipe {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match &mut self.inner {
            TransferReadInner::Sctk(pipe) => pipe.read(buffer),
            TransferReadInner::Memory(cursor) => cursor.read(buffer),
        }
    }
}

pub(crate) fn spawn_write_pipe(name: &str, mut pipe: WritePipe, bytes: Arc<[u8]>) {
    let _ = thread::Builder::new()
        .name(name.to_string())
        .spawn(move || {
            let _ = pipe.write_all(&bytes);
            let _ = pipe.flush();
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_replaces_duplicate_mime_with_last_payload() {
        let content = TransferContent::new([
            MimePayload::new("text/plain", Arc::<[u8]>::from(&b"old"[..])).unwrap(),
            MimePayload::new("text/plain", Arc::<[u8]>::from(&b"new"[..])).unwrap(),
        ])
        .unwrap();
        assert_eq!(&*content.bytes_for_mime("text/plain").unwrap(), b"new");
    }

    #[test]
    fn text_content_offers_common_wayland_text_mimes() {
        let content = TransferContent::text("hello");
        assert_eq!(content.mime_types().collect::<Vec<_>>(), TEXT_MIME_TYPES);
    }
}
