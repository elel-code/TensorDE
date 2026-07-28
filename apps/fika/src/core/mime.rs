use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

pub const GENERIC_BINARY_MIME: &str = "application/octet-stream";
const MIME_MAGIC_READ_LIMIT: usize = 4096;

#[derive(Clone, Debug)]
pub struct MimeDatabase {
    extension_mime: HashMap<String, String>,
    literal_mime: HashMap<String, String>,
    suffix_mime: Vec<(String, String)>,
    icon_names: HashMap<String, String>,
    generic_icon_names: HashMap<String, String>,
}

impl MimeDatabase {
    pub fn load() -> Self {
        let mut icon_names = load_mime_icon_name_map("icons");
        let mut generic_icon_names = load_mime_icon_name_map("generic-icons");
        load_mime_xml_icon_name_maps(&mut icon_names, &mut generic_icon_names);

        let glob_maps = load_mime_glob_maps();
        Self {
            extension_mime: glob_maps.extension_mime,
            literal_mime: glob_maps.literal_mime,
            suffix_mime: glob_maps.suffix_mime,
            icon_names,
            generic_icon_names,
        }
    }

    pub fn shared() -> &'static Self {
        static DATABASE: OnceLock<MimeDatabase> = OnceLock::new();
        DATABASE.get_or_init(Self::load)
    }

    pub fn from_maps(
        extension_mime: HashMap<String, String>,
        icon_names: HashMap<String, String>,
        generic_icon_names: HashMap<String, String>,
    ) -> Self {
        Self {
            extension_mime,
            literal_mime: HashMap::new(),
            suffix_mime: Vec::new(),
            icon_names,
            generic_icon_names,
        }
    }

    pub fn mime_for_path(
        &self,
        path: &Path,
        is_dir: bool,
        magic: Option<&[u8]>,
    ) -> Option<Arc<str>> {
        if is_dir {
            return Some(Arc::from("inode/directory"));
        }
        let filename = path
            .file_name()
            .and_then(|filename| filename.to_str())
            .unwrap_or_default();
        Some(self.mime_for_name(filename, is_dir, magic))
    }

    pub fn mime_for_name(&self, filename: &str, is_dir: bool, magic: Option<&[u8]>) -> Arc<str> {
        if is_dir {
            return Arc::from("inode/directory");
        }
        if let Some(mime) = magic.and_then(detect_mime_from_magic) {
            return Arc::from(mime);
        }

        Arc::from(
            self.mime_for_filename(filename)
                .or_else(|| {
                    Path::new(filename)
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .and_then(|extension| self.mime_for_extension(extension))
                })
                .unwrap_or(GENERIC_BINARY_MIME),
        )
    }

    pub fn mime_for_extension(&self, extension: &str) -> Option<&str> {
        self.extension_mime
            .get(&extension.to_ascii_lowercase())
            .map(String::as_str)
    }

    fn mime_for_filename(&self, filename: &str) -> Option<&str> {
        let filename = filename.to_ascii_lowercase();
        if let Some(mime) = self.literal_mime.get(&filename) {
            return Some(mime);
        }

        self.suffix_mime
            .iter()
            .find(|(suffix, _)| filename_matches_mime_suffix(&filename, suffix))
            .map(|(_, mime)| mime.as_str())
    }

    pub fn icon_name_for_mime(&self, mime: &str) -> Option<&str> {
        self.icon_names.get(mime).map(String::as_str)
    }

    pub fn generic_icon_name_for_mime(&self, mime: &str) -> Option<&str> {
        self.generic_icon_names
            .get(mime)
            .map(String::as_str)
            .or_else(|| generic_mime_icon_name(mime))
    }
}

impl std::default::Default for MimeDatabase {
    fn default() -> Self {
        Self::load()
    }
}

pub fn mime_icon_name(mime: &str) -> Option<String> {
    let mime = mime.trim();
    (!mime.is_empty() && mime.contains('/')).then(|| mime.replace('/', "-"))
}

pub fn generic_mime_icon_name(mime: &str) -> Option<&'static str> {
    match mime.split_once('/').map(|(family, _)| family) {
        Some("text") => Some("text-x-generic"),
        Some("image") => Some("image-x-generic"),
        Some("audio") => Some("audio-x-generic"),
        Some("video") => Some("video-x-generic"),
        Some("font") => Some("font-x-generic"),
        Some("inode") => Some("inode-directory"),
        Some("application") => Some("application-octet-stream"),
        _ => None,
    }
}

