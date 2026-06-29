use super::*;

pub(super) fn collect_feature_hints_from_entry(
    source_type: SourceType,
    root: &Path,
    entry_file: &str,
    features: &mut BTreeSet<String>,
) {
    collect_feature_hints_from_string(source_type, entry_file, features);
    if !should_scan_entry_contents(source_type, entry_file) {
        return;
    }
    let Ok(relative) = normalize_relative_path(entry_file) else {
        return;
    };
    let entry_path = root.join(relative);
    let Some(contents) = read_feature_scan_contents(&entry_path) else {
        return;
    };
    if entry_file
        .rsplit_once('.')
        .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("json"))
    {
        if let Ok(value) = serde_json::from_str::<Value>(&contents) {
            collect_feature_hints_from_value(source_type, &value, features);
            return;
        }
    }
    collect_feature_hints_from_string(source_type, &contents, features);
}

fn should_scan_entry_contents(source_type: SourceType, entry_file: &str) -> bool {
    let Some(extension) = Path::new(entry_file)
        .extension()
        .and_then(|extension| extension.to_str())
    else {
        return matches!(
            source_type,
            SourceType::Web | SourceType::Scene | SourceType::Shader
        );
    };

    matches!(
        extension.to_ascii_lowercase().as_str(),
        "css"
            | "cjs"
            | "frag"
            | "fragment"
            | "fs"
            | "glsl"
            | "htm"
            | "html"
            | "js"
            | "json"
            | "mjs"
            | "shader"
            | "txt"
            | "vert"
            | "vertex"
            | "vs"
            | "wgsl"
    )
}

fn read_feature_scan_contents(path: &Path) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }

    let scan_len = metadata.len().min(FEATURE_SCAN_MAX_BYTES);
    let mut bytes = Vec::with_capacity(scan_len as usize);
    let mut file = fs::File::open(path).ok()?.take(FEATURE_SCAN_MAX_BYTES);
    file.read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

pub(super) fn collect_feature_hints_from_value(
    source_type: SourceType,
    value: &Value,
    features: &mut BTreeSet<String>,
) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                collect_feature_hints_from_key(source_type, key, features);
                collect_feature_hints_from_value(source_type, value, features);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_feature_hints_from_value(source_type, value, features);
            }
        }
        Value::String(value) => collect_feature_hints_from_string(source_type, value, features),
        Value::Bool(_) | Value::Number(_) | Value::Null => {}
    }
}

fn collect_feature_hints_from_key(
    source_type: SourceType,
    key: &str,
    features: &mut BTreeSet<String>,
) {
    let normalized = normalize_project_key(key);
    if source_type == SourceType::Scene {
        if normalized.contains("script") {
            features.insert("scenescript".to_owned());
        }
        if normalized == "objects" || normalized == "layers" || normalized.ends_with("layers") {
            features.insert("scene-layers".to_owned());
        }
    }
    if normalized.contains("shader")
        || normalized.contains("fragment")
        || normalized.contains("vertex")
        || normalized.contains("material")
    {
        features.insert("shader".to_owned());
    }
    if normalized.contains("particle") || normalized.contains("emitter") {
        features.insert("particles".to_owned());
    }
    if normalized.contains("timeline")
        || normalized.contains("animation")
        || normalized.contains("keyframe")
    {
        features.insert("timeline".to_owned());
    }
    if normalized.contains("parallax") || normalized.contains("mouseparallax") {
        features.insert("parallax".to_owned());
    }
    if normalized.contains("playlist") || normalized.contains("collection") {
        features.insert("playlist".to_owned());
    }
    if normalized.contains("media") || normalized.contains("nowplaying") {
        features.insert("media-integration".to_owned());
    }
    if normalized.contains("audio")
        || normalized.contains("sound")
        || normalized.contains("music")
        || normalized == "volume"
    {
        features.insert("audio".to_owned());
        if source_type == SourceType::Scene
            && (normalized.contains("audioresponse")
                || normalized.contains("audio_response")
                || normalized.contains("spectrum")
                || normalized.contains("fft"))
        {
            features.insert("audio-response".to_owned());
        }
    }
}

