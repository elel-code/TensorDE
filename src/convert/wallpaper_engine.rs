use crate::core::{FORMAT_NAME, FORMAT_VERSION, MANIFEST_FILE, load_gwpdir};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

const PROJECT_FILE: &str = "project.json";
const FFMPEG_BINARY: &str = "ffmpeg";
const FFPROBE_BINARY: &str = "ffprobe";
const VIDEO_POSTER_WIDTH: u32 = 1920;
const VIDEO_THUMBNAIL_WIDTH: u32 = 512;
const FEATURE_SCAN_MAX_BYTES: u64 = 2 * 1024 * 1024;
const STATIC_IMAGE_VARIANTS: &[StaticImageVariantSpec] = &[
    StaticImageVariantSpec {
        id: "landscape-1080p",
        width: 1920,
        height: 1080,
        file_name: "landscape-1080p.png",
    },
    StaticImageVariantSpec {
        id: "landscape-2160p",
        width: 3840,
        height: 2160,
        file_name: "landscape-2160p.png",
    },
];

#[derive(Debug, Clone, Copy)]
struct StaticImageVariantSpec {
    id: &'static str,
    width: u32,
    height: u32,
    file_name: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConversionSummary {
    pub source_type: String,
    pub title: String,
    pub output_dir: PathBuf,
    pub manifest_file: PathBuf,
    pub report_file: PathBuf,
}

pub fn convert_project(
    source_dir: impl AsRef<Path>,
    output_dir: impl AsRef<Path>,
) -> Result<ConversionSummary, ConversionError> {
    let source_dir = source_dir.as_ref();
    let output_dir = output_dir.as_ref();
    let project = WallpaperEngineProject::load(source_dir)?;

    prepare_output_dir(source_dir, output_dir)?;
    fs::create_dir_all(output_dir.join("assets")).map_err(ConversionError::CreateDir)?;
    fs::create_dir_all(output_dir.join("previews")).map_err(ConversionError::CreateDir)?;
    fs::create_dir_all(output_dir.join("metadata")).map_err(ConversionError::CreateDir)?;

    let mut report = ConversionReport::new(project.source_type.as_str());
    report.detected_features.extend(project.detected_features());

    let result = match project.source_type {
        SourceType::Image => convert_static_image(&project, output_dir, &mut report),
        SourceType::Video => convert_video(&project, output_dir, &mut report),
        SourceType::Web => convert_web(&project, output_dir, &mut report),
        SourceType::Scene => convert_scene_lite(&project, output_dir, &mut report),
        SourceType::Application => {
            report
                .unsupported_features
                .push("executable-application".to_owned());
            report
                .errors
                .push("Executable Wallpaper Engine projects cannot be converted.".to_owned());
            Err(ConversionError::UnsupportedType {
                source_type: project.source_type.as_str().to_owned(),
            })
        }
        SourceType::Unknown => {
            report.unsupported_features.push("unknown-type".to_owned());
            report
                .errors
                .push("Could not identify the Wallpaper Engine project type.".to_owned());
            Err(ConversionError::UnsupportedType {
                source_type: project.source_type.as_str().to_owned(),
            })
        }
    };

    write_metadata(&project, output_dir, &report)?;

    match result {
        Ok(manifest) => {
            write_json_pretty(output_dir.join(MANIFEST_FILE), &manifest)?;
            load_gwpdir(output_dir).map_err(|source| ConversionError::InvalidOutput {
                output_dir: output_dir.to_path_buf(),
                source: source.to_string(),
            })?;
            Ok(ConversionSummary {
                source_type: project.source_type.as_str().to_owned(),
                title: project.title.clone(),
                output_dir: output_dir.to_path_buf(),
                manifest_file: output_dir.join(MANIFEST_FILE),
                report_file: output_dir.join("metadata/conversion-report.json"),
            })
        }
        Err(err) => Err(err),
    }
}

fn convert_static_image(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    report: &mut ConversionReport,
) -> Result<Value, ConversionError> {
    convert_static_image_with_variant_tools(project, output_dir, report, None)
}

fn convert_static_image_with_variant_tools(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    report: &mut ConversionReport,
    variant_tools: Option<StaticImageVariantTools<'_>>,
) -> Result<Value, ConversionError> {
    let source = project.entry_file.as_ref().ok_or_else(|| {
        ConversionError::MissingEntry("image project does not define an entry file".to_owned())
    })?;
    let copied = copy_project_file(
        &project.root,
        source,
        output_dir.join("assets"),
        "wallpaper",
        report,
    )?;
    let preview = copy_preview_or_generate(
        project,
        output_dir,
        report,
        MissingPreviewFallback::StaticImage { source },
    )?;
    report.converted_features.push("static-image".to_owned());
    let variants = match variant_tools {
        Some(tools) => {
            generate_static_image_variants_with_tools(project, output_dir, source, report, tools)
        }
        None => generate_static_image_variants(project, output_dir, source, report),
    };

    let mut manifest = base_manifest(
        project,
        "static-image",
        preview,
        report,
        json!({
            "type": "static-image",
            "source": copied.package_path,
            "fit": "cover",
            "orientation": "from-metadata"
        }),
    );
    if !variants.is_empty()
        && let Some(object) = manifest.as_object_mut()
    {
        object.insert("variants".to_owned(), Value::Array(variants));
    }
    Ok(manifest)
}

fn convert_video(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    report: &mut ConversionReport,
) -> Result<Value, ConversionError> {
    let source = project.entry_file.as_ref().ok_or_else(|| {
        ConversionError::MissingEntry("video project does not define an entry file".to_owned())
    })?;
    let copied = copy_project_file(
        &project.root,
        source,
        output_dir.join("assets"),
        "loop",
        report,
    )?;
    let preview = copy_preview_or_generate(
        project,
        output_dir,
        report,
        MissingPreviewFallback::Video { source },
    )?;
    report.converted_features.push("video".to_owned());

    let poster = preview
        .as_ref()
        .and_then(|preview| preview.poster.clone())
        .map(Value::String)
        .unwrap_or(Value::Null);
    let muted = !project.audio_requested();

    Ok(base_manifest(
        project,
        "video",
        preview,
        report,
        json!({
            "type": "video",
            "source": copied.package_path,
            "poster": poster,
            "loop": true,
            "muted": muted,
            "fit": "cover",
            "max_fps": 60
        }),
    ))
}

fn convert_web(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    report: &mut ConversionReport,
) -> Result<Value, ConversionError> {
    let index = project.entry_file.as_ref().ok_or_else(|| {
        ConversionError::MissingEntry("web project does not define an HTML entry file".to_owned())
    })?;
    let index_path = normalize_relative_path(index)?;
    let web_root = output_dir.join("assets/web");
    fs::create_dir_all(&web_root).map_err(ConversionError::CreateDir)?;
    copy_dir_recursive(
        &project.root,
        &web_root,
        output_dir,
        &[PROJECT_FILE],
        report,
    )?;
    let bridge_path = web_root.join("gilder-bridge.js");
    fs::write(&bridge_path, WEB_BRIDGE).map_err(ConversionError::WriteFile)?;
    report
        .generated_assets
        .push("assets/web/gilder-bridge.js".to_owned());
    report.converted_features.push("web".to_owned());
    record_web_runtime_gaps(report);

    let preview =
        copy_preview_or_generate(project, output_dir, report, MissingPreviewFallback::None)?;
    let index_package_path = path_to_package_string(&index_path);
    Ok(base_manifest(
        project,
        "web",
        preview.clone(),
        report,
        json!({
            "type": "web",
            "root": "assets/web",
            "index": index_package_path,
            "fallback": preview.and_then(|preview| preview.poster).map(Value::String).unwrap_or(Value::Null),
            "max_fps": 30
        }),
    ))
}

fn convert_scene_lite(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    report: &mut ConversionReport,
) -> Result<Value, ConversionError> {
    let source = project.entry_file.as_ref().ok_or_else(|| {
        ConversionError::MissingEntry("scene project does not define an entry file".to_owned())
    })?;
    let copied = copy_project_file(
        &project.root,
        source,
        output_dir.join("assets"),
        "scene",
        report,
    )?;
    let preview = copy_preview_or_generate(
        project,
        output_dir,
        report,
        MissingPreviewFallback::Scene { source },
    )?;
    let fallback = preview
        .as_ref()
        .and_then(|preview| preview.poster.clone())
        .map(Value::String)
        .unwrap_or(Value::Null);

    report.converted_features.push("scene-lite".to_owned());
    record_scene_lite_runtime_gaps(report);
    report.warnings.push(
        "Converted Scene project to scene-lite metadata and fallback only; native scene layers, timelines, scripts, shaders, particles, and complex effects were not executed or translated.".to_owned(),
    );

    Ok(base_manifest(
        project,
        "scene-lite",
        preview,
        report,
        json!({
            "type": "scene-lite",
            "source": copied.package_path,
            "fallback": fallback,
            "max_fps": 60
        }),
    ))
}

fn base_manifest(
    project: &WallpaperEngineProject,
    kind: &str,
    preview: Option<PreviewPaths>,
    report: &mut ConversionReport,
    entry: Value,
) -> Value {
    let properties = convert_properties(project, report);
    let allow_audio = runtime_allow_audio(project, report);
    json!({
        "format": FORMAT_NAME,
        "format_version": FORMAT_VERSION,
        "id": project.gilder_id(),
        "version": "1.0.0",
        "title": project.title,
        "authors": project.authors,
        "license": "unknown",
        "kind": kind,
        "tags": ["converted", "wallpaper-engine"],
        "preview": {
            "thumbnail": preview.as_ref().and_then(|preview| preview.thumbnail.clone()),
            "poster": preview.as_ref().and_then(|preview| preview.poster.clone()),
        },
        "entry": entry,
        "properties": properties,
        "runtime": {
            "pause_when_fullscreen": true,
            "pause_when_unfocused": false,
            "allow_network": false,
            "allow_audio": allow_audio
        }
    })
}

fn runtime_allow_audio(project: &WallpaperEngineProject, report: &mut ConversionReport) -> bool {
    if !project.audio_requested() {
        return false;
    }

    push_unique(&mut report.detected_features, "audio");
    match project.source_type {
        SourceType::Video => {
            push_unique(&mut report.converted_features, "audio-policy");
            true
        }
        SourceType::Web | SourceType::Scene => {
            push_unique(&mut report.unsupported_features, "audio-runtime");
            report.warnings.push(
                "Detected Wallpaper Engine audio features, but audio runtime integration is not available for this converted wallpaper type.".to_owned(),
            );
            false
        }
        SourceType::Image | SourceType::Application | SourceType::Unknown => false,
    }
}

fn record_web_runtime_gaps(report: &mut ConversionReport) {
    push_unique(&mut report.unsupported_features, "web-runtime");
    if report.detected_features.iter().any(|feature| {
        matches!(
            feature.as_str(),
            "network" | "web-audio-listener" | "media-integration"
        )
    }) {
        push_unique(&mut report.unsupported_features, "web-permissions");
        report.warnings.push(
            "Detected Web wallpaper runtime features that need explicit sandbox, network, media, or audio permissions; the converted package keeps them disabled.".to_owned(),
        );
    }
}

fn record_scene_lite_runtime_gaps(report: &mut ConversionReport) {
    push_unique(&mut report.unsupported_features, "scene-runtime");
    for (detected, unsupported) in [
        ("scenescript", "scenescript"),
        ("shader", "custom-shader"),
        ("particles", "complex-particles"),
        ("timeline", "timeline-animation"),
        ("parallax", "parallax"),
        ("audio-response", "audio-runtime"),
    ] {
        if report
            .detected_features
            .iter()
            .any(|feature| feature == detected)
        {
            push_unique(&mut report.unsupported_features, unsupported);
        }
    }
}

fn push_unique(items: &mut Vec<String>, value: &str) {
    if !items.iter().any(|item| item == value) {
        items.push(value.to_owned());
    }
}

fn convert_properties(
    project: &WallpaperEngineProject,
    report: &mut ConversionReport,
) -> BTreeMap<String, Value> {
    let mut converted = BTreeMap::new();
    let Some(properties) = project
        .raw
        .pointer("/general/properties")
        .and_then(Value::as_object)
    else {
        return converted;
    };

    for (name, value) in properties {
        let Some(spec) = value.as_object() else {
            report.warnings.push(format!(
                "Skipped property {name:?}: expected object specification."
            ));
            continue;
        };
        let property_type = string_field(spec, &["type"])
            .unwrap_or_default()
            .to_ascii_lowercase();
        let converted_property = match property_type.as_str() {
            "bool" => Some(json!({
                "type": "bool",
                "default": spec.get("value").and_then(Value::as_bool),
            })),
            "slider" => Some(json!({
                "type": "range",
                "min": number_field(spec, &["min"]).unwrap_or(0.0),
                "max": number_field(spec, &["max"]).unwrap_or(100.0),
                "step": number_field(spec, &["step"]),
                "default": number_field(spec, &["value", "default"]),
            })),
            "combo" => {
                let choices = combo_choices(spec);
                if choices.is_empty() {
                    report.warnings.push(format!(
                        "Skipped combo property {name:?}: no choices found."
                    ));
                    None
                } else {
                    let default = string_field(spec, &["value", "default"])
                        .filter(|value| choices.contains(value));
                    Some(json!({
                        "type": "choice",
                        "choices": choices,
                        "default": default,
                    }))
                }
            }
            "color" => Some(json!({
                "type": "color",
                "default": string_field(spec, &["value", "default"]).map(|value| normalize_color(&value)),
            })),
            "textinput" | "text" => Some(json!({
                "type": "text",
                "default": string_field(spec, &["value", "default"]),
            })),
            "file" | "directory" => {
                report
                    .unsupported_features
                    .push(format!("property:{property_type}"));
                report.warnings.push(format!(
                    "Property {name:?} of type {property_type:?} was not converted; host file selection is not implemented."
                ));
                None
            }
            "" => {
                report
                    .warnings
                    .push(format!("Skipped property {name:?}: missing type."));
                None
            }
            other => {
                report
                    .unsupported_features
                    .push(format!("property:{other}"));
                report.warnings.push(format!(
                    "Skipped unsupported property {name:?} of type {other:?}."
                ));
                None
            }
        };

        if let Some(property) = converted_property {
            converted.insert(name.clone(), property);
            report.converted_features.push(format!("property:{name}"));
        }
    }

    converted
}

fn copy_preview_or_generate(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    report: &mut ConversionReport,
    fallback: MissingPreviewFallback<'_>,
) -> Result<Option<PreviewPaths>, ConversionError> {
    if let Some(preview) = &project.preview_file {
        let copied = copy_project_file(
            &project.root,
            preview,
            output_dir.join("previews"),
            "poster",
            report,
        )?;
        return Ok(Some(PreviewPaths {
            thumbnail: Some(copied.package_path.clone()),
            poster: Some(copied.package_path),
        }));
    }

    match fallback {
        MissingPreviewFallback::StaticImage { source } => {
            let poster = copy_project_file(
                &project.root,
                source,
                output_dir.join("previews"),
                "poster",
                report,
            )?;
            let thumbnail = copy_project_file(
                &project.root,
                source,
                output_dir.join("previews"),
                "thumbnail",
                report,
            )?;
            report.generated_assets.push(poster.package_path.clone());
            report.generated_assets.push(thumbnail.package_path.clone());
            report.warnings.push(
                "No preview image found; generated poster and thumbnail from the source image."
                    .to_owned(),
            );
            Ok(Some(PreviewPaths {
                thumbnail: Some(thumbnail.package_path),
                poster: Some(poster.package_path),
            }))
        }
        MissingPreviewFallback::Video { source } => {
            let preview = generate_video_preview(project, output_dir, source, report)?;
            Ok(Some(preview))
        }
        MissingPreviewFallback::Scene { source } => {
            let preview = generate_svg_placeholder_preview(
                project,
                output_dir,
                source,
                PlaceholderKind::Scene,
                report,
            )?;
            report.warnings.push(
                "No preview image found; generated metadata-based scene fallback poster and thumbnail.".to_owned(),
            );
            Ok(Some(preview))
        }
        MissingPreviewFallback::None => {
            report.warnings.push(
                "No preview image found; poster and thumbnail were not generated.".to_owned(),
            );
            Ok(None)
        }
    }
}

enum MissingPreviewFallback<'a> {
    None,
    StaticImage { source: &'a str },
    Video { source: &'a str },
    Scene { source: &'a str },
}

#[derive(Debug, Clone, Copy)]
struct StaticImageVariantTools<'a> {
    ffmpeg: &'a Path,
    ffprobe: &'a Path,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ImageDimensions {
    width: u32,
    height: u32,
}

impl ImageDimensions {
    fn can_generate(self, spec: StaticImageVariantSpec) -> bool {
        self.width >= spec.width
            && self.height >= spec.height
            && (self.width > spec.width || self.height > spec.height)
    }
}

fn generate_static_image_variants(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    source: &str,
    report: &mut ConversionReport,
) -> Vec<Value> {
    let Some(ffmpeg) = find_executable_on_path(FFMPEG_BINARY) else {
        report.warnings.push(format!(
            "Static image resolution variants were not generated: {FFMPEG_BINARY} executable was not found on PATH."
        ));
        return Vec::new();
    };
    let Some(ffprobe) = find_executable_on_path(FFPROBE_BINARY) else {
        report.warnings.push(format!(
            "Static image resolution variants were not generated: {FFPROBE_BINARY} executable was not found on PATH."
        ));
        return Vec::new();
    };
    generate_static_image_variants_with_tools(
        project,
        output_dir,
        source,
        report,
        StaticImageVariantTools {
            ffmpeg: &ffmpeg,
            ffprobe: &ffprobe,
        },
    )
}

fn generate_static_image_variants_with_tools(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    source: &str,
    report: &mut ConversionReport,
    tools: StaticImageVariantTools<'_>,
) -> Vec<Value> {
    if !is_raster_image_path(source) {
        return Vec::new();
    }

    let relative = match normalize_relative_path(source) {
        Ok(relative) => relative,
        Err(err) => {
            report.warnings.push(format!(
                "Static image resolution variants were not generated: {err}."
            ));
            return Vec::new();
        }
    };
    let source_path = project.root.join(relative);
    let dimensions = match probe_image_dimensions(tools.ffprobe, &source_path) {
        Ok(dimensions) => dimensions,
        Err(err) => {
            report.warnings.push(format!(
                "Static image resolution variants were not generated: {err}."
            ));
            return Vec::new();
        }
    };

    let mut variants = Vec::new();
    for spec in STATIC_IMAGE_VARIANTS {
        if !dimensions.can_generate(*spec) {
            continue;
        }
        let output_path = output_dir.join("variants").join(spec.file_name);
        match generate_static_image_variant(tools.ffmpeg, &source_path, &output_path, *spec) {
            Ok(()) => {
                let package_path = path_to_package_string(
                    output_path.strip_prefix(output_dir).unwrap_or(&output_path),
                );
                report.generated_assets.push(package_path.clone());
                report
                    .converted_features
                    .push(format!("static-image:variant:{}", spec.id));
                variants.push(json!({
                    "id": spec.id,
                    "source": package_path,
                    "width": spec.width,
                    "height": spec.height,
                }));
            }
            Err(err) => {
                let _ = fs::remove_file(&output_path);
                report.warnings.push(format!(
                    "Static image variant {} was not generated: {err}.",
                    spec.id
                ));
            }
        }
    }
    variants
}

fn probe_image_dimensions(ffprobe: &Path, source_path: &Path) -> Result<ImageDimensions, String> {
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "json",
        ])
        .arg(source_path)
        .output()
        .map_err(|err| format!("failed to run {}: {err}", ffprobe.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let reason = if stderr.is_empty() {
            output.status.to_string()
        } else {
            format!("{}: {stderr}", output.status)
        };
        return Err(format!("{} failed: {reason}", ffprobe.display()));
    }

    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|err| format!("{} returned invalid JSON: {err}", ffprobe.display()))?;
    let stream = value
        .get("streams")
        .and_then(Value::as_array)
        .and_then(|streams| streams.first())
        .and_then(Value::as_object)
        .ok_or_else(|| format!("{} did not report an image stream", ffprobe.display()))?;
    let width = stream
        .get("width")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{} did not report a valid image width", ffprobe.display()))?;
    let height = stream
        .get("height")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{} did not report a valid image height", ffprobe.display()))?;
    Ok(ImageDimensions { width, height })
}