pub fn mime_magic_resolution_required(
    is_dir: bool,
    size_bytes: u64,
    mime_type: Option<&str>,
    mime_magic_checked: bool,
) -> bool {
    !mime_magic_checked && !is_dir && size_bytes > 0 && mime_type == Some(GENERIC_BINARY_MIME)
}

pub(crate) fn read_mime_magic(path: &Path) -> io::Result<Option<Vec<u8>>> {
    let mut file = std::fs::File::open(path)?;
    let mut bytes = vec![0; MIME_MAGIC_READ_LIMIT];
    let read = file.read(&mut bytes)?;
    if read == 0 {
        return Ok(None);
    }
    bytes.truncate(read);
    Ok(Some(bytes))
}

pub fn detect_mime_from_magic(bytes: &[u8]) -> Option<&'static str> {
    let trimmed = trim_ascii_prefix(bytes);
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.starts_with(b"%PDF-") {
        Some("application/pdf")
    } else if bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
    {
        Some("application/zip")
    } else if bytes.len() >= 262 && &bytes[257..262] == b"ustar" {
        Some("application/x-tar")
    } else if bytes.starts_with(&[0x1f, 0x8b]) {
        Some("application/gzip")
    } else if bytes.starts_with(b"BZh") {
        Some("application/x-bzip")
    } else if bytes.starts_with(&[0xfd, b'7', b'z', b'X', b'Z', 0x00]) {
        Some("application/x-xz")
    } else if bytes.starts_with(b"7z\xbc\xaf\x27\x1c") {
        Some("application/x-7z-compressed")
    } else if bytes.starts_with(b"Rar!\x1a\x07\x00") || bytes.starts_with(b"Rar!\x1a\x07\x01\x00") {
        Some("application/vnd.rar")
    } else if let Some(mime) = portable_executable_mime_from_magic(bytes) {
        Some(mime)
    } else if bytes.starts_with(b"\x7fELF") {
        Some("application/x-executable")
    } else if bytes.starts_with(b"#!") {
        Some(script_mime_from_shebang(bytes))
    } else if bytes.starts_with(b"ID3")
        || bytes.starts_with(&[0xff, 0xfb])
        || bytes.starts_with(&[0xff, 0xf3])
        || bytes.starts_with(&[0xff, 0xf2])
    {
        Some("audio/mpeg")
    } else if bytes.starts_with(b"fLaC") {
        Some("audio/flac")
    } else if bytes.starts_with(b"OggS") {
        Some("audio/ogg")
    } else if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        iso_base_media_mime(bytes)
    } else if bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) {
        Some("video/x-matroska")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if looks_like_svg(trimmed) {
        Some("image/svg+xml")
    } else if looks_like_html(trimmed) {
        Some("text/html")
    } else if looks_like_xml(trimmed) {
        Some("application/xml")
    } else if looks_like_text(bytes) {
        Some("text/plain")
    } else {
        None
    }
}

fn portable_executable_mime_from_magic(bytes: &[u8]) -> Option<&'static str> {
    if !bytes.starts_with(b"MZ") {
        return None;
    }

    let Some(offset_bytes) = bytes.get(0x3c..0x40) else {
        return Some("application/x-msdownload");
    };
    let pe_offset = u32::from_le_bytes(offset_bytes.try_into().ok()?) as usize;
    if bytes.get(pe_offset..pe_offset.saturating_add(4)) == Some(b"PE\0\0".as_slice()) {
        Some("application/vnd.microsoft.portable-executable")
    } else {
        Some("application/x-msdownload")
    }
}

fn iso_base_media_mime(bytes: &[u8]) -> Option<&'static str> {
    let major = bytes.get(8..12)?;
    if matches!(major, b"avif" | b"avis") || iso_base_media_has_compatible_brand(bytes, b"avif") {
        Some("image/avif")
    } else if matches!(major, b"qt  ") {
        Some("video/quicktime")
    } else {
        Some("video/mp4")
    }
}

