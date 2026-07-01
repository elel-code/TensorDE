use crate::core::{FORMAT_NAME, FORMAT_VERSION, MANIFEST_FILE, load_gwpdir};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

mod effect;
mod gtex;
mod ir;
mod media;
mod source;
mod tex;

use self::ir::{
    SceneAnimationLayerIr, SceneAudioControllerIr, SceneAudioCueConditionIr, SceneControllerIr,
    SceneNumericPropertyBindingIr, SceneNumericPropertyBindingIrResult, SceneParticleIr,
    SceneTimelineIr, scene_particle_seed_from_object,
};
use self::media::{
    ImageDimensions, MissingPreviewFallback, StaticImageVariantTools, copy_preview_or_generate,
    generate_static_image_variants, generate_static_image_variants_with_tools,
    probe_static_image_dimensions_for_manifest,
};
#[cfg(test)]
use self::media::{find_executable_in_path_list, generate_video_first_frame_preview_with_ffmpeg};
use self::source::{
    SourceType, collect_feature_hints_from_entry, collect_feature_hints_from_value,
    detect_source_type, explicit_audio_request, has_shader_extension, is_audio_extension,
    is_image_path, is_raster_image_path, normalize_project_key, static_image_audio_sources,
};
use self::tex::{SceneWeModelFrameSize, SceneWeTexImage, SceneWeTexPayload};

