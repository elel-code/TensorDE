use std::io::Result as IoResult;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};

use crate::platform::{ActiveEventLoop, WaylandClipboard};
use fika_core::{FileClipboardRole, encode_file_clipboard_text};
use wayland_client_runtime::data_transfer::{
    MIME_STRING, MIME_TEXT, MIME_TEXT_PLAIN, MIME_TEXT_PLAIN_UTF8, MIME_UTF8_STRING, MimePayload,
    TransferContent, text_mime_types,
};

const MIME_GNOME_COPIED_FILES: &str = "x-special/gnome-copied-files";
const MIME_TEXT_URI_LIST: &str = "text/uri-list";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FileClipboardExportRequest {
    pub(crate) role: FileClipboardRole,
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) text: String,
}

/// Fika's file-clipboard MIME policy over the shared Wayland transfer runtime.
pub(crate) struct ShellClipboard {
    inner: WaylandClipboard,
}

impl ShellClipboard {
    pub(crate) fn new(event_loop: &ActiveEventLoop) -> Self {
        Self {
            inner: event_loop.clipboard(),
        }
    }

    pub(crate) fn backend(&self) -> &'static str {
        self.inner.backend()
    }

    pub(crate) fn store_text_async(
        &self,
        text: String,
    ) -> Result<mpsc::Receiver<IoResult<()>>, String> {
        self.inner
            .store_text_async(text)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn store_file_clipboard_async(
        &self,
        role: FileClipboardRole,
        paths: Vec<PathBuf>,
        text: String,
    ) -> Result<mpsc::Receiver<IoResult<()>>, String> {
        let content = file_clipboard_content(role, &paths, &text)?;
        self.inner
            .store_async(content)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn load_text_async(&self) -> Result<mpsc::Receiver<IoResult<String>>, String> {
        self.inner
            .load_async(&[
                MIME_GNOME_COPIED_FILES,
                MIME_TEXT_URI_LIST,
                MIME_TEXT_PLAIN_UTF8,
                MIME_UTF8_STRING,
                MIME_TEXT_PLAIN,
                MIME_STRING,
                MIME_TEXT,
            ])
            .map_err(|error| error.to_string())
    }

    /// Primary selection (middle-click paste) text load.
    pub(crate) fn load_primary_text_async(
        &self,
    ) -> Result<mpsc::Receiver<IoResult<String>>, String> {
        self.inner
            .load_primary_async(&[
                MIME_TEXT_PLAIN_UTF8,
                MIME_UTF8_STRING,
                MIME_TEXT_PLAIN,
                MIME_STRING,
                MIME_TEXT,
            ])
            .map_err(|error| error.to_string())
    }
}

fn file_clipboard_content(
    role: FileClipboardRole,
    paths: &[PathBuf],
    text: &str,
) -> Result<TransferContent, String> {
    let uri_list = encode_file_clipboard_text(FileClipboardRole::Copy, paths);
    let gnome_role = match role {
        FileClipboardRole::Copy => "copy",
        FileClipboardRole::Cut => "cut",
    };
    let gnome = if uri_list.is_empty() {
        gnome_role.to_string()
    } else {
        format!("{gnome_role}\n{uri_list}")
    };

    let text_bytes = Arc::<[u8]>::from(text.as_bytes());
    let mut payloads = vec![
        MimePayload::new(MIME_GNOME_COPIED_FILES, Arc::<[u8]>::from(gnome.as_bytes()))
            .map_err(|error| error.to_string())?,
        MimePayload::new(MIME_TEXT_URI_LIST, Arc::<[u8]>::from(uri_list.as_bytes()))
            .map_err(|error| error.to_string())?,
    ];
    payloads.extend(text_mime_types().iter().map(|mime| {
        MimePayload::new(*mime, text_bytes.clone()).expect("built-in text MIME types are non-empty")
    }));
    TransferContent::new(payloads).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload_text(content: &TransferContent, mime: &str) -> String {
        let payload = content
            .payloads()
            .iter()
            .find(|payload| payload.mime() == mime)
            .expect("MIME payload");
        String::from_utf8(payload.bytes().to_vec()).unwrap()
    }

    #[test]
    fn file_clipboard_content_offers_native_file_and_text_mimes() {
        let paths = [PathBuf::from("/tmp/a file.txt")];
        let text = encode_file_clipboard_text(FileClipboardRole::Cut, &paths);
        let content = file_clipboard_content(FileClipboardRole::Cut, &paths, &text).unwrap();

        assert_eq!(
            payload_text(&content, MIME_GNOME_COPIED_FILES),
            "cut\nfile:///tmp/a%20file.txt"
        );
        assert_eq!(
            payload_text(&content, MIME_TEXT_URI_LIST),
            "file:///tmp/a%20file.txt"
        );
        assert_eq!(
            payload_text(&content, MIME_TEXT_PLAIN_UTF8),
            "# fika-cut\nfile:///tmp/a%20file.txt"
        );
    }
}