fn iso_base_media_has_compatible_brand(bytes: &[u8], brand: &[u8; 4]) -> bool {
    bytes
        .get(16..)
        .is_some_and(|brands| brands.chunks_exact(4).any(|chunk| chunk == brand))
}

fn trim_ascii_prefix(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| !matches!(byte, b'\t' | b'\n' | b'\r' | b' '))
        .unwrap_or(bytes.len());
    &bytes[start..]
}

fn script_mime_from_shebang(bytes: &[u8]) -> &'static str {
    let line_end = bytes
        .iter()
        .position(|byte| *byte == b'\n' || *byte == b'\r')
        .unwrap_or(bytes.len())
        .min(256);
    let line = String::from_utf8_lossy(&bytes[..line_end]).to_ascii_lowercase();
    if line.contains("python") {
        "text/x-python"
    } else if line.contains("ruby") {
        "application/x-ruby"
    } else if line.contains("perl") {
        "application/x-perl"
    } else if line.contains("node") || line.contains("deno") {
        "application/javascript"
    } else {
        "text/x-shellscript"
    }
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    bytes.starts_with(b"<svg")
        || bytes
            .windows(b"<svg".len())
            .take(16)
            .any(|window| window.eq_ignore_ascii_case(b"<svg"))
}

fn looks_like_html(bytes: &[u8]) -> bool {
    [b"<!doctype html".as_slice(), b"<html", b"<head", b"<body"]
        .iter()
        .any(|prefix| bytes_ascii_starts_with(bytes, prefix))
}

fn looks_like_xml(bytes: &[u8]) -> bool {
    bytes_ascii_starts_with(bytes, b"<?xml")
}

fn bytes_ascii_starts_with(bytes: &[u8], prefix: &[u8]) -> bool {
    bytes.len() >= prefix.len() && bytes[..prefix.len()].eq_ignore_ascii_case(prefix)
}

fn looks_like_text(bytes: &[u8]) -> bool {
    if bytes.is_empty() || bytes.contains(&0) {
        return false;
    }
    let sample = &bytes[..bytes.len().min(512)];
    std::str::from_utf8(sample).is_ok()
        && sample
            .iter()
            .all(|byte| matches!(byte, b'\t' | b'\n' | b'\r' | 0x20..=0x7e | 0x80..=0xff))
}

pub fn parse_mime_icon_name_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (mime, icon_name) = line.split_once(':')?;
    let mime = mime.trim();
    let icon_name = icon_name.trim();
    if mime.is_empty() || icon_name.is_empty() {
        return None;
    }
    Some((mime.to_string(), icon_name.to_string()))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MimeXmlIconKind {
    Icon,
    GenericIcon,
}

fn parse_mime_xml_icon_names(contents: &str) -> Vec<(String, MimeXmlIconKind, String)> {
    let mut icons = Vec::new();

    let mut cursor = 0;
    while let Some(start) = contents[cursor..].find("<mime-type") {
        let tag_start = cursor + start;
        let Some(tag_end) = xml_tag_end(contents, tag_start) else {
            break;
        };
        let open_tag = &contents[tag_start..=tag_end];
        let Some(mime) = xml_attribute(open_tag, "type").filter(|mime| !mime.is_empty()) else {
            cursor = tag_end + 1;
            continue;
        };

        let content_start = tag_end + 1;
        let Some(close) = contents[content_start..].find("</mime-type>") else {
            cursor = content_start;
            continue;
        };
        let content_end = content_start + close;
        parse_mime_xml_icon_children(&contents[content_start..content_end], &mime, &mut icons);
        cursor = content_end + "</mime-type>".len();
    }

    icons
}

fn parse_mime_xml_icon_children(
    contents: &str,
    mime: &str,
    icons: &mut Vec<(String, MimeXmlIconKind, String)>,
) {
    let mut cursor = 0;
    while let Some(start) = contents[cursor..].find('<') {
        let tag_start = cursor + start;
        let Some(tag_end) = xml_tag_end(contents, tag_start) else {
            break;
        };
        let tag = &contents[tag_start..=tag_end];
        if let Some(kind) = mime_xml_icon_kind(tag)
            && let Some(icon_name) = xml_attribute(tag, "name").filter(|name| !name.is_empty())
        {
            icons.push((mime.to_string(), kind, icon_name));
        }
        cursor = tag_end + 1;
    }
}