fn collect_feature_hints_from_string(
    source_type: SourceType,
    value: &str,
    features: &mut BTreeSet<String>,
) {
    let lowered = value.to_ascii_lowercase();
    if lowered.contains("wallpaperpropertylistener") {
        features.insert("web-property-listener".to_owned());
    }
    if lowered.contains("wallpaperregisteraudiolistener")
        || lowered.contains("wallpaperaudiolistener")
    {
        features.insert("web-audio-listener".to_owned());
        features.insert("audio-response".to_owned());
    }
    if source_type == SourceType::Scene
        && (lowered.contains("audio-response")
            || lowered.contains("audio_response")
            || lowered.contains("audioresponse")
            || lowered.contains("audiospectrum")
            || lowered.contains("audio_spectrum")
            || lowered.contains("fft"))
    {
        features.insert("audio-response".to_owned());
    }
    if lowered.contains("scenescript")
        || (source_type == SourceType::Scene && lowered.contains("script"))
    {
        features.insert("scenescript".to_owned());
    }
    if lowered.contains("http://") || lowered.contains("https://") {
        features.insert("network".to_owned());
    }
    if lowered.contains("parallax") {
        features.insert("parallax".to_owned());
    }
    if lowered.contains("timeline") || lowered.contains("keyframe") {
        features.insert("timeline".to_owned());
    }
    if lowered.contains("particle") || lowered.contains("emitter") {
        features.insert("particles".to_owned());
    }
    if lowered.contains("shader") || has_shader_extension(value) {
        features.insert("shader".to_owned());
    }
    if Path::new(value)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(is_audio_extension)
    {
        features.insert("audio".to_owned());
    }
    if source_type == SourceType::Scene && is_image_path(value) {
        features.insert("image-layer".to_owned());
    }
}

pub(super) fn has_shader_extension(value: &str) -> bool {
    Path::new(value)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "frag" | "fragment" | "fs" | "glsl" | "shader" | "vert" | "vertex" | "vs" | "wgsl"
            )
        })
}

pub(super) fn is_image_path(value: &str) -> bool {
    Path::new(value)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "avif" | "bmp" | "gif" | "jpeg" | "jpg" | "png" | "svg" | "webp"
            )
        })
}

pub(super) fn is_raster_image_path(value: &str) -> bool {
    Path::new(value)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "avif" | "bmp" | "jpeg" | "jpg" | "png" | "webp"
            )
        })
}

pub(super) fn explicit_audio_request(value: &Value) -> bool {
    match value {
        Value::Object(object) => object
            .iter()
            .any(|(key, value)| key_requests_audio(key, value) || explicit_audio_request(value)),
        Value::Array(values) => values.iter().any(explicit_audio_request),
        _ => false,
    }
}

fn key_requests_audio(key: &str, value: &Value) -> bool {
    let normalized = normalize_project_key(key);
    let audio_key = normalized.contains("audio")
        || normalized.contains("sound")
        || normalized.contains("music")
        || normalized == "volume";
    if !audio_key {
        return false;
    }

    match value {
        Value::Bool(enabled) => *enabled,
        Value::Number(number) => number.as_f64().is_some_and(|value| value > 0.0),
        Value::String(value) => audio_field_string_requests_audio(value),
        Value::Array(values) => values.iter().any(audio_field_value_requests_audio),
        Value::Object(object) => object.values().any(audio_field_value_requests_audio),
        Value::Null => false,
    }
}

fn audio_field_value_requests_audio(value: &Value) -> bool {
    match value {
        Value::Bool(enabled) => *enabled,
        Value::Number(number) => number.as_f64().is_some_and(|value| value > 0.0),
        Value::String(value) => audio_field_string_requests_audio(value),
        Value::Array(values) => values.iter().any(audio_field_value_requests_audio),
        Value::Object(object) => object.iter().any(|(key, value)| {
            key_requests_audio(key, value) || audio_field_value_requests_audio(value)
        }),
        Value::Null => false,
    }
}