const PROJECT_FILE: &str = "project.json";
const SCENE_PACKAGE_FILE: &str = "scene.pkg";
const FFMPEG_BINARY: &str = "ffmpeg";
const FFPROBE_BINARY: &str = "ffprobe";
const VIDEO_POSTER_WIDTH: u32 = 1920;
const VIDEO_THUMBNAIL_WIDTH: u32 = 512;
const FEATURE_SCAN_MAX_BYTES: u64 = 2 * 1024 * 1024;
const SCENE_SCRIPT_SINE_TIMELINE_SAMPLES: usize = 64;
const SCENE_SCRIPT_SINE_TIMELINE_MIN_PERIOD_MS: u64 = 250;
const SCENE_SCRIPT_SINE_TIMELINE_MAX_PERIOD_MS: u64 = 60_000;
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
    StaticImageVariantSpec {
        id: "ultrawide-1080p",
        width: 2560,
        height: 1080,
        file_name: "ultrawide-1080p.png",
    },
    StaticImageVariantSpec {
        id: "ultrawide-1440p",
        width: 3440,
        height: 1440,
        file_name: "ultrawide-1440p.png",
    },
    StaticImageVariantSpec {
        id: "portrait-1080p",
        width: 1080,
        height: 1920,
        file_name: "portrait-1080p.png",
    },
    StaticImageVariantSpec {
        id: "portrait-2160p",
        width: 2160,
        height: 3840,
        file_name: "portrait-2160p.png",
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeGtexConversionSummary {
    pub source: PathBuf,
    pub output: PathBuf,
    pub width: u32,
    pub height: u32,
    pub format: &'static str,
    pub payload_bytes: u64,
}

pub fn convert_png_to_native_gtex(
    source: &Path,
    output: &Path,
) -> Result<NativeGtexConversionSummary, String> {
    let mut image = gtex::read_png_as_rgba(source)?;
    gtex::flip_rgba_rows_vertically(&mut image.rgba, image.width, image.height)?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    let payload_bytes = gtex::bc7_payload_len(image.width, image.height)?;
    gtex::write_bc7_gtex(output, &image)?;
    Ok(NativeGtexConversionSummary {
        source: source.to_path_buf(),
        output: output.to_path_buf(),
        width: image.width,
        height: image.height,
        format: "BC7_UNORM_BLOCK",
        payload_bytes,
    })
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
        SourceType::Scene => convert_scene(&project, output_dir, &mut report),
        SourceType::Shader => convert_shader(&project, output_dir, &mut report),
        SourceType::Playlist => convert_playlist(&project, output_dir, &mut report),
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
    let audio_sources = static_image_audio_sources(project);
    if !audio_sources.is_empty() {
        return convert_static_image_audio_scene_with_variant_tools(
            project,
            output_dir,
            report,
            variant_tools,
            audio_sources,
        );
    }

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
    let dimensions =
        probe_static_image_dimensions_for_manifest(project, source, report, variant_tools);
    let variants = match variant_tools {
        Some(tools) => {
            generate_static_image_variants_with_tools(project, output_dir, source, report, tools)
        }
        None => generate_static_image_variants(project, output_dir, source, report),
    };
    let mut entry = json!({
        "type": "static-image",
        "source": copied.package_path,
        "fit": "cover",
        "orientation": "from-metadata"
    });
    if let Some(dimensions) = dimensions
        && let Some(object) = entry.as_object_mut()
    {
        object.insert("width".to_owned(), json!(dimensions.width));
        object.insert("height".to_owned(), json!(dimensions.height));
    }

    let mut manifest = base_manifest(project, "static-image", preview, report, entry);
    if !variants.is_empty()
        && let Some(object) = manifest.as_object_mut()
    {
        object.insert("variants".to_owned(), Value::Array(variants));
    }
    Ok(manifest)
}

fn convert_static_image_audio_scene_with_variant_tools(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    report: &mut ConversionReport,
    variant_tools: Option<StaticImageVariantTools<'_>>,
    audio_sources: Vec<String>,
) -> Result<Value, ConversionError> {
    let source = project.entry_file.as_ref().ok_or_else(|| {
        ConversionError::MissingEntry("image project does not define an entry file".to_owned())
    })?;
    let image_package_path =
        convert_static_image_audio_scene_texture(project, output_dir, source, report)?;
    let preview = copy_preview_or_generate(
        project,
        output_dir,
        report,
        MissingPreviewFallback::StaticImage { source },
    )?;
    let dimensions =
        probe_static_image_dimensions_for_manifest(project, source, report, variant_tools);
    let scene_source = write_static_image_audio_scene_document(
        project,
        output_dir,
        source,
        &image_package_path,
        dimensions,
        &audio_sources,
        report,
    )?;

    push_unique(&mut report.detected_features, "audio");
    push_unique(&mut report.converted_features, "static-image");
    push_unique(&mut report.converted_features, "static-image-audio-scene");
    push_unique(&mut report.converted_features, "scene");
    push_unique(&mut report.converted_features, "audio-policy");
    push_unique(
        &mut report.converted_features,
        "scene-audio-cue-renderer-boundary",
    );
    push_unique(
        &mut report.converted_features,
        "scene-audio-cue-pipewire-present-runtime",
    );
    record_full_scene_runtime_boundary(report, None);
    report.warnings.push(
        "Converted static image with audio to a first-class Gilder scene document: one static image layer plus native FFmpeg/PipeWire scene audio cues. Static audio is not dropped."
            .to_owned(),
    );

    Ok(base_manifest(
        project,
        "scene",
        preview,
        report,
        json!({
            "type": "scene",
            "source": scene_source
        }),
    ))
}

fn convert_static_image_audio_scene_texture(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    source: &str,
    report: &mut ConversionReport,
) -> Result<String, ConversionError> {
    let relative = normalize_relative_path(source)?;
    let source_path = project.root.join(relative);
    if !source_path.is_file() {
        return Err(ConversionError::MissingFile(source_path));
    }
    let dest_dir = output_dir.join("assets");
    fs::create_dir_all(&dest_dir).map_err(ConversionError::CreateDir)?;
    let dest = dest_dir.join("wallpaper.gtex");
    convert_png_to_native_gtex(&source_path, &dest).map_err(|err| {
        ConversionError::InvalidProject(format!(
            "static image audio scene requires an image that can be converted offline to native BC7 .gtex: {}: {err}",
            source_path.display()
        ))
    })?;
    let package_path = path_to_package_string(dest.strip_prefix(output_dir).unwrap_or(&dest));
    push_unique(
        &mut report.converted_features,
        "static-image-bc7-gtex-conversion",
    );
    report.generated_assets.push(package_path.clone());
    Ok(package_path)
}

fn write_static_image_audio_scene_document(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    source_entry: &str,
    image_package_path: &str,
    dimensions: Option<ImageDimensions>,
    audio_sources: &[String],
    report: &mut ConversionReport,
) -> Result<String, ConversionError> {
    let package_path = "assets/scene.gscene.json";
    let scene_path = output_dir.join(package_path);
    if let Some(parent) = scene_path.parent() {
        fs::create_dir_all(parent).map_err(ConversionError::CreateDir)?;
    }

    let mut static_image_resource = json!({
        "id": "static-image",
        "type": "image",
        "source": image_package_path
    });
    if let Some(dimensions) = dimensions {
        static_image_resource["width"] = json!(dimensions.width);
        static_image_resource["height"] = json!(dimensions.height);
    }
    let mut resources = vec![static_image_resource];
    let mut cues = Vec::with_capacity(audio_sources.len());
    for (index, source) in audio_sources.iter().enumerate() {
        let copied = copy_project_file(
            &project.root,
            source,
            output_dir.join("assets"),
            &format!("audio-cue-{index}"),
            report,
        )?;
        let resource_id = format!("static-audio-{index}");
        resources.push(json!({
            "id": resource_id,
            "type": "audio",
            "source": copied.package_path
        }));
        cues.push(json!({
            "resource": resource_id,
            "source": copied.package_path,
            "playback_mode": "loop"
        }));
    }

    let mut document = json!({
        "version": 1,
        "profile": "native-vulkan-full-scene",
        "source": {
            "format": "wallpaper-engine-image",
            "entry": source_entry
        },
        "resources": resources,
        "nodes": [{
            "id": "static-image-layer",
            "type": "image",
            "resource": "static-image",
            "fit": "cover",
            "audio": cues
        }],
        "systems": {
            "audio_response": "absent"
        },
        "native_lowering": scene_native_lowering_from_status(
            &FullSceneConversionStatus::native_vulkan_scene_boundary()
        )
    });
    if let Some(dimensions) = dimensions
        && let Some(object) = document.as_object_mut()
    {
        object.insert(
            "size".to_owned(),
            json!({ "width": dimensions.width, "height": dimensions.height }),
        );
    }
    write_json_pretty(&scene_path, &document)?;
    let package_path = path_to_package_string(Path::new(package_path));
    report.generated_assets.push(package_path.clone());
    Ok(package_path)
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

fn convert_scene(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    report: &mut ConversionReport,
) -> Result<Value, ConversionError> {
    let source = project.entry_file.as_ref().ok_or_else(|| {
        ConversionError::MissingEntry("scene project does not define an entry file".to_owned())
    })?;
    let original_scene = copy_project_file(
        &project.root,
        source,
        output_dir.join("metadata"),
        "source-scene",
        report,
    )?;
    let preview = copy_preview_or_generate(
        project,
        output_dir,
        report,
        MissingPreviewFallback::Scene { source },
    )?;
    let source_scene = read_wallpaper_engine_scene_metadata(project, source, report);
    let scene_source = write_scene_document(
        project,
        output_dir,
        source,
        &original_scene.package_path,
        source_scene.as_ref(),
        report,
    )?;

    report.converted_features.push("scene".to_owned());
    if let Some(scene_package) = &project.scene_package {
        push_unique(&mut report.converted_features, "scene-we-package-import");
        report.warnings.push(format!(
            "Imported Wallpaper Engine {SCENE_PACKAGE_FILE} {} with {} entries into the first-class gscene conversion path.",
            scene_package.version,
            scene_package.entry_count
        ));
    }
    record_scene_runtime_gaps(report);
    record_full_scene_runtime_boundary(report, Some(&original_scene.package_path));
    let explicit_systems = scene_explicit_runtime_system_summary(report);
    report.warnings.push(format!(
        "Converted Scene project to a first-class Gilder scene document; original scene metadata was preserved at {}. Native particle emitter parameters are lowered into the gscene particle runtime when present; {}",
        original_scene.package_path, explicit_systems
    ));

    Ok(base_manifest(
        project,
        "scene",
        preview,
        report,
        json!({
            "type": "scene",
            "source": scene_source
        }),
    ))
}

fn scene_explicit_runtime_system_summary(report: &ConversionReport) -> String {
    let mut systems = Vec::new();
    if report
        .detected_features
        .iter()
        .any(|feature| feature == "scenescript")
    {
        systems.push("SceneScript");
    }
    if report
        .detected_features
        .iter()
        .any(|feature| feature == "shader")
        || report
            .unsupported_features
            .iter()
            .any(|feature| feature == "custom-shader")
    {
        systems.push("shader/effect graphs");
    }
    if report
        .detected_features
        .iter()
        .any(|feature| feature == "audio-response")
    {
        systems.push("audio-response visuals");
    }
    if systems.is_empty() {
        "No legacy runtime fallback was emitted.".to_owned()
    } else {
        format!(
            "{} remain explicit native scene systems until their native runtimes are implemented.",
            systems.join(", ")
        )
    }
}

fn convert_shader(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    report: &mut ConversionReport,
) -> Result<Value, ConversionError> {
    let source = project.entry_file.as_ref().ok_or_else(|| {
        ConversionError::MissingEntry("shader project does not define an entry file".to_owned())
    })?;
    let copied = copy_project_file(
        &project.root,
        source,
        output_dir.join("shaders"),
        "main",
        report,
    )?;
    let preview = copy_preview_or_generate(
        project,
        output_dir,
        report,
        MissingPreviewFallback::Shader { source },
    )?;
    let fallback = preview
        .as_ref()
        .and_then(|preview| preview.poster.clone())
        .map(Value::String)
        .unwrap_or(Value::Null);

    push_unique(&mut report.converted_features, "shader");
    record_shader_runtime_gap(report);
    report.warnings.push(
        "Converted Shader project to a native shader manifest with fallback; GPU shader execution is not implemented yet, so current renderers display the fallback poster.".to_owned(),
    );

    Ok(base_manifest(
        project,
        "shader",
        preview,
        report,
        json!({
            "type": "shader",
            "source": copied.package_path,
            "fallback": fallback,
            "language": shader_language_for_source(source),
            "max_fps": 60,
            "uniforms": shader_uniforms(project)
        }),
    ))
}

fn convert_playlist(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    report: &mut ConversionReport,
) -> Result<Value, ConversionError> {
    let object = project.raw.as_object().ok_or_else(|| {
        ConversionError::InvalidProject("playlist project must be an object".to_owned())
    })?;
    let source_items = playlist_items_from_project(object).ok_or_else(|| {
        ConversionError::MissingEntry("playlist project does not define an item array".to_owned())
    })?;
    let preview =
        copy_preview_or_generate(project, output_dir, report, MissingPreviewFallback::None)?;
    let mut items = Vec::new();
    for (index, item) in source_items.iter().enumerate() {
        let playlist_fallback = preview
            .as_ref()
            .and_then(|preview| preview.poster.as_deref());
        match convert_playlist_item(project, output_dir, index, item, playlist_fallback, report) {
            Ok(Some(item)) => items.push(item),
            Ok(None) => {}
            Err(err) => {
                report
                    .warnings
                    .push(format!("Skipped playlist item {index}: {err}."));
            }
        }
    }

    if items.is_empty() {
        report
            .errors
            .push("Playlist did not contain convertible items.".to_owned());
        return Err(ConversionError::MissingEntry(
            "playlist project did not contain convertible items".to_owned(),
        ));
    }

    push_unique(&mut report.converted_features, "playlist");
    Ok(base_manifest(
        project,
        "playlist",
        preview,
        report,
        json!({
            "type": "playlist",
            "selection": "first-match",
            "items": items,
        }),
    ))
}

fn convert_playlist_item(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    index: usize,
    value: &Value,
    playlist_fallback: Option<&str>,
    report: &mut ConversionReport,
) -> Result<Option<Value>, ConversionError> {
    let Some(object) = value.as_object() else {
        report
            .warnings
            .push(format!("Skipped playlist item {index}: expected object."));
        return Ok(None);
    };
    let Some(source) = playlist_item_source(object) else {
        push_unique(
            &mut report.unsupported_features,
            "playlist-item:missing-source",
        );
        report.warnings.push(format!(
            "Skipped playlist item {index}: no source file was found."
        ));
        return Ok(None);
    };
    let source_type = playlist_item_source_type(object, &source);
    record_playlist_item_detected_features(project, source_type, &source, report);
    let id = playlist_item_id(object, index);
    let weight = playlist_item_weight(object).unwrap_or(1);
    let entry = match source_type {
        SourceType::Image => {
            let copied = copy_project_file(
                &project.root,
                &source,
                output_dir.join("assets"),
                &format!("playlist-{index}"),
                report,
            )?;
            push_unique(&mut report.converted_features, "playlist-item:image");
            json!({
                "type": "static-image",
                "source": copied.package_path,
                "fit": "cover",
                "orientation": "from-metadata"
            })
        }
        SourceType::Video => {
            let copied = copy_project_file(
                &project.root,
                &source,
                output_dir.join("assets"),
                &format!("playlist-{index}"),
                report,
            )?;
            push_unique(&mut report.converted_features, "playlist-item:video");
            json!({
                "type": "video",
                "source": copied.package_path,
                "loop": true,
                "muted": true,
                "fit": "cover",
                "max_fps": 60
            })
        }
        SourceType::Web => {
            let index_path =
                convert_playlist_web_item(project, output_dir, index, &source, report)?;
            push_unique(&mut report.converted_features, "playlist-item:web");
            record_web_runtime_gaps(report);
            json!({
                "type": "web",
                "root": format!("assets/playlist-{index}-web"),
                "index": index_path,
                "fallback": playlist_fallback.map(|path| Value::String(path.to_owned())).unwrap_or(Value::Null),
                "max_fps": 30
            })
        }
        SourceType::Scene => {
            let scene_source =
                convert_playlist_scene_item(project, output_dir, index, &source, report)?;
            push_unique(&mut report.converted_features, "playlist-item:scene");
            record_scene_runtime_gaps(report);
            json!({
                "type": "scene",
                "source": scene_source
            })
        }
        SourceType::Shader => {
            let copied = copy_project_file(
                &project.root,
                &source,
                output_dir.join("shaders"),
                &format!("playlist-{index}"),
                report,
            )?;
            push_unique(&mut report.converted_features, "playlist-item:shader");
            record_shader_runtime_gap(report);
            json!({
                "type": "shader",
                "source": copied.package_path,
                "fallback": playlist_fallback.map(|path| Value::String(path.to_owned())).unwrap_or(Value::Null),
                "language": shader_language_for_source(&source),
                "max_fps": 60,
                "uniforms": shader_uniforms(project)
            })
        }
        SourceType::Playlist | SourceType::Application | SourceType::Unknown => {
            let feature = format!("playlist-item:{}", source_type.as_str());
            push_unique(&mut report.unsupported_features, &feature);
            report.warnings.push(format!(
                "Skipped playlist item {id:?}: unsupported source type {}.",
                source_type.as_str()
            ));
            return Ok(None);
        }
    };

    let mut item = json!({
        "id": id,
        "entry": entry,
    });
    if weight != 1
        && let Some(object) = item.as_object_mut()
    {
        object.insert("weight".to_owned(), json!(weight));
    }
    Ok(Some(item))
}

fn convert_playlist_web_item(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    index: usize,
    source: &str,
    report: &mut ConversionReport,
) -> Result<String, ConversionError> {
    let index_path = normalize_relative_path(source)?;
    let source_path = project.root.join(&index_path);
    if !source_path.is_file() {
        return Err(ConversionError::MissingFile(source_path));
    }
    let web_root = output_dir.join(format!("assets/playlist-{index}-web"));
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
        .push(format!("assets/playlist-{index}-web/gilder-bridge.js"));
    Ok(path_to_package_string(&index_path))
}

fn convert_playlist_scene_item(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    index: usize,
    source: &str,
    report: &mut ConversionReport,
) -> Result<String, ConversionError> {
    let original_scene = copy_project_file(
        &project.root,
        source,
        output_dir.join("metadata"),
        &format!("playlist-{index}-source-scene"),
        report,
    )?;
    let source_scene = read_wallpaper_engine_scene_metadata(project, source, report);
    let scene_source = write_scene_document_to(
        project,
        output_dir,
        source,
        &original_scene.package_path,
        source_scene.as_ref(),
        &format!("assets/playlist-{index}-scene.gscene.json"),
        report,
    )?;
    report.warnings.push(format!(
        "Converted playlist Scene item {index} to a first-class Gilder scene document; original scene metadata was preserved at {}.",
        original_scene.package_path
    ));
    record_full_scene_runtime_boundary(report, Some(&original_scene.package_path));
    Ok(scene_source)
}

fn record_playlist_item_detected_features(
    project: &WallpaperEngineProject,
    source_type: SourceType,
    source: &str,
    report: &mut ConversionReport,
) {
    push_unique(
        &mut report.detected_features,
        &format!("playlist-item:{}", source_type.as_str()),
    );
    let mut features = BTreeSet::new();
    collect_feature_hints_from_entry(source_type, &project.root, source, &mut features);
    for feature in features {
        push_unique(&mut report.detected_features, &feature);
    }
}

fn playlist_items_from_project(object: &Map<String, Value>) -> Option<&Vec<Value>> {
    for key in ["items", "playlist", "wallpapers", "entries", "children"] {
        if let Some(items) = object
            .get(key)
            .and_then(Value::as_array)
            .filter(|items| items.iter().any(playlist_item_value_has_source))
        {
            return Some(items);
        }
    }
    object
        .get("collection")
        .and_then(Value::as_object)
        .and_then(playlist_items_from_project)
}

fn playlist_item_value_has_source(value: &Value) -> bool {
    value.as_object().and_then(playlist_item_source).is_some()
}

fn playlist_item_source(object: &Map<String, Value>) -> Option<String> {
    string_field(
        object,
        &[
            "file", "source", "path", "entry", "main", "index", "content",
        ],
    )
}

fn playlist_item_source_type(object: &Map<String, Value>, source: &str) -> SourceType {
    let source_type = string_field(object, &["type", "wallpaperType", "contentType"])
        .map(|value| {
            let lowered = value.to_ascii_lowercase();
            if lowered.contains("application") || lowered.contains("exe") {
                SourceType::Application
            } else if lowered.contains("playlist") || lowered.contains("collection") {
                SourceType::Playlist
            } else if lowered.contains("video") {
                SourceType::Video
            } else if lowered.contains("web") {
                SourceType::Web
            } else if lowered.contains("shader") {
                SourceType::Shader
            } else if lowered.contains("scene") {
                SourceType::Scene
            } else if lowered.contains("image") {
                SourceType::Image
            } else {
                SourceType::Unknown
            }
        })
        .unwrap_or(SourceType::Unknown);
    if source_type != SourceType::Unknown {
        source_type
    } else {
        Path::new(source)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(SourceType::from_extension)
            .unwrap_or(SourceType::Unknown)
    }
}

fn playlist_item_id(object: &Map<String, Value>, index: usize) -> String {
    string_field(object, &["id", "title", "name"])
        .map(|value| slug_id(&value))
        .filter(|value| !value.is_empty())
        .map(|value| format!("item-{index}-{value}"))
        .unwrap_or_else(|| format!("item-{index}"))
}

fn playlist_item_weight(object: &Map<String, Value>) -> Option<u32> {
    let weight = number_field(object, &["weight", "probability", "chance"])?;
    if !weight.is_finite() || weight <= 0.0 {
        return None;
    }
    let rounded = weight.round().clamp(1.0, u32::MAX as f64);
    Some(rounded as u32)
}

fn slug_id(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn read_wallpaper_engine_scene_metadata(
    project: &WallpaperEngineProject,
    source: &str,
    report: &mut ConversionReport,
) -> Option<Value> {
    let relative = match normalize_relative_path(source) {
        Ok(relative) => relative,
        Err(err) => {
            push_unique(&mut report.unsupported_features, "scene-source-path");
            report.warnings.push(format!(
                "Skipped scene metadata scan for {source:?}: {err}."
            ));
            return None;
        }
    };
    let path = project.root.join(relative);
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) => {
            push_unique(&mut report.unsupported_features, "scene-source-read");
            report.warnings.push(format!(
                "Skipped scene metadata scan for {}: {err}.",
                path.display()
            ));
            return None;
        }
    };
    match serde_json::from_str::<Value>(&contents) {
        Ok(value) => Some(value),
        Err(err) => {
            push_unique(&mut report.unsupported_features, "scene-source-json");
            report.warnings.push(format!(
                "Scene metadata {} is preserved but was not parsed as JSON: {err}.",
                path.display()
            ));
            None
        }
    }
}

fn write_scene_document(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    source_entry: &str,
    source_metadata: &str,
    source_scene: Option<&Value>,
    report: &mut ConversionReport,
) -> Result<String, ConversionError> {
    write_scene_document_to(
        project,
        output_dir,
        source_entry,
        source_metadata,
        source_scene,
        "assets/scene.gscene.json",
        report,
    )
}

fn write_scene_document_to(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    source_entry: &str,
    source_metadata: &str,
    source_scene: Option<&Value>,
    package_path: &str,
    report: &mut ConversionReport,
) -> Result<String, ConversionError> {
    let scene_path = output_dir.join(package_path);
    if let Some(parent) = scene_path.parent() {
        fs::create_dir_all(parent).map_err(ConversionError::CreateDir)?;
    }

    let viewport_extent = scene_document_extent(source_scene);
    let mut context = SceneDocumentBuildContext {
        resource_scope: scene_resource_scope(package_path),
        viewport_width: viewport_extent.map(|(width, _)| width),
        viewport_height: viewport_extent.map(|(_, height)| height),
        source_script_count: source_scene
            .map(scene_source_script_count)
            .unwrap_or_default(),
        project_property_defaults: scene_project_property_defaults(project),
        model_blend_opacity_defaults: scene_model_blend_opacity_defaults(source_scene),
        ..SceneDocumentBuildContext::default()
    };
    let mut resources = Vec::new();
    if let Some(scene) = source_scene {
        scene_collect_puppet_attachment_maps_from_value(project, scene, &mut context);
    }
    let mut nodes = source_scene
        .map(|scene| {
            collect_scene_nodes_from_value(
                project,
                output_dir,
                scene,
                report,
                &mut context,
                &mut resources,
            )
        })
        .unwrap_or_default();
    if let Some(scene) = source_scene {
        scene_collect_audio_controllers(scene, &mut context);
        scene_collect_root_timelines(scene, &mut context);
    }
    for feature in &context.converted_features {
        push_unique(&mut report.converted_features, feature);
    }
    if !context.timelines.is_empty() {
        push_unique(&mut report.converted_features, "scene-keyframe-timeline");
    }
    nodes = scene_rebuild_parent_graph(nodes);
    scene_lower_we_image_mesh_uvs(&mut nodes, &mut context);
    scene_lower_pending_controllers(&mut nodes, &mut context);
    scene_lower_pending_audio_controllers(&mut nodes, &mut context);
    if context.all_detected_scripts_native_lowered() {
        push_unique(
            &mut context.converted_features,
            "wallpaper-engine-all-detected-scenescript-native-lowering",
        );
    }
    if !context.timelines.is_empty() {
        push_unique(&mut report.converted_features, "scene-keyframe-timeline");
    }
    for feature in &context.converted_features {
        push_unique(&mut report.converted_features, feature);
    }
    if nodes.is_empty() {
        scene_push_unsupported(
            &mut context,
            "empty-scene-graph",
            "Wallpaper Engine scene conversion produced no native gscene nodes; preview images remain package metadata and are not used as a scene runtime fallback.",
            Some(source_entry),
        );
    }
    let video_visibility = scene_video_visibility_counts(&nodes);
    let mut full_scene_status = scene_full_scene_status(report, &context, video_visibility);
    if let Some(previous) = &report.full_scene {
        for source_scene_metadata in &previous.source_scene_metadata {
            push_unique(
                &mut full_scene_status.source_scene_metadata,
                source_scene_metadata,
            );
        }
    }
    push_unique(
        &mut full_scene_status.source_scene_metadata,
        source_metadata,
    );
    let native_lowering = scene_native_lowering_from_status(&full_scene_status);
    report.full_scene = Some(full_scene_status);
    let properties = convert_properties(project, report);

    let document = json!({
        "version": 1,
        "profile": "native-vulkan-full-scene",
        "source": {
            "format": "wallpaper-engine-scene",
            "metadata": source_metadata,
            "entry": source_entry
        },
        "size": scene_document_size(source_scene),
        "render": scene_render_settings(source_scene),
        "camera": scene_camera_settings(source_scene),
        "import": scene_import_metadata(source_scene),
        "properties": properties,
        "resources": resources,
        "nodes": nodes,
        "timelines": context.timelines,
        "property_bindings": context.property_bindings,
        "systems": scene_system_statuses(report, &context),
        "native_lowering": native_lowering,
        "unsupported_features": scene_unsupported_features(report, context.unsupported_features)
    });
    fs::write(
        &scene_path,
        serde_json::to_vec_pretty(&document).map_err(ConversionError::Serialize)?,
    )
    .map_err(ConversionError::WriteFile)?;
    let package_path = path_to_package_string(Path::new(package_path));
    report.generated_assets.push(package_path.clone());
    Ok(package_path)
}

fn scene_document_size(source_scene: Option<&Value>) -> Value {
    let Some((width, height)) = scene_document_extent(source_scene) else {
        return Value::Null;
    };
    json!({ "width": width as u32, "height": height as u32 })
}

fn scene_document_extent(source_scene: Option<&Value>) -> Option<(f64, f64)> {
    let Some(general) = source_scene
        .and_then(|scene| scene.get("general"))
        .and_then(Value::as_object)
    else {
        return None;
    };
    let Some(projection) = general
        .get("orthogonalprojection")
        .and_then(Value::as_object)
    else {
        return None;
    };
    let width = projection.get("width").and_then(value_to_u32);
    let height = projection.get("height").and_then(value_to_u32);
    match (width, height) {
        (Some(width), Some(height)) if width > 0 && height > 0 => {
            Some((f64::from(width), f64::from(height)))
        }
        _ => None,
    }
}

fn scene_render_settings(source_scene: Option<&Value>) -> Value {
    let Some(general) = source_scene
        .and_then(|scene| scene.get("general"))
        .and_then(Value::as_object)
    else {
        return json!({});
    };
    let mut render = Map::new();
    if let Some(clear_color) = general.get("clearcolor").and_then(scene_color_from_value) {
        render.insert("clear_color".to_owned(), Value::String(clear_color));
    }
    if let Some(clear_enabled) = general.get("clearenabled").and_then(value_to_bool) {
        render.insert("clear_enabled".to_owned(), Value::Bool(clear_enabled));
    }
    if let Some(ambient_color) = general.get("ambientcolor").and_then(scene_color_from_value) {
        render.insert("ambient_color".to_owned(), Value::String(ambient_color));
    }
    if let Some(hdr) = general.get("hdr").and_then(value_to_bool) {
        render.insert("hdr".to_owned(), Value::Bool(hdr));
    }
    let bloom = scene_bloom_settings(general);
    if !bloom.is_null() {
        render.insert("bloom".to_owned(), bloom);
    }
    let parallax = scene_parallax_settings(general);
    if !parallax.is_null() {
        render.insert("parallax".to_owned(), parallax);
    }
    let environment = scene_environment_settings(general);
    if !environment.is_empty() {
        render.insert("environment".to_owned(), Value::Object(environment));
    }
    Value::Object(render)
}

fn scene_bloom_settings(general: &Map<String, Value>) -> Value {
    let mut bloom = Map::new();
    for (source, target) in [
        ("bloomstrength", "strength"),
        ("bloomthreshold", "threshold"),
        ("bloomhdrstrength", "hdr_strength"),
        ("bloomhdrthreshold", "hdr_threshold"),
    ] {
        if let Some(value) = general.get(source).and_then(value_to_f64) {
            bloom.insert(target.to_owned(), json!(value));
        }
    }
    if let Some(tint) = general.get("bloomtint").and_then(scene_color_from_value) {
        bloom.insert("tint".to_owned(), Value::String(tint));
    }
    if bloom.is_empty() {
        Value::Null
    } else {
        Value::Object(bloom)
    }
}

fn scene_parallax_settings(general: &Map<String, Value>) -> Value {
    let mut parallax = Map::new();
    if let Some(value) = general.get("cameraparallaxamount").and_then(value_to_f64) {
        parallax.insert("amount".to_owned(), json!(value));
    }
    if let Some(value) = general.get("cameraparallaxdelay").and_then(value_to_f64) {
        parallax.insert("delay".to_owned(), json!(value));
    }
    if let Some(value) = general.get("cameraparallaxmouseinfluence") {
        parallax.insert("mouse_influence".to_owned(), value.clone());
    }
    if parallax.is_empty() {
        Value::Null
    } else {
        Value::Object(parallax)
    }
}

fn scene_environment_settings(general: &Map<String, Value>) -> Map<String, Value> {
    let mut environment = Map::new();
    for key in [
        "skylightcolor",
        "gravitydirection",
        "gravitystrength",
        "winddirection",
        "windenabled",
        "windstrength",
        "lightconfig",
    ] {
        if let Some(value) = general.get(key) {
            environment.insert(key.to_owned(), value.clone());
        }
    }
    environment
}

fn scene_camera_settings(source_scene: Option<&Value>) -> Value {
    let camera = source_scene
        .and_then(|scene| scene.get("camera"))
        .and_then(Value::as_object);
    let general = source_scene
        .and_then(|scene| scene.get("general"))
        .and_then(Value::as_object);
    let mut result = Map::new();
    if let Some(camera) = camera {
        for key in ["center", "eye", "up"] {
            if let Some(vector) = camera.get(key).and_then(scene_vector3_from_value) {
                result.insert(key.to_owned(), vector);
            }
        }
    }
    if let Some(general) = general {
        for (source, target) in [
            ("nearz", "near_z"),
            ("farz", "far_z"),
            ("fov", "fov"),
            ("zoom", "zoom"),
        ] {
            if let Some(value) = general.get(source).and_then(value_to_f64) {
                result.insert(target.to_owned(), json!(value));
            }
        }
    }
    Value::Object(result)
}

fn scene_import_metadata(source_scene: Option<&Value>) -> Value {
    let object_count = source_scene
        .and_then(|scene| scene.get("objects"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    let mut model_object_count = 0usize;
    let mut audio_object_count = 0usize;
    let mut particle_object_count = 0usize;
    let mut effect_count = 0usize;
    if let Some(objects) = source_scene
        .and_then(|scene| scene.get("objects"))
        .and_then(Value::as_array)
    {
        for object in objects.iter().filter_map(Value::as_object) {
            if object.get("image").is_some() {
                model_object_count += 1;
            }
            if !scene_sound_sources_from_object(object).is_empty() {
                audio_object_count += 1;
            }
            if object.get("particle").is_some()
                || string_field(object, &["type", "class", "kind"]).is_some_and(|kind| {
                    let kind = kind.to_ascii_lowercase();
                    kind.contains("particle") || kind.contains("emitter")
                })
            {
                particle_object_count += 1;
            }
            effect_count += object
                .get("effects")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or_default();
        }
    }
    let mut feature_counts = Map::new();
    feature_counts.insert("model".to_owned(), json!(model_object_count));
    feature_counts.insert("audio".to_owned(), json!(audio_object_count));
    feature_counts.insert("particle".to_owned(), json!(particle_object_count));
    feature_counts.insert("effect".to_owned(), json!(effect_count));
    json!({
        "source_format": "wallpaper-engine-scene",
        "source_version": source_scene.and_then(|scene| scene.get("version")).and_then(Value::as_i64),
        "object_count": object_count,
        "feature_counts": feature_counts
    })
}

#[derive(Debug, Default)]
struct SceneDocumentBuildContext {
    next_node: usize,
    next_resource: usize,
    next_timeline: usize,
    resource_scope: String,
    viewport_width: Option<f64>,
    viewport_height: Option<f64>,
    source_script_count: usize,
    native_script_lowering_count: usize,
    source_node_ids: BTreeMap<String, String>,
    pending_controllers: Vec<SceneControllerIr>,
    pending_audio_controllers: Vec<SceneAudioControllerIr>,
    timelines: Vec<Value>,
    property_bindings: Vec<Value>,
    project_property_defaults: BTreeMap<String, Value>,
    model_blend_opacity_defaults: BTreeMap<String, f64>,
    converted_features: Vec<String>,
    unsupported_features: Vec<Value>,
    puppet_attachments_by_source_id: BTreeMap<String, ScenePuppetAttachmentMap>,
    puppet_attachments_by_model_path: BTreeMap<String, ScenePuppetAttachmentMap>,
    copied_puppet_mdl_ids: BTreeMap<String, String>,
    decoded_tex_resources: BTreeMap<SceneDecodedTexResourceKey, SceneDecodedTexResource>,
    builtin_particle_texture_resources: BTreeMap<String, String>,
}

impl SceneDocumentBuildContext {
    fn all_detected_scripts_native_lowered(&self) -> bool {
        self.source_script_count > 0
            && self.native_script_lowering_count >= self.source_script_count
    }
}

fn scene_record_native_script_lowering(context: &mut SceneDocumentBuildContext) {
    context.native_script_lowering_count = context.native_script_lowering_count.saturating_add(1);
}

fn scene_source_script_count(value: &Value) -> usize {
    match value {
        Value::Object(object) => {
            usize::from(object.get("script").and_then(Value::as_str).is_some())
                + object
                    .values()
                    .map(scene_source_script_count)
                    .sum::<usize>()
        }
        Value::Array(values) => values.iter().map(scene_source_script_count).sum(),
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null => 0,
    }
}

#[derive(Debug, Clone)]
struct SceneSourceModelConversion {
    value: Value,
    render_kind: Option<&'static str>,
    render_resource: Option<String>,
    render_properties: Option<Value>,
    render_size: Option<SceneWeModelFrameSize>,
    render_bounds: Option<ScenePuppetMeshBounds>,
    render_mesh: Option<Value>,
    original_path: String,
}

#[derive(Debug, Clone)]
struct ScenePuppetAttachmentMap {
    attachments: BTreeMap<String, ScenePuppetAttachment>,
    mesh_bounds: Option<ScenePuppetMeshBounds>,
    mesh: Option<ScenePuppetMesh>,
}

impl ScenePuppetAttachmentMap {
    fn to_value(&self) -> Value {
        let mut attachments = Map::new();
        for (name, attachment) in &self.attachments {
            let mut value = Map::new();
            value.insert("bone_index".to_owned(), json!(attachment.bone_index));
            value.insert("x".to_owned(), json!(attachment.x));
            value.insert("y".to_owned(), json!(attachment.y));
            value.insert("z".to_owned(), json!(attachment.z));
            value.insert(
                "local_position".to_owned(),
                json!(attachment.local_position),
            );
            value.insert(
                "placement_source".to_owned(),
                json!(attachment.placement_source),
            );
            if let Some((x, y, z)) = attachment.target_position {
                value.insert("target_x".to_owned(), json!(x));
                value.insert("target_y".to_owned(), json!(y));
                value.insert("target_z".to_owned(), json!(z));
            }
            attachments.insert(name.clone(), Value::Object(value));
        }
        Value::Object(attachments)
    }
}

#[derive(Debug, Clone, Copy)]
struct ScenePuppetMeshBounds {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    anchor_x: f64,
    anchor_y: f64,
}

#[derive(Debug, Clone)]
struct ScenePuppetMesh {
    bounds: ScenePuppetMeshBounds,
    vertices: Vec<ScenePuppetMeshVertex>,
    indices: Vec<u32>,
    skin_vertices: Vec<ScenePuppetSkinVertex>,
    skin: Option<ScenePuppetSkin>,
    clips: Vec<ScenePuppetAnimationClip>,
}

impl ScenePuppetMesh {
    fn to_scene_mesh_value(&self) -> Value {
        let mut mesh = Map::new();
        mesh.insert(
            "vertices".to_owned(),
            Value::Array(
                self.vertices
                    .iter()
                    .map(ScenePuppetMeshVertex::to_value)
                    .collect(),
            ),
        );
        mesh.insert(
            "indices".to_owned(),
            Value::Array(self.indices.iter().map(|index| json!(index)).collect()),
        );
        if let Some(skin) = &self.skin {
            mesh.insert("skin".to_owned(), skin.to_value());
        }
        if !self.clips.is_empty() {
            mesh.insert(
                "puppet_clips".to_owned(),
                Value::Array(
                    self.clips
                        .iter()
                        .map(ScenePuppetAnimationClip::to_value)
                        .collect(),
                ),
            );
        }
        Value::Object(mesh)
    }
}

#[derive(Debug, Clone)]
struct ScenePuppetSkin {
    bones: Vec<ScenePuppetSkinBone>,
    vertices: Vec<ScenePuppetSkinVertex>,
    attachments: Vec<ScenePuppetSkinAttachment>,
}

impl ScenePuppetSkin {
    fn to_value(&self) -> Value {
        let mut skin = Map::new();
        skin.insert(
            "bones".to_owned(),
            Value::Array(
                self.bones
                    .iter()
                    .map(|bone| bone.to_value())
                    .collect::<Vec<_>>(),
            ),
        );
        skin.insert(
            "vertices".to_owned(),
            Value::Array(
                self.vertices
                    .iter()
                    .map(|vertex| vertex.to_value())
                    .collect::<Vec<_>>(),
            ),
        );
        if !self.attachments.is_empty() {
            skin.insert(
                "attachments".to_owned(),
                Value::Array(
                    self.attachments
                        .iter()
                        .map(|attachment| attachment.to_value())
                        .collect(),
                ),
            );
        }
        Value::Object(skin)
    }
}

#[derive(Debug, Clone)]
struct ScenePuppetSkinAttachment {
    name: String,
    bone_index: usize,
    local_position: [f64; 3],
    bind_position: [f64; 3],
}

impl ScenePuppetSkinAttachment {
    fn to_value(&self) -> Value {
        json!({
            "name": self.name,
            "bone_index": self.bone_index,
            "local_position": self.local_position,
            "bind_position": self.bind_position
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct ScenePuppetSkinBone {
    parent: Option<usize>,
    bind: ScenePuppetTransform,
    inverse_bind: [f64; 16],
}

impl ScenePuppetSkinBone {
    fn to_value(self) -> Value {
        json!({
            "parent": self.parent,
            "bind": self.bind.to_value(),
            "inverse_bind": self.inverse_bind
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct ScenePuppetSkinVertex {
    bone_indices: [usize; 4],
    weights: [f64; 4],
}

impl ScenePuppetSkinVertex {
    fn to_value(self) -> Value {
        json!({
            "bone_indices": self.bone_indices,
            "weights": self.weights
        })
    }
}

#[derive(Debug, Clone)]
struct ScenePuppetAnimationClip {
    id: u32,
    name: Option<String>,
    fps: f64,
    frame_count: u32,
    looping: bool,
    bones: Vec<ScenePuppetAnimationBone>,
}

impl ScenePuppetAnimationClip {
    fn to_value(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "fps": self.fps,
            "frame_count": self.frame_count,
            "looping": self.looping,
            "bones": self
                .bones
                .iter()
                .map(ScenePuppetAnimationBone::to_value)
                .collect::<Vec<_>>()
        })
    }

    fn summary_value(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "fps": self.fps,
            "frame_count": self.frame_count,
            "looping": self.looping,
            "bone_count": self.bones.len()
        })
    }
}

#[derive(Debug, Clone)]
struct ScenePuppetAnimationBone {
    frames: Vec<ScenePuppetTransform>,
}

impl ScenePuppetAnimationBone {
    fn to_value(&self) -> Value {
        json!({
            "frames": self
                .frames
                .iter()
                .map(|frame| frame.to_value())
                .collect::<Vec<_>>()
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct ScenePuppetTransform {
    translation: [f64; 3],
    rotation: [f64; 3],
    scale: [f64; 3],
    opacity: f64,
}

impl ScenePuppetTransform {
    fn identity() -> Self {
        Self {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
            opacity: 1.0,
        }
    }

    fn to_value(self) -> Value {
        let mut value = json!({
            "translation": self.translation,
            "rotation": self.rotation,
            "scale": self.scale
        });
        if (self.opacity - 1.0).abs() > 0.000_01
            && let Some(object) = value.as_object_mut()
        {
            object.insert("opacity".to_owned(), json!(self.opacity));
        }
        value
    }

    fn matrix(self) -> [f64; 16] {
        let rx = scene_puppet_rotation_x_matrix(self.rotation[0]);
        let ry = scene_puppet_rotation_y_matrix(self.rotation[1]);
        let rz = scene_puppet_rotation_z_matrix(self.rotation[2]);
        let rotation = scene_puppet_matrix_mul(scene_puppet_matrix_mul(rz, ry), rx);
        let scale = scene_puppet_scale_matrix(self.scale);
        let translation = scene_puppet_translation_matrix(self.translation);
        scene_puppet_matrix_mul(translation, scene_puppet_matrix_mul(rotation, scale))
    }
}

impl Default for ScenePuppetTransform {
    fn default() -> Self {
        Self::identity()
    }
}

#[derive(Debug, Clone, Copy)]
struct ScenePuppetMeshVertex {
    x: f64,
    y: f64,
    u: f64,
    v: f64,
    opacity: f64,
}

impl ScenePuppetMeshVertex {
    fn to_value(&self) -> Value {
        let mut value = json!({
            "x": self.x,
            "y": self.y,
            "u": self.u,
            "v": self.v
        });
        if (self.opacity - 1.0).abs() > 0.000_01
            && let Some(object) = value.as_object_mut()
        {
            object.insert("opacity".to_owned(), json!(self.opacity));
        }
        value
    }
}

impl ScenePuppetMeshBounds {
    fn to_value(self) -> Value {
        json!({
            "left": self.left,
            "top": self.top,
            "width": self.width,
            "height": self.height,
            "anchor_x": self.anchor_x,
            "anchor_y": self.anchor_y
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct ScenePuppetAttachment {
    bone_index: usize,
    x: f64,
    y: f64,
    z: f64,
    local_position: [f64; 3],
    placement_source: &'static str,
    target_position: Option<(f64, f64, f64)>,
}

impl ScenePuppetAttachment {
    fn skin_attachment(&self, name: String) -> ScenePuppetSkinAttachment {
        ScenePuppetSkinAttachment {
            name,
            bone_index: self.bone_index,
            local_position: self.local_position,
            bind_position: [self.x, self.y, self.z],
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ScenePuppetBone {
    parent: Option<usize>,
    translation: (f64, f64, f64),
    target_position: Option<(f64, f64, f64)>,
    bind: ScenePuppetTransform,
}

impl ScenePuppetBone {
    fn skin_bone(&self, inverse_bind: [f64; 16]) -> ScenePuppetSkinBone {
        ScenePuppetSkinBone {
            parent: self.parent,
            bind: self.bind,
            inverse_bind,
        }
    }
}

#[derive(Debug, Clone)]
struct SceneParticleConversion {
    properties: Value,
    render_resource: Option<String>,
    render_properties: Option<Value>,
}

#[derive(Debug, Clone)]
struct SceneDecodedTexResource {
    resource_id: String,
    render_kind: &'static str,
    spritesheet: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SceneDecodedTexResourceKey {
    source_path: String,
    frame_width: Option<u32>,
    frame_height: Option<u32>,
    spritesheet_enabled: bool,
}

impl SceneDecodedTexResourceKey {
    fn new(
        source_path: &Path,
        frame_size: Option<SceneWeModelFrameSize>,
        spritesheet_enabled: bool,
    ) -> Self {
        Self {
            source_path: path_to_package_string(source_path),
            frame_width: frame_size.map(|frame| frame.width),
            frame_height: frame_size.map(|frame| frame.height),
            spritesheet_enabled,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct SceneWeTexFrameLayout {
    frame_width: u32,
    frame_height: u32,
    columns: u32,
    rows: u32,
    frame_count: u32,
}

#[derive(Debug, Clone, Copy, Default)]
struct SceneVisibleConversion {
    static_visible: Option<bool>,
    initial_opacity: Option<f64>,
}

fn collect_scene_nodes_from_value(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    value: &Value,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
    resources: &mut Vec<Value>,
) -> Vec<Value> {
    match value {
        Value::Array(values) => values
            .iter()
            .flat_map(|value| {
                collect_scene_nodes_from_value(
                    project, output_dir, value, report, context, resources,
                )
            })
            .collect(),
        Value::Object(object) if scene_object_has_node_signal(object) => {
            vec![scene_node_from_object(
                project, output_dir, object, report, context, resources,
            )]
        }
        Value::Object(object) => object
            .iter()
            .filter(|(key, _)| scene_container_key(key))
            .flat_map(|(_, value)| {
                collect_scene_nodes_from_value(
                    project, output_dir, value, report, context, resources,
                )
            })
            .collect(),
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null => Vec::new(),
    }
}

fn scene_collect_puppet_attachment_maps_from_value(
    project: &WallpaperEngineProject,
    value: &Value,
    context: &mut SceneDocumentBuildContext,
) {
    match value {
        Value::Array(values) => {
            for value in values {
                scene_collect_puppet_attachment_maps_from_value(project, value, context);
            }
        }
        Value::Object(object) => {
            scene_collect_puppet_attachment_map_from_object(project, object, context);
            for (_, value) in object.iter().filter(|(key, _)| scene_container_key(key)) {
                scene_collect_puppet_attachment_maps_from_value(project, value, context);
            }
        }
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null => {}
    }
}

fn scene_collect_puppet_attachment_map_from_object(
    project: &WallpaperEngineProject,
    object: &Map<String, Value>,
    context: &mut SceneDocumentBuildContext,
) {
    let Some(source_id) = object.get("id").and_then(value_to_string) else {
        return;
    };
    let Some(model_path) = scene_model_path_from_object(object) else {
        return;
    };
    let Some(frame_size) = scene_frame_size_from_object_size(object) else {
        return;
    };
    let Some(attachments) =
        scene_puppet_attachment_map_for_model_path(project, &model_path, frame_size, context)
    else {
        return;
    };
    context
        .puppet_attachments_by_source_id
        .insert(source_id, attachments);
}

fn scene_rebuild_parent_graph(nodes: Vec<Value>) -> Vec<Value> {
    if nodes.len() < 2 {
        return nodes;
    }
    let source_ids = nodes
        .iter()
        .filter_map(scene_node_source_id)
        .collect::<BTreeSet<_>>();
    if source_ids.is_empty()
        || nodes.iter().all(|node| {
            scene_node_parent_id(node).is_some_and(|parent| source_ids.contains(&parent))
        })
    {
        return nodes;
    }

    let mut roots = Vec::new();
    let mut children_by_parent = BTreeMap::<String, Vec<Value>>::new();
    for node in nodes {
        if let Some(parent_id) = scene_node_parent_id(&node)
            && source_ids.contains(&parent_id)
        {
            children_by_parent.entry(parent_id).or_default().push(node);
        } else {
            roots.push(node);
        }
    }

    let mut rebuilt = roots
        .into_iter()
        .map(|node| scene_attach_parented_children(node, &mut children_by_parent))
        .collect::<Vec<_>>();
    for (_, children) in children_by_parent {
        rebuilt.extend(children);
    }
    rebuilt
}

fn scene_attach_parented_children(
    mut node: Value,
    children_by_parent: &mut BTreeMap<String, Vec<Value>>,
) -> Value {
    let Some(source_id) = scene_node_source_id(&node) else {
        return node;
    };
    let Some(children) = children_by_parent.remove(&source_id) else {
        return node;
    };
    let children = children
        .into_iter()
        .map(|child| scene_attach_parented_children(child, children_by_parent))
        .collect::<Vec<_>>();
    if let Some(object) = node.as_object_mut() {
        match object.get_mut("children").and_then(Value::as_array_mut) {
            Some(existing) => existing.extend(children),
            None => {
                object.insert("children".to_owned(), Value::Array(children));
            }
        }
    }
    node
}

fn scene_lower_we_image_mesh_uvs(nodes: &mut [Value], context: &mut SceneDocumentBuildContext) {
    for node in nodes {
        scene_lower_we_image_mesh_uv(node, false, context);
    }
}

fn scene_lower_we_image_mesh_uv(
    node: &mut Value,
    parent_is_attachment_group: bool,
    context: &mut SceneDocumentBuildContext,
) {
    let Some(object) = node.as_object_mut() else {
        return;
    };
    let is_we_model_image = scene_node_is_we_model_image(object);
    if (parent_is_attachment_group || is_we_model_image) && scene_insert_we_image_quad_mesh(object)
    {
        if parent_is_attachment_group {
            push_unique(
                &mut context.converted_features,
                "wallpaper-engine-attachment-child-image-uv-y-flip-lowering",
            );
        }
        if is_we_model_image {
            push_unique(
                &mut context.converted_features,
                "wallpaper-engine-model-image-uv-y-flip-lowering",
            );
        }
    }
    let is_attachment_group = scene_node_is_empty_attachment_group(object);
    if let Some(children) = object.get_mut("children").and_then(Value::as_array_mut) {
        for child in children {
            scene_lower_we_image_mesh_uv(child, is_attachment_group, context);
        }
    }
}

fn scene_node_is_we_model_image(object: &Map<String, Value>) -> bool {
    object.get("type").and_then(Value::as_str) == Some("image")
        && object.get("resource").is_some()
        && object.get("mesh").is_none()
        && object
            .get("provenance")
            .and_then(Value::as_object)
            .is_some_and(|provenance| {
                provenance.get("source_format").and_then(Value::as_str)
                    == Some("wallpaper-engine-scene")
                    && provenance
                        .get("model")
                        .and_then(Value::as_object)
                        .is_some_and(|model| {
                            model
                                .get("texture_resources")
                                .and_then(Value::as_array)
                                .is_some_and(|resources| !resources.is_empty())
                        })
            })
}

fn scene_insert_we_image_quad_mesh(object: &mut Map<String, Value>) -> bool {
    if object.get("type").and_then(Value::as_str) != Some("image")
        || object.get("mesh").is_some()
        || object.get("resource").is_none()
    {
        return false;
    }
    let Some(width) = object.get("width").and_then(Value::as_f64) else {
        return false;
    };
    let Some(height) = object.get("height").and_then(Value::as_f64) else {
        return false;
    };
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return false;
    }
    let half_width = width * 0.5;
    let half_height = height * 0.5;
    object.insert(
        "mesh".to_owned(),
        json!({
            "vertices": [
                { "x": -half_width, "y": -half_height, "u": 0.0, "v": 0.0 },
                { "x": half_width, "y": -half_height, "u": 1.0, "v": 0.0 },
                { "x": -half_width, "y": half_height, "u": 0.0, "v": 1.0 },
                { "x": half_width, "y": half_height, "u": 1.0, "v": 1.0 }
            ],
            "indices": [0, 1, 2, 2, 1, 3]
        }),
    );
    true
}

fn scene_node_is_empty_attachment_group(object: &Map<String, Value>) -> bool {
    object.get("type").and_then(Value::as_str) == Some("group")
        && object
            .get("provenance")
            .and_then(Value::as_object)
            .and_then(|provenance| provenance.get("attachment"))
            .and_then(Value::as_str)
            .is_some()
        && object
            .get("provenance")
            .and_then(Value::as_object)
            .is_none_or(|provenance| !provenance.contains_key("model"))
}

fn scene_node_source_id(node: &Value) -> Option<String> {
    node.pointer("/provenance/source_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn scene_node_parent_id(node: &Value) -> Option<String> {
    node.pointer("/provenance/parent_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn scene_collect_root_timelines(source_scene: &Value, context: &mut SceneDocumentBuildContext) {
    let Some(object) = source_scene.as_object() else {
        return;
    };
    for key in [
        "timeline",
        "timelines",
        "animation",
        "animations",
        "keyframes",
    ] {
        if let Some(value) = object.get(key) {
            scene_collect_timeline_entries(value, None, context);
        }
    }
}

fn scene_collect_object_timelines(
    object: &Map<String, Value>,
    node_id: &str,
    context: &mut SceneDocumentBuildContext,
    puppet_animation_layers_lowered: bool,
) {
    for key in [
        "timeline",
        "timelines",
        "animation",
        "animations",
        "keyframes",
    ] {
        if let Some(value) = object.get(key) {
            scene_collect_timeline_entries(value, Some(node_id), context);
        }
    }
    scene_collect_embedded_property_timelines(object, node_id, context);
    if !puppet_animation_layers_lowered
        && let Some(animation_layers) = object.get("animationlayers")
    {
        scene_collect_animation_layer_timelines(animation_layers, node_id, context);
    }
}

fn scene_collect_embedded_property_timelines(
    object: &Map<String, Value>,
    node_id: &str,
    context: &mut SceneDocumentBuildContext,
) {
    for (property, value) in object {
        if !SceneTimelineIr::supports_wallpaper_engine_property(property) {
            continue;
        }
        let Some(timeline) = scene_embedded_property_timeline_value(property, value) else {
            continue;
        };
        let before = context.timelines.len();
        scene_collect_timeline_entries(&timeline, Some(node_id), context);
        if context.timelines.len() > before {
            push_unique(
                &mut context.converted_features,
                "scene-we-embedded-property-timeline",
            );
        }
    }
}

fn scene_embedded_property_timeline_value(property: &str, value: &Value) -> Option<Value> {
    match value {
        Value::Object(object) => {
            if let Some(timeline) =
                scene_embedded_property_component_animation_timeline(property, object)
            {
                return Some(timeline);
            }
            let source = scene_embedded_timeline_source(object)?;
            Some(scene_embedded_property_timeline_object(
                property,
                source.clone(),
                object,
            ))
        }
        Value::Array(values) if values.iter().any(scene_timeline_entry_like_value) => Some(
            scene_embedded_property_timeline_object(property, value.clone(), &Map::new()),
        ),
        Value::Array(_) | Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null => {
            None
        }
    }
}

fn scene_embedded_property_component_animation_timeline(
    property: &str,
    object: &Map<String, Value>,
) -> Option<Value> {
    let animation = object.get("animation")?.as_object()?;
    let options = animation.get("options").and_then(Value::as_object);
    let fps = options
        .and_then(|options| number_value_field(options, &["fps"]))
        .filter(|fps| fps.is_finite() && *fps > 0.0)
        .unwrap_or(30.0);
    let relative = animation
        .get("relative")
        .and_then(value_to_bool_unwrapped)
        .unwrap_or(false);
    let wraploop = animation
        .get("wraploop")
        .and_then(value_to_bool_unwrapped)
        .or_else(|| options.and_then(|options| scene_bool_value_field(options, &["wraploop"])))
        .unwrap_or(false);
    let loop_playback = animation
        .get("loop")
        .and_then(value_to_bool_unwrapped)
        .or_else(|| options.and_then(|options| scene_bool_value_field(options, &["loop"])))
        .unwrap_or_else(|| {
            options
                .and_then(|options| value_field(options, &["mode"]))
                .is_some_and(|mode| normalize_project_key(&mode) == "loop")
                || wraploop
        });
    let length_frame = options
        .and_then(|options| number_value_field(options, &["length", "frames"]))
        .filter(|length| length.is_finite() && *length > 0.0);
    let channels = scene_embedded_property_component_animation_channels(
        property,
        object,
        animation,
        fps,
        relative,
        wraploop,
        length_frame,
    )?;
    let mut timeline = Map::new();
    timeline.insert("channels".to_owned(), Value::Array(channels));
    timeline.insert("loop".to_owned(), json!(loop_playback));
    Some(Value::Object(timeline))
}

fn scene_embedded_property_component_animation_channels(
    property: &str,
    object: &Map<String, Value>,
    animation: &Map<String, Value>,
    fps: f64,
    relative: bool,
    wraploop: bool,
    length_frame: Option<f64>,
) -> Option<Vec<Value>> {
    let targets = scene_embedded_property_component_animation_targets(property, object)?;
    let mut channels = Vec::new();
    for target in targets {
        let Some(entries) = animation
            .get(target.source_channel)
            .and_then(Value::as_array)
            .filter(|entries| !entries.is_empty())
        else {
            continue;
        };
        let mut keyframes = entries
            .iter()
            .filter_map(|entry| {
                scene_embedded_property_component_animation_keyframe(
                    entry,
                    fps,
                    target.value_scale,
                    target.base_value,
                    relative,
                )
            })
            .collect::<Vec<_>>();
        keyframes.sort_by_key(|keyframe| {
            keyframe
                .get("time_ms")
                .and_then(Value::as_u64)
                .unwrap_or_default()
        });
        if wraploop
            && keyframes.len() >= 2
            && let Some(length_ms) =
                length_frame.and_then(|length| scene_frame_to_time_ms(length, fps))
            && keyframes
                .last()
                .and_then(|keyframe| keyframe.get("time_ms"))
                .and_then(Value::as_u64)
                .is_some_and(|last_time_ms| last_time_ms < length_ms)
            && let Some(first_value) = keyframes
                .first()
                .and_then(|keyframe| keyframe.get("value"))
                .cloned()
        {
            keyframes.push(json!({
                "time_ms": length_ms,
                "value": first_value
            }));
        }
        if keyframes.is_empty() {
            continue;
        }
        channels.push(json!({
            "property": target.target_property,
            "keyframes": keyframes
        }));
    }
    (!channels.is_empty()).then_some(channels)
}

#[derive(Debug, Clone, Copy)]
struct SceneEmbeddedComponentAnimationTarget {
    source_channel: &'static str,
    target_property: &'static str,
    base_value: f64,
    value_scale: f64,
}

fn scene_embedded_property_component_animation_targets(
    property: &str,
    object: &Map<String, Value>,
) -> Option<Vec<SceneEmbeddedComponentAnimationTarget>> {
    let normalized = normalize_project_key(property);
    let vector = object
        .get("value")
        .and_then(vector3_components_from_value)
        .unwrap_or((0.0, 0.0, 0.0));
    let scalar = object
        .get("value")
        .and_then(value_to_f64_unwrapped)
        .unwrap_or(0.0);
    let to_degrees = 180.0 / std::f64::consts::PI;
    let target = |source_channel, target_property, base_value, value_scale| {
        SceneEmbeddedComponentAnimationTarget {
            source_channel,
            target_property,
            base_value,
            value_scale,
        }
    };
    match normalized.as_str() {
        "origin" | "position" | "translation" => Some(vec![
            target("c0", "x", vector.0, 1.0),
            target("c1", "y", vector.1, 1.0),
        ]),
        "x" | "left" | "originx" | "positionx" | "translationx" => {
            Some(vec![target("c0", "x", scalar, 1.0)])
        }
        "y" | "top" | "originy" | "positiony" | "translationy" => {
            Some(vec![target("c0", "y", scalar, 1.0)])
        }
        "scale" => Some(vec![
            target("c0", "scale-x", vector.0, 1.0),
            target("c1", "scale-y", vector.1, 1.0),
        ]),
        "scalex" => Some(vec![target("c0", "scale-x", scalar, 1.0)]),
        "scaley" => Some(vec![target("c0", "scale-y", scalar, 1.0)]),
        "opacity" | "alpha" | "visible" | "visibility" => {
            Some(vec![target("c0", "opacity", scalar, 1.0)])
        }
        "angles" => Some(vec![target(
            "c2",
            "rotation-deg",
            vector.2 * to_degrees,
            to_degrees,
        )]),
        "anglesz" => Some(vec![target(
            "c0",
            "rotation-deg",
            scalar * to_degrees,
            to_degrees,
        )]),
        "rotation" | "rotationdeg" | "angle" | "rotationz" => {
            Some(vec![target("c0", "rotation-deg", scalar, 1.0)])
        }
        "width" | "w" | "sizex" => Some(vec![target("c0", "width", scalar, 1.0)]),
        "height" | "h" | "sizey" => Some(vec![target("c0", "height", scalar, 1.0)]),
        "size" | "dimensions" => Some(vec![
            target("c0", "width", vector.0, 1.0),
            target("c1", "height", vector.1, 1.0),
        ]),
        "radius" | "cornerradius" | "borderradius" => {
            Some(vec![target("c0", "corner-radius", scalar, 1.0)])
        }
        _ => None,
    }
}

fn scene_embedded_property_component_animation_keyframe(
    entry: &Value,
    fps: f64,
    value_scale: f64,
    base_value: f64,
    relative: bool,
) -> Option<Value> {
    let object = entry.as_object()?;
    let frame = number_value_field(object, &["frame", "f"])
        .or_else(|| number_value_field(object, &["time", "seconds", "sec"]).map(|time| time * fps))
        .or_else(|| {
            number_value_field(object, &["time_ms", "timeMs", "ms"])
                .map(|time_ms| time_ms * fps / 1000.0)
        })?;
    let raw_value = number_value_field(object, &["value", "val", "v"])?;
    let value = raw_value * value_scale;
    let value = if relative { base_value + value } else { value };
    if !value.is_finite() {
        return None;
    }
    Some(json!({
        "time_ms": scene_frame_to_time_ms(frame, fps)?,
        "value": value
    }))
}

fn scene_frame_to_time_ms(frame: f64, fps: f64) -> Option<u64> {
    if !frame.is_finite() || frame < 0.0 || !fps.is_finite() || fps <= 0.0 {
        return None;
    }
    let time_ms = frame / fps * 1000.0;
    (time_ms.is_finite() && time_ms >= 0.0 && time_ms <= u64::MAX as f64)
        .then_some(time_ms.round() as u64)
}

fn scene_embedded_timeline_source(object: &Map<String, Value>) -> Option<&Value> {
    [
        "keyframes",
        "frames",
        "values",
        "points",
        "timeline",
        "timelines",
        "animation",
        "animations",
    ]
    .iter()
    .filter_map(|key| object.get(*key))
    .next()
}

fn scene_embedded_property_timeline_object(
    property: &str,
    keyframes: Value,
    source: &Map<String, Value>,
) -> Value {
    let mut timeline = Map::new();
    timeline.insert("property".to_owned(), Value::String(property.to_owned()));
    timeline.insert("keyframes".to_owned(), keyframes);
    for key in [
        "loop",
        "repeat",
        "loop_playback",
        "loopPlayback",
        "curve",
        "easing",
        "interpolation",
    ] {
        if let Some(value) = source.get(key) {
            timeline.insert(key.to_owned(), value.clone());
        }
    }
    Value::Object(timeline)
}

fn scene_timeline_entry_like_value(value: &Value) -> bool {
    match value {
        Value::Object(object) => [
            "time_ms",
            "timeMs",
            "timestamp_ms",
            "timestampMs",
            "at_ms",
            "atMs",
            "milliseconds",
            "millis",
            "ms",
            "time_seconds",
            "timeSeconds",
            "seconds",
            "secs",
            "sec",
            "time",
        ]
        .iter()
        .any(|key| object.contains_key(*key)),
        Value::Array(values) => values.len() >= 2 && values.first().is_some_and(value_is_number),
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null => false,
    }
}

fn value_is_number(value: &Value) -> bool {
    value_to_f64_unwrapped(value).is_some()
}

fn scene_collect_animation_layer_timelines(
    value: &Value,
    node_id: &str,
    context: &mut SceneDocumentBuildContext,
) {
    let animation_layer = SceneAnimationLayerIr::from_wallpaper_engine_value(value, node_id);
    let unlowered_layer_count = animation_layer.unlowered_layer_count();
    let rate_scaled_layer_count = animation_layer.rate_scaled_layer_count();
    let native_script_lowering_count = animation_layer.native_script_lowering_count();
    let phase_offset_layer_count = animation_layer.phase_offset_layer_count();
    let mut timeline_count = 0usize;
    for timeline in animation_layer.into_timelines() {
        let timeline_id = scene_next_timeline_id(context, timeline.hint().or(Some(node_id)));
        context.timelines.push(timeline.timeline_value(timeline_id));
        timeline_count += 1;
    }
    if timeline_count > 0 {
        push_unique(
            &mut context.converted_features,
            "scene-we-animation-layer-timeline",
        );
    }
    if rate_scaled_layer_count > 0 {
        push_unique(
            &mut context.converted_features,
            "scene-we-animation-layer-rate-time-scale",
        );
    }
    if timeline_count > 0 {
        for _ in 0..native_script_lowering_count {
            scene_record_native_script_lowering(context);
        }
        if phase_offset_layer_count > 0 {
            push_unique(
                &mut context.converted_features,
                "scene-we-animation-layer-initial-frame-phase",
            );
        }
    }
    if unlowered_layer_count > 0 {
        scene_push_unsupported(
            context,
            "we-animation-layer-blending",
            "Wallpaper Engine animation layer blend/weight references that cannot be represented as direct gscene keyframe channels remain preserved in provenance.",
            None,
        );
    }
}

fn scene_collect_timeline_entries(
    value: &Value,
    default_target_node: Option<&str>,
    context: &mut SceneDocumentBuildContext,
) {
    match value {
        Value::Array(entries) => {
            for entry in entries {
                scene_collect_timeline_entries(entry, default_target_node, context);
            }
        }
        Value::Object(object) => {
            if let Some(timeline) = scene_timeline_from_object(object, default_target_node, context)
            {
                context.timelines.push(timeline);
            }
        }
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null => {}
    }
}

fn scene_timeline_from_object(
    object: &Map<String, Value>,
    default_target_node: Option<&str>,
    context: &mut SceneDocumentBuildContext,
) -> Option<Value> {
    let target_node = scene_timeline_target_node(object, default_target_node, context)?;
    let timeline_id = scene_next_timeline_id(
        context,
        string_field(object, &["timeline_id", "timelineId", "name"])
            .as_deref()
            .or(Some(target_node.as_str())),
    );
    let timeline = SceneTimelineIr::from_wallpaper_engine_object(object, target_node)?;
    Some(timeline.timeline_value(timeline_id))
}

fn scene_timeline_target_node(
    object: &Map<String, Value>,
    default_target_node: Option<&str>,
    context: &SceneDocumentBuildContext,
) -> Option<String> {
    for key in ["target_node", "targetNode"] {
        if let Some(value) = object.get(key).and_then(value_to_string) {
            if let Some(node_id) = scene_timeline_mapped_node_id(&value, context) {
                return Some(node_id);
            }
        }
    }
    for key in [
        "target",
        "target_id",
        "targetId",
        "object",
        "object_id",
        "objectId",
        "node",
        "node_id",
        "nodeId",
    ] {
        let Some(value) = object.get(key).and_then(scene_timeline_target_source_id) else {
            continue;
        };
        if let Some(node_id) = scene_timeline_mapped_node_id(&value, context) {
            return Some(node_id);
        }
    }
    default_target_node.map(str::to_owned)
}

fn scene_timeline_mapped_node_id(
    value: &str,
    context: &SceneDocumentBuildContext,
) -> Option<String> {
    if let Some(node_id) = context.source_node_ids.get(value) {
        return Some(node_id.clone());
    }
    if context
        .source_node_ids
        .values()
        .any(|node_id| node_id == value)
    {
        return Some(value.to_owned());
    }
    None
}

fn scene_timeline_target_source_id(value: &Value) -> Option<String> {
    value_to_string(value).or_else(|| {
        let object = value.as_object()?;
        [
            "id",
            "source_id",
            "sourceId",
            "target",
            "target_id",
            "targetId",
        ]
        .iter()
        .filter_map(|key| object.get(*key))
        .find_map(value_to_string)
    })
}

fn scene_visible_from_object(
    object: &Map<String, Value>,
    node_id: &str,
    context: &mut SceneDocumentBuildContext,
) -> SceneVisibleConversion {
    let Some(value) = object.get("visible") else {
        return SceneVisibleConversion::default();
    };
    if let Some(visible) = value_to_bool(value) {
        return SceneVisibleConversion {
            static_visible: Some(visible),
            initial_opacity: None,
        };
    }
    let Some(binding) = value.as_object() else {
        return SceneVisibleConversion::default();
    };
    let initial_visible = scene_visible_initial_value(binding, context);
    if let Some(property) = scene_visible_bool_property(binding) {
        context.property_bindings.push(json!({
            "property": property,
            "target_node": node_id,
            "target": "opacity",
            "scale": 1.0,
            "offset": 0.0
        }));
        SceneVisibleConversion {
            static_visible: Some(true),
            initial_opacity: Some(if initial_visible { 1.0 } else { 0.0 }),
        }
    } else if scene_visible_user_condition_value(binding, context).is_some() {
        SceneVisibleConversion {
            static_visible: Some(true),
            initial_opacity: None,
        }
    } else {
        SceneVisibleConversion {
            static_visible: Some(initial_visible),
            initial_opacity: None,
        }
    }
}

fn scene_visible_initial_value(
    binding: &Map<String, Value>,
    context: &SceneDocumentBuildContext,
) -> bool {
    let authored_visible = binding.get("value").and_then(value_to_bool).unwrap_or(true);
    let Some(user) = binding.get("user") else {
        return authored_visible;
    };
    if let Some(property) = value_to_string(user) {
        return context
            .project_property_defaults
            .get(&property)
            .and_then(value_to_bool)
            .unwrap_or(authored_visible);
    }
    let Some(user) = user.as_object() else {
        return authored_visible;
    };
    let Some(property) = string_field(user, &["name", "property", "user"]) else {
        return authored_visible;
    };
    let Some(expected) = value_field(user, &["condition", "value", "equals", "eq"]) else {
        return authored_visible;
    };
    let Some(actual) = context
        .project_property_defaults
        .get(&property)
        .and_then(value_to_string_unwrapped)
    else {
        return authored_visible;
    };
    normalize_project_key(&actual) == normalize_project_key(&expected)
}

fn scene_visible_bool_property(binding: &Map<String, Value>) -> Option<String> {
    binding
        .get("user")
        .and_then(value_to_string)
        .or_else(|| binding.get("property").and_then(value_to_string))
}

fn scene_visible_user_condition_value(
    binding: &Map<String, Value>,
    context: &SceneDocumentBuildContext,
) -> Option<Value> {
    let user = binding.get("user")?.as_object()?;
    let property = string_field(user, &["name", "property", "user"])?;
    let condition = value_field(user, &["condition", "value", "equals", "eq"])?;
    let default_visible = scene_visible_initial_value(binding, context);
    Some(json!({
        "runtime": "wallpaper-engine-user-condition",
        "property": property,
        "condition": condition,
        "default_visible": default_visible,
        "authored_value": binding.get("value").cloned().unwrap_or(Value::Bool(true))
    }))
}

fn scene_push_numeric_property_binding(
    object: &Map<String, Value>,
    keys: &[&str],
    node_id: &str,
    target: &str,
    context: &mut SceneDocumentBuildContext,
    scale: f64,
    offset: f64,
) {
    let Some(binding) = keys
        .iter()
        .filter_map(|key| object.get(*key))
        .find_map(|value| scene_numeric_property_binding(value, context))
    else {
        return;
    };
    context
        .property_bindings
        .push(binding.property_binding_value(node_id, target, scale, offset));
}

fn scene_push_vector_component_script_property_bindings(
    value: Option<&Value>,
    components: &[(&str, &str)],
    node_id: &str,
    context: &mut SceneDocumentBuildContext,
) {
    let Some(object) = value.and_then(Value::as_object) else {
        return;
    };
    let Some(script) = string_field(object, &["script"]) else {
        return;
    };
    let Some(script_properties) = object
        .get("scriptproperties")
        .or_else(|| object.get("scriptProperties"))
        .and_then(Value::as_object)
    else {
        return;
    };

    let mut lowered = false;
    for (component, target) in components {
        let Some(property) =
            scene_vector_component_script_user_property(&script, script_properties, component)
        else {
            continue;
        };
        context.property_bindings.push(json!({
            "property": property,
            "target_node": node_id,
            "target": target,
            "scale": 1.0,
            "offset": 0.0
        }));
        lowered = true;
    }
    if lowered {
        scene_record_native_script_lowering(context);
        push_unique(
            &mut context.converted_features,
            "scene-deterministic-scenescript-expression",
        );
    }
}

fn scene_push_vector_component_script_timeline_bindings(
    value: Option<&Value>,
    components: &[(&str, &str)],
    node_id: &str,
    context: &mut SceneDocumentBuildContext,
) {
    let Some(object) = value.and_then(Value::as_object) else {
        return;
    };
    let Some(script) = string_field(object, &["script"]) else {
        return;
    };
    let Some(script_properties) = object
        .get("scriptproperties")
        .or_else(|| object.get("scriptProperties"))
        .and_then(Value::as_object)
    else {
        return;
    };

    let mut lowered = false;
    for (component, target) in components {
        let Some(timeline) = scene_vector_component_script_sine_timeline(
            &script,
            script_properties,
            component,
            target,
            node_id,
            context,
        ) else {
            continue;
        };
        context.timelines.push(timeline);
        lowered = true;
    }
    if lowered {
        scene_record_native_script_lowering(context);
        push_unique(
            &mut context.converted_features,
            "scene-deterministic-scenescript-expression",
        );
        push_unique(
            &mut context.converted_features,
            "scene-deterministic-scenescript-sine-timeline",
        );
    }
}

fn scene_vector_component_script_sine_timeline(
    script: &str,
    script_properties: &Map<String, Value>,
    component: &str,
    target: &str,
    node_id: &str,
    context: &mut SceneDocumentBuildContext,
) -> Option<Value> {
    let expression = scene_vector_component_script_assignment_expression(script, component)?;
    if !expression.contains("Math.sin") || !expression.contains("engine.runtime") {
        return None;
    }
    let angular_speed =
        scene_script_expression_primary_runtime_sine_speed(expression, script_properties)?;
    if angular_speed.abs() <= f64::EPSILON {
        return None;
    }
    let period_ms = ((std::f64::consts::TAU / angular_speed.abs()) * 1000.0)
        .round()
        .clamp(
            SCENE_SCRIPT_SINE_TIMELINE_MIN_PERIOD_MS as f64,
            SCENE_SCRIPT_SINE_TIMELINE_MAX_PERIOD_MS as f64,
        ) as u64;
    let period_seconds = period_ms as f64 / 1000.0;
    let mut keyframes = Vec::with_capacity(SCENE_SCRIPT_SINE_TIMELINE_SAMPLES + 1);
    for sample in 0..=SCENE_SCRIPT_SINE_TIMELINE_SAMPLES {
        let progress = sample as f64 / SCENE_SCRIPT_SINE_TIMELINE_SAMPLES as f64;
        let runtime_seconds = progress * period_seconds;
        let value = scene_eval_deterministic_script_expression(
            expression,
            script_properties,
            runtime_seconds,
        )?;
        let time_ms = ((period_ms as f64) * progress).round() as u64;
        keyframes.push(json!({
            "time_ms": time_ms,
            "value": value,
            "curve": "linear"
        }));
    }

    let timeline_id = scene_next_timeline_id(
        context,
        Some(&format!("{node_id}-{target}-scenescript-sine")),
    );
    Some(json!({
        "id": timeline_id,
        "target_node": node_id,
        "channels": [
            {
                "property": target,
                "loop": true,
                "keyframes": keyframes
            }
        ]
    }))
}

fn scene_vector_component_script_user_property(
    script: &str,
    script_properties: &Map<String, Value>,
    component: &str,
) -> Option<String> {
    let local_property =
        scene_vector_component_script_local_property(script, component)?.to_owned();
    script_properties
        .get(&local_property)
        .and_then(Value::as_object)
        .and_then(|property| string_field(property, &["user", "property"]))
}

fn scene_vector_component_script_local_property<'a>(
    script: &'a str,
    component: &str,
) -> Option<&'a str> {
    let expression = scene_vector_component_script_assignment_expression(script, component)?;
    scene_script_properties_access_identifier(expression)
}

fn scene_vector_component_script_assignment_expression<'a>(
    script: &'a str,
    component: &str,
) -> Option<&'a str> {
    let needle = format!("value.{component}");
    let mut offset = 0usize;
    while let Some(index) = script[offset..].find(&needle) {
        let absolute = offset + index;
        let before = script[..absolute].chars().next_back();
        if before.is_some_and(scene_script_identifier_character) {
            offset = absolute + needle.len();
            continue;
        }
        let after_component = &script[absolute + needle.len()..];
        let after_component = after_component.trim_start();
        let Some(after_assignment) = after_component.strip_prefix('=') else {
            offset = absolute + needle.len();
            continue;
        };
        if after_assignment.starts_with('=') {
            offset = absolute + needle.len();
            continue;
        }
        let expression = after_assignment.trim_start();
        let expression_end = expression
            .find(|character: char| matches!(character, ';' | '\n' | '\r'))
            .unwrap_or(expression.len());
        return Some(expression[..expression_end].trim());
    }
    None
}

fn scene_script_expression_primary_runtime_sine_speed(
    expression: &str,
    script_properties: &Map<String, Value>,
) -> Option<f64> {
    for argument in scene_script_math_sin_arguments(expression) {
        let start = scene_eval_deterministic_script_expression(argument, script_properties, 0.0)?;
        let end = scene_eval_deterministic_script_expression(argument, script_properties, 1.0)?;
        let speed = end - start;
        if speed.abs() > f64::EPSILON {
            return Some(speed);
        }
    }
    None
}

fn scene_script_math_sin_arguments(expression: &str) -> Vec<&str> {
    let mut arguments = Vec::new();
    let mut offset = 0usize;
    while let Some(index) = expression[offset..].find("Math.sin") {
        let function_start = offset + index;
        let Some(open_relative) = expression[function_start..].find('(') else {
            break;
        };
        let open = function_start + open_relative;
        let Some(close) = scene_script_matching_parenthesis(expression, open) else {
            break;
        };
        arguments.push(expression[open + 1..close].trim());
        offset = close + 1;
    }
    arguments
}

fn scene_script_matching_parenthesis(expression: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (index, character) in expression[open..].char_indices() {
        match character {
            '(' => depth = depth.saturating_add(1),
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(open + index);
                }
            }
            _ => {}
        }
    }
    None
}

fn scene_eval_deterministic_script_expression(
    expression: &str,
    script_properties: &Map<String, Value>,
    runtime_seconds: f64,
) -> Option<f64> {
    let mut parser = SceneScriptExpressionParser {
        input: expression,
        offset: 0,
        script_properties,
        runtime_seconds,
    };
    let value = parser.parse_expression()?;
    parser.skip_whitespace();
    (parser.offset == parser.input.len() && value.is_finite()).then_some(value)
}

struct SceneScriptExpressionParser<'a> {
    input: &'a str,
    offset: usize,
    script_properties: &'a Map<String, Value>,
    runtime_seconds: f64,
}

impl SceneScriptExpressionParser<'_> {
    fn parse_expression(&mut self) -> Option<f64> {
        self.parse_add_sub()
    }

    fn parse_add_sub(&mut self) -> Option<f64> {
        let mut value = self.parse_mul_div()?;
        loop {
            self.skip_whitespace();
            if self.consume_char('+') {
                value += self.parse_mul_div()?;
            } else if self.consume_char('-') {
                value -= self.parse_mul_div()?;
            } else {
                return Some(value);
            }
        }
    }

    fn parse_mul_div(&mut self) -> Option<f64> {
        let mut value = self.parse_unary()?;
        loop {
            self.skip_whitespace();
            if self.consume_char('*') {
                value *= self.parse_unary()?;
            } else if self.consume_char('/') {
                let divisor = self.parse_unary()?;
                if divisor.abs() <= f64::EPSILON {
                    return None;
                }
                value /= divisor;
            } else {
                return Some(value);
            }
        }
    }

    fn parse_unary(&mut self) -> Option<f64> {
        self.skip_whitespace();
        if self.consume_char('+') {
            self.parse_unary()
        } else if self.consume_char('-') {
            self.parse_unary().map(|value| -value)
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Option<f64> {
        self.skip_whitespace();
        if self.consume_char('(') {
            let value = self.parse_expression()?;
            self.skip_whitespace();
            return self.consume_char(')').then_some(value);
        }
        if self
            .peek_char()
            .is_some_and(|character| character.is_ascii_digit() || character == '.')
        {
            return self.parse_number();
        }
        let identifier = self.parse_identifier()?;
        if identifier == "Math.sin" {
            self.skip_whitespace();
            if !self.consume_char('(') {
                return None;
            }
            let value = self.parse_expression()?.sin();
            self.skip_whitespace();
            return self.consume_char(')').then_some(value);
        }
        if identifier == "engine.runtime" {
            return Some(self.runtime_seconds);
        }
        if let Some(property) = identifier
            .strip_prefix("scriptProperties.")
            .or_else(|| identifier.strip_prefix("this.scriptProperties."))
        {
            return self.script_property_value(property);
        }
        None
    }

    fn parse_number(&mut self) -> Option<f64> {
        let start = self.offset;
        let mut seen_digit = false;
        let mut seen_dot = false;
        while let Some(character) = self.peek_char() {
            if character.is_ascii_digit() {
                seen_digit = true;
                self.bump_char();
            } else if character == '.' && !seen_dot {
                seen_dot = true;
                self.bump_char();
            } else {
                break;
            }
        }
        if matches!(self.peek_char(), Some('e' | 'E')) {
            self.bump_char();
            if matches!(self.peek_char(), Some('+' | '-')) {
                self.bump_char();
            }
            while self
                .peek_char()
                .is_some_and(|character| character.is_ascii_digit())
            {
                self.bump_char();
            }
        }
        if !seen_digit {
            return None;
        }
        self.input[start..self.offset].parse().ok()
    }

    fn parse_identifier(&mut self) -> Option<String> {
        let start = self.offset;
        while let Some(character) = self.peek_char() {
            if scene_script_identifier_character(character) || character == '.' {
                self.bump_char();
            } else {
                break;
            }
        }
        (self.offset > start).then(|| self.input[start..self.offset].to_owned())
    }

    fn script_property_value(&self, property: &str) -> Option<f64> {
        self.script_properties
            .get(property)
            .and_then(value_to_f64_unwrapped)
    }

    fn consume_char(&mut self, expected: char) -> bool {
        if self.peek_char() == Some(expected) {
            self.bump_char();
            true
        } else {
            false
        }
    }

    fn skip_whitespace(&mut self) {
        while self.peek_char().is_some_and(char::is_whitespace) {
            self.bump_char();
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.offset..].chars().next()
    }

    fn bump_char(&mut self) {
        if let Some(character) = self.peek_char() {
            self.offset += character.len_utf8();
        }
    }
}

fn scene_script_properties_access_identifier(expression: &str) -> Option<&str> {
    let expression = expression.trim();
    let property = expression
        .strip_prefix("scriptProperties.")
        .or_else(|| expression.strip_prefix("this.scriptProperties."))?;
    let end = property
        .char_indices()
        .find_map(|(index, character)| {
            (!scene_script_identifier_character(character)).then_some(index)
        })
        .unwrap_or(property.len());
    (end > 0).then_some(&property[..end])
}

fn scene_script_identifier_character(character: char) -> bool {
    character == '_' || character.is_ascii_alphanumeric()
}

fn scene_numeric_property_binding(
    value: &Value,
    context: &mut SceneDocumentBuildContext,
) -> Option<SceneNumericPropertyBindingIr> {
    let object = value.as_object()?;
    let default_property = string_field(object, &["user", "property"]);
    let default_value = object.get("value").and_then(value_to_f64);
    let script = string_field(object, &["script"]);
    match SceneNumericPropertyBindingIr::from_wallpaper_engine_parts(
        default_property,
        default_value,
        script.as_deref(),
    )? {
        SceneNumericPropertyBindingIrResult::Lowered {
            binding,
            used_script,
        } => {
            if used_script {
                scene_record_native_script_lowering(context);
                push_unique(
                    &mut context.converted_features,
                    "scene-deterministic-scenescript-expression",
                );
            }
            Some(binding)
        }
        SceneNumericPropertyBindingIrResult::UnsupportedScriptWithProperty => {
            scene_push_unsupported(
                context,
                "scenescript-expression-lowering",
                "Wallpaper Engine numeric SceneScript expression references a user property but is outside the deterministic gscene linear-expression lowering subset.",
                None,
            );
            None
        }
    }
}

fn scene_node_from_object(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    object: &Map<String, Value>,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
    resources: &mut Vec<Value>,
) -> Value {
    let original_type = string_field(object, &["type", "class", "kind"]);
    let source_path = scene_resource_path_from_object(object);
    let source_model =
        scene_source_model_from_object(project, output_dir, object, report, context, resources);
    let kind = scene_node_kind_from_object(
        object,
        source_path.as_deref(),
        source_model.as_ref(),
        original_type.as_deref(),
    );
    let node_id = scene_next_node_id(
        context,
        kind,
        original_type
            .as_deref()
            .or(source_path.as_deref())
            .or_else(|| {
                source_model
                    .as_ref()
                    .map(|model| model.original_path.as_str())
            }),
    );
    if let Some(source_id) = object.get("id").and_then(value_to_string) {
        context.source_node_ids.insert(source_id, node_id.clone());
    }
    let mut node = Map::new();
    node.insert("id".to_owned(), Value::String(node_id.clone()));
    node.insert("type".to_owned(), Value::String(kind.to_owned()));
    if let Some(name) = string_field(object, &["name", "id", "label"]) {
        node.insert("name".to_owned(), Value::String(name));
    }
    let visible = scene_visible_from_object(object, &node_id, context);
    if let Some(visible) = visible.static_visible {
        node.insert("visible".to_owned(), Value::Bool(visible));
    }
    if let Some(visibility_condition) = object
        .get("visible")
        .and_then(Value::as_object)
        .and_then(|visible| scene_visible_user_condition_value(visible, context))
    {
        scene_merge_node_properties(
            &mut node,
            json!({ "visibility_condition": visibility_condition }),
        );
        push_unique(
            &mut context.converted_features,
            "wallpaper-engine-conditional-visibility-ir",
        );
    }
    scene_push_numeric_property_binding(
        object,
        &["opacity", "alpha"],
        &node_id,
        "opacity",
        context,
        1.0,
        0.0,
    );
    let blend_opacity = number_value_field(object, &["blend"])
        .filter(|blend| blend.is_finite())
        .map(|blend| blend.clamp(0.0, 1.0))
        .unwrap_or(1.0);
    let object_opacity = number_value_field(object, &["opacity", "alpha"]);
    let inferred_blend_opacity = object_opacity
        .or_else(|| scene_inferred_blend_opacity(object, source_model.as_ref(), context));
    if let Some(opacity) = inferred_blend_opacity {
        let opacity = if let Some(visible_opacity) = visible.initial_opacity {
            opacity * visible_opacity
        } else {
            opacity
        };
        node.insert(
            "opacity".to_owned(),
            json!((opacity * blend_opacity).clamp(0.0, 1.0)),
        );
    } else if let Some(opacity) = visible.initial_opacity {
        node.insert(
            "opacity".to_owned(),
            json!((opacity * blend_opacity).clamp(0.0, 1.0)),
        );
    } else if blend_opacity < 1.0 {
        node.insert("opacity".to_owned(), json!(blend_opacity));
    }
    if let Some(blend) = scene_blend_properties_from_object(object) {
        scene_merge_node_properties(&mut node, json!({ "wallpaper_engine_blend": blend }));
        push_unique(
            &mut context.converted_features,
            "wallpaper-engine-layer-blend-lowering",
        );
    }
    if let Some(transform) = scene_transform_from_object(object, &node_id, context) {
        node.insert("transform".to_owned(), transform);
    }
    if let Some(attachment) = string_field(object, &["attachment"]) {
        node.insert("puppet_attachment".to_owned(), Value::String(attachment));
        push_unique(
            &mut context.converted_features,
            "wallpaper-engine-puppet-attachment-runtime",
        );
    }
    if let Some(depth) = number_value_field(object, &["parallax_depth", "parallaxDepth"]) {
        node.insert("parallax_depth".to_owned(), json!(depth));
    }
    if let Some(color_binding) = scene_color_property_binding_from_object(
        object,
        &[
            "color",
            "fill",
            "background",
            "backgroundColor",
            "backgroundcolor",
            "tint",
        ],
        context,
    ) {
        scene_merge_node_properties(&mut node, json!({ "color_binding": color_binding }));
        push_unique(
            &mut context.converted_features,
            "wallpaper-engine-user-color-ir",
        );
    }
    if let Some(color) = scene_color_from_object(object)
        .or_else(|| scene_builtin_util_default_color(source_model.as_ref()))
    {
        node.insert("color".to_owned(), Value::String(color));
    } else if kind == "text" {
        node.insert("color".to_owned(), Value::String("#ffffff".to_owned()));
    }
    if let Some(stroke_binding) = scene_color_property_binding_from_object(
        object,
        &["stroke_color", "strokeColor", "stroke"],
        context,
    ) {
        scene_merge_node_properties(&mut node, json!({ "stroke_color_binding": stroke_binding }));
        push_unique(
            &mut context.converted_features,
            "wallpaper-engine-user-color-ir",
        );
    }
    if let Some(stroke) = scene_stroke_color_from_object(object) {
        node.insert("stroke_color".to_owned(), Value::String(stroke));
    }
    if let Some(stroke_width) =
        number_value_field(object, &["stroke_width", "strokeWidth", "strokewidth"])
    {
        node.insert("stroke_width".to_owned(), json!(stroke_width.max(0.0)));
    }
    scene_push_numeric_property_binding(
        object,
        &["width", "w"],
        &node_id,
        "width",
        context,
        1.0,
        0.0,
    );
    scene_push_numeric_property_binding(
        object,
        &["height", "h"],
        &node_id,
        "height",
        context,
        1.0,
        0.0,
    );
    if let Some(width) = number_value_field(object, &["width", "w"]) {
        node.insert("width".to_owned(), json!(width.max(0.0)));
    }
    if let Some(height) = number_value_field(object, &["height", "h"]) {
        node.insert("height".to_owned(), json!(height.max(0.0)));
    }
    if let Some(text) = scene_text_from_object(object) {
        node.insert("text".to_owned(), Value::String(text));
    }
    if let Some(text_binding) = scene_text_binding_from_object(object) {
        scene_merge_node_properties(&mut node, json!({ "text_binding": text_binding }));
        scene_record_native_script_lowering(context);
        push_unique(
            &mut context.converted_features,
            "scene-we-deterministic-clock-text",
        );
    }
    if let Some(font_size) = scene_font_size_from_object(object) {
        node.insert("font_size".to_owned(), json!(font_size.max(1.0)));
    }
    if let Some(font_family) = scene_font_family_from_object(object) {
        if let Some(font_resource) = scene_copy_font_resource_if_path(
            project,
            output_dir,
            &font_family,
            report,
            context,
            resources,
        ) {
            node.insert("font_resource".to_owned(), Value::String(font_resource));
        }
        node.insert("font_family".to_owned(), Value::String(font_family));
    }
    if let Some(font_weight) = string_field(object, &["font_weight", "fontWeight", "weight"]) {
        node.insert("font_weight".to_owned(), Value::String(font_weight));
    }
    if let Some(align) = scene_text_align_from_object(object) {
        node.insert("text_align".to_owned(), Value::String(align.to_owned()));
    }
    if kind == "path"
        && let Some(path_data) = scene_vector_path_from_object(object)
    {
        node.insert("path".to_owned(), Value::String(path_data));
        if let Some(fill_rule) = scene_path_fill_rule_from_object(object) {
            node.insert(
                "path_fill_rule".to_owned(),
                Value::String(fill_rule.to_owned()),
            );
        }
    }
    if let Some(fit) = scene_fit_from_object(object) {
        node.insert("fit".to_owned(), Value::String(fit.to_owned()));
    }
    if let Some(width) = scene_size_component_from_object(object, 0) {
        node.entry("width".to_owned())
            .or_insert_with(|| json!(width));
    }
    if let Some(height) = scene_size_component_from_object(object, 1) {
        node.entry("height".to_owned())
            .or_insert_with(|| json!(height));
    }
    if let Some(size) = source_model.as_ref().and_then(|model| model.render_size) {
        node.entry("width".to_owned())
            .or_insert_with(|| json!(f64::from(size.width)));
        node.entry("height".to_owned())
            .or_insert_with(|| json!(f64::from(size.height)));
    }
    let source_model_mesh = source_model
        .as_ref()
        .and_then(|model| model.render_mesh.as_ref());
    if string_field(object, &["attachment"]).is_some()
        && source_model_mesh.is_none()
        && let Some(bounds) = source_model.as_ref().and_then(|model| model.render_bounds)
    {
        node.insert("width".to_owned(), json!(bounds.width));
        node.insert("height".to_owned(), json!(bounds.height));
        scene_apply_render_bounds_anchor_to_node(&mut node, bounds);
    }
    if scene_builtin_util_uses_viewport(source_model.as_ref())
        && let (Some(width), Some(height)) = (context.viewport_width, context.viewport_height)
    {
        node.entry("width".to_owned())
            .or_insert_with(|| json!(width));
        node.entry("height".to_owned())
            .or_insert_with(|| json!(height));
    }
    if kind == "rectangle"
        && let Some(radius) = scene_corner_radius_from_object(object)
    {
        scene_push_numeric_property_binding(
            object,
            &[
                "radius",
                "corner_radius",
                "cornerRadius",
                "cornerradius",
                "border_radius",
                "borderRadius",
            ],
            &node_id,
            "corner-radius",
            context,
            1.0,
            0.0,
        );
        node.insert("corner_radius".to_owned(), json!(radius));
    }

    if let Some(source_model) = &source_model
        && let Some(resource) = &source_model.render_resource
    {
        node.insert("resource".to_owned(), Value::String(resource.clone()));
    }
    if let Some(source_model) = &source_model
        && let Some(properties) = &source_model.render_properties
    {
        scene_merge_node_properties(&mut node, properties.clone());
    }
    if let Some(mesh) = source_model_mesh {
        node.insert("mesh".to_owned(), mesh.clone());
        push_unique(
            &mut context.converted_features,
            "wallpaper-engine-puppet-mesh-lowering",
        );
    }
    let mesh_has_puppet_clips = source_model_mesh
        .and_then(|mesh| mesh.get("puppet_clips"))
        .is_some_and(|clips| clips.as_array().is_some_and(|clips| !clips.is_empty()));
    let mut puppet_animation_layers_lowered = false;
    if mesh_has_puppet_clips {
        let layers = scene_puppet_animation_layers_from_object(object);
        if !layers.is_empty() {
            node.insert("puppet_animation_layers".to_owned(), Value::Array(layers));
            puppet_animation_layers_lowered = true;
            push_unique(
                &mut context.converted_features,
                "wallpaper-engine-puppet-animation-layer-lowering",
            );
        }
    }
    if let Some((controller, pending_controller)) =
        scene_controller_from_object(object, &node_id, source_model.as_ref())
    {
        scene_merge_node_properties(&mut node, json!({ "controller": controller }));
        if scene_object_visible_script(object) {
            scene_record_native_script_lowering(context);
        }
        context.pending_controllers.push(pending_controller);
        push_unique(
            &mut context.converted_features,
            "wallpaper-engine-util-controller-lowering",
        );
    }
    if kind == "particle-emitter"
        && let Some(particle) = scene_particle_conversion_from_object(
            project, output_dir, object, report, context, resources,
        )
    {
        scene_merge_node_properties(&mut node, particle.properties);
        if let Some(properties) = particle.render_properties {
            scene_merge_node_properties(&mut node, properties);
        }
        if let Some(resource) = particle.render_resource {
            node.entry("resource".to_owned())
                .or_insert_with(|| Value::String(resource));
        }
        push_unique(&mut report.converted_features, "native-particle-runtime");
    }
    if let Some(source_path) = &source_path {
        if let Some(resource_id) = scene_copy_resource(
            project,
            output_dir,
            source_path,
            kind,
            report,
            context,
            resources,
        ) {
            node.insert("resource".to_owned(), Value::String(resource_id));
        }
    }
    let effects = effect::scene_effects_from_object(
        project, output_dir, object, &node_id, report, context, resources,
    );
    if !effects.is_empty() {
        node.insert("effects".to_owned(), Value::Array(effects));
    }
    let audio =
        scene_audio_cues_from_object(project, output_dir, object, report, context, resources);
    if !audio.is_empty() {
        node.insert("audio".to_owned(), Value::Array(audio));
    }
    let native_audio_response_ready =
        scene_enable_native_audio_response_if_recordable(&node, &node_id, report, context);
    if let Some(provenance) = scene_node_provenance_from_object(
        object,
        original_type.as_deref(),
        source_path.as_deref(),
        source_model.as_ref(),
    ) {
        node.insert("provenance".to_owned(), provenance);
    }
    scene_collect_object_timelines(object, &node_id, context, puppet_animation_layers_lowered);

    let children =
        scene_child_nodes_from_object(project, output_dir, object, report, context, resources);
    if !children.is_empty() {
        node.insert("children".to_owned(), Value::Array(children));
    }
    let native_particle_ready = kind == "particle-emitter"
        && node.get("properties").is_some_and(|properties| {
            properties
                .as_object()
                .is_some_and(|properties| properties.contains_key("particle"))
        });
    let native_script_ready =
        kind == "script" && scene_builtin_util_script_native_ready(source_model.as_ref(), &node);
    if kind == "shader"
        || (kind == "script" && !native_script_ready)
        || kind == "unknown"
        || (kind == "audio-response" && !native_audio_response_ready)
        || (kind == "particle-emitter" && !native_particle_ready)
    {
        scene_push_unsupported(
            context,
            kind,
            "Wallpaper Engine runtime system is represented in gscene but not executed by the native scene runtime yet.",
            source_path.as_deref().or_else(|| {
                source_model
                    .as_ref()
                    .map(|model| model.original_path.as_str())
            }),
        );
    }
    Value::Object(node)
}

fn scene_apply_render_bounds_anchor_to_node(
    node: &mut Map<String, Value>,
    bounds: ScenePuppetMeshBounds,
) {
    let entry = node
        .entry("transform".to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    if let Some(transform) = entry.as_object_mut() {
        transform.insert("anchor_x".to_owned(), json!(bounds.anchor_x));
        transform.insert("anchor_y".to_owned(), json!(bounds.anchor_y));
    }
}

fn scene_builtin_util_script_native_ready(
    source_model: Option<&SceneSourceModelConversion>,
    node: &Map<String, Value>,
) -> bool {
    let Some(model) = source_model else {
        return false;
    };
    if !model
        .value
        .get("builtin")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return false;
    }
    let utility = model.value.get("utility").and_then(Value::as_str);
    if !matches!(utility, Some("fullscreenlayer" | "composelayer")) {
        return false;
    }
    node.get("properties")
        .and_then(Value::as_object)
        .and_then(|properties| properties.get("controller"))
        .and_then(Value::as_object)
        .and_then(|controller| controller.get("runtime"))
        .and_then(Value::as_str)
        .is_none_or(|runtime| runtime == "native")
}

fn scene_merge_node_properties(node: &mut Map<String, Value>, properties: Value) {
    let Some(new_properties) = properties.as_object() else {
        return;
    };
    let entry = node
        .entry("properties".to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(existing) = entry.as_object_mut() else {
        *entry = Value::Object(new_properties.clone());
        return;
    };
    for (key, value) in new_properties {
        existing.insert(key.clone(), value.clone());
    }
}

fn scene_puppet_animation_layers_from_object(object: &Map<String, Value>) -> Vec<Value> {
    let Some(value) = object.get("animationlayers") else {
        return Vec::new();
    };
    let mut layers = Vec::new();
    scene_collect_puppet_animation_layers(value, &mut layers);
    layers
}

fn scene_collect_puppet_animation_layers(value: &Value, layers: &mut Vec<Value>) {
    match value {
        Value::Array(values) => {
            for value in values {
                scene_collect_puppet_animation_layers(value, layers);
            }
        }
        Value::Object(object) => {
            if let Some(layer) = scene_puppet_animation_layer_from_object(object) {
                layers.push(layer);
            }
            for key in ["layers", "children", "items"] {
                if let Some(value) = object.get(key) {
                    scene_collect_puppet_animation_layers(value, layers);
                }
            }
        }
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null => {}
    }
}

fn scene_puppet_animation_layer_from_object(object: &Map<String, Value>) -> Option<Value> {
    let clip_id = object.get("animation").and_then(value_to_u32)?;
    let visible = object
        .get("visible")
        .and_then(value_to_bool_unwrapped)
        .unwrap_or(true);
    if !visible {
        return None;
    }
    let mut layer = Map::new();
    layer.insert("clip_id".to_owned(), json!(clip_id));
    if let Some(name) = string_field(object, &["name"]) {
        layer.insert("name".to_owned(), Value::String(name));
    }
    if let Some(additive) = object.get("additive").and_then(value_to_bool_unwrapped) {
        layer.insert("additive".to_owned(), json!(additive));
    }
    if let Some(blend) = number_value_field(object, &["blend", "weight", "strength"])
        && blend.is_finite()
    {
        layer.insert("blend".to_owned(), json!(blend));
    }
    if let Some(rate) = number_value_field(object, &["rate", "speed", "timescale"])
        && rate.is_finite()
    {
        layer.insert("rate".to_owned(), json!(rate.max(0.0)));
    }
    if let Some(phase) = scene_puppet_animation_layer_initial_phase(object) {
        layer.insert("initial_phase".to_owned(), json!(phase));
    }
    Some(Value::Object(layer))
}

fn scene_puppet_animation_layer_initial_phase(object: &Map<String, Value>) -> Option<f64> {
    let visible = object.get("visible")?.as_object()?;
    let script = visible.get("script")?.as_str()?;
    let normalized = script.split_whitespace().collect::<String>();
    if !normalized.contains("setFrame(")
        || !normalized.contains("frameCount*scriptProperties.percentage")
    {
        return None;
    }
    let phase = visible
        .get("scriptproperties")
        .and_then(Value::as_object)
        .and_then(|properties| properties.get("percentage"))
        .and_then(value_to_f64_unwrapped)
        .unwrap_or(0.0);
    phase.is_finite().then_some(phase.clamp(0.0, 1.0))
}

fn scene_enable_native_audio_response_if_recordable(
    node: &Map<String, Value>,
    node_id: &str,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
) -> bool {
    if node.get("type").and_then(Value::as_str) != Some("audio-response") {
        return false;
    }
    let width = node.get("width").and_then(value_to_f64);
    let height = node.get("height").and_then(value_to_f64);
    let has_paint = node
        .get("color")
        .and_then(Value::as_str)
        .is_some_and(|color| !color.is_empty())
        || node
            .get("stroke_color")
            .and_then(Value::as_str)
            .is_some_and(|color| !color.is_empty());
    let recordable = width.is_some_and(|width| width.is_finite() && width > 0.0)
        && height.is_some_and(|height| height.is_finite() && height > 0.0)
        && has_paint;
    if !recordable {
        return false;
    }

    push_unique(
        &mut report.converted_features,
        "native-audio-response-runtime",
    );
    if !context.property_bindings.iter().any(|binding| {
        binding
            .get("target_node")
            .and_then(Value::as_str)
            .is_some_and(|target| target == node_id)
            && binding
                .get("property")
                .and_then(Value::as_str)
                .is_some_and(scene_property_is_audio_response)
    }) {
        let base_width = width.unwrap_or(1.0).max(1.0);
        context.property_bindings.push(json!({
            "property": "audio.bass",
            "target_node": node_id,
            "target": "width",
            "scale": base_width * 0.7,
            "offset": base_width * 0.3
        }));
    }
    true
}

fn scene_property_is_audio_response(property: &str) -> bool {
    let property = property.to_ascii_lowercase();
    property.contains("audio")
        || property.contains("spectrum")
        || property.contains("bass")
        || property.contains("mid")
        || property.contains("treble")
}

fn scene_particle_conversion_from_object(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    object: &Map<String, Value>,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
    resources: &mut Vec<Value>,
) -> Option<SceneParticleConversion> {
    let spawn_width = scene_size_component_from_object(object, 0);
    let spawn_height = scene_size_component_from_object(object, 1);
    let spawn_size = match (spawn_width, spawn_height) {
        (Some(width), Some(height)) => Some((width, height)),
        _ => None,
    };
    let particle_definition =
        scene_particle_definition_from_object(project, object, report, context);
    let mut properties = SceneParticleIr::from_wallpaper_engine_object(
        object,
        scene_particle_seed_from_object(object),
        spawn_size,
        particle_definition.as_ref(),
    )
    .map(|particle| particle.properties_value())?;
    let (render_resource, render_properties) = particle_definition
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|definition| {
            scene_particle_material_from_definition(
                project,
                output_dir,
                definition,
                &mut properties,
                report,
                context,
                resources,
            )
        })
        .unwrap_or((None, None));
    Some(SceneParticleConversion {
        properties,
        render_resource,
        render_properties,
    })
}

fn scene_particle_definition_from_object(
    project: &WallpaperEngineProject,
    object: &Map<String, Value>,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
) -> Option<Value> {
    let particle = object.get("particle")?;
    if particle.is_object() {
        push_unique(
            &mut context.converted_features,
            "wallpaper-engine-particle-definition-lowering",
        );
        push_unique(
            &mut report.converted_features,
            "wallpaper-engine-particle-definition-lowering",
        );
        return Some(particle.clone());
    }
    let source = particle.as_str()?.trim();
    if !scene_particle_definition_source_path(source) {
        return None;
    }
    let definition = read_scene_project_json(project, source, "we-particle-json", report, context)?;
    push_unique(
        &mut context.converted_features,
        "wallpaper-engine-particle-definition-lowering",
    );
    push_unique(
        &mut report.converted_features,
        "wallpaper-engine-particle-definition-lowering",
    );
    Some(definition)
}

fn scene_particle_definition_source_path(source: &str) -> bool {
    let source = source.replace('\\', "/");
    source.eq_ignore_ascii_case("particle.json")
        || source
            .rsplit_once('.')
            .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("json"))
        || source.to_ascii_lowercase().starts_with("particles/")
}

fn scene_particle_material_from_definition(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    definition: &Map<String, Value>,
    particle_properties: &mut Value,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
    resources: &mut Vec<Value>,
) -> Option<(Option<String>, Option<Value>)> {
    let material = string_field(definition, &["material"])?;
    let material_path = scene_material_path(&material);
    if let Some(resource) = scene_copy_resource_as(
        project,
        output_dir,
        &material_path,
        "material",
        Some("we-particle-material"),
        report,
        context,
        resources,
    ) {
        scene_particle_insert_property_string(particle_properties, "material_resource", resource);
    }
    let Some(material_json) = read_scene_project_json(
        project,
        &material_path,
        "we-particle-material-json",
        report,
        context,
    ) else {
        return Some((None, None));
    };
    let (textures, texture_resources, render_resource, render_properties, render_kind) =
        scene_material_textures(
            project,
            output_dir,
            &material_json,
            None,
            report,
            context,
            resources,
        );
    if !textures.is_empty() {
        scene_particle_insert_property_array(
            particle_properties,
            "textures",
            textures.into_iter().map(Value::String).collect(),
        );
    }
    if !texture_resources.is_empty() {
        scene_particle_insert_property_array(
            particle_properties,
            "texture_resources",
            texture_resources.into_iter().map(Value::String).collect(),
        );
    }
    if let Some(resource) = &render_resource {
        scene_particle_insert_property_string(
            particle_properties,
            "render_resource",
            resource.clone(),
        );
        push_unique(
            &mut context.converted_features,
            "scene-we-particle-material-runtime",
        );
        push_unique(
            &mut report.converted_features,
            "scene-we-particle-material-runtime",
        );
    } else {
        scene_push_unsupported(
            context,
            "we-particle-material-texture-runtime",
            "Wallpaper Engine particle material was preserved, but no renderable texture resource was resolved.",
            Some(&material_path),
        );
    }
    if let Some(render_kind) = render_kind {
        scene_particle_insert_property_string(
            particle_properties,
            "render_kind",
            render_kind.to_owned(),
        );
    }
    Some((render_resource, render_properties))
}

fn scene_particle_insert_property_string(properties: &mut Value, key: &str, value: String) {
    if let Some(particle) = properties
        .as_object_mut()
        .and_then(|properties| properties.get_mut("particle"))
        .and_then(Value::as_object_mut)
    {
        particle.insert(key.to_owned(), Value::String(value));
    }
}

fn scene_particle_insert_property_array(properties: &mut Value, key: &str, value: Vec<Value>) {
    if value.is_empty() {
        return;
    }
    if let Some(particle) = properties
        .as_object_mut()
        .and_then(|properties| properties.get_mut("particle"))
        .and_then(Value::as_object_mut)
    {
        particle.insert(key.to_owned(), Value::Array(value));
    }
}

fn scene_child_nodes_from_object(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    object: &Map<String, Value>,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
    resources: &mut Vec<Value>,
) -> Vec<Value> {
    object
        .iter()
        .filter(|(key, _)| scene_container_key(key))
        .flat_map(|(_, value)| {
            collect_scene_nodes_from_value(project, output_dir, value, report, context, resources)
        })
        .collect()
}

fn scene_object_has_node_signal(object: &Map<String, Value>) -> bool {
    string_field(object, &["type", "class", "kind"]).is_some()
        || scene_resource_path_from_object(object).is_some()
        || scene_model_path_from_object(object).is_some()
        || scene_color_from_object(object).is_some()
        || scene_shape_kind_from_object(object).is_some()
        || scene_text_from_object(object).is_some()
        || scene_vector_path_from_object(object).is_some()
        || object
            .get("effects")
            .and_then(Value::as_array)
            .is_some_and(|effects| !effects.is_empty())
        || !scene_sound_sources_from_object(object).is_empty()
        || object.get("particle").is_some()
        || scene_object_is_transform_container(object)
        || scene_controller_target_layer_from_script_properties(object).is_some()
}

fn scene_container_key(key: &str) -> bool {
    matches!(
        normalize_project_key(key).as_str(),
        "objects" | "layers" | "children" | "nodes" | "items"
    )
}

fn scene_node_kind_from_object(
    object: &Map<String, Value>,
    source_path: Option<&str>,
    source_model: Option<&SceneSourceModelConversion>,
    original_type: Option<&str>,
) -> &'static str {
    let type_hint = original_type
        .unwrap_or_default()
        .to_ascii_lowercase()
        .replace(['_', '-'], "");
    if source_path.is_some_and(is_video_path) || type_hint.contains("video") {
        return "video";
    }
    if let Some(source_model) = source_model {
        if let Some(kind) = scene_builtin_util_node_kind(object, source_model) {
            return kind;
        }
        if let Some(render_kind) = source_model.render_kind {
            return render_kind;
        }
        if scene_model_solid_layer(Some(source_model)) && scene_color_from_object(object).is_some()
        {
            return "rectangle";
        }
        return "image";
    }
    if object.get("particle").is_some()
        || type_hint.contains("particle")
        || type_hint.contains("emitter")
    {
        return "particle-emitter";
    }
    if source_path.is_some_and(is_image_path)
        || type_hint.contains("image")
        || type_hint.contains("sprite")
        || type_hint.contains("texture")
    {
        return "image";
    }
    if type_hint.contains("shader") || type_hint.contains("material") {
        return "shader";
    }
    if object.get("sound").is_some() || type_hint.contains("audio") || type_hint.contains("sound") {
        if scene_object_is_audio_cue_only(object, source_path, source_model) {
            return "audio";
        }
        return "audio-response";
    }
    if scene_controller_target_layer_from_script_properties(object).is_some() {
        return "script";
    }
    if type_hint.contains("script") {
        return "script";
    }
    if type_hint.contains("rectangle") || type_hint == "rect" {
        return "rectangle";
    }
    if type_hint.contains("ellipse") || type_hint.contains("circle") {
        return "ellipse";
    }
    if let Some(shape_kind) = scene_shape_kind_from_object(object) {
        return shape_kind;
    }
    if type_hint.contains("text") || scene_text_from_object(object).is_some() {
        return "text";
    }
    if type_hint.contains("path") || scene_vector_path_from_object(object).is_some() {
        return "path";
    }
    if type_hint.contains("color") || scene_color_from_object(object).is_some() {
        return "color";
    }
    if scene_child_nodes_from_keys(object) {
        return "group";
    }
    if scene_object_is_transform_container(object) {
        return "group";
    }
    "unknown"
}

fn scene_builtin_util_node_kind(
    object: &Map<String, Value>,
    source_model: &SceneSourceModelConversion,
) -> Option<&'static str> {
    let utility = source_model.value.get("utility").and_then(Value::as_str)?;
    if scene_controller_target_layer_from_script_properties(object).is_some() {
        return Some("script");
    }
    if object
        .get("visible")
        .and_then(Value::as_object)
        .and_then(|visible| visible.get("script"))
        .is_some()
    {
        return source_model.render_kind;
    }
    if scene_child_nodes_from_keys(object) {
        return Some("group");
    }
    if object
        .get("effects")
        .and_then(Value::as_array)
        .is_some_and(|effects| !effects.is_empty())
        || scene_color_from_object(object).is_some()
        || scene_size_component_from_object(object, 0).is_some()
        || scene_size_component_from_object(object, 1).is_some()
        || number_value_field(object, &["width", "w"]).is_some()
        || number_value_field(object, &["height", "h"]).is_some()
    {
        return Some("rectangle");
    }
    if matches!(utility, "fullscreenlayer" | "composelayer") {
        return source_model.render_kind;
    }
    None
}

fn scene_builtin_util_uses_viewport(source_model: Option<&SceneSourceModelConversion>) -> bool {
    source_model
        .and_then(|model| model.value.get("utility"))
        .and_then(Value::as_str)
        .is_some_and(|utility| utility == "fullscreenlayer")
}

fn scene_builtin_util_default_color(
    source_model: Option<&SceneSourceModelConversion>,
) -> Option<String> {
    source_model
        .and_then(|model| model.value.get("utility"))
        .and_then(Value::as_str)
        .is_some_and(|utility| utility == "solidlayer")
        .then(|| "#ffffff".to_owned())
}

fn scene_object_is_audio_cue_only(
    object: &Map<String, Value>,
    source_path: Option<&str>,
    source_model: Option<&SceneSourceModelConversion>,
) -> bool {
    !scene_sound_sources_from_object(object).is_empty()
        && source_path.is_none()
        && source_model.is_none()
        && scene_color_from_object(object).is_none()
        && scene_stroke_color_from_object(object).is_none()
        && scene_text_from_object(object).is_none()
        && scene_vector_path_from_object(object).is_none()
        && number_value_field(object, &["width", "w"]).is_none()
        && number_value_field(object, &["height", "h"]).is_none()
        && scene_size_component_from_object(object, 0).is_none()
        && scene_size_component_from_object(object, 1).is_none()
        && !object
            .get("effects")
            .and_then(Value::as_array)
            .is_some_and(|effects| !effects.is_empty())
        && object.get("particle").is_none()
        && !scene_child_nodes_from_keys(object)
}

fn scene_shape_kind_from_object(object: &Map<String, Value>) -> Option<&'static str> {
    if scene_bool_value_field(object, &["solid", "issolid", "isSolid"]).unwrap_or(false)
        && scene_shape_object_has_draw_payload(object)
    {
        return Some("rectangle");
    }

    let shape = value_field(object, &["shape", "primitive", "geometry"])?;
    let normalized = shape.to_ascii_lowercase().replace(['_', '-', ' '], "");
    if normalized.contains("ellipse") || normalized.contains("circle") {
        return Some("ellipse");
    }
    if normalized.contains("rect")
        || normalized.contains("box")
        || normalized.contains("quad")
        || normalized.contains("rounded")
        || normalized.contains("solid")
    {
        return Some("rectangle");
    }
    None
}

fn scene_shape_object_has_draw_payload(object: &Map<String, Value>) -> bool {
    scene_color_from_object(object).is_some()
        || scene_stroke_color_from_object(object).is_some()
        || scene_text_from_object(object).is_some()
        || scene_vector_path_from_object(object).is_some()
        || number_value_field(object, &["width", "w"]).is_some()
        || number_value_field(object, &["height", "h"]).is_some()
        || scene_size_component_from_object(object, 0).is_some()
        || scene_size_component_from_object(object, 1).is_some()
}

fn scene_object_is_transform_container(object: &Map<String, Value>) -> bool {
    object.get("origin").is_some()
        || object.get("scale").is_some()
        || object.get("angles").is_some()
        || object.get("parent").is_some()
        || object.get("parallaxDepth").is_some()
        || object.get("parallax_depth").is_some()
}

fn scene_child_nodes_from_keys(object: &Map<String, Value>) -> bool {
    object.keys().any(|key| scene_container_key(key))
}

fn scene_source_model_from_object(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    object: &Map<String, Value>,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
    resources: &mut Vec<Value>,
) -> Option<SceneSourceModelConversion> {
    let model_path = scene_model_path_from_object(object)?;
    let object_frame_size = scene_frame_size_from_object_size(object);
    if let Some(model) = scene_builtin_util_model(&model_path) {
        push_unique(
            &mut context.converted_features,
            "wallpaper-engine-util-model-lowering",
        );
        return Some(model);
    }
    let mut model = Map::new();
    model.insert("source".to_owned(), Value::String(model_path.clone()));
    if let Some(resource) = scene_copy_resource_as(
        project,
        output_dir,
        &model_path,
        "model",
        Some("we-model"),
        report,
        context,
        resources,
    ) {
        model.insert("model_resource".to_owned(), Value::String(resource));
    }

    let Some(model_json) =
        read_scene_project_json(project, &model_path, "we-model-json", report, context)
    else {
        scene_push_unsupported(
            context,
            "we-model-json",
            "Wallpaper Engine object image points to a model file that could not be parsed.",
            Some(&model_path),
        );
        return Some(SceneSourceModelConversion {
            value: Value::Object(model),
            render_kind: None,
            render_resource: None,
            render_properties: None,
            render_size: None,
            render_bounds: None,
            render_mesh: None,
            original_path: model_path,
        });
    };

    let mut render_bounds = None;
    let mut render_mesh = None;
    if let Some(model_object) = model_json.as_object() {
        if let Some(material) = string_field(model_object, &["material"]) {
            let material_path = scene_material_path(&material);
            model.insert("material".to_owned(), Value::String(material_path.clone()));
            if let Some(resource) = scene_copy_resource_as(
                project,
                output_dir,
                &material_path,
                "material",
                Some("we-material"),
                report,
                context,
                resources,
            ) {
                model.insert("material_resource".to_owned(), Value::String(resource));
            }
            if let Some(material_json) = read_scene_project_json(
                project,
                &material_path,
                "we-material-json",
                report,
                context,
            ) {
                let frame_size = scene_model_frame_size(model_object).or(object_frame_size);
                let (textures, texture_resources, render_resource, render_properties, render_kind) =
                    scene_material_textures(
                        project,
                        output_dir,
                        &material_json,
                        frame_size,
                        report,
                        context,
                        resources,
                    );
                if !textures.is_empty() {
                    model.insert(
                        "textures".to_owned(),
                        Value::Array(textures.into_iter().map(Value::String).collect()),
                    );
                }
                if !texture_resources.is_empty() {
                    model.insert(
                        "texture_resources".to_owned(),
                        Value::Array(texture_resources.into_iter().map(Value::String).collect()),
                    );
                }
                if render_resource.is_none() {
                    scene_push_unsupported(
                        context,
                        "we-model-material-texture-runtime",
                        "Wallpaper Engine model resolved to material textures that are preserved as resources but are not directly renderable by the native scene image path yet.",
                        Some(&material_path),
                    );
                }
                if let Some(puppet) = string_field(model_object, &["puppet"]) {
                    if let Some(mesh) = scene_insert_puppet_model_conversion(
                        project,
                        output_dir,
                        &model_path,
                        &puppet,
                        frame_size,
                        &mut model,
                        report,
                        context,
                        resources,
                    ) {
                        render_bounds = Some(mesh.bounds);
                        render_mesh = Some(mesh.to_scene_mesh_value());
                    }
                }
                insert_optional_bool(model_object, "solidlayer", "solid_layer", &mut model);
                insert_optional_bool(model_object, "passthrough", "passthrough", &mut model);
                return Some(SceneSourceModelConversion {
                    value: Value::Object(model),
                    render_kind,
                    render_resource,
                    render_properties,
                    render_size: frame_size,
                    render_bounds,
                    render_mesh,
                    original_path: model_path,
                });
            }
        }
        if let Some(puppet) = string_field(model_object, &["puppet"]) {
            if let Some(mesh) = scene_insert_puppet_model_conversion(
                project,
                output_dir,
                &model_path,
                &puppet,
                scene_model_frame_size(model_object).or(object_frame_size),
                &mut model,
                report,
                context,
                resources,
            ) {
                render_bounds = Some(mesh.bounds);
                render_mesh = Some(mesh.to_scene_mesh_value());
            }
        }
        insert_optional_bool(model_object, "solidlayer", "solid_layer", &mut model);
        insert_optional_bool(model_object, "passthrough", "passthrough", &mut model);
    }

    scene_push_unsupported(
        context,
        "we-model-material-resolution",
        "Wallpaper Engine model was preserved, but no material texture graph was resolved for native scene rendering.",
        Some(&model_path),
    );
    Some(SceneSourceModelConversion {
        value: Value::Object(model),
        render_kind: None,
        render_resource: None,
        render_properties: None,
        render_size: model_json
            .as_object()
            .and_then(scene_model_frame_size)
            .or(object_frame_size),
        render_bounds,
        render_mesh,
        original_path: model_path,
    })
}

#[allow(clippy::too_many_arguments)]
fn scene_insert_puppet_model_conversion(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    model_path: &str,
    puppet: &str,
    frame_size: Option<SceneWeModelFrameSize>,
    model: &mut Map<String, Value>,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
    resources: &mut Vec<Value>,
) -> Option<ScenePuppetMesh> {
    model.insert("puppet".to_owned(), Value::String(puppet.to_owned()));
    let _ = scene_copy_puppet_mdl(project, output_dir, puppet, report, context, resources);
    if let Some(frame_size) = frame_size
        && let Some(attachments) =
            scene_puppet_attachment_map_for_model_path(project, model_path, frame_size, context)
    {
        if !attachments.attachments.is_empty() {
            model.insert("puppet_attachments".to_owned(), attachments.to_value());
            push_unique(
                &mut context.converted_features,
                "wallpaper-engine-puppet-attachment-lowering",
            );
        }
        if let Some(mesh) = attachments.mesh.clone() {
            model.insert("puppet_mesh_bounds".to_owned(), mesh.bounds.to_value());
            if !mesh.clips.is_empty() {
                model.insert(
                    "puppet_animation_clips".to_owned(),
                    Value::Array(
                        mesh.clips
                            .iter()
                            .map(ScenePuppetAnimationClip::summary_value)
                            .collect(),
                    ),
                );
                push_unique(
                    &mut context.converted_features,
                    "wallpaper-engine-puppet-animation-clips",
                );
            }
            push_unique(
                &mut context.converted_features,
                "wallpaper-engine-puppet-mesh-lowering",
            );
            return Some(mesh);
        } else if let Some(mesh_bounds) = attachments.mesh_bounds {
            model.insert("puppet_mesh_bounds".to_owned(), mesh_bounds.to_value());
            push_unique(
                &mut context.converted_features,
                "wallpaper-engine-puppet-mesh-bounds-lowering",
            );
        }
    }
    None
}

fn scene_puppet_attachment_map_for_model_path(
    project: &WallpaperEngineProject,
    model_path: &str,
    frame_size: SceneWeModelFrameSize,
    context: &mut SceneDocumentBuildContext,
) -> Option<ScenePuppetAttachmentMap> {
    let cache_key = format!("{}#{}x{}", model_path, frame_size.width, frame_size.height);
    if let Some(attachments) = context.puppet_attachments_by_model_path.get(&cache_key) {
        return Some(attachments.clone());
    }
    let relative = normalize_relative_path(model_path).ok()?;
    let model_json = fs::read_to_string(project.root.join(relative)).ok()?;
    let model_json = serde_json::from_str::<Value>(&model_json).ok()?;
    let model_object = model_json.as_object()?;
    let puppet = string_field(model_object, &["puppet"])?;
    let attachments = scene_puppet_attachment_map_for_puppet_path(project, &puppet, frame_size)?;
    context
        .puppet_attachments_by_model_path
        .insert(cache_key, attachments.clone());
    Some(attachments)
}

fn scene_puppet_attachment_map_for_puppet_path(
    project: &WallpaperEngineProject,
    puppet: &str,
    frame_size: SceneWeModelFrameSize,
) -> Option<ScenePuppetAttachmentMap> {
    let relative = normalize_relative_path(puppet).ok()?;
    let bytes = fs::read(project.root.join(relative)).ok()?;
    scene_parse_puppet_attachment_map(&bytes, frame_size).ok()
}

fn scene_copy_puppet_mdl(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    puppet: &str,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
    resources: &mut Vec<Value>,
) -> Option<String> {
    if let Some(resource_id) = context.copied_puppet_mdl_ids.get(puppet) {
        push_unique(
            &mut context.converted_features,
            "wallpaper-engine-puppet-resource-dedup",
        );
        return Some(resource_id.clone());
    }
    let resource_id = scene_copy_resource_as(
        project,
        output_dir,
        puppet,
        "model",
        Some("we-puppet-mdl"),
        report,
        context,
        resources,
    )?;
    context
        .copied_puppet_mdl_ids
        .insert(puppet.to_owned(), resource_id.clone());
    push_unique(
        &mut context.converted_features,
        "wallpaper-engine-puppet-mdl",
    );
    Some(resource_id)
}

fn scene_parse_puppet_attachment_map(
    bytes: &[u8],
    frame_size: SceneWeModelFrameSize,
) -> Result<ScenePuppetAttachmentMap, String> {
    let mdls_offset = scene_find_mdl_section(bytes, b"MDLS")
        .ok_or_else(|| "Wallpaper Engine puppet MDL does not contain MDLS.".to_owned())?;
    let mut mesh = scene_puppet_mesh(bytes, mdls_offset);
    let mesh_bounds = mesh.as_ref().map(|mesh| mesh.bounds);
    let (mdls_end, bone_count, mut position) =
        scene_mdl_section_end_count_start(bytes, mdls_offset, "MDLS")?;
    let mut bones = Vec::with_capacity(bone_count);
    for bone_index in 0..bone_count {
        let bone_start = position;
        let _index = scene_take_u32_le(bytes, &mut position, mdls_end, "MDLS bone index")?;
        scene_skip_bytes(bytes, &mut position, mdls_end, 1, "MDLS bone flags")?;
        let parent = scene_take_i32_le(bytes, &mut position, mdls_end, "MDLS bone parent")?;
        let entry_bytes =
            scene_take_u32_le(bytes, &mut position, mdls_end, "MDLS bone matrix size")?;
        if entry_bytes < 64 || entry_bytes % 4 != 0 || entry_bytes > 1024 {
            return Err(format!(
                "Wallpaper Engine puppet MDLS bone {bone_index} has invalid matrix byte length {entry_bytes}."
            ));
        }
        let matrix = scene_take_mdl_matrix(bytes, &mut position, mdls_end)?;
        let skip = usize::try_from(entry_bytes - 64)
            .map_err(|_| "Wallpaper Engine puppet MDLS matrix skip overflowed.".to_owned())?;
        scene_skip_bytes(
            bytes,
            &mut position,
            mdls_end,
            skip,
            "MDLS bone matrix tail",
        )?;
        let info = scene_take_mdl_c_string(bytes, &mut position, mdls_end, "MDLS bone info")?;
        bones.push(ScenePuppetBone {
            parent: usize::try_from(parent)
                .ok()
                .filter(|parent| *parent < bone_count),
            translation: (matrix[12], matrix[13], matrix[14]),
            target_position: scene_puppet_bone_target_position(&info, frame_size),
            bind: scene_puppet_transform_from_mdl_matrix(matrix),
        });
        if position <= bone_start {
            return Err("Wallpaper Engine puppet MDLS parser did not advance.".to_owned());
        }
    }
    if let Some(mesh) = mesh.as_mut()
        && scene_puppet_skin_vertices_valid(&mesh.skin_vertices, bone_count)
    {
        let inverse_binds = scene_parse_puppet_inverse_bind_matrices(bytes, mdls_end, &bones)?;
        mesh.skin = Some(ScenePuppetSkin {
            bones: bones
                .iter()
                .zip(inverse_binds)
                .map(|(bone, inverse_bind)| bone.skin_bone(inverse_bind))
                .collect(),
            vertices: mesh.skin_vertices.clone(),
            attachments: Vec::new(),
        });
        mesh.clips =
            scene_parse_puppet_animation_clips(bytes, mdls_end, bone_count).unwrap_or_default();
    }

    let Some(mdat_offset) = scene_find_mdl_section_after(bytes, b"MDAT", mdls_end) else {
        return Ok(ScenePuppetAttachmentMap {
            attachments: BTreeMap::new(),
            mesh_bounds,
            mesh,
        });
    };
    let (mdat_end, attachment_count, mut position) =
        scene_mdat_section_end_count_start(bytes, mdat_offset)?;
    let mut attachments = BTreeMap::new();
    for _ in 0..attachment_count {
        let bone_index = usize::from(scene_take_u16_le(
            bytes,
            &mut position,
            mdat_end,
            "MDAT attachment bone index",
        )?);
        let name = scene_take_mdl_c_string(bytes, &mut position, mdat_end, "MDAT attachment name")?;
        let attachment_matrix = scene_take_mdl_matrix(bytes, &mut position, mdat_end)?;
        let local_position = [
            f64::from(attachment_matrix[12]),
            f64::from(attachment_matrix[13]),
            f64::from(attachment_matrix[14]),
        ];
        let Some(chain_position) = scene_puppet_attachment_chain_position(bone_index, &bones)
        else {
            continue;
        };
        let target_position = scene_puppet_attachment_target_position(bone_index, &bones).map(
            |(_target_bone_index, target_position)| {
                (
                    target_position.0 + local_position[0],
                    target_position.1 + local_position[1],
                    target_position.2 + local_position[2],
                )
            },
        );
        attachments.insert(
            name,
            ScenePuppetAttachment {
                bone_index,
                x: chain_position.0 + local_position[0],
                y: chain_position.1 + local_position[1],
                z: chain_position.2 + local_position[2],
                local_position,
                placement_source: "mdls-bone-matrix-chain",
                target_position,
            },
        );
    }
    if let Some(skin) = mesh.as_mut().and_then(|mesh| mesh.skin.as_mut()) {
        skin.attachments = attachments
            .iter()
            .map(|(name, attachment)| attachment.skin_attachment(name.clone()))
            .collect();
    }
    let mesh_bounds = mesh.as_ref().map(|mesh| mesh.bounds).or(mesh_bounds);
    Ok(ScenePuppetAttachmentMap {
        attachments,
        mesh_bounds,
        mesh,
    })
}

fn scene_puppet_mesh(bytes: &[u8], mdls_offset: usize) -> Option<ScenePuppetMesh> {
    const MARKER_SIZE: usize = 9;
    const MESH_HEADER_SIZE: usize = 8;
    const VERTEX_STRIDE: usize = 80;
    const POSITION_OFFSET: usize = 0;
    const BONE_INDEX_OFFSET: usize = 40;
    const BONE_WEIGHT_OFFSET: usize = 56;
    const UV_OFFSET: usize = 72;
    const TRIANGLE_INDEX_BYTES: usize = 6;

    if mdls_offset <= MARKER_SIZE + MESH_HEADER_SIZE + 4 {
        return None;
    }
    for offset in MARKER_SIZE..mdls_offset.saturating_sub(MESH_HEADER_SIZE + 4) {
        let vertex_bytes = usize::try_from(scene_read_u32_le_at(bytes, offset + 4)?).ok()?;
        let vertices_offset = offset.checked_add(MESH_HEADER_SIZE)?;
        let index_length_offset = vertices_offset.checked_add(vertex_bytes)?;
        if vertex_bytes == 0
            || vertex_bytes % VERTEX_STRIDE != 0
            || index_length_offset.checked_add(4)? > mdls_offset
        {
            continue;
        }
        let index_bytes =
            usize::try_from(scene_read_u32_le_at(bytes, index_length_offset)?).ok()?;
        let indices_offset = index_length_offset.checked_add(4)?;
        if index_bytes == 0
            || index_bytes % TRIANGLE_INDEX_BYTES != 0
            || indices_offset.checked_add(index_bytes)? > mdls_offset
        {
            continue;
        }
        return scene_puppet_mesh_from_block(
            bytes,
            vertices_offset,
            vertex_bytes / VERTEX_STRIDE,
            VERTEX_STRIDE,
            POSITION_OFFSET,
            BONE_INDEX_OFFSET,
            BONE_WEIGHT_OFFSET,
            UV_OFFSET,
            indices_offset,
            index_bytes / 2,
        );
    }
    None
}

fn scene_puppet_mesh_from_block(
    bytes: &[u8],
    vertices_offset: usize,
    vertex_count: usize,
    vertex_stride: usize,
    position_offset: usize,
    bone_index_offset: usize,
    bone_weight_offset: usize,
    uv_offset: usize,
    indices_offset: usize,
    index_count: usize,
) -> Option<ScenePuppetMesh> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut vertices = Vec::with_capacity(vertex_count);
    let mut skin_vertices = Vec::with_capacity(vertex_count);

    for index in 0..vertex_count {
        let vertex_base = vertices_offset.checked_add(index.checked_mul(vertex_stride)?)?;
        let position_base = vertex_base.checked_add(position_offset)?;
        let bone_index_base = vertex_base.checked_add(bone_index_offset)?;
        let bone_weight_base = vertex_base.checked_add(bone_weight_offset)?;
        let uv_base = vertex_base.checked_add(uv_offset)?;
        let raw_x = scene_read_f32_le_at(bytes, position_base)?;
        let raw_y = scene_read_f32_le_at(bytes, position_base + 4)?;
        let u = scene_read_f32_le_at(bytes, uv_base)?;
        let raw_v = scene_read_f32_le_at(bytes, uv_base + 4)?;
        let v = 1.0 - raw_v;
        if !raw_x.is_finite() || !raw_y.is_finite() || !u.is_finite() || !raw_v.is_finite() {
            return None;
        }
        let x = raw_x;
        let y = raw_y;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
        vertices.push(ScenePuppetMeshVertex {
            x,
            y,
            u,
            v,
            opacity: 1.0,
        });
        skin_vertices.push(ScenePuppetSkinVertex {
            bone_indices: [
                usize::try_from(scene_read_u32_le_at(bytes, bone_index_base)?).ok()?,
                usize::try_from(scene_read_u32_le_at(bytes, bone_index_base + 4)?).ok()?,
                usize::try_from(scene_read_u32_le_at(bytes, bone_index_base + 8)?).ok()?,
                usize::try_from(scene_read_u32_le_at(bytes, bone_index_base + 12)?).ok()?,
            ],
            weights: [
                f64::from(scene_read_f32_le_at(bytes, bone_weight_base)?),
                f64::from(scene_read_f32_le_at(bytes, bone_weight_base + 4)?),
                f64::from(scene_read_f32_le_at(bytes, bone_weight_base + 8)?),
                f64::from(scene_read_f32_le_at(bytes, bone_weight_base + 12)?),
            ],
        });
    }

    if vertices.len() < 3 {
        return None;
    }
    let width = max_x - min_x;
    let height = max_y - min_y;
    if !width.is_finite() || !height.is_finite() || width <= f64::EPSILON || height <= f64::EPSILON
    {
        return None;
    }
    let mut indices = Vec::with_capacity(index_count);
    for index in 0..index_count {
        let offset = indices_offset.checked_add(index.checked_mul(2)?)?;
        let value = u32::from(scene_read_u16_le_at(bytes, offset)?);
        if usize::try_from(value)
            .ok()
            .is_none_or(|value| value >= vertices.len())
        {
            return None;
        }
        indices.push(value);
    }
    if indices.len() < 3 || indices.len() % 3 != 0 {
        return None;
    }
    Some(ScenePuppetMesh {
        bounds: ScenePuppetMeshBounds {
            left: min_x,
            top: min_y,
            width,
            height,
            anchor_x: (-min_x / width).clamp(0.0, 1.0),
            anchor_y: (-min_y / height).clamp(0.0, 1.0),
        },
        vertices,
        indices,
        skin_vertices,
        skin: None,
        clips: Vec::new(),
    })
}

fn scene_puppet_attachment_chain_position(
    bone_index: usize,
    bones: &[ScenePuppetBone],
) -> Option<(f64, f64, f64)> {
    let mut current = Some(bone_index);
    let mut visited = BTreeSet::new();
    let mut accumulated = (0.0, 0.0, 0.0);
    while let Some(index) = current {
        if !visited.insert(index) {
            return None;
        }
        let bone = bones.get(index)?;
        accumulated.0 += bone.translation.0;
        accumulated.1 += bone.translation.1;
        accumulated.2 += bone.translation.2;
        current = bone.parent.filter(|parent| *parent != index);
    }
    Some(accumulated)
}

fn scene_puppet_attachment_target_position(
    bone_index: usize,
    bones: &[ScenePuppetBone],
) -> Option<(usize, (f64, f64, f64))> {
    let mut current = Some(bone_index);
    let mut visited = BTreeSet::new();
    let mut accumulated = (0.0, 0.0, 0.0);
    while let Some(index) = current {
        if !visited.insert(index) {
            return None;
        }
        let bone = bones.get(index)?;
        if let Some(target_position) = bone.target_position {
            return Some((
                index,
                (
                    target_position.0 + accumulated.0,
                    target_position.1 + accumulated.1,
                    target_position.2 + accumulated.2,
                ),
            ));
        }
        accumulated.0 += bone.translation.0;
        accumulated.1 += bone.translation.1;
        accumulated.2 += bone.translation.2;
        current = bone.parent.filter(|parent| *parent != index);
    }
    None
}

fn scene_puppet_bone_target_position(
    info: &str,
    frame_size: SceneWeModelFrameSize,
) -> Option<(f64, f64, f64)> {
    let object = serde_json::from_str::<Value>(info).ok()?;
    let tp = object.get("tp").and_then(Value::as_str)?;
    let (x, y, z) = vector3_components_from_value(&Value::String(tp.to_owned()))?;
    Some((
        x - f64::from(frame_size.width) * 0.5,
        y - f64::from(frame_size.height) * 0.5,
        z,
    ))
}

fn scene_puppet_skin_vertices_valid(vertices: &[ScenePuppetSkinVertex], bone_count: usize) -> bool {
    !vertices.is_empty()
        && vertices.iter().any(|vertex| {
            vertex
                .weights
                .iter()
                .any(|weight| weight.is_finite() && *weight > f64::EPSILON)
        })
        && vertices.iter().all(|vertex| {
            vertex
                .weights
                .iter()
                .all(|weight| weight.is_finite() && *weight >= 0.0 && *weight <= 1.0 + f64::EPSILON)
                && vertex
                    .bone_indices
                    .iter()
                    .zip(vertex.weights.iter())
                    .all(|(bone_index, weight)| *weight <= f64::EPSILON || *bone_index < bone_count)
        })
}

fn scene_parse_puppet_inverse_bind_matrices(
    bytes: &[u8],
    after_offset: usize,
    bones: &[ScenePuppetBone],
) -> Result<Vec<[f64; 16]>, String> {
    let bone_count = bones.len();
    let Some(mdle_offset) = scene_find_mdl_section_after(bytes, b"MDLE", after_offset) else {
        return scene_puppet_bind_inverse_matrices_from_mdls(bones);
    };
    let (mdle_end, matrix_count, mut position) =
        scene_mdl_section_end_count_start(bytes, mdle_offset, "MDLE")?;
    if matrix_count != bone_count {
        return Err(format!(
            "Wallpaper Engine puppet MDLE matrix count {matrix_count} does not match MDLS bone count {bone_count}."
        ));
    }
    let mut matrices = Vec::with_capacity(matrix_count);
    for bone_index in 0..matrix_count {
        let matrix = scene_take_mdl_matrix(bytes, &mut position, mdle_end)?;
        for (component, value) in matrix.iter().enumerate() {
            if !value.is_finite() {
                return Err(format!(
                    "Wallpaper Engine puppet MDLE bone {bone_index} matrix component {component} must be finite."
                ));
            }
        }
        matrices.push(matrix);
    }
    Ok(matrices)
}

fn scene_puppet_bind_inverse_matrices_from_mdls(
    bones: &[ScenePuppetBone],
) -> Result<Vec<[f64; 16]>, String> {
    let bind_world = scene_puppet_world_matrices(
        bones.iter().map(|bone| bone.parent),
        bones.iter().map(|bone| bone.bind.matrix()),
    )
    .ok_or_else(|| "Wallpaper Engine puppet MDLS bind matrix hierarchy is invalid.".to_owned())?;
    bind_world
        .into_iter()
        .enumerate()
        .map(|(bone_index, matrix)| {
            scene_puppet_inverse_affine_matrix(matrix).ok_or_else(|| {
                format!(
                    "Wallpaper Engine puppet MDLS bone {bone_index} bind matrix is not invertible."
                )
            })
        })
        .collect()
}

fn scene_puppet_world_matrices<P, M>(parents: P, local_matrices: M) -> Option<Vec<[f64; 16]>>
where
    P: IntoIterator<Item = Option<usize>>,
    M: IntoIterator<Item = [f64; 16]>,
{
    let parents = parents.into_iter().collect::<Vec<_>>();
    let locals = local_matrices.into_iter().collect::<Vec<_>>();
    if parents.len() != locals.len() {
        return None;
    }
    let mut worlds = vec![scene_puppet_identity_matrix(); locals.len()];
    for index in 0..locals.len() {
        worlds[index] = if let Some(parent) = parents[index] {
            if parent >= index {
                return None;
            }
            scene_puppet_matrix_mul(worlds[parent], locals[index])
        } else {
            locals[index]
        };
    }
    Some(worlds)
}

fn scene_puppet_identity_matrix() -> [f64; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn scene_puppet_translation_matrix(translation: [f64; 3]) -> [f64; 16] {
    let mut matrix = scene_puppet_identity_matrix();
    matrix[12] = translation[0];
    matrix[13] = translation[1];
    matrix[14] = translation[2];
    matrix
}

fn scene_puppet_scale_matrix(scale: [f64; 3]) -> [f64; 16] {
    [
        scale[0], 0.0, 0.0, 0.0, 0.0, scale[1], 0.0, 0.0, 0.0, 0.0, scale[2], 0.0, 0.0, 0.0, 0.0,
        1.0,
    ]
}

fn scene_puppet_rotation_x_matrix(angle: f64) -> [f64; 16] {
    let (sin, cos) = angle.sin_cos();
    [
        1.0, 0.0, 0.0, 0.0, 0.0, cos, sin, 0.0, 0.0, -sin, cos, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn scene_puppet_rotation_y_matrix(angle: f64) -> [f64; 16] {
    let (sin, cos) = angle.sin_cos();
    [
        cos, 0.0, -sin, 0.0, 0.0, 1.0, 0.0, 0.0, sin, 0.0, cos, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn scene_puppet_rotation_z_matrix(angle: f64) -> [f64; 16] {
    let (sin, cos) = angle.sin_cos();
    [
        cos, sin, 0.0, 0.0, -sin, cos, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn scene_puppet_matrix_mul(a: [f64; 16], b: [f64; 16]) -> [f64; 16] {
    let mut output = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            output[column * 4 + row] = (0..4)
                .map(|index| a[index * 4 + row] * b[column * 4 + index])
                .sum();
        }
    }
    output
}

fn scene_puppet_inverse_affine_matrix(matrix: [f64; 16]) -> Option<[f64; 16]> {
    let a00 = matrix[0];
    let a01 = matrix[4];
    let a02 = matrix[8];
    let a10 = matrix[1];
    let a11 = matrix[5];
    let a12 = matrix[9];
    let a20 = matrix[2];
    let a21 = matrix[6];
    let a22 = matrix[10];
    let det = a00 * (a11 * a22 - a12 * a21) - a01 * (a10 * a22 - a12 * a20)
        + a02 * (a10 * a21 - a11 * a20);
    if !det.is_finite() || det.abs() <= f64::EPSILON {
        return None;
    }
    let inv_det = 1.0 / det;
    let b00 = (a11 * a22 - a12 * a21) * inv_det;
    let b01 = (a02 * a21 - a01 * a22) * inv_det;
    let b02 = (a01 * a12 - a02 * a11) * inv_det;
    let b10 = (a12 * a20 - a10 * a22) * inv_det;
    let b11 = (a00 * a22 - a02 * a20) * inv_det;
    let b12 = (a02 * a10 - a00 * a12) * inv_det;
    let b20 = (a10 * a21 - a11 * a20) * inv_det;
    let b21 = (a01 * a20 - a00 * a21) * inv_det;
    let b22 = (a00 * a11 - a01 * a10) * inv_det;
    let tx = matrix[12];
    let ty = matrix[13];
    let tz = matrix[14];
    Some([
        b00,
        b10,
        b20,
        0.0,
        b01,
        b11,
        b21,
        0.0,
        b02,
        b12,
        b22,
        0.0,
        -(b00 * tx + b01 * ty + b02 * tz),
        -(b10 * tx + b11 * ty + b12 * tz),
        -(b20 * tx + b21 * ty + b22 * tz),
        1.0,
    ])
}

fn scene_parse_puppet_animation_clips(
    bytes: &[u8],
    after_offset: usize,
    bone_count: usize,
) -> Result<Vec<ScenePuppetAnimationClip>, String> {
    let Some(mdla_offset) = scene_find_mdl_section_after(bytes, b"MDLA", after_offset) else {
        return Ok(Vec::new());
    };
    let (mdla_end, clip_count, mut position) =
        scene_mdl_section_end_count_start(bytes, mdla_offset, "MDLA")?;
    let mut clips = Vec::with_capacity(clip_count);
    for clip_index in 0..clip_count {
        while position < mdla_end && bytes.get(position) == Some(&0) {
            position += 1;
        }
        let clip_id = scene_take_u32_le(bytes, &mut position, mdla_end, "MDLA clip id")?;
        scene_skip_bytes(bytes, &mut position, mdla_end, 4, "MDLA clip flags")?;
        let name = scene_take_mdl_c_string(bytes, &mut position, mdla_end, "MDLA clip name")?;
        let playback =
            scene_take_mdl_c_string(bytes, &mut position, mdla_end, "MDLA clip playback")?;
        let fps = scene_take_f32_le(bytes, &mut position, mdla_end, "MDLA clip fps")?;
        let frame_count =
            scene_take_u32_le(bytes, &mut position, mdla_end, "MDLA clip frame count")?;
        scene_skip_bytes(
            bytes,
            &mut position,
            mdla_end,
            4,
            "MDLA clip reserved frame field",
        )?;
        let clip_bone_count = usize::try_from(scene_take_u32_le(
            bytes,
            &mut position,
            mdla_end,
            "MDLA clip bone count",
        )?)
        .map_err(|_| "Wallpaper Engine puppet MDLA bone count overflowed.".to_owned())?;
        if clip_bone_count != bone_count {
            return Err(format!(
                "Wallpaper Engine puppet MDLA clip {clip_index} bone count {clip_bone_count} does not match MDLS bone count {bone_count}."
            ));
        }
        let mut bones = Vec::with_capacity(clip_bone_count);
        let expected_samples = usize::try_from(frame_count)
            .ok()
            .and_then(|frame_count| frame_count.checked_add(1))
            .ok_or_else(|| "Wallpaper Engine puppet MDLA frame count overflowed.".to_owned())?;
        for bone_index in 0..clip_bone_count {
            scene_skip_bytes(bytes, &mut position, mdla_end, 4, "MDLA bone track flags")?;
            let byte_count = usize::try_from(scene_take_u32_le(
                bytes,
                &mut position,
                mdla_end,
                "MDLA bone frame byte count",
            )?)
            .map_err(|_| "Wallpaper Engine puppet MDLA byte count overflowed.".to_owned())?;
            if byte_count % 36 != 0 {
                return Err(format!(
                    "Wallpaper Engine puppet MDLA clip {clip_index} bone {bone_index} has invalid frame byte count {byte_count}."
                ));
            }
            let sample_count = byte_count / 36;
            if sample_count != expected_samples {
                return Err(format!(
                    "Wallpaper Engine puppet MDLA clip {clip_index} bone {bone_index} has {sample_count} samples, expected {expected_samples}."
                ));
            }
            let mut frames = Vec::with_capacity(sample_count);
            for _ in 0..sample_count {
                frames.push(ScenePuppetTransform {
                    translation: [
                        scene_take_f32_le(
                            bytes,
                            &mut position,
                            mdla_end,
                            "MDLA frame translation x",
                        )?,
                        scene_take_f32_le(
                            bytes,
                            &mut position,
                            mdla_end,
                            "MDLA frame translation y",
                        )?,
                        scene_take_f32_le(
                            bytes,
                            &mut position,
                            mdla_end,
                            "MDLA frame translation z",
                        )?,
                    ],
                    rotation: [
                        scene_take_f32_le(bytes, &mut position, mdla_end, "MDLA frame rotation x")?,
                        scene_take_f32_le(bytes, &mut position, mdla_end, "MDLA frame rotation y")?,
                        scene_take_f32_le(bytes, &mut position, mdla_end, "MDLA frame rotation z")?,
                    ],
                    scale: [
                        scene_take_f32_le(bytes, &mut position, mdla_end, "MDLA frame scale x")?,
                        scene_take_f32_le(bytes, &mut position, mdla_end, "MDLA frame scale y")?,
                        scene_take_f32_le(bytes, &mut position, mdla_end, "MDLA frame scale z")?,
                    ],
                    opacity: 1.0,
                });
            }
            bones.push(ScenePuppetAnimationBone { frames });
        }
        let opacity_tracks = scene_parse_puppet_animation_opacity_tracks(
            bytes,
            &mut position,
            mdla_end,
            clip_bone_count,
            expected_samples,
        )
        .unwrap_or_default();
        for (bone_index, opacity_frames) in opacity_tracks.into_iter().enumerate() {
            let Some(bone) = bones.get_mut(bone_index) else {
                continue;
            };
            for (frame, opacity) in bone.frames.iter_mut().zip(opacity_frames) {
                frame.opacity = opacity.clamp(0.0, 1.0);
            }
        }
        clips.push(ScenePuppetAnimationClip {
            id: clip_id,
            name: (!name.is_empty()).then_some(name),
            fps,
            frame_count,
            looping: normalize_project_key(&playback) == "loop",
            bones,
        });
    }
    Ok(clips)
}

fn scene_parse_puppet_animation_opacity_tracks(
    bytes: &[u8],
    position: &mut usize,
    mdla_end: usize,
    bone_count: usize,
    sample_count: usize,
) -> Option<Vec<Vec<f64>>> {
    let track_bytes = sample_count.checked_mul(4)?;
    let block_bytes = track_bytes.checked_add(8)?;
    let start = *position;
    for preamble_bytes in 0..=16usize {
        let base = start.checked_add(preamble_bytes)?;
        let total_bytes = bone_count.checked_mul(block_bytes)?;
        let end = base.checked_add(total_bytes)?;
        if end > mdla_end || end > bytes.len() {
            continue;
        }
        let mut tracks = Vec::with_capacity(bone_count);
        let mut valid = true;
        for bone_index in 0..bone_count {
            let block = base + bone_index * block_bytes;
            let byte_count = usize::try_from(scene_read_u32_le_at(bytes, block + 4)?).ok()?;
            if byte_count != track_bytes {
                valid = false;
                break;
            }
            let data_start = block + 8;
            let data_end = data_start + track_bytes;
            let mut frames = Vec::with_capacity(sample_count);
            for offset in (data_start..data_end).step_by(4) {
                let value = f64::from(scene_read_f32_le_at(bytes, offset)?);
                if !value.is_finite() {
                    valid = false;
                    break;
                }
                frames.push(value.clamp(0.0, 1.0));
            }
            if !valid || frames.len() != sample_count {
                valid = false;
                break;
            }
            tracks.push(frames);
        }
        if valid {
            *position = end;
            return Some(tracks);
        }
    }
    None
}

fn scene_puppet_transform_from_mdl_matrix(matrix: [f64; 16]) -> ScenePuppetTransform {
    let scale_x = (matrix[0] * matrix[0] + matrix[1] * matrix[1] + matrix[2] * matrix[2])
        .sqrt()
        .max(f64::EPSILON);
    let scale_y = (matrix[4] * matrix[4] + matrix[5] * matrix[5] + matrix[6] * matrix[6])
        .sqrt()
        .max(f64::EPSILON);
    let scale_z = (matrix[8] * matrix[8] + matrix[9] * matrix[9] + matrix[10] * matrix[10])
        .sqrt()
        .max(f64::EPSILON);
    ScenePuppetTransform {
        translation: [matrix[12], matrix[13], matrix[14]],
        rotation: [0.0, 0.0, (matrix[1] / scale_x).atan2(matrix[0] / scale_x)],
        scale: [scale_x, scale_y, scale_z],
        opacity: 1.0,
    }
}

fn scene_find_mdl_section(bytes: &[u8], section: &[u8; 4]) -> Option<usize> {
    bytes
        .windows(section.len())
        .position(|window| window == section)
}

fn scene_find_mdl_section_after(bytes: &[u8], section: &[u8; 4], offset: usize) -> Option<usize> {
    let haystack = bytes.get(offset..)?;
    haystack
        .windows(section.len())
        .position(|window| window == section)
        .map(|relative| offset + relative)
}

fn scene_mdl_section_end_count_start(
    bytes: &[u8],
    section_offset: usize,
    section_name: &str,
) -> Result<(usize, usize, usize), String> {
    for metadata_offset in [section_offset + 9, section_offset + 8] {
        let Some(end) = scene_read_u32_le_at(bytes, metadata_offset)
            .and_then(|value| usize::try_from(value).ok())
        else {
            continue;
        };
        let Some(count) = scene_read_u32_le_at(bytes, metadata_offset + 4)
            .and_then(|value| usize::try_from(value).ok())
        else {
            continue;
        };
        let start = metadata_offset + 8;
        if start < end && end <= bytes.len() && count <= 4096 {
            return Ok((end, count, start));
        }
    }
    Err(format!(
        "Wallpaper Engine puppet {section_name} header is malformed."
    ))
}

fn scene_mdat_section_end_count_start(
    bytes: &[u8],
    section_offset: usize,
) -> Result<(usize, usize, usize), String> {
    for metadata_offset in [section_offset + 9, section_offset + 8] {
        let Some(end) = scene_read_u32_le_at(bytes, metadata_offset)
            .and_then(|value| usize::try_from(value).ok())
        else {
            continue;
        };
        let Some(count) = scene_read_u16_le_at(bytes, metadata_offset + 4).map(usize::from) else {
            continue;
        };
        let start = metadata_offset + 6;
        if start <= end && end <= bytes.len() && count <= 4096 {
            return Ok((end, count, start));
        }
    }
    Err("Wallpaper Engine puppet MDAT header is malformed.".to_owned())
}

fn scene_take_i32_le(
    bytes: &[u8],
    position: &mut usize,
    end: usize,
    field: &str,
) -> Result<i32, String> {
    let value = scene_take_u32_le(bytes, position, end, field)?;
    Ok(i32::from_le_bytes(value.to_le_bytes()))
}

fn scene_take_u32_le(
    bytes: &[u8],
    position: &mut usize,
    end: usize,
    field: &str,
) -> Result<u32, String> {
    let start = *position;
    let value = scene_read_u32_le_at(bytes, start)
        .ok_or_else(|| format!("Wallpaper Engine puppet {field} is truncated."))?;
    *position = start + 4;
    if *position > end {
        return Err(format!(
            "Wallpaper Engine puppet {field} extends outside its section."
        ));
    }
    Ok(value)
}

fn scene_take_u16_le(
    bytes: &[u8],
    position: &mut usize,
    end: usize,
    field: &str,
) -> Result<u16, String> {
    let start = *position;
    let value = scene_read_u16_le_at(bytes, start)
        .ok_or_else(|| format!("Wallpaper Engine puppet {field} is truncated."))?;
    *position = start + 2;
    if *position > end {
        return Err(format!(
            "Wallpaper Engine puppet {field} extends outside its section."
        ));
    }
    Ok(value)
}

fn scene_take_f32_le(
    bytes: &[u8],
    position: &mut usize,
    end: usize,
    field: &str,
) -> Result<f64, String> {
    let start = *position;
    let value = scene_read_f32_le_at(bytes, start)
        .ok_or_else(|| format!("Wallpaper Engine puppet {field} is truncated."))?;
    *position = start + 4;
    if *position > end {
        return Err(format!(
            "Wallpaper Engine puppet {field} extends outside its section."
        ));
    }
    if !value.is_finite() {
        return Err(format!("Wallpaper Engine puppet {field} must be finite."));
    }
    Ok(value)
}

fn scene_take_mdl_matrix(
    bytes: &[u8],
    position: &mut usize,
    end: usize,
) -> Result<[f64; 16], String> {
    let start = *position;
    let matrix_end = start
        .checked_add(64)
        .ok_or_else(|| "Wallpaper Engine puppet matrix offset overflowed.".to_owned())?;
    if matrix_end > end || matrix_end > bytes.len() {
        return Err("Wallpaper Engine puppet matrix is truncated.".to_owned());
    }
    let mut matrix = [0.0; 16];
    for (index, value) in matrix.iter_mut().enumerate() {
        let offset = start + index * 4;
        let bytes = bytes[offset..offset + 4]
            .try_into()
            .expect("matrix float slice length checked");
        *value = f32::from_le_bytes(bytes) as f64;
    }
    *position = matrix_end;
    Ok(matrix)
}

fn scene_skip_bytes(
    bytes: &[u8],
    position: &mut usize,
    end: usize,
    count: usize,
    field: &str,
) -> Result<(), String> {
    let next = position
        .checked_add(count)
        .ok_or_else(|| format!("Wallpaper Engine puppet {field} offset overflowed."))?;
    if next > end || next > bytes.len() {
        return Err(format!("Wallpaper Engine puppet {field} is truncated."));
    }
    *position = next;
    Ok(())
}

fn scene_take_mdl_c_string(
    bytes: &[u8],
    position: &mut usize,
    end: usize,
    field: &str,
) -> Result<String, String> {
    let section = bytes
        .get(*position..end)
        .ok_or_else(|| format!("Wallpaper Engine puppet {field} section is truncated."))?;
    let nul = section
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| format!("Wallpaper Engine puppet {field} is not NUL terminated."))?;
    let value = std::str::from_utf8(&section[..nul])
        .map_err(|err| format!("Wallpaper Engine puppet {field} is not UTF-8: {err}."))?
        .to_owned();
    *position += nul + 1;
    Ok(value)
}

fn scene_read_u32_le_at(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn scene_read_u16_le_at(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn scene_read_f32_le_at(bytes: &[u8], offset: usize) -> Option<f64> {
    Some(f32::from_le_bytes(bytes.get(offset..offset + 4)?.try_into().ok()?) as f64)
}

fn scene_model_solid_layer(source_model: Option<&SceneSourceModelConversion>) -> bool {
    source_model
        .and_then(|model| model.value.get("solid_layer"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn scene_builtin_util_model(model_path: &str) -> Option<SceneSourceModelConversion> {
    let normalized = model_path.replace('\\', "/").to_ascii_lowercase();
    let utility = match normalized.as_str() {
        "models/util/fullscreenlayer.json" => "fullscreenlayer",
        "models/util/composelayer.json" => "composelayer",
        "models/util/solidlayer.json" => "solidlayer",
        _ => return None,
    };
    let mut model = Map::new();
    model.insert("source".to_owned(), Value::String(model_path.to_owned()));
    model.insert("builtin".to_owned(), Value::Bool(true));
    model.insert("utility".to_owned(), Value::String(utility.to_owned()));
    if utility == "solidlayer" {
        model.insert("solid_layer".to_owned(), Value::Bool(true));
    } else {
        model.insert("passthrough".to_owned(), Value::Bool(true));
    }
    Some(SceneSourceModelConversion {
        value: Value::Object(model),
        render_kind: Some(if utility == "solidlayer" {
            "rectangle"
        } else {
            "script"
        }),
        render_resource: None,
        render_properties: None,
        render_size: None,
        render_bounds: None,
        render_mesh: None,
        original_path: model_path.to_owned(),
    })
}

fn scene_controller_from_object(
    object: &Map<String, Value>,
    node_id: &str,
    source_model: Option<&SceneSourceModelConversion>,
) -> Option<(Value, SceneControllerIr)> {
    let script_properties = scene_script_properties_from_object(object)?;
    let target_layer = scene_controller_target_layer(script_properties)?;
    if target_layer.trim().is_empty() {
        return None;
    }
    let utility = source_model
        .and_then(|model| model.value.get("utility").and_then(Value::as_str))
        .unwrap_or("visibility-script");
    if utility == "visibility-script"
        && !scene_controller_script_properties_have_timed_visibility(script_properties)
    {
        return None;
    }
    let default_hide_target = scene_script_property_bool(
        script_properties,
        &["defaultHideTarget", "defaulthidetarget"],
    )
    .unwrap_or(false);
    let controller = SceneControllerIr::from_wallpaper_engine_utility(
        node_id,
        utility,
        &target_layer,
        default_hide_target,
        script_properties,
    );
    Some((controller.metadata_value(), controller))
}

fn scene_audio_controller_from_object(
    object: &Map<String, Value>,
) -> Option<SceneAudioControllerIr> {
    let visible = object.get("visible").and_then(Value::as_object)?;
    SceneAudioControllerIr::from_wallpaper_engine_visible_script(visible)
}

fn scene_collect_audio_controllers(value: &Value, context: &mut SceneDocumentBuildContext) {
    match value {
        Value::Object(object) => {
            if let Some(audio_controller) = scene_audio_controller_from_object(object) {
                scene_record_native_script_lowering(context);
                push_unique(
                    &mut context.converted_features,
                    "wallpaper-engine-audio-controller-lowering",
                );
                push_unique(
                    &mut context.converted_features,
                    audio_controller.completed_feature_name(),
                );
                context.pending_audio_controllers.push(audio_controller);
            }
            for value in object.values() {
                scene_collect_audio_controllers(value, context);
            }
        }
        Value::Array(values) => {
            for value in values {
                scene_collect_audio_controllers(value, context);
            }
        }
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null => {}
    }
}

fn scene_object_visible_script(object: &Map<String, Value>) -> bool {
    object
        .get("visible")
        .and_then(Value::as_object)
        .and_then(|visible| visible.get("script"))
        .and_then(Value::as_str)
        .is_some()
}

fn scene_controller_script_properties_have_timed_visibility(
    script_properties: &Map<String, Value>,
) -> bool {
    [
        "targetLayerName",
        "targetlayername",
        "target_layer_name",
        "enableAutoControl",
        "enableautocontrol",
        "enable_auto_control",
        "showDuration",
        "showduration",
        "show_duration",
        "hideOnStart",
        "hideonstart",
        "hide_on_start",
        "fadeDuration",
        "fadeduration",
        "fade_duration",
    ]
    .iter()
    .any(|key| script_properties.contains_key(*key))
}

fn scene_controller_target_layer_from_script_properties(
    object: &Map<String, Value>,
) -> Option<String> {
    scene_script_properties_from_object(object).and_then(scene_controller_target_layer)
}

fn scene_controller_target_layer(script_properties: &Map<String, Value>) -> Option<String> {
    string_field(
        script_properties,
        &[
            "targetLayerId",
            "targetlayerid",
            "target_layer_id",
            "targetLayerName",
            "targetlayername",
            "target_layer_name",
        ],
    )
}

fn scene_script_properties_from_object(object: &Map<String, Value>) -> Option<&Map<String, Value>> {
    object
        .get("visible")
        .and_then(Value::as_object)
        .and_then(|visible| visible.get("scriptproperties"))
        .or_else(|| object.get("scriptproperties"))
        .and_then(Value::as_object)
}

fn scene_script_property_bool(object: &Map<String, Value>, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .filter_map(|key| object.get(*key))
        .find_map(value_to_bool)
}

fn scene_model_frame_size(model_object: &Map<String, Value>) -> Option<SceneWeModelFrameSize> {
    let width = model_object.get("width").and_then(value_to_u32)?;
    let height = model_object.get("height").and_then(value_to_u32)?;
    if width == 0 || height == 0 {
        None
    } else {
        Some(SceneWeModelFrameSize { width, height })
    }
}

fn scene_frame_size_from_object_size(object: &Map<String, Value>) -> Option<SceneWeModelFrameSize> {
    let (width, height, _) = object.get("size").and_then(vector3_components_from_value)?;
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return None;
    }
    let width = width.round();
    let height = height.round();
    if width > f64::from(u32::MAX) || height > f64::from(u32::MAX) {
        return None;
    }
    Some(SceneWeModelFrameSize {
        width: width as u32,
        height: height as u32,
    })
}

fn scene_node_provenance_from_object(
    object: &Map<String, Value>,
    original_type: Option<&str>,
    source_path: Option<&str>,
    source_model: Option<&SceneSourceModelConversion>,
) -> Option<Value> {
    let mut provenance = Map::new();
    provenance.insert(
        "source_format".to_owned(),
        Value::String("wallpaper-engine-scene".to_owned()),
    );
    if let Some(source_id) = object.get("id").and_then(value_to_string) {
        provenance.insert("source_id".to_owned(), Value::String(source_id));
    }
    if let Some(parent_id) = object.get("parent").and_then(value_to_string) {
        provenance.insert("parent_id".to_owned(), Value::String(parent_id));
    }
    if let Some(attachment) = string_field(object, &["attachment"]) {
        provenance.insert("attachment".to_owned(), Value::String(attachment));
    }
    if let Some(lock_transforms) = object.get("locktransforms").and_then(Value::as_bool) {
        provenance.insert("lock_transforms".to_owned(), Value::Bool(lock_transforms));
    }
    if let Some(dependencies) = scene_dependencies_from_object(object) {
        provenance.insert("dependencies".to_owned(), dependencies);
    }
    if let Some(original_type) = original_type {
        provenance.insert(
            "original_type".to_owned(),
            Value::String(original_type.to_owned()),
        );
    }
    if let Some(path) =
        source_path.or_else(|| source_model.map(|model| model.original_path.as_str()))
    {
        provenance.insert("original_path".to_owned(), Value::String(path.to_owned()));
    }
    if let Some(transform) = scene_source_transform_from_object(object) {
        provenance.insert("transform".to_owned(), transform);
    }
    if let Some(source_model) = source_model {
        provenance.insert("model".to_owned(), source_model.value.clone());
    }
    for (source, target) in [
        ("particle", "particle"),
        ("animationlayers", "animation_layers"),
        ("instance", "instance"),
        ("instanceoverride", "instance_override"),
    ] {
        if let Some(value) = object.get(source) {
            provenance.insert(target.to_owned(), value.clone());
        }
    }
    if provenance.len() <= 1 {
        None
    } else {
        Some(Value::Object(provenance))
    }
}

fn scene_dependencies_from_object(object: &Map<String, Value>) -> Option<Value> {
    let dependencies = object.get("dependencies")?.as_array()?;
    let dependencies = dependencies
        .iter()
        .filter_map(value_to_string)
        .map(Value::String)
        .collect::<Vec<_>>();
    if dependencies.is_empty() {
        None
    } else {
        Some(Value::Array(dependencies))
    }
}

fn scene_source_transform_from_object(object: &Map<String, Value>) -> Option<Value> {
    let mut transform = Map::new();
    for (source, target) in [
        ("origin", "origin"),
        ("angles", "angles"),
        ("scale", "scale"),
        ("pivot", "pivot"),
        ("size", "size"),
    ] {
        if let Some(value) = object.get(source).and_then(scene_vector3_from_value) {
            transform.insert(target.to_owned(), value);
        }
    }
    if let Some(alignment) = string_field(object, &["alignment"]) {
        transform.insert("alignment".to_owned(), Value::String(alignment));
    }
    if transform.is_empty() {
        None
    } else {
        Some(Value::Object(transform))
    }
}

fn scene_audio_cues_from_object(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    object: &Map<String, Value>,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
    resources: &mut Vec<Value>,
) -> Vec<Value> {
    scene_sound_sources_from_object(object)
        .into_iter()
        .map(|source| {
            let mut cue = Map::new();
            cue.insert("source".to_owned(), Value::String(source.clone()));
            if let Some(resource) = scene_copy_resource_as(
                project,
                output_dir,
                &source,
                "audio",
                Some("scene-audio"),
                report,
                context,
                resources,
            ) {
                cue.insert("resource".to_owned(), Value::String(resource));
            }
            if let Some(playback_mode) = string_field(object, &["playbackmode"]) {
                cue.insert("playback_mode".to_owned(), Value::String(playback_mode));
            }
            if let Some(volume) = object.get("volume") {
                cue.insert("volume".to_owned(), volume.clone());
            }
            if let Some(start_silent) = object.get("startsilent").and_then(value_to_bool) {
                cue.insert("start_silent".to_owned(), Value::Bool(start_silent));
            }
            cue
        })
        .map(Value::Object)
        .collect()
}

fn scene_sound_sources_from_object(object: &Map<String, Value>) -> Vec<String> {
    match object.get("sound") {
        Some(Value::String(source)) => vec![source.clone()],
        Some(Value::Array(sources)) => sources.iter().filter_map(value_to_string).collect(),
        _ => Vec::new(),
    }
}

fn scene_material_textures(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    material_json: &Value,
    frame_size: Option<SceneWeModelFrameSize>,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
    resources: &mut Vec<Value>,
) -> (
    Vec<String>,
    Vec<String>,
    Option<String>,
    Option<Value>,
    Option<&'static str>,
) {
    let texture_paths = scene_material_texture_paths(material_json);
    let spritesheet_enabled = scene_material_spritesheet_enabled(material_json);
    let mut texture_resources = Vec::new();
    let mut render_resource = None;
    let mut render_properties = scene_material_runtime_properties(material_json);
    let mut render_kind = None;
    for texture in &texture_paths {
        if texture.starts_with("_rt_") {
            scene_push_unsupported(
                context,
                "we-runtime-texture",
                "Wallpaper Engine runtime texture was preserved as a texture reference; it is not a standalone project asset.",
                Some(texture),
            );
            continue;
        }
        if let Some(resource) = scene_generate_builtin_particle_texture_resource(
            output_dir, texture, report, context, resources,
        ) {
            if render_resource.is_none() {
                render_resource = Some(resource.clone());
                render_kind = Some("image");
            }
            texture_resources.push(resource);
            continue;
        }
        if texture.ends_with(".tex") {
            if let Some(decoded) = scene_copy_decoded_tex_resource_as(
                project,
                output_dir,
                texture,
                frame_size,
                spritesheet_enabled,
                report,
                context,
                resources,
            ) {
                if render_resource.is_none() {
                    render_resource = Some(decoded.resource_id.clone());
                    render_kind = Some(decoded.render_kind);
                }
                if let Some(spritesheet) = decoded.spritesheet {
                    scene_merge_render_properties(
                        &mut render_properties,
                        json!({ "spritesheet": spritesheet }),
                    );
                }
                texture_resources.push(decoded.resource_id);
            } else {
                scene_push_unsupported(
                    context,
                    "we-tex-decode",
                    "Wallpaper Engine .tex texture is preserved as an original source reference but not emitted as a native scene runtime resource yet.",
                    Some(texture),
                );
            }
            continue;
        }
        let resource_kind = if is_image_path(texture) {
            "image"
        } else if is_video_path(texture) {
            "video"
        } else {
            "texture"
        };
        let raw_resource = scene_copy_resource_as(
            project,
            output_dir,
            texture,
            resource_kind,
            Some("we-material-texture"),
            report,
            context,
            resources,
        );
        if let Some(resource) = raw_resource {
            if render_resource.is_none() && (is_image_path(texture) || is_video_path(texture)) {
                render_resource = Some(resource.clone());
                render_kind = Some(resource_kind);
            }
            texture_resources.push(resource);
        }
    }
    if render_resource.is_some() {
        push_unique(
            &mut context.converted_features,
            "scene-we-material-graph-runtime",
        );
    }
    (
        texture_paths,
        texture_resources,
        render_resource,
        render_properties,
        render_kind,
    )
}

fn scene_merge_render_properties(properties: &mut Option<Value>, update: Value) {
    let Some(update) = update.as_object() else {
        return;
    };
    let entry = properties
        .get_or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut();
    let Some(entry) = entry else {
        return;
    };
    for (key, value) in update {
        entry.insert(key.clone(), value.clone());
    }
}

fn scene_material_runtime_properties(material_json: &Value) -> Option<Value> {
    let passes = scene_material_runtime_passes(material_json);
    (!passes.is_empty()).then(|| {
        json!({
            "material": {
                "runtime": "wallpaper-engine-material",
                "passes": passes
            }
        })
    })
}

fn scene_material_runtime_passes(material_json: &Value) -> Vec<Value> {
    let Some(passes) = material_json.get("passes").and_then(Value::as_array) else {
        return Vec::new();
    };
    passes
        .iter()
        .filter_map(Value::as_object)
        .map(|pass| {
            let mut output = Map::new();
            for key in ["shader", "blending", "cullmode", "depthtest", "depthwrite"] {
                if let Some(value) = pass.get(key) {
                    output.insert(key.to_owned(), value.clone());
                }
            }
            if let Some(combos) = pass.get("combos").and_then(scene_i64_map_from_value) {
                output.insert("combos".to_owned(), combos);
            }
            if let Some(values) = pass.get("constantshadervalues").and_then(Value::as_object) {
                output.insert(
                    "constant_shader_values".to_owned(),
                    Value::Object(values.clone()),
                );
            }
            if let Some(textures) = pass.get("textures") {
                output.insert("textures".to_owned(), textures.clone());
            }
            Value::Object(output)
        })
        .collect()
}

fn scene_material_texture_paths(material_json: &Value) -> Vec<String> {
    let Some(passes) = material_json.get("passes").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut textures = Vec::new();
    for pass in passes.iter().filter_map(Value::as_object) {
        if let Some(values) = pass.get("textures") {
            collect_scene_texture_references(values, &mut textures);
        }
    }
    textures
        .into_iter()
        .map(|texture| scene_material_texture_path(&texture))
        .collect()
}

fn collect_scene_texture_references(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::String(value) => output.push(value.clone()),
        Value::Array(values) => {
            for value in values {
                collect_scene_texture_references(value, output);
            }
        }
        Value::Object(object) => {
            if let Some(path) = string_field(object, &["path", "file", "source", "texture"]) {
                output.push(path);
            }
            for value in object.values() {
                if value.is_array() {
                    collect_scene_texture_references(value, output);
                }
            }
        }
        Value::Number(_) | Value::Bool(_) | Value::Null => {}
    }
}

fn scene_material_path(material: &str) -> String {
    if Path::new(material).extension().is_some() || material.contains('/') {
        material.to_owned()
    } else {
        format!("materials/{material}.json")
    }
}

fn scene_material_texture_path(texture: &str) -> String {
    if Path::new(texture).extension().is_some()
        || texture.starts_with("_rt_")
        || scene_builtin_particle_texture_stem(texture).is_some()
    {
        texture.to_owned()
    } else {
        format!("materials/{texture}.tex")
    }
}

fn scene_effect_texture_resource_from_reference(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    texture: &str,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
    resources: &mut Vec<Value>,
) -> Option<String> {
    let texture = scene_material_texture_path(texture);
    if texture.starts_with("_rt_") {
        scene_push_unsupported(
            context,
            "we-effect-runtime-texture",
            "Wallpaper Engine effect runtime texture was preserved as a texture reference; it is not a standalone project asset.",
            Some(&texture),
        );
        return None;
    }
    if let Some(resource) = scene_generate_builtin_particle_texture_resource(
        output_dir, &texture, report, context, resources,
    ) {
        push_unique(
            &mut context.converted_features,
            "scene-we-effect-texture-resource",
        );
        push_unique(
            &mut report.converted_features,
            "scene-we-effect-texture-resource",
        );
        return Some(resource);
    }
    if texture.ends_with(".tex") {
        let decoded = scene_copy_decoded_tex_resource_as(
            project, output_dir, &texture, None, false, report, context, resources,
        );
        if let Some(decoded) = decoded {
            push_unique(
                &mut context.converted_features,
                "scene-we-effect-texture-resource",
            );
            push_unique(
                &mut report.converted_features,
                "scene-we-effect-texture-resource",
            );
            return Some(decoded.resource_id);
        }
        scene_push_unsupported(
            context,
            "we-effect-tex-decode",
            "Wallpaper Engine effect .tex texture was preserved as an original source reference but not emitted as a native scene runtime resource.",
            Some(&texture),
        );
        return None;
    }
    if is_image_path(&texture) {
        let resource = scene_copy_resource_as(
            project,
            output_dir,
            &texture,
            "image",
            Some("we-effect-texture"),
            report,
            context,
            resources,
        );
        if resource.is_some() {
            push_unique(
                &mut context.converted_features,
                "scene-we-effect-texture-resource",
            );
            push_unique(
                &mut report.converted_features,
                "scene-we-effect-texture-resource",
            );
        }
        return resource;
    }
    None
}

fn read_scene_project_json(
    project: &WallpaperEngineProject,
    source_path: &str,
    feature: &str,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
) -> Option<Value> {
    let relative = match normalize_relative_path(source_path) {
        Ok(relative) => relative,
        Err(err) => {
            scene_push_unsupported(context, feature, &err.to_string(), Some(source_path));
            return None;
        }
    };
    let path = project.root.join(relative);
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) => {
            scene_push_unsupported(
                context,
                feature,
                &format!("Referenced Wallpaper Engine JSON resource could not be read: {err}."),
                Some(source_path),
            );
            report.warnings.push(format!(
                "Scene JSON resource {source_path:?} was referenced but not read at {}: {err}.",
                path.display()
            ));
            return None;
        }
    };
    match serde_json::from_str(&contents) {
        Ok(value) => Some(value),
        Err(err) => {
            scene_push_unsupported(
                context,
                feature,
                &format!("Referenced Wallpaper Engine JSON resource could not be parsed: {err}."),
                Some(source_path),
            );
            None
        }
    }
}

fn scene_resource_path_from_object(object: &Map<String, Value>) -> Option<String> {
    string_field(
        object,
        &[
            "path",
            "source",
            "file",
            "filename",
            "texture",
            "video",
            "src",
            "background",
        ],
    )
    .filter(|path| is_scene_resource_path(path))
}

fn scene_model_path_from_object(object: &Map<String, Value>) -> Option<String> {
    string_field(object, &["image"]).filter(|path| {
        Path::new(path)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(extension.to_ascii_lowercase().as_str(), "json" | "model")
            })
    })
}

fn is_scene_resource_path(path: &str) -> bool {
    is_image_path(path)
        || is_video_path(path)
        || Path::new(path)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| is_audio_extension(extension) || has_shader_extension(path))
}

fn is_video_path(value: &str) -> bool {
    Path::new(value)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "avi" | "m4v" | "mkv" | "mov" | "mp4" | "webm"
            )
        })
}

fn scene_resource_kind(kind: &str, source_path: &str) -> &'static str {
    if is_video_path(source_path) {
        "video"
    } else if is_image_path(source_path) {
        "image"
    } else if Path::new(source_path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(is_audio_extension)
    {
        "audio"
    } else if has_shader_extension(source_path) || kind == "shader" {
        "shader"
    } else if kind == "script" {
        "script"
    } else {
        "other"
    }
}

fn scene_copy_resource(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    source_path: &str,
    kind: &str,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
    resources: &mut Vec<Value>,
) -> Option<String> {
    scene_copy_resource_as(
        project,
        output_dir,
        source_path,
        scene_resource_kind(kind, source_path),
        None,
        report,
        context,
        resources,
    )
}

fn scene_copy_resource_as(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    source_path: &str,
    resource_kind: &str,
    role: Option<&str>,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
    resources: &mut Vec<Value>,
) -> Option<String> {
    let relative = match normalize_relative_path(source_path) {
        Ok(relative) => relative,
        Err(err) => {
            scene_push_unsupported(
                context,
                "resource-path",
                &err.to_string(),
                Some(source_path),
            );
            return None;
        }
    };
    let source = project.root.join(&relative);
    if !source.is_file() {
        scene_push_unsupported(
            context,
            "missing-resource",
            "Referenced Wallpaper Engine scene resource is missing from the project directory.",
            Some(source_path),
        );
        report.warnings.push(format!(
            "Scene resource {source_path:?} was referenced but not found at {}.",
            source.display()
        ));
        return None;
    }
    let extension = source
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!(".{}", extension.to_ascii_lowercase()))
        .unwrap_or_default();
    let stem = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("resource");
    let resource_id = scene_next_resource_id(context, resource_kind, stem);
    let dest_dir = output_dir
        .join("assets/scene-resources")
        .join(&context.resource_scope);
    if let Err(err) = fs::create_dir_all(&dest_dir) {
        report
            .errors
            .push(format!("Failed to create scene resource directory: {err}."));
        return None;
    }
    let dest = dest_dir.join(format!("{resource_id}{extension}"));
    if let Err(err) = fs::copy(&source, &dest) {
        report.errors.push(format!(
            "Failed to copy scene resource {} to {}: {err}.",
            source.display(),
            dest.display()
        ));
        return None;
    }
    let package_path = path_to_package_string(dest.strip_prefix(output_dir).unwrap_or(&dest));
    report.copied_assets.push(package_path.clone());
    let mut resource = json!({
        "id": resource_id,
        "type": resource_kind,
        "source": package_path,
        "original_source": source_path
    });
    if let Some(role) = role
        && let Some(object) = resource.as_object_mut()
    {
        object.insert("role".to_owned(), Value::String(role.to_owned()));
    }
    resources.push(resource);
    Some(resource_id)
}

fn scene_generate_builtin_particle_texture_resource(
    output_dir: &Path,
    source_path: &str,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
    resources: &mut Vec<Value>,
) -> Option<String> {
    let builtin = scene_builtin_particle_texture(source_path)?;
    if let Some(resource_id) = context
        .builtin_particle_texture_resources
        .get(&builtin.stem)
    {
        push_unique(
            &mut context.converted_features,
            "wallpaper-engine-builtin-particle-texture-dedup",
        );
        return Some(resource_id.clone());
    }
    let resource_id = scene_next_resource_id(context, "image", &builtin.stem);
    let dest_dir = output_dir
        .join("assets/scene-resources")
        .join(&context.resource_scope);
    if let Err(err) = fs::create_dir_all(&dest_dir) {
        report.errors.push(format!(
            "Failed to create built-in particle texture directory: {err}."
        ));
        return None;
    }
    let dest = dest_dir.join(format!("{resource_id}.gtex"));
    let image = scene_builtin_particle_texture_image(builtin.kind);
    if let Err(err) = gtex::write_bc7_gtex(&dest, &image) {
        report.errors.push(format!(
            "Failed to write built-in Wallpaper Engine particle texture {source_path:?} to {}: {err}.",
            dest.display()
        ));
        return None;
    }
    let package_path = path_to_package_string(dest.strip_prefix(output_dir).unwrap_or(&dest));
    report.generated_assets.push(package_path.clone());
    resources.push(json!({
        "id": resource_id,
        "type": "image",
        "source": package_path,
        "width": image.width,
        "height": image.height,
        "original_source": source_path,
        "role": "we-builtin-particle-texture"
    }));
    push_unique(
        &mut context.converted_features,
        "wallpaper-engine-builtin-particle-texture",
    );
    push_unique(
        &mut report.converted_features,
        "wallpaper-engine-builtin-particle-texture",
    );
    context
        .builtin_particle_texture_resources
        .insert(builtin.stem, resource_id.clone());
    Some(resource_id)
}

fn scene_builtin_particle_texture_stem(source_path: &str) -> Option<String> {
    scene_builtin_particle_texture(source_path).map(|texture| texture.stem)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SceneBuiltinParticleTexture {
    stem: String,
    kind: SceneBuiltinParticleTextureKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SceneBuiltinParticleTextureKind {
    Bubble,
    ChromaticDot,
    Flare,
    Halo,
    Splash,
}

fn scene_builtin_particle_texture(source_path: &str) -> Option<SceneBuiltinParticleTexture> {
    let normalized = source_path.replace('\\', "/").to_ascii_lowercase();
    let path = normalized
        .rsplit_once('.')
        .map(|(path, _)| path)
        .unwrap_or(&normalized);
    let stem = path
        .rsplit('/')
        .next()
        .filter(|stem| !stem.is_empty())
        .unwrap_or("particle");
    let kind = if path.starts_with("particle/bubbles/bubble") {
        SceneBuiltinParticleTextureKind::Bubble
    } else if path == "particle/chromaticdot" || path == "materials/particle/chromaticdot" {
        SceneBuiltinParticleTextureKind::ChromaticDot
    } else if path.starts_with("particle/light/flare_")
        || path.starts_with("materials/particle/light/flare_")
    {
        SceneBuiltinParticleTextureKind::Flare
    } else if path.starts_with("particle/halo_") || path.starts_with("materials/particle/halo_") {
        SceneBuiltinParticleTextureKind::Halo
    } else if path.starts_with("particle/water/splash_")
        || path.starts_with("materials/particle/water/splash_")
    {
        SceneBuiltinParticleTextureKind::Splash
    } else {
        return None;
    };
    Some(SceneBuiltinParticleTexture {
        stem: format!("we-builtin-{stem}"),
        kind,
    })
}

fn scene_builtin_particle_texture_image(kind: SceneBuiltinParticleTextureKind) -> SceneWeTexImage {
    match kind {
        SceneBuiltinParticleTextureKind::Bubble => scene_builtin_particle_bubble_image(),
        SceneBuiltinParticleTextureKind::ChromaticDot => {
            scene_builtin_particle_chromatic_dot_image()
        }
        SceneBuiltinParticleTextureKind::Flare => scene_builtin_particle_flare_image(),
        SceneBuiltinParticleTextureKind::Halo => scene_builtin_particle_halo_image(),
        SceneBuiltinParticleTextureKind::Splash => scene_builtin_particle_splash_image(),
    }
}

fn scene_builtin_particle_bubble_image() -> SceneWeTexImage {
    const SIZE: u32 = 64;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    let center = (SIZE as f64 - 1.0) * 0.5;
    let radius = center;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = (x as f64 - center) / radius;
            let dy = (y as f64 - center) / radius;
            let distance = (dx * dx + dy * dy).sqrt();
            let rim = (1.0 - ((distance - 0.58).abs() / 0.22)).clamp(0.0, 1.0);
            let fill = (1.0 - distance).clamp(0.0, 1.0).powf(1.8) * 0.35;
            let highlight_dx = dx + 0.32;
            let highlight_dy = dy + 0.35;
            let highlight_distance = (highlight_dx * highlight_dx + highlight_dy * highlight_dy)
                .sqrt()
                .min(1.0);
            let highlight = (1.0 - highlight_distance / 0.32).clamp(0.0, 1.0).powf(2.2);
            let alpha = ((rim * 0.65 + fill + highlight * 0.45)
                * (1.0 - (distance - 0.98).max(0.0) / 0.02).clamp(0.0, 1.0))
            .clamp(0.0, 1.0);
            let color_boost = (fill + highlight * 0.5).clamp(0.0, 1.0);
            let offset = ((y * SIZE + x) * 4) as usize;
            rgba[offset] = (190.0 + 65.0 * color_boost) as u8;
            rgba[offset + 1] = (225.0 + 30.0 * color_boost) as u8;
            rgba[offset + 2] = 255;
            rgba[offset + 3] = (alpha * 255.0).round() as u8;
        }
    }
    SceneWeTexImage {
        width: SIZE,
        height: SIZE,
        rgba,
        r8: None,
    }
}

fn scene_builtin_particle_halo_image() -> SceneWeTexImage {
    scene_builtin_particle_radial_image(|distance, dx, dy| {
        let ring = (1.0 - ((distance - 0.52).abs() / 0.20)).clamp(0.0, 1.0);
        let core = (1.0 - distance / 0.72).clamp(0.0, 1.0).powf(2.8) * 0.28;
        let alpha = (ring.powf(1.8) * 0.72 + core).clamp(0.0, 1.0);
        let tint = (1.0 - (dx * 0.25 + dy * 0.18).abs()).clamp(0.0, 1.0);
        [
            (210.0 + 45.0 * tint) as u8,
            (228.0 + 27.0 * tint) as u8,
            255,
            (alpha * 255.0).round() as u8,
        ]
    })
}

fn scene_builtin_particle_flare_image() -> SceneWeTexImage {
    scene_builtin_particle_radial_image(|distance, dx, dy| {
        let horizontal = (1.0 - dy.abs() / 0.12).clamp(0.0, 1.0)
            * (1.0 - dx.abs() / 1.0).clamp(0.0, 1.0).powf(0.55);
        let vertical = (1.0 - dx.abs() / 0.18).clamp(0.0, 1.0)
            * (1.0 - dy.abs() / 0.78).clamp(0.0, 1.0).powf(1.2)
            * 0.4;
        let core = (1.0 - distance / 0.34).clamp(0.0, 1.0).powf(1.6);
        let alpha = (horizontal * 0.72 + vertical + core * 0.85).clamp(0.0, 1.0);
        [
            255,
            (214.0 + core * 41.0) as u8,
            (150.0 + core * 105.0) as u8,
            (alpha * 255.0).round() as u8,
        ]
    })
}

fn scene_builtin_particle_splash_image() -> SceneWeTexImage {
    scene_builtin_particle_radial_image(|distance, dx, dy| {
        let droplet = (1.0 - distance / 0.62).clamp(0.0, 1.0).powf(1.5);
        let crown = (1.0 - ((distance - 0.46).abs() / 0.12)).clamp(0.0, 1.0)
            * (1.0 - (dy + 0.22).abs() / 0.72).clamp(0.0, 1.0);
        let streak = (1.0 - dx.abs() / 0.18).clamp(0.0, 1.0)
            * (1.0 - (dy + 0.18).abs() / 0.92).clamp(0.0, 1.0)
            * 0.42;
        let alpha = (droplet * 0.58 + crown * 0.38 + streak).clamp(0.0, 1.0);
        [
            (160.0 + droplet * 55.0) as u8,
            (214.0 + droplet * 32.0) as u8,
            255,
            (alpha * 255.0).round() as u8,
        ]
    })
}

fn scene_builtin_particle_chromatic_dot_image() -> SceneWeTexImage {
    scene_builtin_particle_radial_image(|distance, dx, dy| {
        let core = (1.0 - distance / 0.58).clamp(0.0, 1.0).powf(1.9);
        let fringe_r = (1.0 - ((dx - 0.10).hypot(dy) / 0.68)).clamp(0.0, 1.0);
        let fringe_b = (1.0 - ((dx + 0.10).hypot(dy) / 0.68)).clamp(0.0, 1.0);
        let alpha = (core * 0.78 + (fringe_r + fringe_b) * 0.08).clamp(0.0, 1.0);
        [
            (120.0 + fringe_r * 135.0) as u8,
            (140.0 + core * 90.0) as u8,
            (160.0 + fringe_b * 95.0) as u8,
            (alpha * 255.0).round() as u8,
        ]
    })
}

fn scene_builtin_particle_radial_image(
    mut shade: impl FnMut(f64, f64, f64) -> [u8; 4],
) -> SceneWeTexImage {
    const SIZE: u32 = 64;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    let center = (SIZE as f64 - 1.0) * 0.5;
    let radius = center;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = (x as f64 - center) / radius;
            let dy = (y as f64 - center) / radius;
            let distance = (dx * dx + dy * dy).sqrt();
            let pixel = shade(distance, dx, dy);
            let offset = ((y * SIZE + x) * 4) as usize;
            rgba[offset..offset + 4].copy_from_slice(&pixel);
        }
    }
    SceneWeTexImage {
        width: SIZE,
        height: SIZE,
        rgba,
        r8: None,
    }
}

fn scene_copy_decoded_tex_resource_as(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    source_path: &str,
    frame_size: Option<SceneWeModelFrameSize>,
    spritesheet_enabled: bool,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
    resources: &mut Vec<Value>,
) -> Option<SceneDecodedTexResource> {
    let relative = match normalize_relative_path(source_path) {
        Ok(relative) => relative,
        Err(err) => {
            scene_push_unsupported(
                context,
                "resource-path",
                &err.to_string(),
                Some(source_path),
            );
            return None;
        }
    };
    let cache_key = SceneDecodedTexResourceKey::new(&relative, frame_size, spritesheet_enabled);
    if let Some(resource) = context.decoded_tex_resources.get(&cache_key) {
        push_unique(
            &mut context.converted_features,
            "scene-we-tex-resource-dedup",
        );
        return Some(resource.clone());
    }
    let source = project.root.join(&relative);
    let bytes = match fs::read(&source) {
        Ok(bytes) => bytes,
        Err(err) => {
            report.warnings.push(format!(
                "Scene .tex resource {source_path:?} was referenced but not read at {}: {err}.",
                source.display()
            ));
            return None;
        }
    };
    let decoded = match tex::decode_we_tex_payload(&bytes) {
        Ok(decoded) => decoded,
        Err(err) => {
            report.warnings.push(format!(
                "Scene .tex resource {source_path:?} could not be decoded as a native scene resource: {err}."
            ));
            return None;
        }
    };
    let decoded = match decoded {
        SceneWeTexPayload::Image(decoded) => decoded,
        SceneWeTexPayload::BlockCompressedImage(decoded) => {
            let resource = scene_copy_block_compressed_tex_resource(
                output_dir,
                source_path,
                &source,
                decoded,
                report,
                context,
                resources,
            );
            if let Some(resource) = &resource {
                context
                    .decoded_tex_resources
                    .insert(cache_key, resource.clone());
            }
            return resource;
        }
        video @ SceneWeTexPayload::Video(_) => {
            let resource = scene_copy_decoded_tex_video_resource(
                output_dir,
                source_path,
                &source,
                video,
                report,
                context,
                resources,
            );
            if let Some(resource) = &resource {
                context
                    .decoded_tex_resources
                    .insert(cache_key, resource.clone());
            }
            return resource;
        }
    };
    let atlas_width = decoded.width;
    let atlas_height = decoded.height;
    let layout = scene_we_tex_frame_layout(&decoded, frame_size);
    let (decoded, role, resource_suffix, spritesheet) = if spritesheet_enabled
        && layout.frame_count > 1
    {
        let spritesheet = json!({
            "type": "atlas-grid",
            "atlas_width": atlas_width,
            "atlas_height": atlas_height,
            "frame_width": layout.frame_width,
            "frame_height": layout.frame_height,
            "columns": layout.columns,
            "rows": layout.rows,
            "frame_count": layout.frame_count,
            "fps": 12.0,
            "loop": true,
            "source_format": "wallpaper-engine-spritesheet"
        });
        (
            decoded,
            "we-material-texture-decoded-atlas",
            "atlas",
            Some(spritesheet),
        )
    } else {
        let decoded = match scene_we_tex_crop_first_frame(decoded, layout) {
            Ok(decoded) => decoded,
            Err(err) => {
                report.warnings.push(format!(
                        "Scene .tex resource {source_path:?} could not be cropped to its model frame: {err}."
                    ));
                return None;
            }
        };
        (
            decoded,
            "we-material-texture-decoded-frame",
            "frame-0",
            None,
        )
    };
    let frame_count = layout.frame_count;
    if spritesheet_enabled && frame_count > 1 {
        push_unique(
            &mut context.converted_features,
            "scene-we-spritesheet-atlas-runtime",
        );
    }
    let stem = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("texture");
    let resource_id =
        scene_next_resource_id(context, "image", &format!("{stem}-{resource_suffix}"));
    let dest_dir = output_dir
        .join("assets/scene-resources")
        .join(&context.resource_scope);
    if let Err(err) = fs::create_dir_all(&dest_dir) {
        report
            .errors
            .push(format!("Failed to create scene resource directory: {err}."));
        return None;
    }
    let dest = dest_dir.join(format!("{resource_id}.gtex"));
    let write_result = if let Some(r8) = decoded.r8.as_deref() {
        gtex::write_r8_gtex(&dest, decoded.width, decoded.height, r8)
    } else {
        gtex::write_bc7_gtex(&dest, &decoded)
    };
    if let Err(err) = write_result {
        report.errors.push(format!(
            "Failed to write native scene texture {} to {}: {err}.",
            source.display(),
            dest.display()
        ));
        return None;
    }
    let package_path = path_to_package_string(dest.strip_prefix(output_dir).unwrap_or(&dest));
    report.generated_assets.push(package_path.clone());
    resources.push(json!({
        "id": resource_id,
        "type": "image",
        "source": package_path,
        "width": decoded.width,
        "height": decoded.height,
        "original_source": source_path,
        "role": role
    }));
    push_unique(
        &mut context.converted_features,
        if decoded.r8.is_some() {
            "scene-we-tex-r8-gpu-texture"
        } else {
            "scene-we-tex-bc7-gpu-texture"
        },
    );
    let resource = SceneDecodedTexResource {
        resource_id,
        render_kind: "image",
        spritesheet,
    };
    context
        .decoded_tex_resources
        .insert(cache_key, resource.clone());
    Some(resource)
}

fn scene_copy_block_compressed_tex_resource(
    output_dir: &Path,
    source_path: &str,
    source: &Path,
    decoded: tex::SceneWeTexBlockCompressedImage<'_>,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
    resources: &mut Vec<Value>,
) -> Option<SceneDecodedTexResource> {
    let stem = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("texture");
    let format_label = decoded.format.label();
    let format_suffix = match decoded.format {
        tex::SceneWeTexBlockCompressedFormat::Bc1RgbaUnormBlock => "bc1",
        tex::SceneWeTexBlockCompressedFormat::Bc3UnormBlock => "bc3",
        tex::SceneWeTexBlockCompressedFormat::Bc7UnormBlock => "bc7",
    };
    let resource_id = scene_next_resource_id(context, "image", &format!("{stem}-{format_suffix}"));
    let dest_dir = output_dir
        .join("assets/scene-resources")
        .join(&context.resource_scope);
    if let Err(err) = fs::create_dir_all(&dest_dir) {
        report
            .errors
            .push(format!("Failed to create scene resource directory: {err}."));
        return None;
    }
    let dest = dest_dir.join(format!("{resource_id}.gtex"));
    if let Err(err) = gtex::write_bc_payload_gtex(
        &dest,
        decoded.width,
        decoded.height,
        decoded.format.gtex_format(),
        decoded.payload.as_ref(),
    ) {
        report.errors.push(format!(
            "Failed to wrap Wallpaper Engine {format_label} texture {} as native gtex {}: {err}.",
            source.display(),
            dest.display()
        ));
        return None;
    }
    let package_path = path_to_package_string(dest.strip_prefix(output_dir).unwrap_or(&dest));
    report.generated_assets.push(package_path.clone());
    resources.push(json!({
        "id": resource_id,
        "type": "image",
        "source": package_path,
        "width": decoded.width,
        "height": decoded.height,
        "original_source": source_path,
        "role": format!("we-material-texture-{}-passthrough", format_suffix)
    }));
    push_unique(
        &mut context.converted_features,
        "scene-we-tex-bc-gpu-texture",
    );
    push_unique(
        &mut context.converted_features,
        match decoded.format {
            tex::SceneWeTexBlockCompressedFormat::Bc1RgbaUnormBlock => {
                "scene-we-tex-bc1-passthrough"
            }
            tex::SceneWeTexBlockCompressedFormat::Bc3UnormBlock => "scene-we-tex-bc3-passthrough",
            tex::SceneWeTexBlockCompressedFormat::Bc7UnormBlock => "scene-we-tex-bc7-passthrough",
        },
    );
    if decoded.format == tex::SceneWeTexBlockCompressedFormat::Bc7UnormBlock {
        push_unique(
            &mut context.converted_features,
            "scene-we-tex-bc7-gpu-texture",
        );
    }
    Some(SceneDecodedTexResource {
        resource_id,
        render_kind: "image",
        spritesheet: None,
    })
}

fn scene_copy_decoded_tex_video_resource(
    output_dir: &Path,
    source_path: &str,
    source: &Path,
    decoded: SceneWeTexPayload<'_>,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
    resources: &mut Vec<Value>,
) -> Option<SceneDecodedTexResource> {
    let SceneWeTexPayload::Video(video) = decoded else {
        return None;
    };
    let stem = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("video");
    let resource_id = scene_next_resource_id(context, "video", &format!("{stem}-video"));
    let dest_dir = output_dir
        .join("assets/scene-resources")
        .join(&context.resource_scope);
    if let Err(err) = fs::create_dir_all(&dest_dir) {
        report
            .errors
            .push(format!("Failed to create scene resource directory: {err}."));
        return None;
    }
    let dest = dest_dir.join(format!("{resource_id}.{}", video.extension));
    if let Err(err) = fs::write(&dest, video.payload) {
        report.errors.push(format!(
            "Failed to extract native scene video texture {} to {}: {err}.",
            source.display(),
            dest.display()
        ));
        return None;
    }
    let package_path = path_to_package_string(dest.strip_prefix(output_dir).unwrap_or(&dest));
    report.generated_assets.push(package_path.clone());
    resources.push(json!({
        "id": resource_id,
        "type": "video",
        "source": package_path,
        "original_source": source_path,
        "role": "we-material-video-texture",
        "width": video.width,
        "height": video.height
    }));
    push_unique(
        &mut context.converted_features,
        "scene-we-tex-video-layer-runtime",
    );
    Some(SceneDecodedTexResource {
        resource_id,
        render_kind: "video",
        spritesheet: None,
    })
}

fn scene_we_tex_frame_layout(
    image: &SceneWeTexImage,
    frame_size: Option<SceneWeModelFrameSize>,
) -> SceneWeTexFrameLayout {
    let frame_width = frame_size
        .map(|frame| frame.width)
        .filter(|width| *width > 0 && *width <= image.width && image.width % *width == 0)
        .unwrap_or(image.width);
    let frame_height = frame_size
        .map(|frame| frame.height)
        .filter(|height| *height > 0 && *height <= image.height && image.height % *height == 0)
        .unwrap_or(image.height);
    let columns = (image.width / frame_width).max(1);
    let rows = (image.height / frame_height).max(1);
    let frame_count = columns.saturating_mul(rows).max(1);
    SceneWeTexFrameLayout {
        frame_width,
        frame_height,
        columns,
        rows,
        frame_count,
    }
}

fn scene_we_tex_crop_first_frame(
    image: SceneWeTexImage,
    layout: SceneWeTexFrameLayout,
) -> Result<SceneWeTexImage, String> {
    if layout.frame_width == image.width && layout.frame_height == image.height {
        return Ok(image);
    }
    let rgba = scene_we_tex_crop_first_frame_bytes(&image.rgba, image.width, layout, 4, "RGBA")?;
    let r8 = image
        .r8
        .as_deref()
        .map(|r8| scene_we_tex_crop_first_frame_bytes(r8, image.width, layout, 1, "R8"))
        .transpose()?;
    Ok(SceneWeTexImage {
        width: layout.frame_width,
        height: layout.frame_height,
        rgba,
        r8,
    })
}

fn scene_we_tex_crop_first_frame_bytes(
    bytes: &[u8],
    atlas_width: u32,
    layout: SceneWeTexFrameLayout,
    bytes_per_pixel: usize,
    label: &str,
) -> Result<Vec<u8>, String> {
    let row_bytes = usize::try_from(layout.frame_width)
        .ok()
        .and_then(|width| width.checked_mul(bytes_per_pixel))
        .ok_or_else(|| format!("{label} frame row byte count overflowed"))?;
    let stride = usize::try_from(atlas_width)
        .ok()
        .and_then(|width| width.checked_mul(bytes_per_pixel))
        .ok_or_else(|| format!("{label} atlas row byte count overflowed"))?;
    let frame_len = usize::try_from(layout.frame_height)
        .ok()
        .and_then(|height| height.checked_mul(row_bytes))
        .ok_or_else(|| format!("{label} frame byte count overflowed"))?;
    let mut frame = Vec::with_capacity(frame_len);
    for row in 0..usize::try_from(layout.frame_height)
        .map_err(|_| format!("{label} frame height does not fit this platform"))?
    {
        let start = row
            .checked_mul(stride)
            .ok_or_else(|| format!("{label} atlas row offset overflowed"))?;
        let end = start
            .checked_add(row_bytes)
            .ok_or_else(|| format!("{label} atlas row range overflowed"))?;
        let row = bytes
            .get(start..end)
            .ok_or_else(|| format!("decoded {label} atlas is shorter than declared dimensions"))?;
        frame.extend_from_slice(row);
    }
    Ok(frame)
}

#[cfg(test)]
fn scene_we_tex_first_frame(
    image: SceneWeTexImage,
    frame_size: Option<SceneWeModelFrameSize>,
) -> Result<(SceneWeTexImage, u32), String> {
    let layout = scene_we_tex_frame_layout(&image, frame_size);
    let frame_count = layout.frame_count;
    scene_we_tex_crop_first_frame(image, layout).map(|image| (image, frame_count))
}

fn scene_material_spritesheet_enabled(material_json: &Value) -> bool {
    material_json
        .get("passes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter_map(|pass| pass.get("combos").and_then(Value::as_object))
        .any(|combos| {
            combos.iter().any(|(key, value)| {
                key.eq_ignore_ascii_case("SPRITESHEET") && scene_combo_value_enabled(value)
            })
        })
}

fn scene_combo_value_enabled(value: &Value) -> bool {
    value_to_i64(value).is_some_and(|value| value != 0) || value.as_bool().unwrap_or(false)
}

fn scene_resource_scope(package_path: &str) -> String {
    let file_name = Path::new(package_path)
        .file_name()
        .and_then(|stem| stem.to_str())
        .unwrap_or(package_path);
    let stem = file_name
        .strip_suffix(".gscene.json")
        .or_else(|| file_name.strip_suffix(".json"))
        .unwrap_or(file_name);
    let stem = Some(slug_id(stem)).filter(|stem| !stem.is_empty());
    stem.unwrap_or_else(|| "scene".to_owned())
}

fn scene_next_node_id(
    context: &mut SceneDocumentBuildContext,
    kind: &str,
    hint: Option<&str>,
) -> String {
    context.next_node += 1;
    let hint = hint.map(slug_id).filter(|hint| !hint.is_empty());
    match hint {
        Some(hint) => format!("node-{}-{hint}", context.next_node),
        None => format!("node-{}-{kind}", context.next_node),
    }
}

fn scene_next_resource_id(
    context: &mut SceneDocumentBuildContext,
    kind: &str,
    hint: &str,
) -> String {
    context.next_resource += 1;
    let hint = slug_id(hint);
    if hint.is_empty() {
        format!("resource-{}-{kind}", context.next_resource)
    } else {
        format!("resource-{}-{hint}", context.next_resource)
    }
}

fn scene_next_timeline_id(context: &mut SceneDocumentBuildContext, hint: Option<&str>) -> String {
    context.next_timeline += 1;
    let hint = hint.map(slug_id).filter(|hint| !hint.is_empty());
    match hint {
        Some(hint) => format!("timeline-{}-{hint}", context.next_timeline),
        None => format!("timeline-{}", context.next_timeline),
    }
}

fn scene_transform_from_object(
    object: &Map<String, Value>,
    node_id: &str,
    context: &mut SceneDocumentBuildContext,
) -> Option<Value> {
    let mut transform = Map::new();
    if let Some(origin) = object.get("origin").and_then(vector3_components_from_value) {
        transform.insert("x".to_owned(), json!(origin.0));
        transform.insert("y".to_owned(), json!(origin.1));
    }
    scene_apply_attachment_transform_from_object(object, &mut transform, context);
    scene_push_vector_component_script_property_bindings(
        object.get("origin"),
        &[("x", "x"), ("y", "y")],
        node_id,
        context,
    );
    scene_push_vector_component_script_timeline_bindings(
        object.get("origin"),
        &[("x", "x"), ("y", "y")],
        node_id,
        context,
    );
    scene_push_numeric_property_binding(object, &["x", "left"], node_id, "x", context, 1.0, 0.0);
    scene_push_numeric_property_binding(object, &["y", "top"], node_id, "y", context, 1.0, 0.0);
    scene_push_numeric_property_binding(
        object,
        &["scale_x", "scaleX", "scalex"],
        node_id,
        "scale-x",
        context,
        1.0,
        0.0,
    );
    scene_push_numeric_property_binding(
        object,
        &["scale_y", "scaleY", "scaley"],
        node_id,
        "scale-y",
        context,
        1.0,
        0.0,
    );
    scene_push_numeric_property_binding(
        object,
        &["rotation_deg", "rotationDeg", "rotation", "angle"],
        node_id,
        "rotation-deg",
        context,
        1.0,
        0.0,
    );
    if let Some(x) = number_value_field(object, &["x", "left"]) {
        transform.insert("x".to_owned(), json!(x));
    }
    if let Some(y) = number_value_field(object, &["y", "top"]) {
        transform.insert("y".to_owned(), json!(y));
    }
    if let Some(scale) = object.get("scale").and_then(vector3_components_from_value) {
        transform.insert("scale_x".to_owned(), json!(scale.0.abs().max(f64::EPSILON)));
        transform.insert("scale_y".to_owned(), json!(scale.1.abs().max(f64::EPSILON)));
    }
    if let Some(scale_x) = number_value_field(object, &["scale_x", "scaleX", "scalex"]) {
        transform.insert("scale_x".to_owned(), json!(scale_x.max(f64::EPSILON)));
    }
    if let Some(scale_y) = number_value_field(object, &["scale_y", "scaleY", "scaley"]) {
        transform.insert("scale_y".to_owned(), json!(scale_y.max(f64::EPSILON)));
    }
    if let Some(angles) = object.get("angles").and_then(vector3_components_from_value) {
        transform.insert("rotation_deg".to_owned(), json!(angles.2.to_degrees()));
    }
    if let Some(rotation) = number_value_field(
        object,
        &["rotation_deg", "rotationDeg", "rotation", "angle"],
    ) {
        transform.insert("rotation_deg".to_owned(), json!(rotation));
    }
    if let Some((anchor_x, anchor_y)) = scene_anchor_from_object(object) {
        transform.insert("anchor_x".to_owned(), json!(anchor_x));
        transform.insert("anchor_y".to_owned(), json!(anchor_y));
    }
    if transform.is_empty() {
        None
    } else {
        Some(Value::Object(transform))
    }
}

fn scene_apply_attachment_transform_from_object(
    object: &Map<String, Value>,
    transform: &mut Map<String, Value>,
    context: &mut SceneDocumentBuildContext,
) {
    if transform.contains_key("x")
        || transform.contains_key("y")
        || number_value_field(object, &["x", "left"]).is_some()
        || number_value_field(object, &["y", "top"]).is_some()
    {
        return;
    }
    let Some(attachment_name) = string_field(object, &["attachment"]) else {
        return;
    };
    let Some(parent_id) = object.get("parent").and_then(value_to_string) else {
        scene_push_unsupported(
            context,
            "wallpaper-engine-puppet-attachment",
            "Wallpaper Engine scene object uses an attachment but has no parent model to resolve it.",
            Some(&attachment_name),
        );
        return;
    };
    let attachment = context
        .puppet_attachments_by_source_id
        .get(&parent_id)
        .and_then(|attachments| attachments.attachments.get(&attachment_name))
        .copied();
    let Some(attachment) = attachment else {
        scene_push_unsupported(
            context,
            "wallpaper-engine-puppet-attachment",
            "Wallpaper Engine scene object uses an attachment that was not found in the parent puppet model.",
            Some(&attachment_name),
        );
        return;
    };
    transform.insert("x".to_owned(), json!(attachment.x));
    transform.insert("y".to_owned(), json!(attachment.y));
    push_unique(
        &mut context.converted_features,
        "wallpaper-engine-puppet-attachment-lowering",
    );
}

fn scene_anchor_from_object(object: &Map<String, Value>) -> Option<(f64, f64)> {
    let size = object.get("size").and_then(vector3_components_from_value)?;
    if size.0 <= 0.0 || size.1 <= 0.0 {
        return None;
    }
    let pivot = object
        .get("pivot")
        .and_then(vector3_components_from_value)
        .unwrap_or((0.0, 0.0, 0.0));
    let alignment = string_field(object, &["alignment"]).unwrap_or_else(|| "center".to_owned());
    let offset = scene_alignment_offset(&alignment, size.0, size.1);
    Some((
        ((pivot.0 + offset.0) / size.0 + 0.5).clamp(0.0, 1.0),
        ((pivot.1 + offset.1) / size.1 + 0.5).clamp(0.0, 1.0),
    ))
}

fn scene_alignment_offset(alignment: &str, width: f64, height: f64) -> (f64, f64) {
    let hx = width * 0.5;
    let hy = height * 0.5;
    match alignment
        .to_ascii_lowercase()
        .replace(['-', '_'], "")
        .as_str()
    {
        "left" => (-hx, 0.0),
        "right" => (hx, 0.0),
        "top" => (0.0, hy),
        "bottom" => (0.0, -hy),
        "topleft" => (-hx, hy),
        "topright" => (hx, hy),
        "bottomleft" => (-hx, -hy),
        "bottomright" => (hx, -hy),
        _ => (0.0, 0.0),
    }
}

fn scene_size_component_from_object(object: &Map<String, Value>, index: usize) -> Option<f64> {
    let size = object.get("size").and_then(vector3_components_from_value)?;
    match index {
        0 if size.0.is_finite() && size.0 >= 0.0 => Some(size.0),
        1 if size.1.is_finite() && size.1 >= 0.0 => Some(size.1),
        _ => None,
    }
}

fn scene_corner_radius_from_object(object: &Map<String, Value>) -> Option<f64> {
    let radius = number_value_field(
        object,
        &[
            "radius",
            "corner_radius",
            "cornerRadius",
            "cornerradius",
            "border_radius",
            "borderRadius",
        ],
    )?;
    if radius.is_finite() {
        Some(radius.max(0.0))
    } else {
        None
    }
}

fn scene_color_from_object(object: &Map<String, Value>) -> Option<String> {
    for key in [
        "color",
        "fill",
        "background",
        "backgroundColor",
        "backgroundcolor",
        "tint",
    ] {
        let Some(value) = object.get(key) else {
            continue;
        };
        if let Some(raw) = value_to_string_unwrapped(value)
            && is_scene_resource_path(&raw)
        {
            continue;
        }
        if let Some(color) = scene_color_from_value(value) {
            return Some(color);
        }
    }
    None
}

fn scene_stroke_color_from_object(object: &Map<String, Value>) -> Option<String> {
    ["stroke_color", "strokeColor", "stroke"]
        .iter()
        .filter_map(|key| object.get(*key))
        .find_map(scene_color_from_value)
}

fn scene_inferred_blend_opacity(
    object: &Map<String, Value>,
    source_model: Option<&SceneSourceModelConversion>,
    context: &mut SceneDocumentBuildContext,
) -> Option<f64> {
    let color_blend_mode = scene_color_blend_mode_from_object(object)?;
    let owned_model_path;
    let model_path = if let Some(source_model) = source_model {
        source_model.original_path.as_str()
    } else {
        owned_model_path = scene_model_path_from_object(object)?;
        owned_model_path.as_str()
    };
    let key = scene_model_blend_opacity_key(model_path, color_blend_mode);
    let opacity = context.model_blend_opacity_defaults.get(&key).copied()?;
    push_unique(
        &mut context.converted_features,
        "wallpaper-engine-color-blend-opacity-inference",
    );
    Some(opacity)
}

fn scene_blend_properties_from_object(object: &Map<String, Value>) -> Option<Value> {
    let mut blend = Map::new();
    for key in ["blend", "blendin", "blendout", "blendtime"] {
        if let Some(value) = object.get(key) {
            blend.insert(key.to_owned(), value.clone());
        }
    }
    if let Some(color_blend_mode) = scene_color_blend_mode_from_object(object) {
        blend.insert("colorBlendMode".to_owned(), json!(color_blend_mode));
    }
    (!blend.is_empty()).then_some(Value::Object(blend))
}

fn scene_color_blend_mode_from_object(object: &Map<String, Value>) -> Option<i64> {
    ["colorBlendMode", "colorblendmode", "blendMode", "blendmode"]
        .iter()
        .filter_map(|key| object.get(*key))
        .find_map(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str()?.parse().ok())
                .or_else(|| {
                    value
                        .as_object()?
                        .get("value")
                        .and_then(scene_color_blend_mode_value)
                })
        })
}

fn scene_color_blend_mode_value(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_str()?.parse().ok())
}

fn scene_color_property_binding_from_object(
    object: &Map<String, Value>,
    keys: &[&str],
    context: &SceneDocumentBuildContext,
) -> Option<Value> {
    for key in keys {
        let Some(value) = object.get(*key) else {
            continue;
        };
        let Some(binding) = value.as_object() else {
            continue;
        };
        let property = binding
            .get("user")
            .and_then(value_to_string)
            .or_else(|| binding.get("property").and_then(value_to_string))?;
        let property = property.trim();
        if property.is_empty() {
            continue;
        }
        let default_color = scene_color_from_value(value).or_else(|| {
            context
                .project_property_defaults
                .get(property)
                .and_then(scene_color_from_value)
        })?;
        return Some(json!({
            "runtime": "wallpaper-engine-user-color",
            "property": property,
            "default": default_color
        }));
    }
    None
}

fn scene_vector_path_from_object(object: &Map<String, Value>) -> Option<String> {
    string_field(object, &["path_data", "pathData", "d"]).or_else(|| {
        object.get("path").and_then(Value::as_str).and_then(|path| {
            if is_scene_resource_path(path) {
                None
            } else {
                Some(path.to_owned())
            }
        })
    })
}

fn scene_path_fill_rule_from_object(object: &Map<String, Value>) -> Option<&'static str> {
    let value = value_field(
        object,
        &[
            "path_fill_rule",
            "pathFillRule",
            "fill_rule",
            "fillRule",
            "fillrule",
            "winding",
        ],
    )?;
    let normalized = value
        .chars()
        .filter(|character| !matches!(character, '-' | '_' | ' '))
        .flat_map(char::to_lowercase)
        .collect::<String>();
    match normalized.as_str() {
        "evenodd" | "alternate" => Some("evenodd"),
        "nonzero" | "winding" | "nonzerowinding" => Some("nonzero"),
        _ => None,
    }
}

fn scene_text_from_object(object: &Map<String, Value>) -> Option<String> {
    value_field(object, &["text", "caption", "value"])
}

fn scene_text_binding_from_object(object: &Map<String, Value>) -> Option<Value> {
    let text = object.get("text")?.as_object()?;
    let script = text.get("script").and_then(Value::as_str)?;
    let script_properties = text.get("scriptproperties").and_then(Value::as_object);
    if scene_script_is_clock_time_text(script) {
        let use_24h = script_properties
            .and_then(|properties| scene_script_property_value(properties, "use24hFormat"))
            .and_then(value_to_bool)
            .unwrap_or(true);
        let show_seconds = script_properties
            .and_then(|properties| scene_script_property_value(properties, "showSeconds"))
            .and_then(value_to_bool)
            .unwrap_or(false);
        let delimiter = script_properties
            .and_then(|properties| scene_script_property_value(properties, "delimiter"))
            .and_then(value_to_string)
            .unwrap_or_else(|| ":".to_owned());
        if delimiter == ":" {
            let property = match (use_24h, show_seconds) {
                (true, false) => "scene.clock.local.time.hm24",
                (true, true) => "scene.clock.local.time.hms24",
                (false, false) => "scene.clock.local.time.hm12",
                (false, true) => "scene.clock.local.time.hms12",
            };
            return Some(json!({
                "runtime": "native",
                "kind": "clock-time",
                "property": property
            }));
        }
    }
    if scene_script_is_vertical_date_text(script, script_properties) {
        return Some(json!({
            "runtime": "native",
            "kind": "clock-date",
            "property": "scene.clock.local.we-date.vertical-month-abbrev"
        }));
    }
    if scene_script_is_vertical_weekday_text(script, script_properties) {
        return Some(json!({
            "runtime": "native",
            "kind": "clock-weekday",
            "property": "scene.clock.local.we-day.vertical-weekday-abbrev-upper"
        }));
    }
    None
}

fn scene_script_property_value<'a>(
    script_properties: &'a Map<String, Value>,
    key: &str,
) -> Option<&'a Value> {
    script_properties
        .get(key)
        .map(|value| value.get("value").unwrap_or(value))
}

fn scene_script_property_string(
    script_properties: Option<&Map<String, Value>>,
    key: &str,
) -> Option<String> {
    script_properties
        .and_then(|properties| scene_script_property_value(properties, key))
        .and_then(value_to_string)
}

fn scene_script_property_bool_default(
    script_properties: Option<&Map<String, Value>>,
    key: &str,
    default: bool,
) -> bool {
    script_properties
        .and_then(|properties| scene_script_property_value(properties, key))
        .and_then(value_to_bool)
        .unwrap_or(default)
}

fn scene_script_is_clock_time_text(script: &str) -> bool {
    script.contains("new Date()")
        && script.contains("getHours()")
        && script.contains("getMinutes()")
        && script.contains("use24hFormat")
}

fn scene_script_is_vertical_date_text(
    script: &str,
    script_properties: Option<&Map<String, Value>>,
) -> bool {
    script.contains("new Date()")
        && script.contains("getFullYear()")
        && script.contains("getMonth()")
        && script.contains("dtt[date.getDate()]")
        && scene_script_property_string(script_properties, "monthFormat").as_deref() == Some("2")
        && scene_script_property_bool_default(script_properties, "alignVertical", false)
        && !scene_script_property_bool_default(script_properties, "showDay", true)
        && !scene_script_property_bool_default(script_properties, "useDelimiter", true)
}

fn scene_script_is_vertical_weekday_text(
    script: &str,
    script_properties: Option<&Map<String, Value>>,
) -> bool {
    script.contains("new Date()")
        && script.contains("day[date.getDay()]")
        && scene_script_property_string(script_properties, "dayFormat").as_deref() == Some("1")
        && scene_script_property_bool_default(script_properties, "alignVertical", false)
        && scene_script_property_bool_default(script_properties, "showDay", false)
        && !scene_script_property_bool_default(script_properties, "useDelimiter", true)
}

fn scene_font_size_from_object(object: &Map<String, Value>) -> Option<f64> {
    number_value_field(
        object,
        &[
            "pointsize",
            "pointSize",
            "font_size",
            "fontSize",
            "fontsize",
            "size",
        ],
    )
}

fn scene_font_family_from_object(object: &Map<String, Value>) -> Option<String> {
    value_field(object, &["font_family", "fontFamily", "font"])
}

fn scene_copy_font_resource_if_path(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    font: &str,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
    resources: &mut Vec<Value>,
) -> Option<String> {
    if !is_font_path(font) {
        return None;
    }
    let resource = scene_copy_resource_as(
        project,
        output_dir,
        font,
        "font",
        Some("we-font"),
        report,
        context,
        resources,
    )?;
    push_unique(
        &mut context.converted_features,
        "wallpaper-engine-font-resource-lowering",
    );
    Some(resource)
}

fn is_font_path(value: &str) -> bool {
    Path::new(value)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "ttf" | "otf" | "ttc" | "woff" | "woff2"
            )
        })
}