fn mime_xml_icon_kind(tag: &str) -> Option<MimeXmlIconKind> {
    match xml_tag_local_name(tag)? {
        "icon" => Some(MimeXmlIconKind::Icon),
        "generic-icon" => Some(MimeXmlIconKind::GenericIcon),
        _ => None,
    }
}

fn xml_tag_end(contents: &str, tag_start: usize) -> Option<usize> {
    let mut quote = None;
    for (index, value) in contents[tag_start..].char_indices() {
        match value {
            '"' | '\'' if quote == Some(value) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(value),
            '>' if quote.is_none() => return Some(tag_start + index),
            _ => {}
        }
    }
    None
}

fn xml_tag_local_name(tag: &str) -> Option<&str> {
    let tag = tag.trim_start().strip_prefix('<')?.trim_start();
    if tag.starts_with('/') || tag.starts_with('!') || tag.starts_with('?') {
        return None;
    }
    let name_end = tag
        .find(|ch: char| ch.is_whitespace() || matches!(ch, '/' | '>'))
        .unwrap_or(tag.len());
    let name = &tag[..name_end];
    name.rsplit(':').next().filter(|name| !name.is_empty())
}

fn xml_attribute(tag: &str, attribute: &str) -> Option<String> {
    let mut cursor = 0;
    while cursor < tag.len() {
        let attribute_start = tag[cursor..].find(attribute)? + cursor;
        let after_name = attribute_start + attribute.len();
        if !is_xml_name_boundary(tag, attribute_start, after_name) {
            cursor = after_name;
            continue;
        }

        let mut rest = tag[after_name..].trim_start();
        if !rest.starts_with('=') {
            cursor = after_name;
            continue;
        }
        rest = rest[1..].trim_start();
        let mut chars = rest.chars();
        let quote = chars.next()?;
        if quote != '"' && quote != '\'' {
            cursor = after_name;
            continue;
        }
        let value_start = quote.len_utf8();
        let value_end = rest[value_start..].find(quote)? + value_start;
        let value = xml_unescape_attribute(&rest[value_start..value_end]);
        return Some(value);
    }
    None
}

fn is_xml_name_boundary(tag: &str, start: usize, end: usize) -> bool {
    let before = tag[..start].chars().next_back();
    let after = tag[end..].chars().next();
    before.is_none_or(|ch| !is_xml_name_char(ch)) && after.is_none_or(|ch| !is_xml_name_char(ch))
}

fn is_xml_name_char(value: char) -> bool {
    value.is_ascii_alphanumeric() || matches!(value, '_' | '-' | ':' | '.')
}

fn xml_unescape_attribute(value: &str) -> String {
    if !value.contains('&') {
        return value.to_string();
    }
    let mut decoded = String::with_capacity(value.len());
    let mut cursor = value;
    while let Some(index) = cursor.find('&') {
        decoded.push_str(&cursor[..index]);
        let entity = &cursor[index..];
        if let Some(rest) = entity.strip_prefix("&quot;") {
            decoded.push('"');
            cursor = rest;
        } else if let Some(rest) = entity.strip_prefix("&apos;") {
            decoded.push('\'');
            cursor = rest;
        } else if let Some(rest) = entity.strip_prefix("&amp;") {
            decoded.push('&');
            cursor = rest;
        } else if let Some(rest) = entity.strip_prefix("&lt;") {
            decoded.push('<');
            cursor = rest;
        } else if let Some(rest) = entity.strip_prefix("&gt;") {
            decoded.push('>');
            cursor = rest;
        } else {
            decoded.push('&');
            cursor = &cursor[index + 1..];
        }
    }
    decoded.push_str(cursor);
    decoded
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum MimeGlobPattern {
    Extension(String),
    Suffix(String),
    Literal(String),
}

fn parse_mime_globs2_pattern(line: &str) -> Option<(u16, String, MimeGlobPattern)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let mut parts = line.splitn(3, ':');
    let weight = parts.next()?.parse::<u16>().ok()?;
    let mime = parts.next()?.trim();
    let glob = parts.next()?.trim();
    if mime.is_empty() || glob.is_empty() {
        return None;
    }

    if let Some(suffix) = glob.strip_prefix("*.") {
        if suffix.is_empty() || has_unsupported_mime_glob_byte(suffix) {
            return None;
        }
        let suffix = suffix.to_ascii_lowercase();
        let pattern = if suffix.contains('.') {
            MimeGlobPattern::Suffix(suffix)
        } else {
            MimeGlobPattern::Extension(suffix)
        };
        return Some((weight, mime.to_string(), pattern));
    }

    if has_unsupported_mime_glob_byte(glob) {
        return None;
    }
    Some((
        weight,
        mime.to_string(),
        MimeGlobPattern::Literal(glob.to_ascii_lowercase()),
    ))
}