fn generate_static_image_variant(
    ffmpeg: &Path,
    source_path: &Path,
    output_path: &Path,
    spec: StaticImageVariantSpec,
) -> Result<(), String> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create variant directory: {err}"))?;
    }
    let filter = format!(
        "scale={}:{}:force_original_aspect_ratio=increase,crop={}:{}",
        spec.width, spec.height, spec.width, spec.height
    );
    let output = Command::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(source_path)
        .args(["-frames:v", "1", "-an", "-sn", "-vf", &filter])
        .arg(output_path)
        .output()
        .map_err(|err| format!("failed to run {}: {err}", ffmpeg.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let reason = if stderr.is_empty() {
            output.status.to_string()
        } else {
            format!("{}: {stderr}", output.status)
        };
        return Err(format!("{} failed: {reason}", ffmpeg.display()));
    }

    let metadata = fs::metadata(output_path).map_err(|err| {
        format!(
            "{} did not create {}: {err}",
            ffmpeg.display(),
            output_path.display()
        )
    })?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(format!(
            "{} created an empty variant at {}",
            ffmpeg.display(),
            output_path.display()
        ));
    }

    Ok(())
}

fn generate_video_placeholder_preview(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    source: &str,
    report: &mut ConversionReport,
) -> Result<PreviewPaths, ConversionError> {
    generate_svg_placeholder_preview(project, output_dir, source, PlaceholderKind::Video, report)
}

