use super::{
    entries::ItemId,
    pane::{Generation, PaneId},
    pe_icon::windows_executable_icon_ico,
    uri::file_uri_from_path,
};
use std::collections::{HashMap, VecDeque};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{self, Command, ExitStatus, Stdio};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

pub mod scheduler;

pub use scheduler::{
    ThumbnailCandidate, ThumbnailProbeBatch, ThumbnailProbeCancelHandle, ThumbnailProbeResult,
    ThumbnailScheduler, ThumbnailWorkKey, apply_thumbnail_probe_result_to_model,
    thumbnail_candidate_failure_is_cached, thumbnail_probe_results_for_requests,
};

const THUMBNAILS_DIR: &str = "thumbnails";
const NORMAL_DIR: &str = "normal";
const LARGE_DIR: &str = "large";
const X_LARGE_DIR: &str = "x-large";
const XX_LARGE_DIR: &str = "xx-large";
const FAIL_DIR: &str = "fail";
const FAIL_APP_DIR: &str = "gnome-thumbnail-factory";
const PNG_EXTENSION: &str = "png";
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const PNG_CHUNK_HEADER_LEN: usize = 8;
const PNG_CHUNK_CRC_LEN: usize = 4;

const FAILURE_THUMBNAIL_IDAT: &[u8] = &[0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01];

/// Freedesktop freestanding thumbnail sizes (shared with FileManager / GNOME).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ThumbnailSize {
    /// 128px — `~/.cache/thumbnails/normal`
    Normal,
    /// 256px — `~/.cache/thumbnails/large`
    Large,
    /// 512px — `~/.cache/thumbnails/x-large`
    XLarge,
    /// 1024px — `~/.cache/thumbnails/xx-large`
    XXLarge,
}

