use std::path::PathBuf;

use super::network::normalize_network_uri;
use super::uri::{file_uri_to_path, path_uri_from_path};

const FIKA_CUT_MARKER: &str = "# fika-cut";
const GNOME_COPY_MARKER: &str = "copy";
const GNOME_CUT_MARKER: &str = "cut";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FileClipboardRole {
    #[default]
    Copy,
    Cut,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileClipboardPayload {
    pub role: FileClipboardRole,
    pub paths: Vec<PathBuf>,
}

impl FileClipboardPayload {
    pub fn new(role: FileClipboardRole, paths: Vec<PathBuf>) -> Self {
        Self { role, paths }
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

pub fn encode_file_clipboard_text(role: FileClipboardRole, paths: &[PathBuf]) -> String {
    let mut lines = Vec::with_capacity(paths.len() + 1);
    if role == FileClipboardRole::Cut {
        lines.push(FIKA_CUT_MARKER.to_string());
    }
    lines.extend(paths.iter().map(|path| path_uri_from_path(path)));
    lines.join("\n")
}

pub fn decode_file_clipboard_text(text: &str) -> Option<FileClipboardPayload> {
    let mut role = FileClipboardRole::Copy;
    let mut paths = Vec::new();
    let mut first_payload_line = true;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('#') {
            if line == FIKA_CUT_MARKER {
                role = FileClipboardRole::Cut;
            }
            continue;
        }
        if first_payload_line {
            match line {
                GNOME_CUT_MARKER => {
                    role = FileClipboardRole::Cut;
                    first_payload_line = false;
                    continue;
                }
                GNOME_COPY_MARKER => {
                    role = FileClipboardRole::Copy;
                    first_payload_line = false;
                    continue;
                }
                _ => {}
            }
        }
        first_payload_line = false;

        if let Some(path) = parse_clipboard_path_line(line) {
            paths.push(path);
        }
    }

    (!paths.is_empty()).then_some(FileClipboardPayload { role, paths })
}

fn parse_clipboard_path_line(line: &str) -> Option<PathBuf> {
    if line.starts_with("file://") {
        return file_uri_to_path(line);
    }
    if let Ok(uri) = normalize_network_uri(line) {
        return Some(PathBuf::from(uri));
    }
    line.starts_with('/').then(|| PathBuf::from(line))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_clipboard_text_encodes_file_uri_list() {
        let text = encode_file_clipboard_text(
            FileClipboardRole::Copy,
            &[
                PathBuf::from("/tmp/a file.txt"),
                PathBuf::from("/tmp/unicode-文档.txt"),
            ],
        );

        assert_eq!(
            text,
            "file:///tmp/a%20file.txt\nfile:///tmp/unicode-%E6%96%87%E6%A1%A3.txt"
        );
    }

    #[test]
    fn file_clipboard_text_round_trips_cut_role() {
        let paths = vec![PathBuf::from("/tmp/a file.txt"), PathBuf::from("/tmp/b")];
        let text = encode_file_clipboard_text(FileClipboardRole::Cut, &paths);

        assert_eq!(
            decode_file_clipboard_text(&text),
            Some(FileClipboardPayload::new(FileClipboardRole::Cut, paths))
        );
    }

    #[test]
    fn file_clipboard_text_preserves_network_uris() {
        let paths = vec![PathBuf::from("smb://server/share/report%202026.txt")];
        let text = encode_file_clipboard_text(FileClipboardRole::Copy, &paths);

        assert_eq!(text, "smb://server/share/report%202026.txt");
        assert_eq!(
            decode_file_clipboard_text("copy\nsmb://server/share/report%202026.txt\n"),
            Some(FileClipboardPayload::new(FileClipboardRole::Copy, paths))
        );
    }

    #[test]
    fn file_clipboard_text_decodes_plain_absolute_paths() {
        assert_eq!(
            decode_file_clipboard_text("/tmp/a\n/tmp/b\n"),
            Some(FileClipboardPayload::new(
                FileClipboardRole::Copy,
                vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")]
            ))
        );
    }

    #[test]
    fn file_clipboard_text_decodes_gnome_copied_files_markers() {
        assert_eq!(
            decode_file_clipboard_text("cut\nfile:///tmp/a%20file.txt\n"),
            Some(FileClipboardPayload::new(
                FileClipboardRole::Cut,
                vec![PathBuf::from("/tmp/a file.txt")]
            ))
        );
        assert_eq!(
            decode_file_clipboard_text("copy\nfile:///tmp/a.txt\n"),
            Some(FileClipboardPayload::new(
                FileClipboardRole::Copy,
                vec![PathBuf::from("/tmp/a.txt")]
            ))
        );
    }

    #[test]
    fn file_clipboard_text_rejects_non_file_text() {
        assert_eq!(decode_file_clipboard_text("hello world"), None);
        assert_eq!(decode_file_clipboard_text("file:///tmp/%ZZ"), None);
    }
}
