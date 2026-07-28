use super::bus::{BusCallTarget, BusController, BusError, BusKind};
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

pub const ARK_DND_EXTRACT_SERVICE_MIME: &str = "application/x-kde-ark-dndextract-service";
pub const ARK_DND_EXTRACT_PATH_MIME: &str = "application/x-kde-ark-dndextract-path";
pub const ARK_DND_EXTRACT_INTERFACE: &str = "org.kde.ark.DndExtract";
pub const ARK_DND_EXTRACT_METHOD: &str = "extractSelectedFilesTo";

const ARCHIVE_MIME_TYPES: &[&str] = &[
    "application/arj",
    "application/gzip",
    "application/vnd.debian.binary-package",
    "application/vnd.ms-cab-compressed",
    "application/vnd.rar",
    "application/x-7z-compressed",
    "application/x-archive",
    "application/x-arj",
    "application/x-bzip",
    "application/x-bzip-compressed-tar",
    "application/x-bzip2",
    "application/x-bzip2-compressed-tar",
    "application/x-cd-image",
    "application/x-compress",
    "application/x-compressed-tar",
    "application/x-cpio",
    "application/x-cpio-compressed",
    "application/x-deb",
    "application/x-java-archive",
    "application/x-lha",
    "application/x-lrzip",
    "application/x-lrzip-compressed-tar",
    "application/x-lz4",
    "application/x-lz4-compressed-tar",
    "application/x-lzip",
    "application/x-lzip-compressed-tar",
    "application/x-lzma",
    "application/x-lzma-compressed-tar",
    "application/x-lzop",
    "application/x-rpm",
    "application/x-source-rpm",
    "application/x-stuffit",
    "application/x-tar",
    "application/x-tarz",
    "application/x-tzo",
    "application/x-xar",
    "application/x-xz",
    "application/x-xz-compressed-tar",
    "application/x-zstd-compressed-tar",
    "application/zip",
    "application/zlib",
    "application/zstd",
];

const ARCHIVE_EXTENSIONS: &[&str] = &[
    "7z", "arj", "cab", "deb", "gz", "iso", "lha", "lrz", "lz", "lz4", "lzma", "lzo", "rar", "rpm",
    "tar", "tar.bz2", "tar.gz", "tar.lrz", "tar.lz", "tar.lz4", "tar.lzma", "tar.lzo", "tar.xz",
    "tar.zst", "tbz", "tbz2", "tgz", "tlz", "txz", "tzst", "xar", "zip", "zst",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArkDndExtractPayload {
    pub remote_service: String,
    pub remote_path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArkDndExtractRequest {
    pub target: BusCallTarget,
    pub destination: String,
}

#[derive(Debug)]
pub enum ArkDndExtractError {
    InvalidDestination { path: PathBuf, message: String },
    Bus(Box<BusError>),
}

impl fmt::Display for ArkDndExtractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDestination { path, message } => {
                write!(
                    f,
                    "invalid Ark DnD extract destination {}: {message}",
                    path.display()
                )
            }
            Self::Bus(error) => write!(f, "{error}"),
        }
    }
}

impl Error for ArkDndExtractError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Bus(error) => Some(error.as_ref()),
            Self::InvalidDestination { .. } => None,
        }
    }
}

impl From<BusError> for ArkDndExtractError {
    fn from(error: BusError) -> Self {
        Self::Bus(Box::new(error))
    }
}

pub fn is_archive_mime_or_path(mime_type: Option<&str>, path: &Path) -> bool {
    if mime_type
        .map(str::trim)
        .filter(|mime| !mime.is_empty())
        .is_some_and(|mime| {
            ARCHIVE_MIME_TYPES
                .iter()
                .any(|known| mime.eq_ignore_ascii_case(known))
        })
    {
        return true;
    }

    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    ARCHIVE_EXTENSIONS
        .iter()
        .any(|extension| name.ends_with(&format!(".{extension}")))
}

pub fn ark_dnd_extract_payload(
    service_bytes: Option<&[u8]>,
    path_bytes: Option<&[u8]>,
) -> Option<ArkDndExtractPayload> {
    let remote_service = trimmed_utf8_payload(service_bytes?)?;
    let remote_path = trimmed_utf8_payload(path_bytes?)?;
    if !is_dbus_bus_name_candidate(remote_service) || !is_dbus_object_path_candidate(remote_path) {
        return None;
    }
    Some(ArkDndExtractPayload {
        remote_service: remote_service.to_string(),
        remote_path: remote_path.to_string(),
    })
}