impl ThumbnailSize {
    pub fn cache_dir(self) -> &'static str {
        match self {
            Self::Normal => NORMAL_DIR,
            Self::Large => LARGE_DIR,
            Self::XLarge => X_LARGE_DIR,
            Self::XXLarge => XX_LARGE_DIR,
        }
    }

    pub fn max_dimension(self) -> u16 {
        match self {
            Self::Normal => 128,
            Self::Large => 256,
            Self::XLarge => 512,
            Self::XXLarge => 1024,
        }
    }

    /// All freestanding sizes from largest to smallest (shared cache lookup order).
    pub fn all_descending() -> [Self; 4] {
        [Self::XXLarge, Self::XLarge, Self::Large, Self::Normal]
    }

    /// Pick freestanding cache size from on-screen icon / decode pixels.
    ///
    /// Biased toward sharper freestanding buckets than a 1:1 match so default
    /// Icons mode (48px) uses `large/` (256) instead of soft `normal/` (128),
    /// and high zoom can use `x-large` / `xx-large`.
    pub fn for_display_px(size_px: u16) -> Self {
        match size_px {
            0..=32 => Self::Normal,
            33..=96 => Self::Large,
            97..=192 => Self::XLarge,
            _ => Self::XXLarge,
        }
    }

    /// Pick the smallest freestanding bucket that can supply an encoded source.
    ///
    /// Unlike [`Self::for_display_px`], this must not apply the sharpness bias a
    /// second time. The render request may already have been raised from 48px
    /// to 256px; mapping that 256px again would unnecessarily generate a 1024px
    /// `xx-large` thumbnail only to immediately downscale it on the CPU.
    pub fn for_source_px(size_px: u16) -> Self {
        match size_px {
            0..=128 => Self::Normal,
            129..=256 => Self::Large,
            257..=512 => Self::XLarge,
            _ => Self::XXLarge,
        }
    }

    /// Lookup order: requested, nearest sharper buckets, then softer buckets.
    /// A fixed array keeps cache probes allocation-free.
    fn lookup_order(self) -> [Self; 4] {
        match self {
            Self::Normal => [Self::Normal, Self::Large, Self::XLarge, Self::XXLarge],
            Self::Large => [Self::Large, Self::XLarge, Self::XXLarge, Self::Normal],
            Self::XLarge => [Self::XLarge, Self::XXLarge, Self::Large, Self::Normal],
            Self::XXLarge => [Self::XXLarge, Self::XLarge, Self::Large, Self::Normal],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThumbnailCacheHit {
    size: ThumbnailSize,
    path: PathBuf,
}

impl ThumbnailCacheHit {
    pub fn size(&self) -> ThumbnailSize {
        self.size
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThumbnailCachePaths {
    pub normal: PathBuf,
    pub large: PathBuf,
    pub x_large: PathBuf,
    pub xx_large: PathBuf,
    pub failure: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalThumbnailerCommand {
    program: String,
    args: Vec<OsString>,
}

impl ExternalThumbnailerCommand {
    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn args(&self) -> &[OsString] {
        &self.args
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ThumbnailMetadata {
    pub uri: Option<String>,
    pub mtime: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ThumbnailRequestPriority {
    Visible,
    Deferred,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThumbnailRequest {
    pane_id: PaneId,
    generation: Generation,
    item_id: ItemId,
    path: PathBuf,
    uri: String,
    modified_secs: u64,
    mime_type: Option<String>,
    priority: ThumbnailRequestPriority,
}

impl ThumbnailRequest {
    pub fn new(
        pane_id: PaneId,
        generation: Generation,
        item_id: ItemId,
        path: PathBuf,
        priority: ThumbnailRequestPriority,
    ) -> Option<Self> {
        let modified_secs = file_modified_secs(&path)?;
        Self::from_entry_metadata(pane_id, generation, item_id, path, modified_secs, priority)
    }

    pub fn from_entry_metadata(
        pane_id: PaneId,
        generation: Generation,
        item_id: ItemId,
        path: PathBuf,
        modified_secs: u64,
        priority: ThumbnailRequestPriority,
    ) -> Option<Self> {
        Self::from_entry_metadata_with_mime(
            pane_id,
            generation,
            item_id,
            path,
            modified_secs,
            None,
            priority,
        )
    }

    pub fn from_entry_metadata_with_mime(
        pane_id: PaneId,
        generation: Generation,
        item_id: ItemId,
        path: PathBuf,
        modified_secs: u64,
        mime_type: Option<String>,
        priority: ThumbnailRequestPriority,
    ) -> Option<Self> {
        let uri = thumbnail_uri_for_path(&path)?;
        Some(Self {
            pane_id,
            generation,
            item_id,
            path,
            uri,
            modified_secs,
            mime_type: mime_type
                .map(|mime| mime.trim().to_string())
                .filter(|mime| !mime.is_empty()),
            priority,
        })
    }

    pub fn pane_id(&self) -> PaneId {
        self.pane_id
    }

    pub fn generation(&self) -> Generation {
        self.generation
    }

    pub fn item_id(&self) -> ItemId {
        self.item_id
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn uri(&self) -> &str {
        &self.uri
    }

    pub fn modified_secs(&self) -> u64 {
        self.modified_secs
    }

    pub fn mime_type(&self) -> Option<&str> {
        self.mime_type.as_deref()
    }

    pub fn priority(&self) -> ThumbnailRequestPriority {
        self.priority
    }

    fn key(&self) -> ThumbnailRequestKey {
        ThumbnailRequestKey {
            pane_id: self.pane_id,
            generation: self.generation,
            item_id: self.item_id,
            uri: self.uri.clone(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ThumbnailRequestQueue {
    visible: VecDeque<ThumbnailRequest>,
    deferred: VecDeque<ThumbnailRequest>,
    pending: HashMap<ThumbnailRequestKey, ThumbnailRequestPriority>,
}

mod queue;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ThumbnailerRegistry {
    thumbnailers: Vec<ThumbnailerDefinition>,
}

impl ThumbnailerRegistry {
    pub fn shared_system() -> &'static Self {
        static REGISTRY: OnceLock<ThumbnailerRegistry> = OnceLock::new();
        REGISTRY.get_or_init(Self::load_system)
    }

    pub fn load_system() -> Self {
        Self::load_from_dirs(thumbnailer_search_dirs())
    }

    pub fn load_from_dirs(dirs: impl IntoIterator<Item = PathBuf>) -> Self {
        let mut thumbnailers = Vec::new();
        for dir in dirs {
            let Ok(entries) = fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(OsStr::to_str) != Some("thumbnailer") {
                    continue;
                }
                let Ok(contents) = fs::read_to_string(&path) else {
                    continue;
                };
                if let Some(thumbnailer) = parse_thumbnailer_definition(&contents)
                    && thumbnailer.try_exec_is_available()
                {
                    thumbnailers.push(thumbnailer);
                }
            }
        }
        Self { thumbnailers }
    }

    pub fn commands_for_request(
        &self,
        request: &ThumbnailRequest,
        output: &Path,
        size: ThumbnailSize,
    ) -> Vec<ExternalThumbnailerCommand> {
        let mut commands = Vec::new();
        if let Some(mime_type) = request.mime_type() {
            commands.extend(
                self.thumbnailers
                    .iter()
                    .filter(|thumbnailer| thumbnailer.matches_mime(mime_type))
                    .filter_map(|thumbnailer| {
                        thumbnailer.command_for(request.path(), request.uri(), output, size)
                    }),
            );
        }
        if commands.is_empty() {
            commands.extend(external_thumbnailer_commands_for_path(
                request.path(),
                output,
                size,
            ));
        }
        commands
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ThumbnailerDefinition {
    exec: String,
    try_exec: Option<String>,
    mime_types: Vec<String>,
}

impl ThumbnailerDefinition {
    fn matches_mime(&self, mime_type: &str) -> bool {
        self.mime_types
            .iter()
            .any(|mime| thumbnailer_mime_matches(mime, mime_type))
    }

    fn command_for(
        &self,
        input: &Path,
        uri: &str,
        output: &Path,
        size: ThumbnailSize,
    ) -> Option<ExternalThumbnailerCommand> {
        expand_thumbnailer_exec(&self.exec, input, uri, output, size)
    }

    fn try_exec_is_available(&self) -> bool {
        self.try_exec.as_deref().is_none_or(program_exists_in_path)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ThumbnailRequestKey {
    pane_id: PaneId,
    generation: Generation,
    item_id: ItemId,
    uri: String,
}

pub fn default_thumbnail_cache_root() -> PathBuf {
    default_cache_home().join(THUMBNAILS_DIR)
}

pub fn thumbnail_cache_root(cache_home: &Path) -> PathBuf {
    cache_home.join(THUMBNAILS_DIR)
}

pub fn thumbnail_uri_for_path(path: &Path) -> Option<String> {
    path.is_absolute().then(|| file_uri_from_path(path))
}

pub fn thumbnail_cache_key(uri: &str) -> String {
    md5_hex(uri.as_bytes())
}

pub fn thumbnail_cache_path(root: &Path, size: ThumbnailSize, uri: &str) -> PathBuf {
    root.join(size.cache_dir())
        .join(format!("{}.{}", thumbnail_cache_key(uri), PNG_EXTENSION))
}

pub fn thumbnail_failure_path(root: &Path, uri: &str) -> PathBuf {
    root.join(FAIL_DIR).join(FAIL_APP_DIR).join(format!(
        "{}.{}",
        thumbnail_cache_key(uri),
        PNG_EXTENSION
    ))
}

pub fn thumbnail_cache_paths_for_uri(root: &Path, uri: &str) -> ThumbnailCachePaths {
    ThumbnailCachePaths {
        normal: thumbnail_cache_path(root, ThumbnailSize::Normal, uri),
        large: thumbnail_cache_path(root, ThumbnailSize::Large, uri),
        x_large: thumbnail_cache_path(root, ThumbnailSize::XLarge, uri),
        xx_large: thumbnail_cache_path(root, ThumbnailSize::XXLarge, uri),
        failure: thumbnail_failure_path(root, uri),
    }
}

pub fn cached_thumbnail_for_uri(root: &Path, uri: &str) -> Option<ThumbnailCacheHit> {
    cached_thumbnail(root, uri, None)
}

pub fn cached_thumbnail_for_path(root: &Path, path: &Path) -> Option<ThumbnailCacheHit> {
    let uri = thumbnail_uri_for_path(path)?;
    let modified_secs = file_modified_secs(path)?;
    cached_thumbnail(root, &uri, Some(modified_secs))
}

pub fn cached_thumbnail_for_request(
    root: &Path,
    request: &ThumbnailRequest,
) -> Option<ThumbnailCacheHit> {
    cached_thumbnail(root, request.uri(), Some(request.modified_secs()))
}

pub fn thumbnail_metadata(path: &Path) -> io::Result<ThumbnailMetadata> {
    thumbnail_metadata_from_bytes(&fs::read(path)?)
}

fn cached_thumbnail(
    root: &Path,
    uri: &str,
    modified_secs: Option<u64>,
) -> Option<ThumbnailCacheHit> {
    cached_thumbnail_preferring(root, uri, modified_secs, ThumbnailSize::Normal)
}

fn cached_thumbnail_preferring(
    root: &Path,
    uri: &str,
    modified_secs: Option<u64>,
    preferred: ThumbnailSize,
) -> Option<ThumbnailCacheHit> {
    preferred.lookup_order().into_iter().find_map(|size| {
        let path = thumbnail_cache_path(root, size, uri);
        thumbnail_metadata_matches(&path, uri, modified_secs)
            .then_some(ThumbnailCacheHit { size, path })
    })
}

pub fn cached_thumbnail_for_request_size(
    root: &Path,
    request: &ThumbnailRequest,
    preferred: ThumbnailSize,
) -> Option<ThumbnailCacheHit> {
    cached_thumbnail_preferring(
        root,
        request.uri(),
        Some(request.modified_secs()),
        preferred,
    )
}

fn thumbnail_metadata_matches(path: &Path, uri: &str, modified_secs: Option<u64>) -> bool {
    if !path.is_file() {
        return false;
    }
    let Ok(metadata) = thumbnail_metadata(path) else {
        return false;
    };
    if metadata.uri.as_deref() != Some(uri) {
        return false;
    }
    modified_secs.is_none_or(|expected| metadata.mtime == Some(expected))
}

fn file_modified_secs(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| {
            modified
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|duration| duration.as_secs())
        })
}

fn remove_matching_from_queue(
    queue: &mut VecDeque<ThumbnailRequest>,
    predicate: &impl Fn(&ThumbnailRequest) -> bool,
) -> Vec<ThumbnailRequestKey> {
    let mut removed = Vec::new();
    queue.retain(|request| {
        if predicate(request) {
            removed.push(request.key());
            false
        } else {
            true
        }
    });
    removed
}

pub fn thumbnail_failure_is_cached(root: &Path, uri: &str, modified_secs: u64) -> bool {
    thumbnail_metadata_matches(&thumbnail_failure_path(root, uri), uri, Some(modified_secs))
}

pub fn record_thumbnail_failure(root: &Path, uri: &str, modified_secs: u64) -> io::Result<PathBuf> {
    let path = thumbnail_failure_path(root, uri);
    if !thumbnail_failure_is_cached(root, uri, modified_secs) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, failure_thumbnail_png(uri, modified_secs))?;
    }
    Ok(path)
}

pub fn generate_thumbnail_with_external_thumbnailer(
    root: &Path,
    request: &ThumbnailRequest,
) -> io::Result<Option<ThumbnailCacheHit>> {
    generate_thumbnail_with_external_thumbnailer_registry(
        root,
        request,
        ThumbnailerRegistry::shared_system(),
    )
}

pub fn generate_thumbnail_with_external_thumbnailer_registry(
    root: &Path,
    request: &ThumbnailRequest,
    registry: &ThumbnailerRegistry,
) -> io::Result<Option<ThumbnailCacheHit>> {
    generate_thumbnail_with_external_thumbnailer_registry_size(
        root,
        request,
        registry,
        ThumbnailSize::Normal,
    )
}

/// Like [`generate_thumbnail_with_external_thumbnailer_registry`], but write/read
/// the freestanding size that matches on-screen icon pixels (`Large` = 256).
pub fn generate_thumbnail_with_external_thumbnailer_registry_size(
    root: &Path,
    request: &ThumbnailRequest,
    registry: &ThumbnailerRegistry,
    size: ThumbnailSize,
) -> io::Result<Option<ThumbnailCacheHit>> {
    if let Some(hit) = cached_thumbnail_for_request_size(root, request, size) {
        return Ok(Some(hit));
    }
    if thumbnail_failure_is_cached(root, request.uri(), request.modified_secs()) {
        return Ok(None);
    }

    let output_path = thumbnail_cache_path(root, size, request.uri());
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp_path = temporary_thumbnail_path(&output_path);
    let mut attempted = false;
    if thumbnail_request_is_windows_executable(request) {
        attempted = true;
        let encoded_path = windows_executable_icon_cache_path(root, request, size);
        if encoded_path.is_file() {
            return Ok(Some(ThumbnailCacheHit {
                size,
                path: encoded_path,
            }));
        }
        if let Some(ico) = windows_executable_icon_ico(request.path(), size.max_dimension())? {
            if let Some(parent) = encoded_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let encoded_temp = temporary_thumbnail_path(&encoded_path);
            let _ = fs::remove_file(&encoded_temp);
            fs::write(&encoded_temp, ico)?;
            if fs::rename(&encoded_temp, &encoded_path).is_ok() {
                return Ok(Some(ThumbnailCacheHit {
                    size,
                    path: encoded_path,
                }));
            }
        }
    }
    let commands = registry.commands_for_request(request, &temp_path, size);
    for command in commands {
        let _ = fs::remove_file(&temp_path);
        match run_external_thumbnailer_command(&command) {
            Ok(status) => {
                attempted = true;
                if !status.success() || !temp_path.is_file() {
                    continue;
                }
                if write_thumbnail_metadata(&temp_path, request.uri(), request.modified_secs())
                    .is_ok()
                    && fs::rename(&temp_path, &output_path).is_ok()
                    && let Some(hit) = cached_thumbnail_for_request_size(root, request, size)
                {
                    return Ok(Some(hit));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => {
                attempted = true;
            }
        }
    }
    let _ = fs::remove_file(&temp_path);

    if attempted {
        record_thumbnail_failure(root, request.uri(), request.modified_secs())?;
    }
    Ok(None)
}

fn windows_executable_icon_cache_path(
    root: &Path,
    request: &ThumbnailRequest,
    size: ThumbnailSize,
) -> PathBuf {
    root.join("tensor-files-pe-icons")
        .join(size.cache_dir())
        .join(format!(
            "{}-{}.ico",
            thumbnail_cache_key(request.uri()),
            request.modified_secs()
        ))
}

fn run_external_thumbnailer_command(
    command: &ExternalThumbnailerCommand,
) -> io::Result<ExitStatus> {
    Command::new(command.program())
        .args(command.args())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
}

pub fn external_thumbnailer_commands_for_path(
    input: &Path,
    output: &Path,
    size: ThumbnailSize,
) -> Vec<ExternalThumbnailerCommand> {
    let Some(extension) = input.extension().and_then(OsStr::to_str) else {
        return Vec::new();
    };
    let extension = extension.to_ascii_lowercase();
    let input = input.as_os_str().to_os_string();
    let output = output.as_os_str().to_os_string();
    let size_arg = size.max_dimension().to_string();

    if image_thumbnail_extension(&extension) {
        return vec![ExternalThumbnailerCommand {
            program: String::from("gdk-pixbuf-thumbnailer"),
            args: vec![
                OsString::from("-s"),
                OsString::from(size_arg),
                input,
                output,
            ],
        }];
    }

    if video_thumbnail_extension(&extension) {
        return vec![
            ExternalThumbnailerCommand {
                program: String::from("ffmpegthumbnailer"),
                args: vec![
                    OsString::from("-i"),
                    input.clone(),
                    OsString::from("-o"),
                    output.clone(),
                    OsString::from("-s"),
                    OsString::from(size_arg.clone()),
                ],
            },
            ExternalThumbnailerCommand {
                program: String::from("totem-video-thumbnailer"),
                args: vec![
                    OsString::from("-s"),
                    OsString::from(size_arg),
                    input,
                    output,
                ],
            },
        ];
    }

    if document_thumbnail_extension(&extension) {
        return vec![ExternalThumbnailerCommand {
            program: String::from("evince-thumbnailer"),
            args: vec![
                OsString::from("-s"),
                OsString::from(size_arg),
                input,
                output,
            ],
        }];
    }

    Vec::new()
}

pub fn thumbnail_request_may_have_preview(path: &Path, mime_type: Option<&str>) -> bool {
    mime_type.is_some_and(thumbnail_mime_may_have_preview)
        || thumbnail_extension_may_have_preview(path)
}

fn thumbnail_request_is_windows_executable(request: &ThumbnailRequest) -> bool {
    request
        .mime_type()
        .is_some_and(windows_executable_mime_may_have_preview)
        || path_has_windows_executable_thumbnail_extension(request.path())
}

fn thumbnail_mime_may_have_preview(mime_type: &str) -> bool {
    let mime_type = mime_type.trim().to_ascii_lowercase();
    if mime_type.starts_with("text/") {
        return false;
    }
    if mime_type.starts_with("image/") {
        return true;
    }
    if matches!(
        mime_type.as_str(),
        "application/pdf"
            | "application/postscript"
            | "application/eps"
            | "application/epub+zip"
            | "application/x-mobipocket-ebook"
    ) {
        return true;
    }
    windows_executable_mime_may_have_preview(&mime_type)
}

fn windows_executable_mime_may_have_preview(mime_type: &str) -> bool {
    matches!(
        mime_type.trim().to_ascii_lowercase().as_str(),
        "application/vnd.microsoft.portable-executable"
            | "application/x-msdownload"
            | "application/x-ms-dos-executable"
    )
}

fn thumbnail_extension_may_have_preview(path: &Path) -> bool {
    if path_has_windows_executable_thumbnail_extension(path) {
        return true;
    }
    path.extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase)
        .is_some_and(|extension| {
            image_thumbnail_extension(&extension) || document_thumbnail_extension(&extension)
        })
}

fn path_has_windows_executable_thumbnail_extension(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase)
        .is_some_and(|extension| matches!(extension.as_str(), "exe" | "scr" | "cpl" | "dll"))
}

include!("thumbnails/external_registry.rs");
include!("thumbnails/png_metadata.rs");

#[cfg(test)]
#[path = "thumbnails/tests.rs"]
mod tests;