fn generate_video_preview(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    source: &str,
    report: &mut ConversionReport,
) -> Result<PreviewPaths, ConversionError> {
    match generate_video_first_frame_preview(project, output_dir, source, report) {
        Ok(preview) => {
            report
                .converted_features
                .push("video:first-frame-preview".to_owned());
            report.warnings.push(
                "No preview image found; generated poster and thumbnail from the first video frame."
                    .to_owned(),
            );
            Ok(preview)
        }
        Err(reason) => {
            let preview = generate_video_placeholder_preview(project, output_dir, source, report)?;
            report.warnings.push(format!(
                "No preview image found; could not extract the first video frame ({reason}); generated metadata-based video poster and thumbnail fallback."
            ));
            Ok(preview)
        }
    }
}

fn generate_video_first_frame_preview(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    source: &str,
    report: &mut ConversionReport,
) -> Result<PreviewPaths, String> {
    let relative = normalize_relative_path(source).map_err(|err| err.to_string())?;
    let source_path = project.root.join(relative);
    let ffmpeg = find_executable_on_path(FFMPEG_BINARY)
        .ok_or_else(|| format!("{FFMPEG_BINARY} executable was not found on PATH"))?;
    generate_video_first_frame_preview_with_ffmpeg(&ffmpeg, &source_path, output_dir, report)
}