pub fn ark_dnd_extract_request(
    payload: &ArkDndExtractPayload,
    destination: &Path,
) -> Result<ArkDndExtractRequest, ArkDndExtractError> {
    let destination = ark_dnd_extract_destination(destination)?;
    let target = BusCallTarget::new(
        BusKind::Session,
        payload.remote_service.as_str(),
        payload.remote_path.as_str(),
        ARK_DND_EXTRACT_INTERFACE,
        ARK_DND_EXTRACT_METHOD,
    )?;
    Ok(ArkDndExtractRequest {
        target,
        destination,
    })
}

pub async fn execute_ark_dnd_extract(
    payload: &ArkDndExtractPayload,
    destination: &Path,
) -> Result<(), ArkDndExtractError> {
    execute_ark_dnd_extract_with_bus(BusController::shared(), payload, destination).await
}

pub async fn execute_ark_dnd_extract_with_bus(
    bus: &BusController,
    payload: &ArkDndExtractPayload,
    destination: &Path,
) -> Result<(), ArkDndExtractError> {
    let request = ark_dnd_extract_request(payload, destination)?;
    let proxy = bus.proxy(&request.target).await?;
    let method = request.target.method().to_string();
    bus.call_with_retry(&request.target, || {
        let destination = request.destination.clone();
        let method = method.clone();
        let proxy = &proxy;
        async move { proxy.call::<_, _, ()>(method.as_str(), &destination).await }
    })
    .await?;
    Ok(())
}

fn ark_dnd_extract_destination(destination: &Path) -> Result<String, ArkDndExtractError> {
    if !destination.is_absolute() {
        return Err(ArkDndExtractError::InvalidDestination {
            path: destination.to_path_buf(),
            message: "destination must be an absolute local path".to_string(),
        });
    }
    let Some(destination) = destination.to_str() else {
        return Err(ArkDndExtractError::InvalidDestination {
            path: destination.to_path_buf(),
            message: "destination path is not valid UTF-8".to_string(),
        });
    };
    if destination.is_empty() || destination.contains('\0') {
        return Err(ArkDndExtractError::InvalidDestination {
            path: PathBuf::from(destination),
            message: "destination must be a non-empty D-Bus string".to_string(),
        });
    }
    Ok(destination.to_string())
}

fn trimmed_utf8_payload(bytes: &[u8]) -> Option<&str> {
    let value = std::str::from_utf8(bytes).ok()?;
    let value = value.trim_matches(|ch: char| ch == '\0' || ch.is_ascii_whitespace());
    (!value.is_empty() && !value.contains('\0')).then_some(value)
}

fn is_dbus_bus_name_candidate(value: &str) -> bool {
    if value.len() > 255 || value.starts_with('.') || value.ends_with('.') {
        return false;
    }
    if !(value.starts_with(':') || value.contains('.')) {
        return false;
    }

    let name = value.strip_prefix(':').unwrap_or(value);
    if name.is_empty() {
        return false;
    }
    name.split('.')
        .all(|part| !part.is_empty() && part.bytes().all(is_dbus_bus_name_byte))
}

fn is_dbus_bus_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

fn is_dbus_object_path_candidate(value: &str) -> bool {
    if !value.starts_with('/') {
        return false;
    }
    if value == "/" {
        return true;
    }
    if value.ends_with('/') || value.contains("//") {
        return false;
    }
    value
        .split('/')
        .skip(1)
        .all(|part| !part.is_empty() && part.bytes().all(is_dbus_object_path_byte))
}

