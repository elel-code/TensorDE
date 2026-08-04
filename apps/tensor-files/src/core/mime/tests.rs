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