fn generate_video_first_frame_preview_with_ffmpeg(
    ffmpeg: &Path,
    source_path: &Path,
    output_dir: &Path,
    report: &mut ConversionReport,
) -> Result<PreviewPaths, String> {
    let preview_dir = output_dir.join("previews");
    fs::create_dir_all(&preview_dir)
        .map_err(|err| format!("failed to create preview directory: {err}"))?;

    let poster_path = preview_dir.join("poster.jpg");
    let thumbnail_path = preview_dir.join("thumbnail.jpg");
    let result = (|| {
        extract_video_frame(ffmpeg, source_path, &poster_path, VIDEO_POSTER_WIDTH, 2)?;
        extract_video_frame(
            ffmpeg,
            source_path,
            &thumbnail_path,
            VIDEO_THUMBNAIL_WIDTH,
            4,
        )?;
        Ok::<(), String>(())
    })();

    if let Err(err) = result {
        let _ = fs::remove_file(&poster_path);
        let _ = fs::remove_file(&thumbnail_path);
        return Err(err);
    }

    let poster_package_path =
        path_to_package_string(poster_path.strip_prefix(output_dir).unwrap_or(&poster_path));
    let thumbnail_package_path = path_to_package_string(
        thumbnail_path
            .strip_prefix(output_dir)
            .unwrap_or(&thumbnail_path),
    );
    report.generated_assets.push(poster_package_path.clone());
    report.generated_assets.push(thumbnail_package_path.clone());

    Ok(PreviewPaths {
        thumbnail: Some(thumbnail_package_path),
        poster: Some(poster_package_path),
    })
}

fn extract_video_frame(
    ffmpeg: &Path,
    source_path: &Path,
    output_path: &Path,
    width: u32,
    quality: u32,
) -> Result<(), String> {
    let scale_filter = format!("scale={width}:-2");
    let quality = quality.to_string();
    let output = Command::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(source_path)
        .args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-an",
            "-sn",
            "-vf",
            &scale_filter,
            "-q:v",
            &quality,
        ])
        .arg(output_path)
        .output()
        .map_err(|err| format!("failed to run {}: {err}", ffmpeg.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let reason = if stderr.is_empty() {
            output.status.to_string()
        } else {
            format!("{}: {stderr}", output.status)
        };
        return Err(format!("{} failed: {reason}", ffmpeg.display()));
    }

    let metadata = fs::metadata(output_path).map_err(|err| {
        format!(
            "{} did not create {}: {err}",
            ffmpeg.display(),
            output_path.display()
        )
    })?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(format!(
            "{} created an empty frame at {}",
            ffmpeg.display(),
            output_path.display()
        ));
    }

    Ok(())
}

fn find_executable_on_path(program: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    find_executable_in_path_list(program, env::split_paths(&path))
}

fn find_executable_in_path_list(
    program: &str,
    paths: impl IntoIterator<Item = PathBuf>,
) -> Option<PathBuf> {
    paths
        .into_iter()
        .map(|path| path.join(program))
        .find(|candidate| is_executable_file(candidate))
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

enum PlaceholderKind {
    Video,
    Scene,
}

impl PlaceholderKind {
    fn label(&self) -> &'static str {
        match self {
            Self::Video => "Video",
            Self::Scene => "Scene",
        }
    }
}

fn generate_svg_placeholder_preview(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    source: &str,
    kind: PlaceholderKind,
    report: &mut ConversionReport,
) -> Result<PreviewPaths, ConversionError> {
    let poster_path = output_dir.join("previews/poster.svg");
    let thumbnail_path = output_dir.join("previews/thumbnail.svg");
    let source_name = Path::new(source)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(source);
    let poster = placeholder_svg(kind.label(), &project.title, source_name, 1920, 1080);
    let thumbnail = placeholder_svg(kind.label(), &project.title, source_name, 512, 288);
    fs::write(&poster_path, poster).map_err(ConversionError::WriteFile)?;
    fs::write(&thumbnail_path, thumbnail).map_err(ConversionError::WriteFile)?;

    let poster_package_path =
        path_to_package_string(poster_path.strip_prefix(output_dir).unwrap_or(&poster_path));
    let thumbnail_package_path = path_to_package_string(
        thumbnail_path
            .strip_prefix(output_dir)
            .unwrap_or(&thumbnail_path),
    );
    report.generated_assets.push(poster_package_path.clone());
    report.generated_assets.push(thumbnail_package_path.clone());

    Ok(PreviewPaths {
        thumbnail: Some(thumbnail_package_path),
        poster: Some(poster_package_path),
    })
}

fn placeholder_svg(kind: &str, title: &str, source_name: &str, width: u32, height: u32) -> String {
    let kind = escape_xml(kind);
    let title = escape_xml(title);
    let source_name = escape_xml(source_name);
    let font_size = (height / 14).clamp(18, 96);
    let small_font_size = (height / 26).clamp(12, 48);
    let center_y = height / 2;
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
  <rect width="100%" height="100%" fill="#101418"/>
  <rect x="0" y="0" width="100%" height="100%" fill="#18212b"/>
  <circle cx="{cx}" cy="{cy}" r="{radius}" fill="#263442"/>
  <path d="{play_path}" fill="#d7e0ea"/>
  <text x="50%" y="{kind_y}" fill="#94a3b8" font-family="sans-serif" font-size="{small_font_size}" text-anchor="middle" letter-spacing="3">{kind}</text>
  <text x="50%" y="{title_y}" fill="#f1f5f9" font-family="sans-serif" font-size="{font_size}" font-weight="700" text-anchor="middle">{title}</text>
  <text x="50%" y="{source_y}" fill="#94a3b8" font-family="sans-serif" font-size="{small_font_size}" text-anchor="middle">{source_name}</text>
</svg>
"##,
        cx = width / 2,
        cy = center_y - height / 12,
        radius = height / 9,
        play_path = play_path(width, height),
        kind_y = center_y + height / 10,
        title_y = center_y + height / 7,
        source_y = center_y + height / 7 + small_font_size * 2,
    )
}