fn is_dbus_object_path_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use super::{
        ARK_DND_EXTRACT_INTERFACE, ARK_DND_EXTRACT_METHOD, ARK_DND_EXTRACT_PATH_MIME,
        ARK_DND_EXTRACT_SERVICE_MIME, ArkDndExtractError, ArkDndExtractPayload,
        ark_dnd_extract_payload, ark_dnd_extract_request, execute_ark_dnd_extract_with_bus,
        is_archive_mime_or_path,
    };
    use crate::core::bus::{BusConfig, BusController, BusKind};
    use std::path::Path;
    use std::time::Duration;

    #[test]
    fn archive_classifier_detects_common_mime_types_and_extensions() {
        assert!(is_archive_mime_or_path(
            Some("application/zip"),
            Path::new("download.bin")
        ));
        assert!(is_archive_mime_or_path(
            Some("application/x-xz-compressed-tar"),
            Path::new("download.bin")
        ));
        assert!(is_archive_mime_or_path(None, Path::new("project.tar.gz")));
        assert!(is_archive_mime_or_path(None, Path::new("backup.7z")));
        assert!(!is_archive_mime_or_path(
            Some("text/plain"),
            Path::new("archive-notes.txt")
        ));
        assert!(!is_archive_mime_or_path(None, Path::new("zipper")));
    }

    #[test]
    fn ark_dnd_extract_payload_parses_service_and_object_path() {
        assert_eq!(
            ark_dnd_extract_payload(Some(b" :1.42\0"), Some(b"\n/Ark/DndExtract_1\t")),
            Some(ArkDndExtractPayload {
                remote_service: ":1.42".to_string(),
                remote_path: "/Ark/DndExtract_1".to_string(),
            })
        );
        assert_eq!(
            ARK_DND_EXTRACT_SERVICE_MIME,
            "application/x-kde-ark-dndextract-service"
        );
        assert_eq!(
            ARK_DND_EXTRACT_PATH_MIME,
            "application/x-kde-ark-dndextract-path"
        );
    }

    #[test]
    fn ark_dnd_extract_payload_requires_both_values() {
        assert_eq!(ark_dnd_extract_payload(Some(b"org.kde.ark"), None), None);
        assert_eq!(
            ark_dnd_extract_payload(None, Some(b"/Ark/DndExtract")),
            None
        );
    }

    #[test]
    fn ark_dnd_extract_payload_rejects_empty_or_invalid_values() {
        assert_eq!(ark_dnd_extract_payload(Some(b""), Some(b"/Ark")), None);
        assert_eq!(
            ark_dnd_extract_payload(Some(b"org.kde.ark"), Some(b"Ark/DndExtract")),
            None
        );
        assert_eq!(
            ark_dnd_extract_payload(Some(b"org.kde.ark"), Some(b"/Ark//DndExtract")),
            None
        );
        assert_eq!(
            ark_dnd_extract_payload(Some(b"not a service"), Some(b"/Ark/DndExtract")),
            None
        );
        assert_eq!(
            ark_dnd_extract_payload(Some(b"org.kde.ark"), Some(b"/Ark/Dnd-Extract")),
            None
        );
    }

    #[test]
    fn ark_dnd_extract_payload_rejects_non_utf8_or_embedded_nul() {
        assert_eq!(
            ark_dnd_extract_payload(Some(&[0xff]), Some(b"/Ark/DndExtract")),
            None
        );
        assert_eq!(
            ark_dnd_extract_payload(Some(b"org.kde\0ark"), Some(b"/Ark/DndExtract")),
            None
        );
    }

    #[test]
    fn ark_dnd_extract_request_builds_session_bus_call() {
        let payload = ArkDndExtractPayload {
            remote_service: ":1.42".to_string(),
            remote_path: "/Ark/DndExtract_1".to_string(),
        };

        let request = ark_dnd_extract_request(&payload, Path::new("/tmp/fika-dnd-target")).unwrap();

        assert_eq!(request.target.kind(), BusKind::Session);
        assert_eq!(request.target.service(), ":1.42");
        assert_eq!(request.target.path(), "/Ark/DndExtract_1");
        assert_eq!(request.target.interface(), ARK_DND_EXTRACT_INTERFACE);
        assert_eq!(request.target.method(), ARK_DND_EXTRACT_METHOD);
        assert_eq!(request.destination, "/tmp/fika-dnd-target");
    }

    #[test]
    fn ark_dnd_extract_request_rejects_relative_or_invalid_destination() {
        let payload = ArkDndExtractPayload {
            remote_service: "org.kde.ark".to_string(),
            remote_path: "/Ark/DndExtract".to_string(),
        };

        let relative = ark_dnd_extract_request(&payload, Path::new("relative")).unwrap_err();
        assert!(matches!(
            relative,
            ArkDndExtractError::InvalidDestination { .. }
        ));

        let with_nul =
            ark_dnd_extract_request(&payload, Path::new("/tmp/fika\0target")).unwrap_err();
        assert!(matches!(
            with_nul,
            ArkDndExtractError::InvalidDestination { .. }
        ));
    }

    #[test]
    fn ark_dnd_extract_executor_validates_destination_before_bus_connection() {
        let controller = BusController::new(BusConfig {
            retry_attempts: 1,
            retry_backoff: Duration::ZERO,
            call_timeout: Duration::from_millis(1),
            idle_timeout: Duration::from_secs(30),
        });
        let payload = ArkDndExtractPayload {
            remote_service: "org.kde.ark".to_string(),
            remote_path: "/Ark/DndExtract".to_string(),
        };

        let error = futures_lite::future::block_on(execute_ark_dnd_extract_with_bus(
            &controller,
            &payload,
            Path::new("relative"),
        ))
        .unwrap_err();

        assert!(matches!(
            error,
            ArkDndExtractError::InvalidDestination { .. }
        ));
    }
}
