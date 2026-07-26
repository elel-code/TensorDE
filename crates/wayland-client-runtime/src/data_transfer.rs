//! MIME payloads and pipes shared by clipboard and drag-and-drop transfers.

use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::thread;

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
    /// Bytes already fully buffered (unit tests / future prefetched paths).
    #[cfg(test)]
    Memory(io::Cursor<Vec<u8>>),
    /// Live pipe from `wl_data_offer.receive` — read off the event-loop thread.
    Stream(UnixStream),
}

impl TransferReadPipe {
    /// Build a pipe from bytes already read (unit tests / prefetched paths).
    #[cfg(test)]
    pub(crate) fn from_bytes(mime: String, bytes: Vec<u8>) -> Self {
        Self {
            mime,
            inner: TransferReadInner::Memory(io::Cursor::new(bytes)),
        }
    }

    /// Build a pipe around the read end of a `wl_data_offer.receive` transfer.
    ///
    /// The caller must flush the Wayland connection after creating the offer
    /// receive. Reading may block until the source writes; never do that on
    /// the display thread.
    pub(crate) fn from_stream(mime: String, stream: UnixStream) -> Self {
        Self {
            mime,
            inner: TransferReadInner::Stream(stream),
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
            #[cfg(test)]
            TransferReadInner::Memory(cursor) => cursor.read(buffer),
            TransferReadInner::Stream(stream) => stream.read(buffer),
        }
    }
}

/// Write offer payload to a compositor-supplied fd without blocking the
/// Wayland dispatch thread (pipe buffer back-pressure / peer readiness).
pub(crate) fn spawn_write_fd(name: &str, fd: std::os::fd::OwnedFd, bytes: Arc<[u8]>) {
    let _ = thread::Builder::new().name(name.to_string()).spawn(move || {
        let mut file = std::fs::File::from(fd);
        let _ = file.write_all(&bytes);
        let _ = file.flush();
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

    #[test]
    fn memory_pipe_reads_prefetched_bytes_and_normalizes_text() {
        let pipe = TransferReadPipe::from_bytes(
            MIME_TEXT_PLAIN_UTF8.to_string(),
            b"hello\r\nworld\r".to_vec(),
        );
        assert_eq!(pipe.mime(), MIME_TEXT_PLAIN_UTF8);
        assert_eq!(pipe.read_text().unwrap(), "hello\nworld\n");
    }
}