pub(super) fn static_image_audio_sources(project: &WallpaperEngineProject) -> Vec<String> {
    let mut sources = BTreeSet::new();
    collect_static_image_audio_sources_from_value(&project.raw, false, &mut sources);
    sources
        .into_iter()
        .filter(|source| {
            normalize_relative_path(source)
                .map(|relative| project.root.join(relative).is_file())
                .unwrap_or(false)
        })
        .collect()
}

fn collect_static_image_audio_sources_from_value(
    value: &Value,
    in_audio_field: bool,
    sources: &mut BTreeSet<String>,
) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                let normalized = normalize_project_key(key);
                let audio_field = normalized.contains("audio")
                    || normalized.contains("sound")
                    || normalized.contains("music");
                collect_static_image_audio_sources_from_value(
                    value,
                    in_audio_field || audio_field,
                    sources,
                );
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_static_image_audio_sources_from_value(value, in_audio_field, sources);
            }
        }
        Value::String(source) if in_audio_field && is_audio_field_media_path(source) => {
            sources.insert(source.clone());
        }
        _ => {}
    }
}

pub(super) fn is_audio_path(value: &str) -> bool {
    Path::new(value.trim())
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(is_audio_extension)
}

fn is_audio_field_media_path(value: &str) -> bool {
    is_audio_path(value) || is_video_path(value)
}

fn audio_field_string_requests_audio(value: &str) -> bool {
    string_requests_audio_with_path_match(value, is_audio_field_media_path)
}

fn string_requests_audio_with_path_match(value: &str, path_match: impl Fn(&str) -> bool) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lowered = trimmed.to_ascii_lowercase();
    if matches!(
        lowered.as_str(),
        "false" | "0" | "off" | "none" | "disabled" | "disable" | "muted" | "mute"
    ) {
        return false;
    }
    if Path::new(trimmed).extension().is_some() {
        path_match(trimmed)
    } else {
        true
    }
}

pub(super) fn is_audio_extension(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "aac" | "flac" | "m4a" | "mp3" | "oga" | "ogg" | "opus" | "wav" | "weba" | "wma"
    )
}

pub(super) fn normalize_project_key(key: &str) -> String {
    key.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub(super) fn detect_source_type(
    object: &Map<String, Value>,
    entry_file: Option<&str>,
) -> SourceType {
    if let Some(kind) = string_field(object, &["type", "wallpaperType", "contentType"]) {
        let kind = kind.to_ascii_lowercase();
        if kind.contains("application") || kind.contains("exe") {
            return SourceType::Application;
        }
        if kind.contains("playlist") || kind.contains("collection") {
            return SourceType::Playlist;
        }
        if kind.contains("video") {
            return SourceType::Video;
        }
        if kind.contains("web") {
            return SourceType::Web;
        }
        if kind.contains("shader") {
            return SourceType::Shader;
        }
        if kind.contains("scene") {
            return SourceType::Scene;
        }
        if kind.contains("image") {
            return SourceType::Image;
        }
    }

    if playlist_items_from_project(object).is_some() {
        return SourceType::Playlist;
    }

    entry_file
        .and_then(|entry| {
            Path::new(entry)
                .extension()
                .and_then(|extension| extension.to_str())
        })
        .map(SourceType::from_extension)
        .unwrap_or(SourceType::Unknown)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceType {
    Image,
    Video,
    Web,
    Scene,
    Shader,
    Playlist,
    Application,
    Unknown,
}

impl SourceType {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
            Self::Web => "web",
            Self::Scene => "scene",
            Self::Shader => "shader",
            Self::Playlist => "playlist",
            Self::Application => "application",
            Self::Unknown => "unknown",
        }
    }

    pub(super) fn from_extension(extension: &str) -> Self {
        match extension.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" | "png" | "webp" | "avif" | "bmp" | "gif" | "svg" => Self::Image,
            "mp4" | "m4v" | "webm" | "mkv" | "mov" | "avi" => Self::Video,
            "html" | "htm" => Self::Web,
            "frag" | "fragment" | "fs" | "glsl" | "shader" | "vert" | "vertex" | "vs" | "wgsl" => {
                Self::Shader
            }
            "json" => Self::Scene,
            "exe" | "bat" | "cmd" | "com" | "scr" => Self::Application,
            _ => Self::Unknown,
        }
    }
}