fn scene_text_align_from_object(object: &Map<String, Value>) -> Option<&'static str> {
    let align = value_field(
        object,
        &[
            "text_align",
            "textAlign",
            "align",
            "horizontalalign",
            "horizontalAlign",
        ],
    )?;
    match align.to_ascii_lowercase().as_str() {
        "center" | "middle" => Some("middle"),
        "right" | "end" => Some("end"),
        "left" | "start" => Some("start"),
        _ => None,
    }
}

fn scene_fit_from_object(object: &Map<String, Value>) -> Option<&'static str> {
    let fit = string_field(object, &["fit", "scaling", "scaleMode"])?;
    match fit.to_ascii_lowercase().as_str() {
        "contain" | "fit" => Some("contain"),
        "stretch" | "fill" => Some("stretch"),
        "center" => Some("center"),
        "tile" | "repeat" => Some("tile"),
        "cover" | "crop" => Some("cover"),
        _ => None,
    }
}

fn scene_system_statuses(report: &ConversionReport, context: &SceneDocumentBuildContext) -> Value {
    json!({
        "scenescript": scene_system_status(report, context, "scenescript"),
        "shader_material_graph": scene_system_status(report, context, "shader"),
        "particles": scene_system_status(report, context, "particles"),
        "parallax": scene_system_status(report, context, "parallax"),
        "audio_response": scene_system_status(report, context, "audio-response")
    })
}