fn play_path(width: u32, height: u32) -> String {
    let cx = width as f32 / 2.0;
    let cy = height as f32 / 2.0 - height as f32 / 12.0;
    let size = height as f32 / 12.0;
    format!(
        "M {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1} Z",
        cx - size * 0.35,
        cy - size * 0.62,
        cx - size * 0.35,
        cy + size * 0.62,
        cx + size * 0.72,
        cy
    )
}

fn escape_xml(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn copy_project_file(
    root: &Path,
    relative_path: &str,
    dest_dir: PathBuf,
    dest_stem: &str,
    report: &mut ConversionReport,
) -> Result<CopiedAsset, ConversionError> {
    let relative = normalize_relative_path(relative_path)?;
    let source = root.join(&relative);
    if !source.is_file() {
        return Err(ConversionError::MissingFile(source));
    }
    fs::create_dir_all(&dest_dir).map_err(ConversionError::CreateDir)?;
    let extension = source
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!(".{}", extension.to_ascii_lowercase()))
        .unwrap_or_default();
    let dest = dest_dir.join(format!("{dest_stem}{extension}"));
    fs::copy(&source, &dest).map_err(ConversionError::CopyFile)?;
    let package_path = path_to_package_string(
        dest.strip_prefix(dest_dir.parent().unwrap_or_else(|| Path::new("")))
            .unwrap_or(&dest),
    );
    report.copied_assets.push(package_path.clone());
    Ok(CopiedAsset { package_path })
}

fn copy_dir_recursive(
    source_root: &Path,
    dest_root: &Path,
    package_root: &Path,
    excluded_names: &[&str],
    report: &mut ConversionReport,
) -> Result<(), ConversionError> {
    for entry in fs::read_dir(source_root).map_err(ConversionError::ReadDir)? {
        let entry = entry.map_err(ConversionError::ReadDirEntry)?;
        let source_path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if excluded_names.iter().any(|excluded| *excluded == name_str) {
            continue;
        }
        let dest_path = dest_root.join(&name);
        if source_path.is_dir() {
            fs::create_dir_all(&dest_path).map_err(ConversionError::CreateDir)?;
            copy_dir_recursive(
                &source_path,
                &dest_path,
                package_root,
                excluded_names,
                report,
            )?;
        } else if source_path.is_file() {
            fs::copy(&source_path, &dest_path).map_err(ConversionError::CopyFile)?;
            let package_path =
                path_to_package_string(dest_path.strip_prefix(package_root).unwrap_or(&dest_path));
            report.copied_assets.push(package_path);
        }
    }
    Ok(())
}

fn write_metadata(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    report: &ConversionReport,
) -> Result<(), ConversionError> {
    write_json_pretty(output_dir.join("metadata/source.json"), &project.raw)?;
    write_json_pretty(output_dir.join("metadata/conversion-report.json"), report)
}

fn write_json_pretty(
    path: impl AsRef<Path>,
    value: &impl Serialize,
) -> Result<(), ConversionError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(ConversionError::CreateDir)?;
    }
    let contents = serde_json::to_vec_pretty(value).map_err(ConversionError::Serialize)?;
    fs::write(path, contents).map_err(ConversionError::WriteFile)
}

fn prepare_output_dir(source_dir: &Path, output_dir: &Path) -> Result<(), ConversionError> {
    if output_dir.starts_with(source_dir) {
        return Err(ConversionError::OutputInsideSource {
            source_dir: source_dir.to_path_buf(),
            output_dir: output_dir.to_path_buf(),
        });
    }
    if output_dir.exists()
        && fs::read_dir(output_dir)
            .map_err(ConversionError::ReadDir)?
            .next()
            .is_some()
    {
        return Err(ConversionError::OutputExists(output_dir.to_path_buf()));
    }
    fs::create_dir_all(output_dir).map_err(ConversionError::CreateDir)
}

fn normalize_relative_path(path: &str) -> Result<PathBuf, ConversionError> {
    let normalized = path.replace('\\', "/");
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(ConversionError::InvalidProjectPath(path.to_owned()));
    }
    Ok(PathBuf::from(normalized))
}

fn path_to_package_string(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn string_field(map: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| map.get(*key))
        .find_map(value_to_string)
}