fn has_unsupported_mime_glob_byte(value: &str) -> bool {
    value
        .bytes()
        .any(|byte| matches!(byte, b'*' | b'?' | b'[' | b']' | b'/'))
}

#[derive(Clone, Debug, Default)]
struct MimeGlobMaps {
    extension_mime: HashMap<String, String>,
    literal_mime: HashMap<String, String>,
    suffix_mime: Vec<(String, String)>,
}

fn load_mime_glob_maps() -> MimeGlobMaps {
    let mut extensions = HashMap::<String, (u16, String)>::new();
    let mut literals = HashMap::<String, (u16, String)>::new();
    let mut suffixes = HashMap::<String, (u16, String)>::new();
    for path in mime_globs_paths() {
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };
        for line in contents.lines() {
            let Some((weight, mime, pattern)) = parse_mime_globs2_pattern(line) else {
                continue;
            };
            match pattern {
                MimeGlobPattern::Extension(extension) => {
                    insert_weighted_mime(&mut extensions, extension, weight, mime);
                }
                MimeGlobPattern::Literal(literal) => {
                    insert_weighted_mime(&mut literals, literal, weight, mime);
                }
                MimeGlobPattern::Suffix(suffix) => {
                    insert_weighted_mime(&mut suffixes, suffix, weight, mime);
                }
            }
        }
    }

    let mut suffix_mime = suffixes
        .into_iter()
        .map(|(suffix, (_, mime))| (suffix, mime))
        .collect::<Vec<_>>();
    suffix_mime.sort_by(|(left, _), (right, _)| {
        right.len().cmp(&left.len()).then_with(|| left.cmp(right))
    });

    MimeGlobMaps {
        extension_mime: extensions
            .into_iter()
            .map(|(extension, (_, mime))| (extension, mime))
            .collect(),
        literal_mime: literals
            .into_iter()
            .map(|(literal, (_, mime))| (literal, mime))
            .collect(),
        suffix_mime,
    }
}

fn insert_weighted_mime(
    map: &mut HashMap<String, (u16, String)>,
    key: String,
    weight: u16,
    mime: String,
) {
    let replace = map
        .get(&key)
        .is_none_or(|(existing_weight, _)| weight >= *existing_weight);
    if replace {
        map.insert(key, (weight, mime));
    }
}

fn filename_matches_mime_suffix(filename: &str, suffix: &str) -> bool {
    let Some(start) = filename.len().checked_sub(suffix.len()) else {
        return false;
    };
    start > 0 && filename.ends_with(suffix) && filename.as_bytes().get(start - 1) == Some(&b'.')
}

fn load_mime_icon_name_map(file_name: &str) -> HashMap<String, String> {
    let mut icon_names = HashMap::new();
    for path in mime_database_paths(file_name) {
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };
        for line in contents.lines() {
            let Some((mime, icon_name)) = parse_mime_icon_name_line(line) else {
                continue;
            };
            icon_names.entry(mime).or_insert(icon_name);
        }
    }
    icon_names
}

fn load_mime_xml_icon_name_maps(
    icon_names: &mut HashMap<String, String>,
    generic_icon_names: &mut HashMap<String, String>,
) {
    for path in mime_xml_paths() {
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };
        for (mime, kind, icon_name) in parse_mime_xml_icon_names(&contents) {
            match kind {
                MimeXmlIconKind::Icon => {
                    icon_names.entry(mime).or_insert(icon_name);
                }
                MimeXmlIconKind::GenericIcon => {
                    generic_icon_names.entry(mime).or_insert(icon_name);
                }
            }
        }
    }
}