fn scene_system_status(
    report: &ConversionReport,
    context: &SceneDocumentBuildContext,
    feature: &str,
) -> &'static str {
    if feature == "shader" && scene_material_graph_runtime_ready(report, context) {
        return "ready";
    }
    if feature == "scenescript" && scene_all_detected_scripts_native_lowered(report) {
        return "ready";
    }
    if feature == "shader"
        && (report
            .converted_features
            .iter()
            .any(|converted| converted == "scene-we-material-graph-runtime")
            || context
                .unsupported_features
                .iter()
                .filter_map(|feature| feature.get("feature").and_then(Value::as_str))
                .any(scene_feature_blocks_material_graph_runtime))
    {
        return "detected";
    }
    if feature == "particles"
        && report
            .converted_features
            .iter()
            .any(|converted| converted == "native-particle-runtime")
    {
        return "ready";
    }
    if feature == "audio-response"
        && report
            .converted_features
            .iter()
            .any(|converted| converted == "native-audio-response-runtime")
    {
        return "ready";
    }
    if report
        .detected_features
        .iter()
        .any(|detected| detected == feature)
    {
        "detected"
    } else {
        "absent"
    }
}

fn scene_full_scene_status(
    report: &ConversionReport,
    context: &SceneDocumentBuildContext,
    video_visibility: SceneVideoVisibilityCounts,
) -> FullSceneConversionStatus {
    let mut status = FullSceneConversionStatus::native_vulkan_scene_boundary();
    if report
        .detected_features
        .iter()
        .any(|feature| feature == "scenescript")
        && !scene_all_detected_scripts_native_lowered(report)
    {
        push_unique(
            &mut status.pending_boundaries,
            "arbitrary-scenescript-runtime",
        );
    }
    if scene_shader_material_graph_boundary_detected(report, context)
        && !scene_material_graph_runtime_ready(report, context)
    {
        push_unique(&mut status.pending_boundaries, "shader-material-graph");
    }
    if report
        .detected_features
        .iter()
        .any(|feature| feature == "particles")
        && !report
            .converted_features
            .iter()
            .any(|feature| feature == "native-particle-runtime")
    {
        push_unique(&mut status.pending_boundaries, "particle-systems");
    }
    if report
        .converted_features
        .iter()
        .any(|feature| feature == "native-particle-runtime")
    {
        push_unique(
            &mut status.completed_boundaries,
            "native-particle-system-runtime",
        );
        status
            .pending_boundaries
            .retain(|boundary| boundary != "particle-systems");
    }
    if report
        .converted_features
        .iter()
        .any(|feature| feature == "scene-we-particle-material-runtime")
    {
        push_unique(
            &mut status.completed_boundaries,
            "scene-we-particle-material-runtime",
        );
    }
    if scene_material_graph_runtime_ready(report, context) {
        push_unique(
            &mut status.completed_boundaries,
            "wallpaper-engine-material-graph-texture-runtime",
        );
        push_unique(&mut status.completed_boundaries, "shader-material-graph");
        status
            .pending_boundaries
            .retain(|boundary| boundary != "shader-material-graph");
    }
    let audio_response_ready = report
        .converted_features
        .iter()
        .any(|feature| feature == "native-audio-response-runtime");
    if audio_response_ready {
        push_unique(
            &mut status.completed_boundaries,
            "native-audio-response-visual-runtime",
        );
        status
            .pending_boundaries
            .retain(|boundary| boundary != "audio-response-runtime");
        push_unique(
            &mut status.pending_boundaries,
            "pipewire-audio-spectrum-input-source",
        );
    } else if report
        .detected_features
        .iter()
        .any(|feature| feature == "audio-response")
    {
        push_unique(&mut status.pending_boundaries, "audio-response-runtime");
    }
    if report
        .converted_features
        .iter()
        .any(|feature| feature == "scene-we-tex-video-layer-runtime")
    {
        push_unique(
            &mut status.completed_boundaries,
            "wallpaper-engine-tex-video-layer-runtime",
        );
        if video_visibility.initial_visible <= 1 {
            push_unique(
                &mut status.completed_boundaries,
                "initial-visible-video-scene-composition",
            );
            push_unique(
                &mut status.completed_boundaries,
                "vulkan-video-scene-layer-composition",
            );
            status
                .pending_boundaries
                .retain(|boundary| boundary != "mixed-video-scene-composition");
            if video_visibility.total > 1
                && report
                    .detected_features
                    .iter()
                    .any(|feature| feature == "scenescript")
            {
                if report
                    .converted_features
                    .iter()
                    .any(|feature| feature == "native-scene-controller-video-switch-binding")
                {
                    let idle_controller_ready = report
                        .converted_features
                        .iter()
                        .any(|feature| feature == "native-scene-controller-idle-input-source");
                    let idle_fade_ramp_ready = report
                        .converted_features
                        .iter()
                        .any(|feature| feature == "native-scene-controller-idle-fade-ramp");
                    let controller_input_pending =
                        report.converted_features.iter().any(|feature| {
                            feature == "native-scene-controller-external-input-source-required"
                        });
                    push_unique(
                        &mut status.completed_boundaries,
                        "script-controlled-video-layer-switching",
                    );
                    if idle_controller_ready {
                        push_unique(
                            &mut status.completed_boundaries,
                            "scene-idle-controller-input-source",
                        );
                    }
                    if idle_fade_ramp_ready {
                        push_unique(
                            &mut status.completed_boundaries,
                            "scene-controller-fade-ramp-runtime",
                        );
                    }
                    if controller_input_pending {
                        push_unique(
                            &mut status.unsupported_boundaries,
                            "scene-controller-input-source",
                        );
                    }
                    if !controller_input_pending && !idle_controller_ready {
                        push_unique(
                            &mut status.pending_boundaries,
                            "scene-controller-input-source",
                        );
                    }
                } else {
                    push_unique(
                        &mut status.pending_boundaries,
                        "script-controlled-video-layer-switching",
                    );
                }
            }
        } else {
            push_unique(
                &mut status.pending_boundaries,
                "mixed-video-scene-composition",
            );
        }
    }
    if report
        .converted_features
        .iter()
        .any(|feature| feature == "scene-we-timed-visibility-controller")
    {
        push_unique(
            &mut status.completed_boundaries,
            "wallpaper-engine-timed-visibility-controller-lowering",
        );
        push_unique(
            &mut status.completed_boundaries,
            "scene-controller-fade-ramp-runtime",
        );
    }
    if report
        .converted_features
        .iter()
        .any(|feature| feature == "scene-we-animation-layer-rate-time-scale")
    {
        push_unique(
            &mut status.completed_boundaries,
            "wallpaper-engine-animation-layer-rate-time-scale",
        );
    }
    if report
        .converted_features
        .iter()
        .any(|feature| feature == "wallpaper-engine-font-resource-lowering")
    {
        push_unique(
            &mut status.completed_boundaries,
            "wallpaper-engine-font-resource-lowering",
        );
    }
    if report
        .converted_features
        .iter()
        .any(|feature| feature == "scene-we-deterministic-clock-text")
    {
        push_unique(
            &mut status.completed_boundaries,
            "wallpaper-engine-deterministic-clock-text-lowering",
        );
    }
    if report
        .converted_features
        .iter()
        .any(|feature| feature == "native-scene-audio-active-condition")
    {
        push_unique(
            &mut status.completed_boundaries,
            "scene-audio-controller-runtime",
        );
    }
    if scene_all_detected_scripts_native_lowered(report) {
        push_unique(
            &mut status.completed_boundaries,
            "wallpaper-engine-detected-scenescript-native-lowering",
        );
        status
            .pending_boundaries
            .retain(|boundary| boundary != "arbitrary-scenescript-runtime");
    }
    scene_finalize_full_scene_status(status)
}