fn number_field(map: &Map<String, Value>, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .filter_map(|key| map.get(*key))
        .find_map(|value| value.as_f64().or_else(|| value.as_str()?.parse().ok()))
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn combo_choices(spec: &Map<String, Value>) -> Vec<String> {
    let Some(options) = spec.get("options") else {
        return Vec::new();
    };
    match options {
        Value::Array(options) => options
            .iter()
            .filter_map(|option| {
                if let Some(value) = value_to_string(option) {
                    Some(value)
                } else {
                    option
                        .as_object()
                        .and_then(|option| string_field(option, &["value", "label", "text"]))
                }
            })
            .collect(),
        Value::Object(options) => options.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

fn normalize_color(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with('#') {
        return trimmed.to_owned();
    }
    let components: Vec<f32> = trimmed
        .split_whitespace()
        .filter_map(|part| part.parse::<f32>().ok())
        .collect();
    if components.len() >= 3 {
        let r = color_component_to_u8(components[0]);
        let g = color_component_to_u8(components[1]);
        let b = color_component_to_u8(components[2]);
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        trimmed.to_owned()
    }
}

fn color_component_to_u8(value: f32) -> u8 {
    if value <= 1.0 {
        (value.clamp(0.0, 1.0) * 255.0).round() as u8
    } else {
        value.clamp(0.0, 255.0).round() as u8
    }
}

#[derive(Debug)]
struct WallpaperEngineProject {
    root: PathBuf,
    raw: Value,
    source_type: SourceType,
    entry_file: Option<String>,
    preview_file: Option<String>,
    title: String,
    authors: Vec<String>,
}

impl WallpaperEngineProject {
    fn load(root: &Path) -> Result<Self, ConversionError> {
        let project_path = root.join(PROJECT_FILE);
        let project_json =
            fs::read_to_string(&project_path).map_err(|source| ConversionError::ReadProject {
                path: project_path.clone(),
                source,
            })?;
        let raw: Value = serde_json::from_str(&project_json).map_err(|source| {
            ConversionError::ParseProject {
                path: project_path,
                source,
            }
        })?;
        let object = raw.as_object().ok_or_else(|| {
            ConversionError::InvalidProject("project.json must be an object".to_owned())
        })?;

        let entry_file = string_field(object, &["file", "entry", "main", "index", "content"]);
        let source_type = detect_source_type(object, entry_file.as_deref());
        let preview_file = string_field(object, &["preview", "previewfile", "thumbnail"]);
        let title = string_field(object, &["title", "name"]).unwrap_or_else(|| {
            root.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Wallpaper Engine Project")
                .to_owned()
        });
        let authors = string_field(object, &["author", "creator"])
            .map(|author| vec![author])
            .unwrap_or_default();

        Ok(Self {
            root: root.to_path_buf(),
            raw,
            source_type,
            entry_file,
            preview_file,
            title,
            authors,
        })
    }

    fn gilder_id(&self) -> String {
        let slug = self
            .title
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() {
                    ch.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .split('-')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        format!(
            "org.gilder.converted.wallpaper-engine.{}",
            if slug.is_empty() { "wallpaper" } else { &slug }
        )
    }

    fn detected_features(&self) -> Vec<String> {
        let mut features = BTreeSet::new();
        features.insert(self.source_type.as_str().to_owned());
        if self.raw.pointer("/general/properties").is_some() {
            features.insert("user-properties".to_owned());
        }
        if self.preview_file.is_some() {
            features.insert("preview".to_owned());
        }
        collect_feature_hints_from_value(self.source_type, &self.raw, &mut features);
        if let Some(entry_file) = &self.entry_file {
            collect_feature_hints_from_entry(
                self.source_type,
                &self.root,
                entry_file,
                &mut features,
            );
        }
        features.into_iter().collect()
    }

    fn audio_requested(&self) -> bool {
        explicit_audio_request(&self.raw)
    }
}

fn collect_feature_hints_from_entry(
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
        return matches!(source_type, SourceType::Web | SourceType::Scene);
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

fn collect_feature_hints_from_value(
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
        if source_type == SourceType::Scene {
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

fn has_shader_extension(value: &str) -> bool {
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

fn is_image_path(value: &str) -> bool {
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

fn is_raster_image_path(value: &str) -> bool {
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

fn explicit_audio_request(value: &Value) -> bool {
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
        Value::String(value) => string_requests_audio(value),
        Value::Array(values) => values.iter().any(value_requests_audio),
        Value::Object(object) => object.values().any(value_requests_audio),
        Value::Null => false,
    }
}

fn value_requests_audio(value: &Value) -> bool {
    match value {
        Value::Bool(enabled) => *enabled,
        Value::Number(number) => number.as_f64().is_some_and(|value| value > 0.0),
        Value::String(value) => string_requests_audio(value),
        Value::Array(values) => values.iter().any(value_requests_audio),
        Value::Object(object) => object
            .iter()
            .any(|(key, value)| key_requests_audio(key, value) || value_requests_audio(value)),
        Value::Null => false,
    }
}

fn string_requests_audio(value: &str) -> bool {
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
    Path::new(trimmed)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(is_audio_extension)
        .unwrap_or(true)
}

fn is_audio_extension(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "aac" | "flac" | "m4a" | "mp3" | "oga" | "ogg" | "opus" | "wav" | "weba" | "wma"
    )
}

fn normalize_project_key(key: &str) -> String {
    key.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn detect_source_type(object: &Map<String, Value>, entry_file: Option<&str>) -> SourceType {
    if let Some(kind) = string_field(object, &["type", "wallpaperType", "contentType"]) {
        let kind = kind.to_ascii_lowercase();
        if kind.contains("application") || kind.contains("exe") {
            return SourceType::Application;
        }
        if kind.contains("video") {
            return SourceType::Video;
        }
        if kind.contains("web") {
            return SourceType::Web;
        }
        if kind.contains("scene") {
            return SourceType::Scene;
        }
        if kind.contains("image") {
            return SourceType::Image;
        }
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
enum SourceType {
    Image,
    Video,
    Web,
    Scene,
    Application,
    Unknown,
}

impl SourceType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
            Self::Web => "web",
            Self::Scene => "scene",
            Self::Application => "application",
            Self::Unknown => "unknown",
        }
    }

    fn from_extension(extension: &str) -> Self {
        match extension.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" | "png" | "webp" | "avif" | "bmp" | "gif" | "svg" => Self::Image,
            "mp4" | "m4v" | "webm" | "mkv" | "mov" | "avi" => Self::Video,
            "html" | "htm" => Self::Web,
            "json" => Self::Scene,
            "exe" | "bat" | "cmd" | "com" | "scr" => Self::Application,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CopiedAsset {
    package_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreviewPaths {
    thumbnail: Option<String>,
    poster: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionReport {
    pub source_type: String,
    pub detected_features: Vec<String>,
    pub converted_features: Vec<String>,
    pub unsupported_features: Vec<String>,
    pub copied_assets: Vec<String>,
    pub generated_assets: Vec<String>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl ConversionReport {
    fn new(source_type: &str) -> Self {
        Self {
            source_type: source_type.to_owned(),
            detected_features: Vec::new(),
            converted_features: Vec::new(),
            unsupported_features: Vec::new(),
            copied_assets: Vec::new(),
            generated_assets: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub enum ConversionError {
    ReadProject {
        path: PathBuf,
        source: io::Error,
    },
    ParseProject {
        path: PathBuf,
        source: serde_json::Error,
    },
    InvalidProject(String),
    InvalidProjectPath(String),
    MissingEntry(String),
    MissingFile(PathBuf),
    OutputInsideSource {
        source_dir: PathBuf,
        output_dir: PathBuf,
    },
    OutputExists(PathBuf),
    UnsupportedType {
        source_type: String,
    },
    ReadDir(io::Error),
    ReadDirEntry(io::Error),
    CreateDir(io::Error),
    CopyFile(io::Error),
    Serialize(serde_json::Error),
    WriteFile(io::Error),
    InvalidOutput {
        output_dir: PathBuf,
        source: String,
    },
}

impl fmt::Display for ConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadProject { path, source } => {
                write!(f, "failed to read {}: {source}", path.display())
            }
            Self::ParseProject { path, source } => {
                write!(f, "failed to parse {}: {source}", path.display())
            }
            Self::InvalidProject(message) => {
                write!(f, "invalid Wallpaper Engine project: {message}")
            }
            Self::InvalidProjectPath(path) => write!(f, "invalid project-relative path: {path}"),
            Self::MissingEntry(message) => write!(f, "{message}"),
            Self::MissingFile(path) => write!(f, "project file does not exist: {}", path.display()),
            Self::OutputInsideSource {
                source_dir,
                output_dir,
            } => write!(
                f,
                "output directory {} must not be inside source directory {}",
                output_dir.display(),
                source_dir.display()
            ),
            Self::OutputExists(path) => {
                write!(f, "output directory is not empty: {}", path.display())
            }
            Self::UnsupportedType { source_type } => {
                write!(
                    f,
                    "unsupported Wallpaper Engine project type: {source_type}"
                )
            }
            Self::ReadDir(source) => write!(f, "failed to read directory: {source}"),
            Self::ReadDirEntry(source) => write!(f, "failed to read directory entry: {source}"),
            Self::CreateDir(source) => write!(f, "failed to create output directory: {source}"),
            Self::CopyFile(source) => write!(f, "failed to copy project asset: {source}"),
            Self::Serialize(source) => write!(f, "failed to serialize conversion output: {source}"),
            Self::WriteFile(source) => write!(f, "failed to write conversion output: {source}"),
            Self::InvalidOutput { output_dir, source } => {
                write!(
                    f,
                    "generated package {} is invalid: {source}",
                    output_dir.display()
                )
            }
        }
    }
}

impl std::error::Error for ConversionError {}

const WEB_BRIDGE: &str = r#"(() => {
  const pending = {};
  window.gilder = {
    setProperty(name, value) {
      pending[name] = value;
      if (window.wallpaperPropertyListener?.applyUserProperties) {
        window.wallpaperPropertyListener.applyUserProperties({ [name]: { value } });
      }
    },
    properties: pending
  };
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn converts_static_image_project() {
        let source = TestDir::new("we-static-source");
        let output = TestDir::new("we-static-output");
        output.remove();
        source.write_file("wallpaper.png", "not real png");
        source.write_file("preview.jpg", "not real jpg");
        source.write_file(
            PROJECT_FILE,
            r##"{
              "type": "image",
              "title": "Static Example",
              "file": "wallpaper.png",
              "preview": "preview.jpg",
              "general": {
                "properties": {
                  "accent": { "type": "color", "value": "1 0.5 0" }
                }
              }
            }"##,
        );

        let summary = convert_project(source.path(), output.path()).unwrap();
        assert_eq!(summary.source_type, "image");
        let manifest: Value =
            serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
                .unwrap();
        assert_eq!(manifest["kind"], "static-image");
        assert_eq!(manifest["properties"]["accent"]["default"], "#ff8000");
        assert!(
            output
                .path()
                .join("metadata/conversion-report.json")
                .exists()
        );
    }

    #[test]
    fn converts_static_image_project_without_preview_from_source_image() {
        let source = TestDir::new("we-static-no-preview-source");
        let output = TestDir::new("we-static-no-preview-output");
        output.remove();
        source.write_file("wallpaper.png", "not real png");
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "image",
              "title": "Static Without Preview",
              "file": "wallpaper.png"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let manifest: Value =
            serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
                .unwrap();
        assert_eq!(manifest["preview"]["poster"], "previews/poster.png");
        assert_eq!(manifest["preview"]["thumbnail"], "previews/thumbnail.png");
        assert!(output.path().join("previews/poster.png").exists());
        assert!(output.path().join("previews/thumbnail.png").exists());
        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .generated_assets
                .contains(&"previews/poster.png".to_owned())
        );
        assert!(
            report
                .generated_assets
                .contains(&"previews/thumbnail.png".to_owned())
        );
    }

    #[test]
    fn converts_static_image_project_with_resolution_variants() {
        let source = TestDir::new("we-static-variant-source");
        let output = TestDir::new("we-static-variant-output");
        let tools = TestDir::new("we-static-variant-tools");
        source.write_file("wallpaper.png", "fake large image");
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "image",
              "title": "Static Variant Source",
              "file": "wallpaper.png"
            }"#,
        );
        let ffprobe = tools.path().join("ffprobe");
        fs::write(
            &ffprobe,
            r#"#!/bin/sh
printf '{"streams":[{"width":7680,"height":4320}]}'
exit 0
"#,
        )
        .unwrap();
        make_executable(&ffprobe);
        let ffmpeg = tools.path().join("ffmpeg");
        fs::write(
            &ffmpeg,
            r#"#!/bin/sh
out=""
for arg in "$@"; do
  out="$arg"
done
printf 'png-variant' > "$out"
exit 0
"#,
        )
        .unwrap();
        make_executable(&ffmpeg);
        let project = WallpaperEngineProject::load(source.path()).unwrap();
        let mut report = ConversionReport::new("image");

        let manifest = convert_static_image_with_variant_tools(
            &project,
            output.path(),
            &mut report,
            Some(StaticImageVariantTools {
                ffmpeg: &ffmpeg,
                ffprobe: &ffprobe,
            }),
        )
        .unwrap();
        write_json_pretty(output.path().join(MANIFEST_FILE), &manifest).unwrap();
        load_gwpdir(output.path()).unwrap();

        let variants = manifest["variants"].as_array().unwrap();
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0]["id"], "landscape-1080p");
        assert_eq!(variants[0]["width"], 1920);
        assert_eq!(variants[0]["height"], 1080);
        assert_eq!(variants[1]["id"], "landscape-2160p");
        assert!(output.path().join("variants/landscape-1080p.png").exists());
        assert!(output.path().join("variants/landscape-2160p.png").exists());
        assert!(
            report
                .generated_assets
                .contains(&"variants/landscape-1080p.png".to_owned())
        );
        assert!(
            report
                .converted_features
                .contains(&"static-image:variant:landscape-2160p".to_owned())
        );
    }

    #[test]
    fn converts_video_project() {
        let source = TestDir::new("we-video-source");
        let output = TestDir::new("we-video-output");
        output.remove();
        source.write_file("loop.mp4", "not real mp4");
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "video",
              "title": "Video Example",
              "file": "loop.mp4"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let manifest: Value =
            serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
                .unwrap();
        assert_eq!(manifest["kind"], "video");
        assert_eq!(manifest["entry"]["source"], "assets/loop.mp4");
        assert_eq!(manifest["entry"]["poster"], "previews/poster.svg");
        assert_eq!(manifest["preview"]["poster"], "previews/poster.svg");
        assert_eq!(manifest["preview"]["thumbnail"], "previews/thumbnail.svg");
        assert!(output.path().join("previews/poster.svg").exists());
        assert!(output.path().join("previews/thumbnail.svg").exists());
        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .generated_assets
                .contains(&"previews/poster.svg".to_owned())
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("metadata-based video poster"))
        );
    }

    #[test]
    fn converts_video_audio_intent_to_runtime_audio_policy() {
        let source = TestDir::new("we-video-audio-source");
        let output = TestDir::new("we-video-audio-output");
        output.remove();
        source.write_file("loop.mp4", "not real mp4");
        source.write_file("music.ogg", "not real audio");
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "video",
              "title": "Video With Audio",
              "file": "loop.mp4",
              "audio": "music.ogg"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let manifest: Value =
            serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
                .unwrap();
        assert_eq!(manifest["entry"]["muted"], false);
        assert_eq!(manifest["runtime"]["allow_audio"], true);
        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(report.detected_features.contains(&"audio".to_owned()));
        assert!(
            report
                .converted_features
                .contains(&"audio-policy".to_owned())
        );
    }

    #[test]
    fn generates_video_preview_from_first_frame_with_ffmpeg() {
        let source = TestDir::new("we-video-frame-source");
        let output = TestDir::new("we-video-frame-output");
        let tools = TestDir::new("we-video-frame-tools");
        output.remove();
        source.write_file("loop.mp4", "not real mp4");
        let ffmpeg = tools.path().join("ffmpeg");
        fs::write(
            &ffmpeg,
            r#"#!/bin/sh
for arg in "$@"; do
  case "$arg" in
    *.jpg) printf 'jpeg-frame' > "$arg" ;;
  esac
done
exit 0
"#,
        )
        .unwrap();
        make_executable(&ffmpeg);

        let mut report = ConversionReport::new("video");
        let preview = generate_video_first_frame_preview_with_ffmpeg(
            &ffmpeg,
            &source.path().join("loop.mp4"),
            output.path(),
            &mut report,
        )
        .unwrap();

        assert_eq!(preview.poster.as_deref(), Some("previews/poster.jpg"));
        assert_eq!(preview.thumbnail.as_deref(), Some("previews/thumbnail.jpg"));
        assert_eq!(
            fs::read(output.path().join("previews/poster.jpg")).unwrap(),
            b"jpeg-frame"
        );
        assert_eq!(
            fs::read(output.path().join("previews/thumbnail.jpg")).unwrap(),
            b"jpeg-frame"
        );
        assert!(
            report
                .generated_assets
                .contains(&"previews/poster.jpg".to_owned())
        );
        assert!(
            report
                .generated_assets
                .contains(&"previews/thumbnail.jpg".to_owned())
        );
    }

    #[test]
    fn finds_executable_on_path_list() {
        let tools = TestDir::new("we-path-tools");
        let ffmpeg = tools.path().join("ffmpeg");
        fs::write(&ffmpeg, "#!/bin/sh\nexit 0\n").unwrap();
        make_executable(&ffmpeg);

        let found = find_executable_in_path_list("ffmpeg", [tools.path().to_path_buf()]);
        assert_eq!(found.as_deref(), Some(ffmpeg.as_path()));
    }

    #[test]
    fn skips_binary_entry_content_when_collecting_feature_hints() {
        let source = TestDir::new("we-feature-binary-source");
        source.write_file(
            "loop.mp4",
            "https://example.invalid\nwindow.wallpaperRegisterAudioListener(() => {});",
        );

        let mut features = BTreeSet::new();
        collect_feature_hints_from_entry(
            SourceType::Video,
            source.path(),
            "loop.mp4",
            &mut features,
        );

        assert!(!features.contains("network"));
        assert!(!features.contains("web-audio-listener"));
    }

    #[test]
    fn caps_large_text_entry_feature_scan() {
        let source = TestDir::new("we-feature-large-source");
        let mut html = "<script>window.wallpaperPropertyListener = {};</script>".to_owned();
        html.push_str(&" ".repeat(FEATURE_SCAN_MAX_BYTES as usize + 128));
        source.write_file("index.html", &html);

        let mut features = BTreeSet::new();
        collect_feature_hints_from_entry(
            SourceType::Web,
            source.path(),
            "index.html",
            &mut features,
        );

        assert!(features.contains("web-property-listener"));
    }

    #[test]
    fn converts_web_project() {
        let source = TestDir::new("we-web-source");
        let output = TestDir::new("we-web-output");
        output.remove();
        source.write_file(
            "index.html",
            "<!doctype html><script src=\"app.js\"></script>",
        );
        source.write_file("app.js", "window.ready = true;");
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "web",
              "title": "Web Example",
              "file": "index.html",
              "general": {
                "properties": {
                  "enabled": { "type": "bool", "value": true },
                  "speed": { "type": "slider", "min": 0, "max": 2, "step": 0.1, "value": 1 }
                }
              }
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let manifest: Value =
            serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
                .unwrap();
        assert_eq!(manifest["kind"], "web");
        assert_eq!(manifest["entry"]["root"], "assets/web");
        assert_eq!(manifest["entry"]["index"], "index.html");
        assert!(output.path().join("assets/web/app.js").exists());
        assert!(output.path().join("assets/web/gilder-bridge.js").exists());
        assert_eq!(manifest["properties"]["enabled"]["type"], "bool");
        assert_eq!(manifest["properties"]["speed"]["type"], "range");
        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .unsupported_features
                .contains(&"web-runtime".to_owned())
        );
    }

    #[test]
    fn reports_web_runtime_feature_gaps() {
        let source = TestDir::new("we-web-runtime-source");
        let output = TestDir::new("we-web-runtime-output");
        output.remove();
        source.write_file(
            "index.html",
            r#"<!doctype html>
<script>
window.wallpaperPropertyListener = {};
window.wallpaperRegisterAudioListener(() => {});
fetch("https://example.invalid/data.json");
</script>"#,
        );
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "web",
              "title": "Web Runtime Features",
              "file": "index.html"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        for feature in ["web-property-listener", "web-audio-listener", "network"] {
            assert!(
                report.detected_features.contains(&feature.to_owned()),
                "missing detected feature {feature}: {:?}",
                report.detected_features
            );
        }
        for feature in ["web-runtime", "web-permissions"] {
            assert!(
                report.unsupported_features.contains(&feature.to_owned()),
                "missing unsupported feature {feature}: {:?}",
                report.unsupported_features
            );
        }
    }

    #[test]
    fn web_audio_intent_is_reported_as_unsupported_runtime_feature() {
        let source = TestDir::new("we-web-audio-source");
        let output = TestDir::new("we-web-audio-output");
        output.remove();
        source.write_file("index.html", "<!doctype html>");
        source.write_file("music.mp3", "not real audio");
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "web",
              "title": "Web Audio Example",
              "file": "index.html",
              "audiofile": "music.mp3"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let manifest: Value =
            serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
                .unwrap();
        assert_eq!(manifest["runtime"]["allow_audio"], false);
        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(report.detected_features.contains(&"audio".to_owned()));
        assert!(
            report
                .unsupported_features
                .contains(&"audio-runtime".to_owned())
        );
    }

    #[test]
    fn converts_scene_project_to_scene_lite_with_fallback() {
        let source = TestDir::new("we-scene-source");
        let output = TestDir::new("we-scene-output");
        output.remove();
        source.write_file(
            "scene.json",
            r#"{"objects":[{"type":"image","path":"background.png"}]}"#,
        );
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Scene Example",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let manifest: Value =
            serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
                .unwrap();
        assert_eq!(manifest["kind"], "scene-lite");
        assert_eq!(manifest["entry"]["type"], "scene-lite");
        assert_eq!(manifest["entry"]["source"], "assets/scene.json");
        assert_eq!(manifest["entry"]["fallback"], "previews/poster.svg");
        assert!(output.path().join("previews/poster.svg").exists());
        assert!(output.path().join("previews/thumbnail.svg").exists());
        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(report.converted_features.contains(&"scene-lite".to_owned()));
        assert!(
            report
                .unsupported_features
                .contains(&"scene-runtime".to_owned())
        );
        assert!(
            report
                .detected_features
                .contains(&"scene-layers".to_owned())
        );
        assert!(report.detected_features.contains(&"image-layer".to_owned()));
    }

    #[test]
    fn reports_scene_runtime_feature_gaps() {
        let source = TestDir::new("we-scene-feature-source");
        let output = TestDir::new("we-scene-feature-output");
        output.remove();
        source.write_file(
            "scene.json",
            r#"{
              "objects": [
                { "type": "image", "path": "background.png" },
                { "type": "particle", "emitter": "sparks" }
              ],
              "timeline": [{ "time": 0, "keyframe": true }],
              "script": "SceneScript.Update = function() {}",
              "shader": "effects/rain.frag",
              "parallax": true,
              "audio": { "response": true }
            }"#,
        );
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Scene Runtime Features",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        for feature in [
            "scene-layers",
            "image-layer",
            "particles",
            "timeline",
            "scenescript",
            "shader",
            "parallax",
            "audio-response",
        ] {
            assert!(
                report.detected_features.contains(&feature.to_owned()),
                "missing detected feature {feature}: {:?}",
                report.detected_features
            );
        }
        for feature in [
            "scene-runtime",
            "complex-particles",
            "timeline-animation",
            "scenescript",
            "custom-shader",
            "parallax",
            "audio-runtime",
        ] {
            assert!(
                report.unsupported_features.contains(&feature.to_owned()),
                "missing unsupported feature {feature}: {:?}",
                report.unsupported_features
            );
        }
    }

    #[test]
    fn rejects_application_project_and_writes_report() {
        let source = TestDir::new("we-app-source");
        let output = TestDir::new("we-app-output");
        output.remove();
        source.write_file("app.exe", "");
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "application",
              "title": "Executable Example",
              "file": "app.exe"
            }"#,
        );

        let error = convert_project(source.path(), output.path()).unwrap_err();
        assert!(matches!(error, ConversionError::UnsupportedType { .. }));
        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .unsupported_features
                .contains(&"executable-application".to_owned())
        );
    }

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(prefix: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("gilder-{prefix}-{}-{nonce}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn write_file(&self, relative_path: &str, contents: &str) {
            let path = self.path.join(relative_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, contents).unwrap();
        }

        fn remove(&self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    #[cfg(not(unix))]
    fn make_executable(_path: &Path) {}
}