fn mime_globs_paths() -> Vec<PathBuf> {
    mime_database_roots()
        .into_iter()
        .map(|root| root.join("globs2"))
        .collect()
}

fn mime_database_paths(file_name: &str) -> Vec<PathBuf> {
    mime_database_roots()
        .into_iter()
        .map(|root| root.join(file_name))
        .collect()
}

fn mime_xml_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for root in mime_database_roots() {
        collect_mime_xml_paths(&root, &mut paths);
    }
    paths
}

fn collect_mime_xml_paths(root: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("xml") {
            push_unique_path(paths, path);
            continue;
        }
        if !path.is_dir() {
            continue;
        }
        let Ok(children) = fs::read_dir(path) else {
            continue;
        };
        for child in children.flatten() {
            let path = child.path();
            if path.extension().and_then(|extension| extension.to_str()) == Some("xml") {
                push_unique_path(paths, path);
            }
        }
    }
}

fn mime_database_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(data_home) = env::var_os("XDG_DATA_HOME").filter(|path| !path.is_empty()) {
        push_unique_path(&mut roots, PathBuf::from(data_home).join("mime"));
    }

    let data_dirs =
        env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    for dir in data_dirs.split(':').filter(|dir| !dir.is_empty()) {
        push_unique_path(&mut roots, Path::new(dir).join("mime"));
    }
    push_unique_path(&mut roots, PathBuf::from("/usr/share/mime"));
    roots
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mime_globs2_extension_entries() {
        assert_eq!(
            parse_mime_globs2_pattern("50:text/rust:*.rs"),
            Some((
                50,
                "text/rust".to_string(),
                MimeGlobPattern::Extension("rs".to_string())
            ))
        );
        assert_eq!(parse_mime_globs2_pattern("# comment"), None);
        assert_eq!(parse_mime_globs2_pattern("50:text/plain:*.[ch]"), None);
    }

    #[test]
    fn parses_mime_globs2_literal_and_multi_suffix_entries() {
        assert_eq!(
            parse_mime_globs2_pattern("60:text/x-makefile:Makefile"),
            Some((
                60,
                "text/x-makefile".to_string(),
                MimeGlobPattern::Literal("makefile".to_string())
            ))
        );
        assert_eq!(
            parse_mime_globs2_pattern("80:application/x-compressed-tar:*.tar.gz"),
            Some((
                80,
                "application/x-compressed-tar".to_string(),
                MimeGlobPattern::Suffix("tar.gz".to_string())
            ))
        );
        assert_eq!(parse_mime_globs2_pattern("50:text/plain:*.[ch]"), None);
    }

    #[test]
    fn parses_mime_icon_name_entries() {
        assert_eq!(
            parse_mime_icon_name_line("application/pdf:x-office-document"),
            Some((
                "application/pdf".to_string(),
                "x-office-document".to_string()
            ))
        );
        assert_eq!(parse_mime_icon_name_line("  "), None);
        assert_eq!(parse_mime_icon_name_line("application/pdf:"), None);
    }

    #[test]
    fn parses_mime_xml_icon_entries() {
        assert_eq!(
            parse_mime_xml_icon_names(
                r#"
<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
  <mime-type type="application/pdf">
    <icon name="application-pdf"/>
    <generic-icon name="x-office-document"/>
  </mime-type>
</mime-info>
"#
            ),
            vec![
                (
                    "application/pdf".to_string(),
                    MimeXmlIconKind::Icon,
                    "application-pdf".to_string()
                ),
                (
                    "application/pdf".to_string(),
                    MimeXmlIconKind::GenericIcon,
                    "x-office-document".to_string()
                )
            ]
        );
    }

    #[test]
    fn detects_common_magic_mime_types() {
        assert_eq!(
            detect_mime_from_magic(b"\x89PNG\r\n\x1a\nrest"),
            Some("image/png")
        );
        assert_eq!(detect_mime_from_magic(b"%PDF-1.7"), Some("application/pdf"));
        assert_eq!(
            detect_mime_from_magic(b"#!/usr/bin/env python\nprint('ok')\n"),
            Some("text/x-python")
        );
        let mut pe = vec![0u8; 0x84];
        pe[0..2].copy_from_slice(b"MZ");
        pe[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");
        assert_eq!(
            detect_mime_from_magic(&pe),
            Some("application/vnd.microsoft.portable-executable")
        );
        assert_eq!(
            detect_mime_from_magic(b"MZstub"),
            Some("application/x-msdownload")
        );
        assert_eq!(
            detect_mime_from_magic(b"\0\0\0\x20ftypavif\0\0\0\0avifmif1"),
            Some("image/avif")
        );
        assert_eq!(
            detect_mime_from_magic(b"\0\0\0\x20ftypavis\0\0\0\0avisavif"),
            Some("image/avif")
        );
        assert_eq!(
            detect_mime_from_magic(b"\0\0\0\x18ftypisom\0\0\0\0avif"),
            Some("image/avif")
        );
        assert_eq!(
            detect_mime_from_magic(b"\0\0\0\x18ftypqt  \0\0\0\0"),
            Some("video/quicktime")
        );
        assert_eq!(
            detect_mime_from_magic(b"\0\0\0\x18ftypisom\0\0\0\0mp41"),
            Some("video/mp4")
        );
        assert_eq!(
            detect_mime_from_magic(b"   <svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
            Some("image/svg+xml")
        );
        assert_eq!(detect_mime_from_magic(b"plain text"), Some("text/plain"));
        assert_eq!(detect_mime_from_magic(&[0, 159, 146, 150]), None);
    }

    #[test]
    fn mime_database_uses_weighted_extension_mapping() {
        let database = MimeDatabase {
            extension_mime: HashMap::from([
                ("foo".to_string(), "text/x-low".to_string()),
                ("rs".to_string(), "text/rust".to_string()),
            ]),
            literal_mime: HashMap::new(),
            suffix_mime: Vec::new(),
            icon_names: HashMap::new(),
            generic_icon_names: HashMap::new(),
        };

        assert_eq!(
            database
                .mime_for_path(Path::new("lib.rs"), false, None)
                .as_deref(),
            Some("text/rust")
        );
        assert_eq!(
            database
                .mime_for_path(Path::new("dir"), true, None)
                .as_deref(),
            Some("inode/directory")
        );
        assert_eq!(
            database
                .mime_for_path(Path::new("archive.foo"), false, Some(b"PK\x03\x04"))
                .as_deref(),
            Some("application/zip")
        );
        assert_eq!(
            database.mime_for_name("lib.rs", false, None).as_ref(),
            "text/rust"
        );
        assert_eq!(
            database.mime_for_name("dir", true, None).as_ref(),
            "inode/directory"
        );
    }

    #[test]
    fn mime_database_matches_literal_names_and_longest_suffix_before_extension() {
        let database = MimeDatabase {
            extension_mime: HashMap::from([("gz".to_string(), "application/gzip".to_string())]),
            literal_mime: HashMap::from([
                ("cargo.toml".to_string(), "text/x-toml".to_string()),
                ("makefile".to_string(), "text/x-makefile".to_string()),
            ]),
            suffix_mime: vec![
                (
                    "tar.gz".to_string(),
                    "application/x-compressed-tar".to_string(),
                ),
                ("gz".to_string(), "application/gzip".to_string()),
            ],
            icon_names: HashMap::new(),
            generic_icon_names: HashMap::new(),
        };

        assert_eq!(
            database
                .mime_for_path(Path::new("Cargo.toml"), false, None)
                .as_deref(),
            Some("text/x-toml")
        );
        assert_eq!(
            database
                .mime_for_path(Path::new("Makefile"), false, None)
                .as_deref(),
            Some("text/x-makefile")
        );
        assert_eq!(
            database
                .mime_for_path(Path::new("archive.tar.gz"), false, None)
                .as_deref(),
            Some("application/x-compressed-tar")
        );
        assert_eq!(
            database
                .mime_for_path(Path::new("plain.gz"), false, None)
                .as_deref(),
            Some("application/gzip")
        );
        assert_eq!(
            database.mime_for_name("Cargo.toml", false, None).as_ref(),
            "text/x-toml"
        );
        assert_eq!(
            database
                .mime_for_name("archive.tar.gz", false, None)
                .as_ref(),
            "application/x-compressed-tar"
        );
    }
}