fn scene_finalize_full_scene_status(
    mut status: FullSceneConversionStatus,
) -> FullSceneConversionStatus {
    status.full_scene_complete = status.pending_boundaries.is_empty();
    status.progress_estimate_percent = if status.full_scene_complete { 100 } else { 99 };
    status
}

fn scene_all_detected_scripts_native_lowered(report: &ConversionReport) -> bool {
    report
        .converted_features
        .iter()
        .any(|feature| feature == "wallpaper-engine-all-detected-scenescript-native-lowering")
}

fn scene_shader_material_graph_boundary_detected(
    report: &ConversionReport,
    context: &SceneDocumentBuildContext,
) -> bool {
    report
        .detected_features
        .iter()
        .any(|feature| feature == "shader")
        || report.converted_features.iter().any(|feature| {
            feature == "scene-we-material-graph-runtime"
                || feature == "native-text-glow-effect-runtime"
        })
        || context.unsupported_features.iter().any(|feature| {
            feature
                .get("feature")
                .and_then(Value::as_str)
                .is_some_and(scene_feature_blocks_material_graph_runtime)
        })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct SceneVideoVisibilityCounts {
    total: usize,
    initial_visible: usize,
}

fn scene_lower_pending_controllers(nodes: &mut [Value], context: &mut SceneDocumentBuildContext) {
    if context.pending_controllers.is_empty() {
        return;
    }
    let index = scene_node_lookup_index(nodes);
    for controller in context.pending_controllers.clone() {
        let Some(target_node_id) = index
            .get(controller.target_layer())
            .or_else(|| index.get(&normalize_project_key(controller.target_layer())))
            .cloned()
        else {
            scene_push_unsupported(
                context,
                "scene-controller-target-resolution",
                "Wallpaper Engine utility controller target layer could not be resolved to a gscene node.",
                Some(controller.target_layer()),
            );
            continue;
        };
        let target_kind = index
            .get(&format!("kind:{target_node_id}"))
            .cloned()
            .unwrap_or_else(|| "unknown".to_owned());
        if let Some(opacity) = controller.initial_target_opacity() {
            scene_set_node_initial_opacity(nodes, &target_node_id, opacity);
        }
        scene_set_controller_target_node(
            nodes,
            controller.controller_node_id(),
            &target_node_id,
            &target_kind,
            controller.input_aliases_value(Some(&target_node_id)),
        );
        if let Some(timeline) = controller.timed_visibility_timeline_value(
            scene_next_timeline_id(
                context,
                Some(&format!(
                    "{}-{}",
                    controller.controller_node_id(),
                    target_node_id
                )),
            ),
            &target_node_id,
        ) {
            context.timelines.push(timeline);
            push_unique(
                &mut context.converted_features,
                "scene-we-timed-visibility-controller",
            );
        } else {
            context
                .property_bindings
                .push(controller.property_binding_value(&target_node_id));
            push_unique(
                &mut context.converted_features,
                "native-scene-controller-property-binding",
            );
        }
        if target_kind == "video" {
            push_unique(
                &mut context.converted_features,
                "native-scene-controller-video-switch-binding",
            );
        }
        let controller_feature = controller.completed_feature_name();
        push_unique(&mut context.converted_features, &controller_feature);
        if controller.uses_native_idle_input_source() {
            push_unique(
                &mut context.converted_features,
                "native-scene-controller-idle-input-source",
            );
        }
        if controller.uses_native_idle_fade_ramp() {
            push_unique(
                &mut context.converted_features,
                "native-scene-controller-idle-fade-ramp",
            );
        }
        if controller.requires_external_input_source() {
            push_unique(
                &mut context.converted_features,
                "native-scene-controller-external-input-source-required",
            );
            scene_push_unsupported(
                context,
                "scene-controller-input-source",
                "Wallpaper Engine click/property controller input needs compositor-specific global pointer or property events; Gilder intentionally does not support that input source yet.",
                Some(controller.controller_node_id()),
            );
        }
    }
}

fn scene_lower_pending_audio_controllers(
    nodes: &mut [Value],
    context: &mut SceneDocumentBuildContext,
) {
    if context.pending_audio_controllers.is_empty() {
        return;
    }
    let index = scene_node_lookup_index(nodes);
    for controller in context.pending_audio_controllers.clone() {
        let source_layer_active_property = controller.source_layer().and_then(|source_layer| {
            index
                .get(source_layer)
                .or_else(|| index.get(&normalize_project_key(source_layer)))
                .and_then(|source_node_id| {
                    scene_property_binding_for_target(context, source_node_id, "opacity")
                })
        });
        let mut lowered = false;
        for audio_layer in controller.target_audio_layers() {
            let Some(audio_node_id) = index
                .get(audio_layer)
                .or_else(|| index.get(&normalize_project_key(audio_layer)))
                .cloned()
            else {
                scene_push_unsupported(
                    context,
                    "scene-audio-controller-target-resolution",
                    "Wallpaper Engine audio controller target layer could not be resolved to a gscene audio node.",
                    Some(audio_layer),
                );
                continue;
            };
            let Some(conditions) = controller
                .conditions_for_audio_layer(audio_layer, source_layer_active_property.as_deref())
            else {
                continue;
            };
            if scene_add_audio_conditions_to_node(nodes, &audio_node_id, conditions) {
                lowered = true;
            }
        }
        if lowered {
            push_unique(
                &mut context.converted_features,
                "native-scene-audio-active-condition",
            );
        }
    }
}

fn scene_property_binding_for_target(
    context: &SceneDocumentBuildContext,
    target_node_id: &str,
    target_property: &str,
) -> Option<String> {
    context.property_bindings.iter().find_map(|binding| {
        let object = binding.as_object()?;
        let target_node = object.get("target_node").and_then(Value::as_str)?;
        let target = object.get("target").and_then(Value::as_str)?;
        if target_node == target_node_id && target == target_property {
            object
                .get("property")
                .and_then(Value::as_str)
                .map(str::to_owned)
        } else {
            None
        }
    })
}

fn scene_add_audio_conditions_to_node(
    nodes: &mut [Value],
    node_id: &str,
    conditions: Vec<SceneAudioCueConditionIr>,
) -> bool {
    for node in nodes {
        let Some(object) = node.as_object_mut() else {
            continue;
        };
        if object.get("id").and_then(Value::as_str) == Some(node_id) {
            let condition_values = conditions
                .iter()
                .map(SceneAudioCueConditionIr::value)
                .collect::<Vec<_>>();
            let Some(audio) = object.get_mut("audio").and_then(Value::as_array_mut) else {
                return false;
            };
            for cue in audio.iter_mut().filter_map(Value::as_object_mut) {
                cue.insert("start_silent".to_owned(), Value::Bool(true));
                let entry = cue
                    .entry("active_conditions".to_owned())
                    .or_insert_with(|| Value::Array(Vec::new()));
                let Some(existing) = entry.as_array_mut() else {
                    continue;
                };
                for condition in &condition_values {
                    if !existing.contains(condition) {
                        existing.push(condition.clone());
                    }
                }
            }
            return true;
        }
        if let Some(children) = object.get_mut("children").and_then(Value::as_array_mut)
            && scene_add_audio_conditions_to_node(children, node_id, conditions.clone())
        {
            return true;
        }
    }
    false
}

fn scene_node_lookup_index(nodes: &[Value]) -> BTreeMap<String, String> {
    let mut index = BTreeMap::new();
    for node in nodes {
        scene_collect_node_lookup_index(node, &mut index);
    }
    index
}

fn scene_collect_node_lookup_index(node: &Value, index: &mut BTreeMap<String, String>) {
    let Some(object) = node.as_object() else {
        return;
    };
    let Some(node_id) = object.get("id").and_then(Value::as_str) else {
        return;
    };
    index.insert(node_id.to_owned(), node_id.to_owned());
    if let Some(name) = object.get("name").and_then(Value::as_str) {
        index.insert(name.to_owned(), node_id.to_owned());
        index.insert(normalize_project_key(name), node_id.to_owned());
    }
    if let Some(source_id) = object
        .get("provenance")
        .and_then(Value::as_object)
        .and_then(|provenance| provenance.get("source_id"))
        .and_then(Value::as_str)
    {
        index.insert(source_id.to_owned(), node_id.to_owned());
    }
    if let Some(kind) = object.get("type").and_then(Value::as_str) {
        index.insert(format!("kind:{node_id}"), kind.to_owned());
    }
    if let Some(children) = object.get("children").and_then(Value::as_array) {
        for child in children {
            scene_collect_node_lookup_index(child, index);
        }
    }
}

fn scene_set_node_initial_opacity(nodes: &mut [Value], node_id: &str, opacity: f64) -> bool {
    for node in nodes {
        let Some(object) = node.as_object_mut() else {
            continue;
        };
        if object.get("id").and_then(Value::as_str) == Some(node_id) {
            object.insert("visible".to_owned(), Value::Bool(true));
            object.insert("opacity".to_owned(), json!(opacity.clamp(0.0, 1.0)));
            return true;
        }
        if let Some(children) = object.get_mut("children").and_then(Value::as_array_mut)
            && scene_set_node_initial_opacity(children, node_id, opacity)
        {
            return true;
        }
    }
    false
}

fn scene_set_controller_target_node(
    nodes: &mut [Value],
    controller_node_id: &str,
    target_node_id: &str,
    target_kind: &str,
    input_aliases: Value,
) -> bool {
    for node in nodes {
        let Some(object) = node.as_object_mut() else {
            continue;
        };
        if object.get("id").and_then(Value::as_str) == Some(controller_node_id) {
            let properties = object
                .entry("properties".to_owned())
                .or_insert_with(|| Value::Object(Map::new()));
            let Some(properties) = properties.as_object_mut() else {
                return false;
            };
            let controller = properties
                .entry("controller".to_owned())
                .or_insert_with(|| Value::Object(Map::new()));
            let Some(controller) = controller.as_object_mut() else {
                return false;
            };
            controller.insert(
                "target_node".to_owned(),
                Value::String(target_node_id.to_owned()),
            );
            controller.insert(
                "target_type".to_owned(),
                Value::String(target_kind.to_owned()),
            );
            controller.insert("input_aliases".to_owned(), input_aliases);
            return true;
        }
        if let Some(children) = object.get_mut("children").and_then(Value::as_array_mut)
            && scene_set_controller_target_node(
                children,
                controller_node_id,
                target_node_id,
                target_kind,
                input_aliases.clone(),
            )
        {
            return true;
        }
    }
    false
}

fn scene_video_visibility_counts(nodes: &[Value]) -> SceneVideoVisibilityCounts {
    let mut counts = SceneVideoVisibilityCounts::default();
    for node in nodes {
        scene_count_video_visibility(node, true, &mut counts);
    }
    counts
}

fn scene_count_video_visibility(
    node: &Value,
    parent_visible: bool,
    counts: &mut SceneVideoVisibilityCounts,
) {
    let Some(object) = node.as_object() else {
        return;
    };
    let visible = parent_visible
        && object
            .get("visible")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        && object.get("opacity").and_then(value_to_f64).unwrap_or(1.0) > 0.0;
    if object.get("type").and_then(Value::as_str) == Some("video") {
        counts.total = counts.total.saturating_add(1);
        if visible {
            counts.initial_visible = counts.initial_visible.saturating_add(1);
        }
    }
    if let Some(children) = object.get("children").and_then(Value::as_array) {
        for child in children {
            scene_count_video_visibility(child, visible, counts);
        }
    }
}

fn scene_native_lowering_from_status(status: &FullSceneConversionStatus) -> Value {
    json!({
        "target_runtime": status.target_runtime,
        "current_runtime": status.current_runtime,
        "progress_estimate_percent": status.progress_estimate_percent,
        "full_scene_complete": status.full_scene_complete,
        "completed_boundaries": status.completed_boundaries,
        "pending_boundaries": status.pending_boundaries,
        "unsupported_boundaries": status.unsupported_boundaries
    })
}

fn scene_material_graph_runtime_ready(
    report: &ConversionReport,
    context: &SceneDocumentBuildContext,
) -> bool {
    let has_native_material_graph_runtime = report
        .converted_features
        .iter()
        .any(|feature| feature == "scene-we-material-graph-runtime");
    let has_native_effect_runtime = report
        .converted_features
        .iter()
        .any(|feature| feature == "native-text-glow-effect-runtime");
    let has_material_graph_blocker = report
        .unsupported_features
        .iter()
        .any(|feature| scene_feature_blocks_material_graph_runtime(feature))
        || context.unsupported_features.iter().any(|feature| {
            feature
                .get("feature")
                .and_then(Value::as_str)
                .is_some_and(scene_feature_blocks_material_graph_runtime)
        });

    (has_native_material_graph_runtime || has_native_effect_runtime) && !has_material_graph_blocker
}

fn scene_feature_blocks_material_graph_runtime(feature: &str) -> bool {
    feature.contains("shader")
        || feature.contains("effect")
        || matches!(
            feature,
            "we-material-texture-runtime"
                | "we-model-material-texture-runtime"
                | "we-runtime-texture"
                | "runtime-texture"
        )
}

fn scene_unsupported_features(
    report: &ConversionReport,
    mut unsupported_features: Vec<Value>,
) -> Vec<Value> {
    for feature in &report.unsupported_features {
        if feature.starts_with("property:")
            || feature == "web-runtime"
            || feature == "shader-runtime"
        {
            continue;
        }
        unsupported_features.push(json!({
            "feature": feature,
            "reason": "Detected in Wallpaper Engine source; represented in gscene metadata but not executed by native scene runtime yet."
        }));
    }
    unsupported_features
}

fn scene_push_unsupported(
    context: &mut SceneDocumentBuildContext,
    feature: &str,
    reason: &str,
    source_path: Option<&str>,
) {
    let mut item = json!({
        "feature": feature,
        "reason": reason
    });
    if let Some(source_path) = source_path
        && let Some(object) = item.as_object_mut()
    {
        object.insert(
            "source_path".to_owned(),
            Value::String(source_path.to_owned()),
        );
    }
    context.unsupported_features.push(item);
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
        SourceType::Video | SourceType::Scene => {
            push_unique(&mut report.converted_features, "audio-policy");
            true
        }
        SourceType::Image if !static_image_audio_sources(project).is_empty() => {
            push_unique(&mut report.converted_features, "audio-policy");
            true
        }
        SourceType::Web | SourceType::Shader => {
            push_unique(&mut report.unsupported_features, "audio-runtime");
            report.warnings.push(
                "Detected Wallpaper Engine audio features, but audio runtime integration is not available for this converted wallpaper type.".to_owned(),
            );
            false
        }
        SourceType::Playlist => {
            push_unique(&mut report.unsupported_features, "playlist-audio-runtime");
            report.warnings.push(
                "Detected audio intent in a Wallpaper Engine playlist; converted playlist items stay muted until per-item audio policy is implemented.".to_owned(),
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

fn record_scene_runtime_gaps(report: &mut ConversionReport) {
    for (detected, unsupported) in [
        ("scenescript", "scenescript"),
        ("shader", "custom-shader"),
        ("particles", "complex-particles"),
        ("timeline", "timeline-animation"),
        ("parallax", "cursor-parallax-input-source"),
        ("audio-response", "audio-response-runtime"),
    ] {
        if report
            .detected_features
            .iter()
            .any(|feature| feature == detected)
        {
            if detected == "particles"
                && report
                    .converted_features
                    .iter()
                    .any(|converted| converted == "native-particle-runtime")
            {
                continue;
            }
            if detected == "scenescript" && scene_all_detected_scripts_native_lowered(report) {
                continue;
            }
            if detected == "timeline"
                && report
                    .converted_features
                    .iter()
                    .any(|converted| converted == "scene-keyframe-timeline")
            {
                continue;
            }
            if detected == "shader"
                && report
                    .converted_features
                    .iter()
                    .any(|converted| converted == "scene-we-material-graph-runtime")
            {
                continue;
            }
            if detected == "audio-response"
                && report
                    .converted_features
                    .iter()
                    .any(|converted| converted == "native-audio-response-runtime")
            {
                continue;
            }
            push_unique(&mut report.unsupported_features, unsupported);
        }
    }
}

fn record_full_scene_runtime_boundary(
    report: &mut ConversionReport,
    source_scene_metadata: Option<&str>,
) {
    let full_scene = report
        .full_scene
        .get_or_insert_with(FullSceneConversionStatus::native_vulkan_scene_boundary);
    if let Some(source_scene_metadata) = source_scene_metadata {
        push_unique(&mut full_scene.source_scene_metadata, source_scene_metadata);
    }
}

fn record_shader_runtime_gap(report: &mut ConversionReport) {
    push_unique(&mut report.unsupported_features, "shader-runtime");
}

fn shader_language_for_source(source: &str) -> &'static str {
    match Path::new(source)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("wgsl") => "wgsl",
        Some("frag" | "fragment" | "fs" | "glsl" | "shader" | "vert" | "vertex" | "vs") => "glsl",
        _ => "auto",
    }
}

fn shader_uniforms(project: &WallpaperEngineProject) -> Vec<Value> {
    let mut uniforms = vec![
        json!({ "name": "u_time", "source": "time" }),
        json!({ "name": "u_resolution", "source": "resolution" }),
        json!({ "name": "u_mouse", "source": "mouse" }),
    ];
    let mut names = BTreeSet::from([
        "u_time".to_owned(),
        "u_resolution".to_owned(),
        "u_mouse".to_owned(),
    ]);

    let Some(properties) = project
        .raw
        .pointer("/general/properties")
        .and_then(Value::as_object)
    else {
        return uniforms;
    };

    for (property, spec) in properties {
        let Some(spec) = spec.as_object() else {
            continue;
        };
        if !shader_property_uniform_supported(spec) {
            continue;
        }
        let uniform_name = unique_shader_uniform_name(property, &mut names);
        uniforms.push(json!({
            "name": uniform_name,
            "source": "property",
            "property": property
        }));
    }

    uniforms
}

fn shader_property_uniform_supported(spec: &Map<String, Value>) -> bool {
    matches!(
        string_field(spec, &["type"])
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "bool" | "slider" | "combo" | "color" | "textinput" | "text"
    )
}

fn unique_shader_uniform_name(property: &str, names: &mut BTreeSet<String>) -> String {
    let base = shader_uniform_name(property);
    if names.insert(base.clone()) {
        return base;
    }

    for suffix in 2.. {
        let candidate = format!("{base}_{suffix}");
        if names.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("unbounded suffix search must eventually find a shader uniform name")
}

fn shader_uniform_name(property: &str) -> String {
    let mut normalized = String::from("u_");
    for character in property.chars() {
        if character.is_ascii_alphanumeric() {
            normalized.push(character.to_ascii_lowercase());
        } else if !normalized.ends_with('_') {
            normalized.push('_');
        }
    }
    while normalized.ends_with('_') {
        normalized.pop();
    }
    if normalized == "u" {
        "u_property".to_owned()
    } else {
        normalized
    }
}

fn push_unique(items: &mut Vec<String>, value: &str) {
    if !items.iter().any(|item| item == value) {
        items.push(value.to_owned());
    }
}

fn scene_project_property_defaults(project: &WallpaperEngineProject) -> BTreeMap<String, Value> {
    let mut defaults = BTreeMap::new();
    let Some(properties) = project
        .raw
        .pointer("/general/properties")
        .and_then(Value::as_object)
    else {
        return defaults;
    };
    for (name, spec) in properties {
        let Some(spec) = spec.as_object() else {
            continue;
        };
        if let Some(value) = spec.get("value").or_else(|| spec.get("default")) {
            defaults.insert(name.clone(), value.clone());
        }
    }
    defaults
}

fn scene_model_blend_opacity_defaults(source_scene: Option<&Value>) -> BTreeMap<String, f64> {
    let mut defaults = BTreeMap::new();
    let mut conflicts = BTreeSet::new();
    if let Some(source_scene) = source_scene {
        scene_collect_model_blend_opacity_defaults(source_scene, &mut defaults, &mut conflicts);
    }
    for key in conflicts {
        defaults.remove(&key);
    }
    defaults
}

fn scene_collect_model_blend_opacity_defaults(
    value: &Value,
    defaults: &mut BTreeMap<String, f64>,
    conflicts: &mut BTreeSet<String>,
) {
    match value {
        Value::Object(object) => {
            if let (Some(model_path), Some(color_blend_mode), Some(opacity)) = (
                scene_model_path_from_object(object),
                scene_color_blend_mode_from_object(object),
                number_value_field(object, &["opacity", "alpha"]),
            ) && opacity.is_finite()
            {
                let opacity = opacity.clamp(0.0, 1.0);
                let key = scene_model_blend_opacity_key(&model_path, color_blend_mode);
                if let Some(existing) = defaults.get(&key) {
                    if (*existing - opacity).abs() > 0.000_001 {
                        conflicts.insert(key);
                    }
                } else {
                    defaults.insert(key, opacity);
                }
            }
            for value in object.values() {
                scene_collect_model_blend_opacity_defaults(value, defaults, conflicts);
            }
        }
        Value::Array(values) => {
            for value in values {
                scene_collect_model_blend_opacity_defaults(value, defaults, conflicts);
            }
        }
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null => {}
    }
}

fn scene_model_blend_opacity_key(model_path: &str, color_blend_mode: i64) -> String {
    format!(
        "{}\u{1f}{color_blend_mode}",
        model_path.replace('\\', "/").to_ascii_lowercase()
    )
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
            "scenetexture" => {
                push_unique(
                    &mut report.converted_features,
                    "wallpaper-engine-scenetexture-property-lowering",
                );
                Some(json!({
                    "type": "text",
                    "default": string_field(spec, &["value", "default"]),
                }))
            }
            "file" | "directory" => {
                push_unique(
                    &mut report.unsupported_features,
                    &format!("property:{property_type}"),
                );
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
                push_unique(
                    &mut report.unsupported_features,
                    &format!("property:{other}"),
                );
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

fn value_field(map: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| map.get(*key))
        .find_map(value_to_string_unwrapped)
}

fn number_field(map: &Map<String, Value>, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .filter_map(|key| map.get(*key))
        .find_map(|value| value.as_f64().or_else(|| value.as_str()?.parse().ok()))
}

fn number_value_field(map: &Map<String, Value>, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .filter_map(|key| map.get(*key))
        .find_map(value_to_f64_unwrapped)
}

fn scene_bool_value_field(map: &Map<String, Value>, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .filter_map(|key| map.get(*key))
        .find_map(value_to_bool_unwrapped)
}

fn value_to_f64(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str()?.parse().ok())
}

fn value_to_f64_unwrapped(value: &Value) -> Option<f64> {
    value_to_f64(value).or_else(|| value.as_object()?.get("value").and_then(value_to_f64))
}

fn value_to_i64(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_str()?.parse().ok())
}

fn value_to_u32(value: &Value) -> Option<u32> {
    if let Some(value) = value.as_u64() {
        return u32::try_from(value).ok();
    }
    let parsed = value.as_str()?.parse::<u32>().ok()?;
    Some(parsed)
}

fn value_to_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(value) => Some(*value),
        Value::Number(value) => value.as_i64().map(|value| value != 0),
        Value::String(value) => match value.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn value_to_bool_unwrapped(value: &Value) -> Option<bool> {
    value_to_bool(value).or_else(|| value.as_object()?.get("value").and_then(value_to_bool))
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn value_to_string_unwrapped(value: &Value) -> Option<String> {
    value_to_string(value).or_else(|| value.as_object()?.get("value").and_then(value_to_string))
}

fn scene_color_from_value(value: &Value) -> Option<String> {
    if let Some(value) = value_to_string_unwrapped(value) {
        return Some(normalize_color(&value));
    }
    let (r, g, b) = vector3_components_from_value(value)?;
    Some(format!(
        "#{:02x}{:02x}{:02x}",
        color_component_to_u8(r as f32),
        color_component_to_u8(g as f32),
        color_component_to_u8(b as f32)
    ))
}

fn scene_vector3_from_value(value: &Value) -> Option<Value> {
    let (x, y, z) = vector3_components_from_value(value)?;
    Some(json!({
        "x": x,
        "y": y,
        "z": z
    }))
}

fn vector3_components_from_value(value: &Value) -> Option<(f64, f64, f64)> {
    match value {
        Value::Array(values) => {
            let x = values.first().and_then(value_to_f64)?;
            let y = values.get(1).and_then(value_to_f64)?;
            let z = values.get(2).and_then(value_to_f64).unwrap_or(0.0);
            Some((x, y, z))
        }
        Value::Object(object) => {
            if let Some(value) = object.get("value")
                && let Some(components) = vector3_components_from_value(value)
            {
                return Some(components);
            }
            let x = object
                .get("x")
                .or_else(|| object.get("r"))
                .and_then(value_to_f64)?;
            let y = object
                .get("y")
                .or_else(|| object.get("g"))
                .and_then(value_to_f64)?;
            let z = object
                .get("z")
                .or_else(|| object.get("b"))
                .and_then(value_to_f64)
                .unwrap_or(0.0);
            Some((x, y, z))
        }
        Value::String(value) => {
            let components = value
                .split_whitespace()
                .filter_map(|part| part.parse::<f64>().ok())
                .collect::<Vec<_>>();
            if components.len() >= 2 {
                Some((
                    components[0],
                    components[1],
                    *components.get(2).unwrap_or(&0.0),
                ))
            } else {
                None
            }
        }
        Value::Number(_) | Value::Bool(_) | Value::Null => None,
    }
}

fn scene_i64_map_from_value(value: &Value) -> Option<Value> {
    let object = value.as_object()?;
    let mut output = Map::new();
    for (key, value) in object {
        if let Some(value) = value_to_i64(value) {
            output.insert(key.clone(), json!(value));
        }
    }
    Some(Value::Object(output))
}

fn insert_optional_bool(
    source: &Map<String, Value>,
    source_key: &str,
    target_key: &str,
    target: &mut Map<String, Value>,
) {
    if let Some(value) = source.get(source_key).and_then(value_to_bool) {
        target.insert(target_key.to_owned(), Value::Bool(value));
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
    scene_package: Option<ScenePackageImport>,
}

#[derive(Debug)]
struct ScenePackageImport {
    version: String,
    entry_count: usize,
    staging_root: PathBuf,
}

#[derive(Debug)]
struct ScenePackageEntry {
    source_path: String,
    relative_path: PathBuf,
    data_offset: usize,
    size: usize,
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
        let mut project_root = root.to_path_buf();
        let mut scene_package = None;
        if source_type == SourceType::Scene && root.join(SCENE_PACKAGE_FILE).is_file() {
            let imported = extract_wallpaper_engine_scene_package(
                root,
                &project_json,
                preview_file.as_deref(),
            )?;
            project_root = imported.staging_root.clone();
            scene_package = Some(imported);
        }

        Ok(Self {
            root: project_root,
            raw,
            source_type,
            entry_file,
            preview_file,
            title,
            authors,
            scene_package,
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
        if self.scene_package.is_some() {
            features.insert("scene-package".to_owned());
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

impl Drop for WallpaperEngineProject {
    fn drop(&mut self) {
        if let Some(scene_package) = &self.scene_package {
            let _ = fs::remove_dir_all(&scene_package.staging_root);
        }
    }
}

fn extract_wallpaper_engine_scene_package(
    root: &Path,
    project_json: &str,
    preview_file: Option<&str>,
) -> Result<ScenePackageImport, ConversionError> {
    let package_path = root.join(SCENE_PACKAGE_FILE);
    let bytes = fs::read(&package_path).map_err(|source| ConversionError::ReadFile {
        path: package_path.clone(),
        source,
    })?;
    let (version, entries) = parse_wallpaper_engine_scene_package(&bytes)?;
    let staging_root = create_scene_package_staging_root(root)?;
    fs::write(staging_root.join(PROJECT_FILE), project_json).map_err(ConversionError::WriteFile)?;
    copy_scene_package_preview(root, &staging_root, preview_file)?;

    let mut seen_paths = BTreeSet::new();
    for entry in &entries {
        if !seen_paths.insert(entry.relative_path.clone()) {
            return Err(ConversionError::InvalidProject(format!(
                "{SCENE_PACKAGE_FILE} contains duplicate entry {}",
                entry.source_path
            )));
        }
        let end = entry
            .data_offset
            .checked_add(entry.size)
            .ok_or_else(|| scene_package_invalid("entry byte range overflowed"))?;
        let payload = bytes
            .get(entry.data_offset..end)
            .ok_or_else(|| scene_package_invalid("entry byte range is outside the package"))?;
        let dest = staging_root.join(&entry.relative_path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(ConversionError::CreateDir)?;
        }
        fs::write(&dest, payload).map_err(ConversionError::WriteFile)?;
    }

    Ok(ScenePackageImport {
        version,
        entry_count: entries.len(),
        staging_root,
    })
}

fn copy_scene_package_preview(
    source_root: &Path,
    staging_root: &Path,
    preview_file: Option<&str>,
) -> Result<(), ConversionError> {
    let Some(preview_file) = preview_file else {
        return Ok(());
    };
    let Ok(relative) = normalize_relative_path(preview_file) else {
        return Ok(());
    };
    let source = source_root.join(&relative);
    if !source.is_file() {
        return Ok(());
    }
    let dest = staging_root.join(relative);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(ConversionError::CreateDir)?;
    }
    fs::copy(source, dest).map_err(ConversionError::CopyFile)?;
    Ok(())
}

fn parse_wallpaper_engine_scene_package(
    bytes: &[u8],
) -> Result<(String, Vec<ScenePackageEntry>), ConversionError> {
    let mut cursor = 0usize;
    let version_len = scene_package_read_len(bytes, &mut cursor, "version length")?;
    if version_len == 0 || version_len > 64 {
        return Err(scene_package_invalid("version length is invalid"));
    }
    let version_bytes = scene_package_take(bytes, &mut cursor, version_len, "version")?;
    let version = std::str::from_utf8(version_bytes)
        .map_err(|_| scene_package_invalid("version is not UTF-8"))?
        .to_owned();
    if !version.starts_with("PKGV") {
        return Err(scene_package_invalid("version marker is not PKGV"));
    }

    let file_count = scene_package_read_len(bytes, &mut cursor, "file count")?;
    if file_count > 100_000 {
        return Err(scene_package_invalid("file count is unrealistically large"));
    }
    let mut parsed_entries = Vec::with_capacity(file_count);
    for _ in 0..file_count {
        let path_len = scene_package_read_len(bytes, &mut cursor, "entry path length")?;
        if path_len == 0 || path_len > 4096 {
            return Err(scene_package_invalid("entry path length is invalid"));
        }
        let path_bytes = scene_package_take(bytes, &mut cursor, path_len, "entry path")?;
        let source_path = std::str::from_utf8(path_bytes)
            .map_err(|_| scene_package_invalid("entry path is not UTF-8"))?
            .to_owned();
        let relative_path = normalize_relative_path(&source_path)?;
        let relative_offset = scene_package_read_len(bytes, &mut cursor, "entry offset")?;
        let size = scene_package_read_len(bytes, &mut cursor, "entry size")?;
        parsed_entries.push((source_path, relative_path, relative_offset, size));
    }

    let data_start = cursor;
    let mut entries = Vec::with_capacity(parsed_entries.len());
    for (source_path, relative_path, relative_offset, size) in parsed_entries {
        let data_offset = data_start
            .checked_add(relative_offset)
            .ok_or_else(|| scene_package_invalid("entry data offset overflowed"))?;
        let end = data_offset
            .checked_add(size)
            .ok_or_else(|| scene_package_invalid("entry data range overflowed"))?;
        if end > bytes.len() {
            return Err(scene_package_invalid(
                "entry data range is outside the package",
            ));
        }
        entries.push(ScenePackageEntry {
            source_path,
            relative_path,
            data_offset,
            size,
        });
    }
    Ok((version, entries))
}

fn scene_package_read_len(
    bytes: &[u8],
    cursor: &mut usize,
    field: &str,
) -> Result<usize, ConversionError> {
    let end = cursor
        .checked_add(4)
        .ok_or_else(|| scene_package_invalid("package cursor overflowed"))?;
    let value = bytes
        .get(*cursor..end)
        .ok_or_else(|| scene_package_invalid(&format!("{field} is truncated")))?;
    *cursor = end;
    let value = u32::from_le_bytes(value.try_into().expect("slice length checked"));
    usize::try_from(value).map_err(|_| scene_package_invalid(&format!("{field} is too large")))
}

fn scene_package_take<'a>(
    bytes: &'a [u8],
    cursor: &mut usize,
    len: usize,
    field: &str,
) -> Result<&'a [u8], ConversionError> {
    let end = cursor
        .checked_add(len)
        .ok_or_else(|| scene_package_invalid("package cursor overflowed"))?;
    let value = bytes
        .get(*cursor..end)
        .ok_or_else(|| scene_package_invalid(&format!("{field} is truncated")))?;
    *cursor = end;
    Ok(value)
}

fn scene_package_invalid(message: &str) -> ConversionError {
    ConversionError::InvalidProject(format!("{SCENE_PACKAGE_FILE}: {message}"))
}

fn create_scene_package_staging_root(root: &Path) -> Result<PathBuf, ConversionError> {
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("scene")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let name = if name.is_empty() { "scene" } else { &name };
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    for index in 0..32 {
        let path = env::temp_dir().join(format!(
            "gilder-we-scene-pkg-{}-{nanos}-{index}-{name}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
            Err(err) => return Err(ConversionError::CreateDir(err)),
        }
    }
    Err(ConversionError::InvalidProject(format!(
        "could not create a unique {SCENE_PACKAGE_FILE} staging directory"
    )))
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_scene: Option<FullSceneConversionStatus>,
    pub copied_assets: Vec<String>,
    pub generated_assets: Vec<String>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullSceneConversionStatus {
    pub target_runtime: String,
    pub current_runtime: String,
    pub progress_estimate_percent: u8,
    #[serde(default)]
    pub full_scene_complete: bool,
    pub execution_model: String,
    pub source_scene_metadata: Vec<String>,
    pub completed_boundaries: Vec<String>,
    pub pending_boundaries: Vec<String>,
    #[serde(default)]
    pub unsupported_boundaries: Vec<String>,
}

impl FullSceneConversionStatus {
    fn native_vulkan_scene_boundary() -> Self {
        Self {
            target_runtime: "native-vulkan-full-scene".to_owned(),
            current_runtime: "native-vulkan-scene-runtime".to_owned(),
            progress_estimate_percent: 100,
            full_scene_complete: true,
            execution_model: "original scene metadata preserved in first-class gscene; native Vulkan full-scene boundaries now lower layer order, WE scene.pkg containers, WE parent ids into gscene children, native scene graph transform/opacity execution, WE text/value wrappers, visible property bindings, shape/solid/radius objects, native deterministic particle emitter expansion, WE particle runtime fields, script/value wrappers, deterministic numeric SceneScript expressions, explicit keyframe timelines, embedded WE property keyframes, deterministic animation-layer keyframes, per-frame fixed-topology timeline geometry updates, geometry field animation, parallax depth, WE TEXV0005/TEXB0004 RGBA textures into native BC7 .gtex GPU textures, WE DXT1/DXT5/BC7 GPU textures into native BC .gtex payloads, and WE TEXB0004 video payloads into native gscene video resources including spritesheet atlases into gscene text/property/shape/timeline/camera/image/video fields, render clear color into snapshot layers, retained sampled-image resources with UV-frame animation, clear-background composition, rounded-rectangle/simple/concave-path tessellation, cubic/smooth-cubic/quadratic/smooth-quadratic/arc path flattening, compound even-odd path fill, stroke geometry, deterministic text glyph geometry, single-video-layer Vulkan Video scene composition, time-sampled scene state, scene timeline animation, property updates, pause/resume policy, package state persistence, scene audio cues resolved into the renderer and played by the native FFmpeg/PipeWire scene present runtime, and explicit unsupported Wallpaper Engine systems without legacy fallback or preview-image scene substitution".to_owned(),
            source_scene_metadata: Vec::new(),
            completed_boundaries: vec![
                "package-scene-detection".to_owned(),
                "wallpaper-engine-scene-pkg-import".to_owned(),
                "source-scene-metadata-preservation".to_owned(),
                "first-class-gscene-document".to_owned(),
                "scene-resource-copy-graph".to_owned(),
                "wallpaper-engine-parent-graph-lowering".to_owned(),
                "native-scene-graph-transform-opacity-execution".to_owned(),
                "render-clear-color-snapshot-layer".to_owned(),
                "wallpaper-engine-text-and-visible-property-lowering".to_owned(),
                "wallpaper-engine-shape-solid-radius-lowering".to_owned(),
                "wallpaper-engine-script-value-wrapper-lowering".to_owned(),
                "wallpaper-engine-deterministic-scenescript-expression-lowering".to_owned(),
                "wallpaper-engine-geometry-user-property-binding-lowering".to_owned(),
                "wallpaper-engine-explicit-keyframe-timeline-lowering".to_owned(),
                "wallpaper-engine-embedded-property-timeline-lowering".to_owned(),
                "wallpaper-engine-animation-layer-keyframe-lowering".to_owned(),
                "wallpaper-engine-tex-bc7-gtex-conversion".to_owned(),
                "wallpaper-engine-tex-bc-gtex-passthrough".to_owned(),
                "scene-we-spritesheet-atlas-runtime".to_owned(),
                "scene-geometry-field-animation-runtime".to_owned(),
                "per-frame-timeline-geometry-runtime".to_owned(),
                "wallpaper-engine-particle-field-lowering".to_owned(),
                "native-particle-system-runtime".to_owned(),
                "parallax-property-camera-model".to_owned(),
                "native-vulkan-sampled-image-scene-path".to_owned(),
                "descriptor-heap-sampled-image-resources".to_owned(),
                "native-vulkan-full-scene-runtime-status".to_owned(),
                "native-runtime-layer-coverage-metric".to_owned(),
                "time-sampled-scene-state".to_owned(),
                "clear-background-layer-composition".to_owned(),
                "solid-vector-shape-quad-geometry".to_owned(),
                "rounded-rectangle-tessellation-runtime".to_owned(),
                "simple-path-tessellation-runtime".to_owned(),
                "curve-path-flattening-runtime".to_owned(),
                "arc-path-flattening-runtime".to_owned(),
                "compound-path-evenodd-fill-runtime".to_owned(),
                "stroke-geometry-runtime".to_owned(),
                "deterministic-text-glyph-geometry-runtime".to_owned(),
                "scene-video-layer-bridge-detection".to_owned(),
                "timeline-animation-runtime".to_owned(),
                "property-update-runtime".to_owned(),
                "pause-resume-policy-runtime".to_owned(),
                "package-state-persistence".to_owned(),
                "scene-audio-cue-renderer-boundary".to_owned(),
                "scene-audio-cue-pipewire-present-runtime".to_owned(),
            ],
            pending_boundaries: Vec::new(),
            unsupported_boundaries: vec!["cursor-parallax-input-source".to_owned()],
        }
    }
}

impl ConversionReport {
    fn new(source_type: &str) -> Self {
        Self {
            source_type: source_type.to_owned(),
            detected_features: Vec::new(),
            converted_features: Vec::new(),
            unsupported_features: Vec::new(),
            full_scene: None,
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
    ReadFile {
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
            Self::ReadFile { path, source } => {
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
mod tests;
