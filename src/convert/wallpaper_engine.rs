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
mod tex;

use self::ir::{
    SceneAnimationLayerIr, SceneControllerIr, SceneNumericPropertyBindingIr,
    SceneNumericPropertyBindingIrResult, SceneParticleIr, SceneTimelineIr,
    scene_particle_seed_from_object,
};
use self::tex::{SceneWeModelFrameSize, SceneWeTexImage, SceneWeTexPayload};

const PROJECT_FILE: &str = "project.json";
const SCENE_PACKAGE_FILE: &str = "scene.pkg";
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
    let dimensions =
        probe_static_image_dimensions_for_manifest(project, source, report, variant_tools);
    let scene_source = write_static_image_audio_scene_document(
        project,
        output_dir,
        source,
        &copied.package_path,
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

    let mut resources = vec![json!({
        "id": "static-image",
        "type": "image",
        "source": image_package_path
    })];
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
        ..SceneDocumentBuildContext::default()
    };
    let mut resources = Vec::new();
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
        scene_collect_root_timelines(scene, &mut context);
    }
    for feature in &context.converted_features {
        push_unique(&mut report.converted_features, feature);
    }
    if !context.timelines.is_empty() {
        push_unique(&mut report.converted_features, "scene-keyframe-timeline");
    }
    nodes = scene_rebuild_parent_graph(nodes);
    scene_lower_pending_controllers(&mut nodes, &mut context);
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
    source_node_ids: BTreeMap<String, String>,
    pending_controllers: Vec<SceneControllerIr>,
    timelines: Vec<Value>,
    property_bindings: Vec<Value>,
    converted_features: Vec<String>,
    unsupported_features: Vec<Value>,
}

#[derive(Debug, Clone)]
struct SceneSourceModelConversion {
    value: Value,
    render_kind: Option<&'static str>,
    render_resource: Option<String>,
    render_properties: Option<Value>,
    render_size: Option<SceneWeModelFrameSize>,
    original_path: String,
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
    if let Some(animation_layers) = object.get("animationlayers") {
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
    if unlowered_layer_count > 0 {
        scene_push_unsupported(
            context,
            "we-animation-layer-blending",
            "Wallpaper Engine animation layer blend/rate references that cannot be represented as direct gscene keyframe channels remain preserved in provenance.",
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
    let initial_visible = binding.get("value").and_then(value_to_bool).unwrap_or(true);
    if let Some(property) = string_field(binding, &["user", "property"]) {
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
    } else {
        SceneVisibleConversion {
            static_visible: Some(initial_visible),
            initial_opacity: None,
        }
    }
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
    scene_push_numeric_property_binding(
        object,
        &["opacity", "alpha"],
        &node_id,
        "opacity",
        context,
        1.0,
        0.0,
    );
    if let Some(opacity) = number_value_field(object, &["opacity", "alpha"]) {
        let opacity = if let Some(visible_opacity) = visible.initial_opacity {
            opacity * visible_opacity
        } else {
            opacity
        };
        node.insert("opacity".to_owned(), json!(opacity.clamp(0.0, 1.0)));
    } else if let Some(opacity) = visible.initial_opacity {
        node.insert("opacity".to_owned(), json!(opacity.clamp(0.0, 1.0)));
    }
    if let Some(transform) = scene_transform_from_object(object, &node_id, context) {
        node.insert("transform".to_owned(), transform);
    }
    if let Some(depth) = number_value_field(object, &["parallax_depth", "parallaxDepth"]) {
        node.insert("parallax_depth".to_owned(), json!(depth));
    }
    if let Some(color) = scene_color_from_object(object) {
        node.insert("color".to_owned(), Value::String(color));
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
    if let Some(font_size) = scene_font_size_from_object(object) {
        node.insert("font_size".to_owned(), json!(font_size.max(1.0)));
    }
    if let Some(font_family) = scene_font_family_from_object(object) {
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
    if let Some((controller, pending_controller)) =
        scene_controller_from_object(object, &node_id, source_model.as_ref())
    {
        scene_merge_node_properties(&mut node, json!({ "controller": controller }));
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
    scene_collect_object_timelines(object, &node_id, context);

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
    if matches!(kind, "shader" | "script" | "unknown")
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
    if scene_bool_value_field(object, &["solid", "issolid", "isSolid"]).unwrap_or(false) {
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
            original_path: model_path,
        });
    };

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
                let frame_size = scene_model_frame_size(model_object);
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
                    model.insert("puppet".to_owned(), Value::String(puppet));
                }
                insert_optional_bool(model_object, "solidlayer", "solid_layer", &mut model);
                insert_optional_bool(model_object, "passthrough", "passthrough", &mut model);
                return Some(SceneSourceModelConversion {
                    value: Value::Object(model),
                    render_kind,
                    render_resource,
                    render_properties,
                    render_size: frame_size,
                    original_path: model_path,
                });
            }
        }
        if let Some(puppet) = string_field(model_object, &["puppet"]) {
            model.insert("puppet".to_owned(), Value::String(puppet));
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
        render_size: model_json.as_object().and_then(scene_model_frame_size),
        original_path: model_path,
    })
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
        _ => return None,
    };
    let mut model = Map::new();
    model.insert("source".to_owned(), Value::String(model_path.to_owned()));
    model.insert("builtin".to_owned(), Value::Bool(true));
    model.insert("utility".to_owned(), Value::String(utility.to_owned()));
    model.insert("passthrough".to_owned(), Value::Bool(true));
    Some(SceneSourceModelConversion {
        value: Value::Object(model),
        render_kind: Some("script"),
        render_resource: None,
        render_properties: None,
        render_size: None,
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
    let mut render_properties = None;
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
                if render_properties.is_none()
                    && let Some(spritesheet) = decoded.spritesheet
                {
                    render_properties = Some(json!({ "spritesheet": spritesheet }));
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
        || texture.contains('/')
        || texture.starts_with("_rt_")
    {
        texture.to_owned()
    } else {
        format!("materials/{texture}.tex")
    }
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
        video @ SceneWeTexPayload::Video(_) => {
            return scene_copy_decoded_tex_video_resource(
                output_dir,
                source_path,
                &source,
                video,
                report,
                context,
                resources,
            );
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
    if let Err(err) = gtex::write_bc7_gtex(&dest, &decoded) {
        report.errors.push(format!(
            "Failed to write native BC7 scene texture {} to {}: {err}.",
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
        "original_source": source_path,
        "role": role
    }));
    push_unique(
        &mut context.converted_features,
        "scene-we-tex-bc7-gpu-texture",
    );
    Some(SceneDecodedTexResource {
        resource_id,
        render_kind: "image",
        spritesheet,
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
    let row_bytes = usize::try_from(layout.frame_width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or_else(|| "frame row byte count overflowed".to_owned())?;
    let stride = usize::try_from(image.width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or_else(|| "atlas row byte count overflowed".to_owned())?;
    let frame_len = tex::rgba_len(layout.frame_width, layout.frame_height)?;
    let mut rgba = Vec::with_capacity(frame_len);
    for row in 0..usize::try_from(layout.frame_height)
        .map_err(|_| "frame height does not fit this platform".to_owned())?
    {
        let start = row
            .checked_mul(stride)
            .ok_or_else(|| "atlas row offset overflowed".to_owned())?;
        let end = start
            .checked_add(row_bytes)
            .ok_or_else(|| "atlas row range overflowed".to_owned())?;
        let row = image
            .rgba
            .get(start..end)
            .ok_or_else(|| "decoded atlas is shorter than declared dimensions".to_owned())?;
        rgba.extend_from_slice(row);
    }
    Ok(SceneWeTexImage {
        width: layout.frame_width,
        height: layout.frame_height,
        rgba,
    })
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
                    if controller_input_pending || !idle_controller_ready {
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
        }
    } else {
        status
            .pending_boundaries
            .retain(|boundary| boundary != "mixed-video-scene-composition");
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
    status
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
        }
    }
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
        "completed_boundaries": status.completed_boundaries,
        "pending_boundaries": status.pending_boundaries
    })
}

fn scene_material_graph_runtime_ready(
    report: &ConversionReport,
    context: &SceneDocumentBuildContext,
) -> bool {
    report
        .converted_features
        .iter()
        .any(|feature| feature == "scene-we-material-graph-runtime")
        && !context.unsupported_features.iter().any(|feature| {
            feature
                .get("feature")
                .and_then(Value::as_str)
                .is_some_and(scene_feature_blocks_material_graph_runtime)
        })
}

fn scene_feature_blocks_material_graph_runtime(feature: &str) -> bool {
    feature.contains("shader")
        || feature.contains("effect")
        || feature.contains("material")
        || feature.contains("tex")
        || feature.contains("runtime-texture")
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
    push_unique(&mut report.unsupported_features, "scene-runtime");
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
                "No preview image found; generated metadata-based scene preview poster and thumbnail.".to_owned(),
            );
            Ok(Some(preview))
        }
        MissingPreviewFallback::Shader { source } => {
            let preview = generate_svg_placeholder_preview(
                project,
                output_dir,
                source,
                PlaceholderKind::Shader,
                report,
            )?;
            report.warnings.push(
                "No preview image found; generated metadata-based shader fallback poster and thumbnail.".to_owned(),
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
    Shader { source: &'a str },
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

fn probe_static_image_dimensions_for_manifest(
    project: &WallpaperEngineProject,
    source: &str,
    report: &mut ConversionReport,
    variant_tools: Option<StaticImageVariantTools<'_>>,
) -> Option<ImageDimensions> {
    if !is_raster_image_path(source) {
        return None;
    }
    let ffprobe = variant_tools
        .map(|tools| tools.ffprobe.to_path_buf())
        .or_else(|| find_executable_on_path(FFPROBE_BINARY));
    let Some(ffprobe) = ffprobe else {
        return None;
    };
    let relative = match normalize_relative_path(source) {
        Ok(relative) => relative,
        Err(err) => {
            report.warnings.push(format!(
                "Static image source dimensions were not recorded: {err}."
            ));
            return None;
        }
    };
    match probe_image_dimensions(&ffprobe, &project.root.join(relative)) {
        Ok(dimensions) => Some(dimensions),
        Err(err) => {
            report.warnings.push(format!(
                "Static image source dimensions were not recorded: {err}."
            ));
            None
        }
    }
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

fn command_output_with_retry(command: &mut Command, executable: &Path) -> Result<Output, String> {
    for attempt in 0..=5 {
        match command.output() {
            Ok(output) => return Ok(output),
            Err(err) if is_executable_file_busy(&err) && attempt < 5 => {
                thread::sleep(Duration::from_millis(10 * (attempt + 1) as u64));
            }
            Err(err) => return Err(format!("failed to run {}: {err}", executable.display())),
        }
    }
    unreachable!("bounded retry loop must return before exhausting attempts")
}

#[cfg(unix)]
fn is_executable_file_busy(err: &io::Error) -> bool {
    err.raw_os_error() == Some(26)
}

#[cfg(not(unix))]
fn is_executable_file_busy(_err: &io::Error) -> bool {
    false
}

fn probe_image_dimensions(ffprobe: &Path, source_path: &Path) -> Result<ImageDimensions, String> {
    let mut command = Command::new(ffprobe);
    command
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
        .arg(source_path);
    let output = command_output_with_retry(&mut command, ffprobe)?;
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
    let mut command = Command::new(ffmpeg);
    command
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(source_path)
        .args(["-frames:v", "1", "-an", "-sn", "-vf", &filter])
        .arg(output_path);
    let output = command_output_with_retry(&mut command, ffmpeg)?;

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
    let mut command = Command::new(ffmpeg);
    command
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
        .arg(output_path);
    let output = command_output_with_retry(&mut command, ffmpeg)?;

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
    Shader,
}

impl PlaceholderKind {
    fn label(&self) -> &'static str {
        match self {
            Self::Video => "Video",
            Self::Scene => "Scene",
            Self::Shader => "Shader",
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

fn static_image_audio_sources(project: &WallpaperEngineProject) -> Vec<String> {
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
        Value::String(source) if in_audio_field && is_audio_path(source) => {
            sources.insert(source.clone());
        }
        _ => {}
    }
}

fn is_audio_path(value: &str) -> bool {
    Path::new(value.trim())
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(is_audio_extension)
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
enum SourceType {
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
    fn as_str(self) -> &'static str {
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

    fn from_extension(extension: &str) -> Self {
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
    pub execution_model: String,
    pub source_scene_metadata: Vec<String>,
    pub completed_boundaries: Vec<String>,
    pub pending_boundaries: Vec<String>,
}

impl FullSceneConversionStatus {
    fn native_vulkan_scene_boundary() -> Self {
        Self {
            target_runtime: "native-vulkan-full-scene".to_owned(),
            current_runtime: "native-vulkan-scene-runtime".to_owned(),
            progress_estimate_percent: 99,
            execution_model: "original scene metadata preserved in first-class gscene; native Vulkan full-scene boundaries now lower layer order, WE scene.pkg containers, WE parent ids into gscene children, native scene graph transform/opacity execution, WE text/value wrappers, visible property bindings, shape/solid/radius objects, native deterministic particle emitter expansion, WE particle runtime fields, script/value wrappers, deterministic numeric SceneScript expressions, explicit keyframe timelines, embedded WE property keyframes, deterministic animation-layer keyframes, per-frame fixed-topology timeline geometry updates, geometry field animation, parallax depth, WE TEXV0005/TEXB0004 RGBA textures into native BC7 .gtex GPU textures, and WE TEXB0004 video payloads into native gscene video resources including spritesheet atlases into gscene text/property/shape/timeline/camera/image/video fields, render clear color into snapshot layers, retained sampled-image resources with UV-frame animation, clear-background composition, rounded-rectangle/simple/concave-path tessellation, cubic/smooth-cubic/quadratic/smooth-quadratic/arc path flattening, compound even-odd path fill, stroke geometry, deterministic text glyph geometry, single-video-layer Vulkan Video scene composition, time-sampled scene state, scene timeline animation, property updates, pause/resume policy, package state persistence, scene audio cues resolved into the renderer and played by the native FFmpeg/PipeWire scene present runtime, and explicit unsupported Wallpaper Engine systems without legacy fallback or preview-image scene substitution".to_owned(),
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
            pending_boundaries: vec![
                "arbitrary-scenescript-runtime".to_owned(),
                "shader-material-graph".to_owned(),
                "cursor-parallax-input-source".to_owned(),
                "mixed-video-scene-composition".to_owned(),
            ],
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
            r##"{
              "type": "image",
              "title": "Static Without Preview",
              "file": "wallpaper.png"
            }"##,
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
    fn converts_static_image_audio_to_scene_audio_cue() {
        let source = TestDir::new("we-static-audio-source");
        let output = TestDir::new("we-static-audio-output");
        output.remove();
        source.write_file("wallpaper.png", "not real png");
        source.write_file("music.ogg", "not real ogg");
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "image",
              "title": "Static With Audio",
              "file": "wallpaper.png",
              "audio": "music.ogg"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let manifest: Value =
            serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
                .unwrap();
        assert_eq!(manifest["kind"], "scene");
        assert_eq!(manifest["entry"]["type"], "scene");
        assert_eq!(manifest["entry"]["source"], "assets/scene.gscene.json");
        assert!(manifest["entry"].get("max_fps").is_none());
        assert_eq!(manifest["runtime"]["allow_audio"], true);
        assert!(output.path().join("assets/wallpaper.png").exists());
        assert!(output.path().join("assets/audio-cue-0.ogg").exists());

        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(scene["resources"][0]["type"], "image");
        assert_eq!(scene["resources"][0]["source"], "assets/wallpaper.png");
        assert_eq!(scene["resources"][1]["type"], "audio");
        assert_eq!(scene["resources"][1]["source"], "assets/audio-cue-0.ogg");
        assert_eq!(scene["nodes"][0]["type"], "image");
        assert_eq!(scene["nodes"][0]["resource"], "static-image");
        assert_eq!(scene["nodes"][0]["audio"][0]["resource"], "static-audio-0");
        assert_eq!(scene["nodes"][0]["audio"][0]["playback_mode"], "loop");

        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .converted_features
                .contains(&"static-image-audio-scene".to_owned())
        );
        assert!(
            report
                .converted_features
                .contains(&"scene-audio-cue-pipewire-present-runtime".to_owned())
        );
        assert!(
            !report
                .unsupported_features
                .contains(&"audio-runtime".to_owned())
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
        write_executable_script(
            &ffprobe,
            r#"#!/bin/sh
printf '{"streams":[{"width":7680,"height":4320}]}'
exit 0
"#,
        );
        let ffmpeg = tools.path().join("ffmpeg");
        write_executable_script(
            &ffmpeg,
            r#"#!/bin/sh
out=""
for arg in "$@"; do
  out="$arg"
done
printf 'png-variant' > "$out"
exit 0
"#,
        );
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
        assert_eq!(manifest["entry"]["width"], 7680);
        assert_eq!(manifest["entry"]["height"], 4320);
        let ids = variants
            .iter()
            .map(|variant| variant["id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![
                "landscape-1080p",
                "landscape-2160p",
                "ultrawide-1080p",
                "ultrawide-1440p",
                "portrait-1080p",
                "portrait-2160p",
            ]
        );
        assert_eq!(variants[0]["width"], 1920);
        assert_eq!(variants[0]["height"], 1080);
        assert_eq!(variants[3]["width"], 3440);
        assert_eq!(variants[3]["height"], 1440);
        assert_eq!(variants[5]["width"], 2160);
        assert_eq!(variants[5]["height"], 3840);
        for id in &ids {
            assert!(output.path().join(format!("variants/{id}.png")).exists());
        }
        assert!(
            report
                .generated_assets
                .contains(&"variants/landscape-1080p.png".to_owned())
        );
        assert!(
            report
                .converted_features
                .contains(&"static-image:variant:portrait-2160p".to_owned())
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
        write_executable_script(
            &ffmpeg,
            r#"#!/bin/sh
for arg in "$@"; do
  case "$arg" in
    *.jpg) printf 'jpeg-frame' > "$arg" ;;
  esac
done
exit 0
"#,
        );

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
        write_executable_script(&ffmpeg, "#!/bin/sh\nexit 0\n");

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
    fn converts_shader_project_with_fallback_manifest() {
        let source = TestDir::new("we-shader-source");
        let output = TestDir::new("we-shader-output");
        output.remove();
        source.write_file(
            "main.frag",
            r#"
uniform float u_time;
uniform vec2 u_resolution;
uniform float u_intensity;
void main() {}
"#,
        );
        source.write_file(
            PROJECT_FILE,
            r##"{
              "type": "shader",
              "title": "Shader Example",
              "file": "main.frag",
              "general": {
                "properties": {
                  "Intensity": { "type": "slider", "min": 0, "max": 1, "value": 0.5 }
                }
              }
            }"##,
        );

        convert_project(source.path(), output.path()).unwrap();
        let manifest: Value =
            serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
                .unwrap();
        assert_eq!(manifest["kind"], "shader");
        assert_eq!(manifest["entry"]["type"], "shader");
        assert_eq!(manifest["entry"]["source"], "shaders/main.frag");
        assert_eq!(manifest["entry"]["fallback"], "previews/poster.svg");
        assert_eq!(manifest["entry"]["language"], "glsl");
        assert_eq!(manifest["entry"]["max_fps"], 60);
        assert_eq!(manifest["properties"]["Intensity"]["type"], "range");
        let uniforms = manifest["entry"]["uniforms"].as_array().unwrap();
        assert!(
            uniforms
                .iter()
                .any(|uniform| { uniform["name"] == "u_time" && uniform["source"] == "time" })
        );
        assert!(uniforms.iter().any(|uniform| {
            uniform["name"] == "u_resolution" && uniform["source"] == "resolution"
        }));
        assert!(
            uniforms
                .iter()
                .any(|uniform| { uniform["name"] == "u_mouse" && uniform["source"] == "mouse" })
        );
        assert!(uniforms.iter().any(|uniform| {
            uniform["name"] == "u_intensity"
                && uniform["source"] == "property"
                && uniform["property"] == "Intensity"
        }));
        assert!(output.path().join("shaders/main.frag").exists());
        assert!(output.path().join("previews/poster.svg").exists());

        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(report.source_type, "shader");
        assert!(report.detected_features.contains(&"shader".to_owned()));
        assert!(report.converted_features.contains(&"shader".to_owned()));
        assert!(
            report
                .unsupported_features
                .contains(&"shader-runtime".to_owned())
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("fallback poster"))
        );
    }

    #[test]
    fn converts_scene_project_to_native_scene_document() {
        let source = TestDir::new("we-scene-source");
        let output = TestDir::new("we-scene-output");
        output.remove();
        source.write_file(
            "scene.json",
            r#"{"objects":[{"type":"image","path":"background.png"}]}"#,
        );
        source.write_file("background.png", "not real png");
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
        assert_eq!(manifest["kind"], "scene");
        assert_eq!(manifest["entry"]["type"], "scene");
        assert_eq!(manifest["entry"]["source"], "assets/scene.gscene.json");
        assert!(manifest["entry"].get("max_fps").is_none());
        assert!(manifest["entry"].get("fallback").is_none());
        assert_eq!(manifest["preview"]["poster"], "previews/poster.svg");
        assert!(output.path().join("metadata/source-scene.json").exists());
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(scene["version"], 1);
        assert_eq!(scene["profile"], "native-vulkan-full-scene");
        assert_eq!(scene["source"]["metadata"], "metadata/source-scene.json");
        assert_eq!(scene["source"]["entry"], "scene.json");
        assert_eq!(scene["nodes"][0]["type"], "image");
        assert_eq!(scene["nodes"][0]["resource"], "resource-1-background");
        assert_eq!(
            scene["resources"][0]["source"],
            "assets/scene-resources/scene/resource-1-background.png"
        );
        assert!(scene["native_lowering"].get("fallback").is_none());
        assert!(output.path().join("previews/poster.svg").exists());
        assert!(output.path().join("previews/thumbnail.svg").exists());
        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(report.converted_features.contains(&"scene".to_owned()));
        let full_scene = report.full_scene.as_ref().expect("full scene status");
        assert_eq!(full_scene.target_runtime, "native-vulkan-full-scene");
        assert_eq!(full_scene.current_runtime, "native-vulkan-scene-runtime");
        assert_eq!(full_scene.progress_estimate_percent, 99);
        assert!(
            full_scene
                .source_scene_metadata
                .contains(&"metadata/source-scene.json".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"descriptor-heap-sampled-image-resources".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"wallpaper-engine-parent-graph-lowering".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"native-scene-graph-transform-opacity-execution".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"render-clear-color-snapshot-layer".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"wallpaper-engine-text-and-visible-property-lowering".to_owned())
        );
        assert!(full_scene.completed_boundaries.contains(
            &"wallpaper-engine-deterministic-scenescript-expression-lowering".to_owned()
        ));
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"wallpaper-engine-animation-layer-keyframe-lowering".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"native-vulkan-full-scene-runtime-status".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"native-runtime-layer-coverage-metric".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"deterministic-text-glyph-geometry-runtime".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"stroke-geometry-runtime".to_owned())
        );
        assert!(
            !full_scene
                .completed_boundaries
                .contains(&"vulkan-video-scene-layer-composition".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"timeline-animation-runtime".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"scene-geometry-field-animation-runtime".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"per-frame-timeline-geometry-runtime".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"wallpaper-engine-particle-field-lowering".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"native-particle-system-runtime".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"wallpaper-engine-tex-bc7-gtex-conversion".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"wallpaper-engine-scene-pkg-import".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"scene-we-spritesheet-atlas-runtime".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"parallax-property-camera-model".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"property-update-runtime".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"pause-resume-policy-runtime".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"package-state-persistence".to_owned())
        );
        assert!(
            !full_scene
                .pending_boundaries
                .contains(&"mixed-video-scene-composition".to_owned())
        );
        assert!(
            !full_scene
                .pending_boundaries
                .contains(&"timeline-animation-runtime".to_owned())
        );
        assert!(
            !full_scene
                .pending_boundaries
                .contains(&"particle-systems".to_owned())
        );
        assert!(
            !full_scene
                .pending_boundaries
                .contains(&"package-state-persistence".to_owned())
        );
        assert!(
            !full_scene
                .pending_boundaries
                .contains(&"full-wallpaper-engine-scene-graph".to_owned())
        );
        assert!(
            !full_scene
                .pending_boundaries
                .contains(&"spritesheet-animation-runtime".to_owned())
        );
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
    fn scene_conversion_does_not_substitute_preview_fallback_node() {
        let source = TestDir::new("we-scene-empty-source");
        let output = TestDir::new("we-scene-empty-output");
        output.remove();
        source.write_file("scene.json", r#"{ "objects": [] }"#);
        source.write_file("preview.png", "not real png");
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Empty Scene",
              "file": "scene.json",
              "preview": "preview.png"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        assert!(scene["nodes"].as_array().unwrap().is_empty());
        assert!(
            scene["resources"]
                .as_array()
                .unwrap()
                .iter()
                .all(|resource| resource["id"] != "resource-preview-fallback")
        );
        assert!(
            scene["unsupported_features"]
                .as_array()
                .unwrap()
                .iter()
                .any(|feature| feature["feature"] == "empty-scene-graph")
        );
    }

    #[test]
    fn converts_wallpaper_engine_scene_pkg_without_preextracted_scene_files() {
        let source = TestDir::new("we-scene-pkg-source");
        let output = TestDir::new("we-scene-pkg-output");
        output.remove();
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Packaged Scene",
              "file": "scene.json",
              "preview": "preview.jpg"
            }"#,
        );
        source.write_bytes("preview.jpg", b"preview");
        source.write_bytes(
            SCENE_PACKAGE_FILE,
            &test_scene_pkg(&[
                (
                    "scene.json",
                    br#"{"objects":[{"type":"image","path":"background.png"}]}"#,
                ),
                ("background.png", b"not real png"),
            ]),
        );

        convert_project(source.path(), output.path()).unwrap();
        let manifest: Value =
            serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
                .unwrap();
        assert_eq!(manifest["kind"], "scene");
        assert_eq!(manifest["entry"]["source"], "assets/scene.gscene.json");
        assert_eq!(manifest["preview"]["poster"], "previews/poster.jpg");
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(scene["source"]["entry"], "scene.json");
        assert_eq!(scene["nodes"][0]["resource"], "resource-1-background");
        assert_eq!(
            scene["resources"][0]["source"],
            "assets/scene-resources/scene/resource-1-background.png"
        );
        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .detected_features
                .contains(&"scene-package".to_owned())
        );
        assert!(
            report
                .converted_features
                .contains(&"scene-we-package-import".to_owned())
        );
        let full_scene = report.full_scene.as_ref().expect("full scene status");
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"wallpaper-engine-scene-pkg-import".to_owned())
        );
    }

    #[test]
    fn scene_pkg_takes_precedence_over_preextracted_scene_files() {
        let source = TestDir::new("we-scene-pkg-precedence-source");
        let output = TestDir::new("we-scene-pkg-precedence-output");
        output.remove();
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Packaged Scene Precedence",
              "file": "scene.json"
            }"#,
        );
        source.write_file(
            "scene.json",
            r#"{"objects":[{"type":"image","path":"loose.png"}]}"#,
        );
        source.write_file("loose.png", "wrong loose png");

        let rgba = vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        let tex = test_we_tex_rgba(2, 2, &rgba);
        source.write_bytes(
            SCENE_PACKAGE_FILE,
            &test_scene_pkg(&[
                (
                    "scene.json",
                    br#"{"objects":[{"id":1,"name":"Packed","image":"models/renderable.json"}]}"#,
                ),
                (
                    "models/renderable.json",
                    br#"{ "material": "materials/renderable.json", "width": 2, "height": 2 }"#,
                ),
                (
                    "materials/renderable.json",
                    br#"{ "passes": [{ "textures": ["atlas"] }] }"#,
                ),
                ("materials/atlas.tex", &tex),
            ]),
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(scene["nodes"][0]["name"], "Packed");
        assert_eq!(scene["nodes"][0]["resource"], "resource-3-atlas-frame-0");
        assert!(
            scene["resources"]
                .as_array()
                .unwrap()
                .iter()
                .all(|resource| {
                    resource["source"]
                        .as_str()
                        .is_none_or(|source| !source.ends_with("loose.png"))
                })
        );
        assert!(
            output
                .path()
                .join("assets/scene-resources/scene/resource-3-atlas-frame-0.gtex")
                .exists()
        );
        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .converted_features
                .contains(&"scene-we-package-import".to_owned())
        );
        assert!(
            report
                .converted_features
                .contains(&"scene-we-tex-bc7-gpu-texture".to_owned())
        );
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
              "audio_response": true
            }"#,
        );
        source.write_file("background.png", "not real png");
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
            "timeline-animation",
            "scenescript",
            "custom-shader",
            "cursor-parallax-input-source",
            "audio-response-runtime",
        ] {
            assert!(
                report.unsupported_features.contains(&feature.to_owned()),
                "missing unsupported feature {feature}: {:?}",
                report.unsupported_features
            );
        }
        assert!(
            report
                .converted_features
                .contains(&"native-particle-runtime".to_owned()),
            "missing native particle conversion marker: {:?}",
            report.converted_features
        );
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        let particle = scene["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["type"] == "particle-emitter")
            .expect("particle node");
        assert_eq!(particle["properties"]["particle"]["source"], "sparks");
        assert_eq!(scene["systems"]["particles"], "ready");
    }

    #[test]
    fn lowers_wallpaper_engine_particle_runtime_fields_to_native_scene() {
        let source = TestDir::new("we-scene-particle-fields-source");
        let output = TestDir::new("we-scene-particle-fields-output");
        output.remove();
        source.write_file(
            "scene.json",
            r##"{
              "objects": [
                {
                  "id": 42,
                  "type": "particle",
                  "particle": "particles/spark.json",
                  "size": [320, 180, 0],
                  "directionDeg": -45,
                  "spreadDeg": 30,
                  "gravityDirection": [0, 1, 0],
                  "gravityStrength": 16,
                  "instanceoverride": {
                    "count": 12,
                    "speedMin": 8,
                    "speedMax": 24,
                    "size": [6, 10, 0],
                    "lifetime": 2,
                    "fadeOut": false,
                    "colorn": [1, 0.5, 0]
                  }
                }
              ]
            }"##,
        );
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Particle Field Scene",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        let particle = &scene["nodes"][0]["properties"]["particle"];
        assert_eq!(particle["source"], "particles/spark.json");
        assert_eq!(particle["seed"], 42);
        assert_eq!(particle["spawn_width"], 320.0);
        assert_eq!(particle["spawn_height"], 180.0);
        assert_eq!(particle["count"], 12);
        assert_eq!(particle["speed_min"], 8.0);
        assert_eq!(particle["speed_max"], 24.0);
        assert_eq!(particle["width"], 6.0);
        assert_eq!(particle["height"], 10.0);
        assert_eq!(particle["lifetime"], 2.0);
        assert_eq!(particle["fade"], false);
        assert_eq!(particle["color"], "#ff8000");
        assert_eq!(particle["direction_deg"], -45.0);
        assert_eq!(particle["spread_deg"], 30.0);
        assert_eq!(particle["gravity_x"], 0.0);
        assert_eq!(particle["gravity_y"], 16.0);

        let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(500, |_| None);
        assert_eq!(snapshot.layers.len(), 12);
        assert_eq!(snapshot.layers[0].width, Some(6.0));
        assert_eq!(snapshot.layers[0].height, Some(10.0));
        assert_eq!(snapshot.layers[0].color.as_deref(), Some("#ff8000"));
    }

    #[test]
    fn lowers_wallpaper_engine_particle_definition_file_to_native_scene() {
        let source = TestDir::new("we-scene-particle-definition-source");
        let output = TestDir::new("we-scene-particle-definition-output");
        output.remove();
        source.write_file(
            "scene.json",
            r##"{
              "objects": [
                {
                  "id": 77,
                  "type": "particle",
                  "particle": "particles/spark.json"
                }
              ]
            }"##,
        );
        source.write_file(
            "particles/spark.json",
            r##"{
              "maxcount": 32,
              "material": "materials/spark.json",
              "emitter": [
                {
                  "name": "boxrandom",
                  "distancemax": [96, 48, 0],
                  "directions": [1, 0.5, 0],
                  "rate": 16,
                  "speedmin": 4,
                  "speedmax": 12
                }
              ],
              "initializer": [
                { "name": "sizerandom", "min": 10, "max": 18 },
                { "name": "lifetimerandom", "min": 1, "max": 3 },
                { "name": "colorrandom", "min": [0, 0.5, 1], "max": [1, 0.5, 0] }
              ],
              "operator": [
                { "name": "movement", "gravity": [0, 18, 0] }
              ],
              "renderer": [
                { "name": "sprite", "fadealpha": true }
              ]
            }"##,
        );
        source.write_file(
            "materials/spark.json",
            r#"{ "passes": [{ "textures": ["textures/spark.png"] }] }"#,
        );
        source.write_file("textures/spark.png", "not real png");
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Particle Definition Scene",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        let particle = &scene["nodes"][0]["properties"]["particle"];
        assert_eq!(particle["source"], "particles/spark.json");
        assert_eq!(particle["seed"], 77);
        assert_eq!(particle["count"], 32);
        assert_eq!(particle["material"], "materials/spark.json");
        assert_eq!(particle["rate"], 16.0);
        assert_eq!(particle["speed_min"], 4.0);
        assert_eq!(particle["speed_max"], 12.0);
        assert_eq!(particle["spawn_width"], 192.0);
        assert_eq!(particle["spawn_height"], 48.0);
        assert_eq!(particle["size"], 7.0);
        assert_eq!(particle["lifetime"], 2.0);
        assert_eq!(particle["color"], "#808080");
        assert_eq!(particle["gravity_x"], 0.0);
        assert_eq!(particle["gravity_y"], 18.0);
        assert_eq!(particle["fade"], true);
        assert_eq!(particle["material_resource"], "resource-1-spark");
        assert_eq!(particle["render_resource"], "resource-2-spark");
        assert_eq!(scene["nodes"][0]["resource"], "resource-2-spark");
        assert_eq!(
            scene["resources"][1]["source"],
            "assets/scene-resources/scene/resource-2-spark.png"
        );

        let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(500, |_| None);
        assert_eq!(snapshot.layers.len(), 32);
        assert_eq!(snapshot.layers[0].kind, crate::core::SceneNodeKind::Image);
        assert_eq!(
            snapshot.layers[0].source.as_ref().map(|path| path.as_str()),
            Some("assets/scene-resources/scene/resource-2-spark.png")
        );

        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .converted_features
                .contains(&"wallpaper-engine-particle-definition-lowering".to_owned())
        );
        assert!(
            report
                .converted_features
                .contains(&"scene-we-particle-material-runtime".to_owned())
        );
        assert!(
            report
                .full_scene
                .as_ref()
                .unwrap()
                .completed_boundaries
                .contains(&"scene-we-particle-material-runtime".to_owned())
        );
    }

    #[test]
    fn converts_recordable_audio_response_to_native_scene_runtime() {
        let source = TestDir::new("we-scene-audio-response-source");
        let output = TestDir::new("we-scene-audio-response-output");
        output.remove();
        source.write_file(
            "scene.json",
            r##"{
              "objects": [
                {
                  "id": 7,
                  "type": "audio-response",
                  "color": "#44ccff",
                  "width": 320,
                  "height": 48,
                  "sound": "sounds/music.ogg"
                }
              ]
            }"##,
        );
        source.write_file("sounds/music.ogg", "not real ogg");
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Scene Audio Response",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();

        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .converted_features
                .contains(&"native-audio-response-runtime".to_owned())
        );
        assert!(
            !report
                .unsupported_features
                .contains(&"audio-response-runtime".to_owned())
        );
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(scene["systems"]["audio_response"], "ready");
        let node = &scene["nodes"].as_array().unwrap()[0];
        assert_eq!(node["type"], "audio-response");
        assert_eq!(node["audio"].as_array().unwrap().len(), 1);
        let bindings = scene["property_bindings"].as_array().unwrap();
        assert!(bindings.iter().any(|binding| {
            binding["target_node"] == node["id"]
                && binding["property"] == "audio.bass"
                && binding["target"] == "width"
        }));
        assert!(
            scene["native_lowering"]["completed_boundaries"]
                .as_array()
                .unwrap()
                .contains(&json!("native-audio-response-visual-runtime"))
        );
        assert!(
            !scene["native_lowering"]["pending_boundaries"]
                .as_array()
                .unwrap()
                .contains(&json!("audio-response-runtime"))
        );
        assert!(
            scene["native_lowering"]["pending_boundaries"]
                .as_array()
                .unwrap()
                .contains(&json!("pipewire-audio-spectrum-input-source"))
        );
    }

    #[test]
    fn converts_pure_scene_sound_object_to_audio_cue_node() {
        let source = TestDir::new("we-scene-audio-cue-node-source");
        let output = TestDir::new("we-scene-audio-cue-node-output");
        output.remove();
        source.write_file(
            "scene.json",
            r#"{
              "objects": [
                {
                  "id": 7,
                  "type": "sound",
                  "name": "Music Cue",
                  "sound": "sounds/music.ogg",
                  "playbackmode": "loop",
                  "startsilent": true
                }
              ]
            }"#,
        );
        source.write_file("sounds/music.ogg", "not real ogg");
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Scene Audio Cue",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();

        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(scene["systems"]["audio_response"], "absent");
        let node = &scene["nodes"].as_array().unwrap()[0];
        assert_eq!(node["type"], "audio");
        assert_eq!(node["audio"][0]["resource"], "resource-1-music");
        assert_eq!(node["audio"][0]["playback_mode"], "loop");
        assert_eq!(node["audio"][0]["start_silent"], true);
        assert!(
            !scene["unsupported_features"]
                .as_array()
                .unwrap()
                .iter()
                .any(|feature| feature["feature"] == "audio-response")
        );
        assert!(
            !scene["native_lowering"]["pending_boundaries"]
                .as_array()
                .unwrap()
                .contains(&json!("audio-response-runtime"))
        );

        let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
        assert_eq!(snapshot.layers[0].kind, crate::core::SceneNodeKind::Audio);
        assert_eq!(snapshot.layers[0].audio.len(), 1);
    }

    #[test]
    fn lowers_wallpaper_engine_builtin_util_models_without_missing_resource_noise() {
        let source = TestDir::new("we-scene-builtin-util-model-source");
        let output = TestDir::new("we-scene-builtin-util-model-output");
        output.remove();
        source.write_file(
            "scene.json",
            r#"{
              "objects": [
                {
                  "id": 9,
                  "name": "Click Controller",
                  "image": "models/util/composelayer.json",
                  "visible": {
                    "script": "export function update(value) { return value; }",
                    "value": true
                  },
                  "size": "512 512"
                }
              ]
            }"#,
        );
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Builtin Util Model Scene",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();

        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        let node = &scene["nodes"][0];
        assert_eq!(node["type"], "script");
        assert_eq!(
            node["provenance"]["model"]["source"],
            "models/util/composelayer.json"
        );
        assert_eq!(node["provenance"]["model"]["builtin"], true);
        assert_eq!(node["provenance"]["model"]["utility"], "composelayer");
        assert!(
            !scene["unsupported_features"]
                .as_array()
                .unwrap()
                .iter()
                .any(|feature| {
                    matches!(
                        feature["feature"].as_str(),
                        Some("missing-resource" | "we-model-json")
                    )
                })
        );

        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .converted_features
                .contains(&"wallpaper-engine-util-model-lowering".to_owned())
        );
        assert!(
            !report
                .warnings
                .iter()
                .any(|warning| warning.contains("models/util/composelayer.json"))
        );
    }

    #[test]
    fn converts_wallpaper_engine_scene_to_clean_gscene_provenance() {
        let source = TestDir::new("we-scene-provenance-source");
        let output = TestDir::new("we-scene-provenance-output");
        output.remove();
        source.write_file(
            "scene.json",
            r#"{
              "version": 42,
              "general": {
                "orthogonalprojection": { "width": 3840, "height": 2160 },
                "clearcolor": [0.1, 0.2, 0.3],
                "clearenabled": true,
                "hdr": true,
                "bloomstrength": 1.5,
                "cameraparallaxamount": 0.25
              },
              "camera": {
                "center": [0, 0, 0],
                "eye": { "x": 0, "y": 0, "z": 1 }
              },
              "objects": [
                {
                  "id": 7,
                  "parent": 3,
                  "dependencies": [1, 2],
                  "name": "Hero",
                  "image": "models/hero.json",
                  "origin": [10, 20, 0],
                  "angles": [0, 0, 1.5707963267948966],
                  "scale": [2, 3, 1],
                  "pivot": [0, 0, 0],
                  "alignment": "left",
                  "size": [100, 50, 0],
                  "effects": [
                    {
                      "file": "effects/glow.json",
                      "id": 9,
                      "name": "Glow",
                      "visible": true,
                      "passes": [
                        {
                          "id": 1,
                          "textures": [null, "mask"],
                          "combos": { "MODE": 2 },
                          "constantshadervalues": { "g_Time": 1.0 },
                          "usertextures": ["custom"]
                        }
                      ]
                    }
                  ],
                  "sound": ["sounds/theme.ogg"],
                  "playbackmode": "loop",
                  "volume": 0.75,
                  "startsilent": false,
                  "particle": "particles/spark.json"
                }
              ]
            }"#,
        );
        source.write_file(
            "models/hero.json",
            r#"{ "material": "hero", "solidlayer": false, "passthrough": false }"#,
        );
        source.write_file(
            "materials/hero.json",
            r#"{ "passes": [{ "textures": ["hero_albedo"] }] }"#,
        );
        source.write_file("materials/hero_albedo.tex", "not real tex");
        source.write_file("effects/glow.json", r#"{ "passes": [] }"#);
        source.write_file("sounds/theme.ogg", "not real ogg");
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Scene Provenance",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        assert!(scene.get("wallpaper_engine").is_none());
        assert_eq!(scene["source"]["format"], "wallpaper-engine-scene");
        assert_eq!(scene["import"]["source_format"], "wallpaper-engine-scene");
        assert_eq!(scene["import"]["source_version"], 42);
        assert_eq!(scene["import"]["object_count"], 1);
        assert_eq!(scene["import"]["feature_counts"]["model"], 1);
        assert_eq!(scene["import"]["feature_counts"]["audio"], 1);
        assert_eq!(scene["import"]["feature_counts"]["particle"], 1);
        assert_eq!(scene["import"]["feature_counts"]["effect"], 1);
        assert_eq!(scene["size"]["width"], 3840);
        assert_eq!(scene["size"]["height"], 2160);
        assert_eq!(scene["render"]["clear_color"], "#1a334d");
        assert_eq!(scene["render"]["hdr"], true);
        assert_eq!(scene["render"]["bloom"]["strength"], 1.5);
        assert_eq!(scene["render"]["parallax"]["amount"], 0.25);
        assert_eq!(scene["camera"]["eye"]["z"], 1.0);

        let node = &scene["nodes"][0];
        assert_eq!(node["type"], "image");
        assert!(node.get("resource").is_none());
        assert_eq!(node["width"], 100.0);
        assert_eq!(node["height"], 50.0);
        assert_eq!(node["transform"]["x"], 10.0);
        assert_eq!(node["transform"]["y"], 20.0);
        assert_eq!(node["transform"]["scale_x"], 2.0);
        assert_eq!(node["transform"]["scale_y"], 3.0);
        assert_eq!(node["transform"]["rotation_deg"], 90.0);
        assert_eq!(node["transform"]["anchor_x"], 0.0);
        assert_eq!(node["transform"]["anchor_y"], 0.5);
        assert_eq!(
            node["provenance"]["source_format"],
            "wallpaper-engine-scene"
        );
        assert_eq!(node["provenance"]["source_id"], "7");
        assert_eq!(node["provenance"]["parent_id"], "3");
        assert_eq!(node["provenance"]["dependencies"][0], "1");
        assert_eq!(node["provenance"]["original_path"], "models/hero.json");
        assert_eq!(node["provenance"]["model"]["source"], "models/hero.json");
        assert_eq!(
            node["provenance"]["model"]["material"],
            "materials/hero.json"
        );
        assert_eq!(
            node["provenance"]["model"]["textures"][0],
            "materials/hero_albedo.tex"
        );
        assert_eq!(node["effects"][0]["file"], "effects/glow.json");
        assert_eq!(node["effects"][0]["resource"], "resource-3-glow");
        assert_eq!(node["effects"][0]["runtime"], "wallpaper-engine-effect");
        assert_eq!(node["effects"][0]["passes"][0]["combos"]["MODE"], 2);
        assert_eq!(node["audio"][0]["source"], "sounds/theme.ogg");
        assert_eq!(node["audio"][0]["resource"], "resource-4-theme");
        assert_eq!(node["audio"][0]["playback_mode"], "loop");
        assert_eq!(node["audio"][0]["volume"], 0.75);
        assert_eq!(node["audio"][0]["start_silent"], false);
        assert_eq!(node["provenance"]["particle"], "particles/spark.json");
        assert_eq!(scene["resources"][0]["type"], "model");
        assert_eq!(scene["resources"][1]["type"], "material");
        assert_eq!(scene["resources"][2]["type"], "effect");
        assert_eq!(scene["resources"][3]["type"], "audio");
        assert!(
            scene["resources"]
                .as_array()
                .unwrap()
                .iter()
                .all(|resource| !resource["source"].as_str().unwrap_or("").ends_with(".tex"))
        );
        assert!(
            scene["unsupported_features"]
                .as_array()
                .unwrap()
                .iter()
                .any(|feature| feature["feature"] == "we-model-material-texture-runtime")
        );
    }

    #[test]
    fn resolves_scene_model_material_image_texture_to_renderable_resource() {
        let source = TestDir::new("we-scene-renderable-model-source");
        let output = TestDir::new("we-scene-renderable-model-output");
        output.remove();
        source.write_file(
            "scene.json",
            r##"{
              "objects": [
                {
                  "id": 1,
                  "name": "Renderable",
                  "image": "models/renderable.json"
                }
              ]
            }"##,
        );
        source.write_file(
            "models/renderable.json",
            r#"{ "material": "materials/renderable.json" }"#,
        );
        source.write_file(
            "materials/renderable.json",
            r#"{ "passes": [{ "textures": ["textures/albedo.png"] }] }"#,
        );
        source.write_file("textures/albedo.png", "not real png");
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Renderable Scene Model",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(scene["nodes"][0]["type"], "image");
        assert_eq!(scene["nodes"][0]["resource"], "resource-3-albedo");
        assert_eq!(scene["resources"][2]["type"], "image");
        assert_eq!(
            scene["resources"][2]["source"],
            "assets/scene-resources/scene/resource-3-albedo.png"
        );
        assert_eq!(
            scene["nodes"][0]["provenance"]["model"]["texture_resources"][0],
            "resource-3-albedo"
        );
        assert_eq!(scene["systems"]["shader_material_graph"], "ready");
        assert!(
            scene["native_lowering"]["completed_boundaries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|boundary| boundary == "shader-material-graph")
        );
        assert!(
            !scene["native_lowering"]["pending_boundaries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|boundary| boundary == "shader-material-graph")
        );
        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .converted_features
                .contains(&"scene-we-material-graph-runtime".to_owned())
        );
    }

    #[test]
    fn preserves_noop_scene_effect_without_blocking_material_graph_runtime() {
        let source = TestDir::new("we-scene-noop-effect-source");
        let output = TestDir::new("we-scene-noop-effect-output");
        output.remove();
        source.write_file(
            "scene.json",
            r#"{
              "objects": [
                {
                  "id": 1,
                  "name": "Renderable With Noop Effect",
                  "image": "models/renderable.json",
                  "effects": [
                    { "file": "effects/noop.json", "visible": true, "passes": [] }
                  ]
                }
              ]
            }"#,
        );
        source.write_file(
            "models/renderable.json",
            r#"{ "material": "materials/renderable.json" }"#,
        );
        source.write_file(
            "materials/renderable.json",
            r#"{ "passes": [{ "textures": ["textures/albedo.png"] }] }"#,
        );
        source.write_file("textures/albedo.png", "not real png");
        source.write_file("effects/noop.json", r#"{ "passes": [] }"#);
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Noop Effect Scene Model",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(scene["nodes"][0]["type"], "image");
        assert_eq!(scene["nodes"][0]["effects"][0]["file"], "effects/noop.json");
        assert_eq!(scene["nodes"][0]["effects"][0]["runtime"], "metadata-only");
        assert!(
            !scene["resources"]
                .as_array()
                .unwrap()
                .iter()
                .any(|resource| resource["role"] == "we-effect")
        );
        assert!(
            !scene["unsupported_features"]
                .as_array()
                .unwrap()
                .iter()
                .any(|feature| feature["feature"] == "we-effect-runtime")
        );
        assert!(
            scene["native_lowering"]["completed_boundaries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|boundary| boundary == "shader-material-graph")
        );
        assert!(
            !scene["native_lowering"]["pending_boundaries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|boundary| boundary == "shader-material-graph")
        );
        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .converted_features
                .contains(&"scene-we-noop-effect-preserved".to_owned())
        );
    }

    #[test]
    fn keeps_runtime_scene_effect_as_material_graph_boundary() {
        let source = TestDir::new("we-scene-runtime-effect-source");
        let output = TestDir::new("we-scene-runtime-effect-output");
        output.remove();
        source.write_file(
            "scene.json",
            r#"{
              "objects": [
                {
                  "id": 1,
                  "name": "Renderable With Runtime Effect",
                  "image": "models/renderable.json",
                  "effects": [
                    { "file": "effects/glow.json", "visible": true }
                  ]
                }
              ]
            }"#,
        );
        source.write_file(
            "models/renderable.json",
            r#"{ "material": "materials/renderable.json" }"#,
        );
        source.write_file(
            "materials/renderable.json",
            r#"{ "passes": [{ "textures": ["textures/albedo.png"] }] }"#,
        );
        source.write_file("textures/albedo.png", "not real png");
        source.write_file(
            "effects/glow.json",
            r#"{ "passes": [{ "textures": ["_rt_FullFrameBuffer"], "combos": { "MODE": 1 } }] }"#,
        );
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Runtime Effect Scene Model",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        assert!(
            scene["unsupported_features"]
                .as_array()
                .unwrap()
                .iter()
                .any(|feature| feature["feature"] == "we-effect-runtime")
        );
        assert_eq!(scene["systems"]["shader_material_graph"], "detected");
        assert_eq!(
            scene["nodes"][0]["effects"][0]["runtime"],
            "wallpaper-engine-effect"
        );
        assert!(
            scene["native_lowering"]["pending_boundaries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|boundary| boundary == "shader-material-graph")
        );
        assert!(
            !scene["native_lowering"]["completed_boundaries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|boundary| boundary == "shader-material-graph")
        );
    }

    #[test]
    fn lowers_wallpaper_engine_opacity_effect_script_to_native_timeline() {
        let source = TestDir::new("we-scene-opacity-effect-source");
        let output = TestDir::new("we-scene-opacity-effect-output");
        output.remove();
        source.write_file(
            "scene.json",
            r##"{
              "objects": [
                {
                  "id": 1,
                  "type": "rectangle",
                  "name": "Fading Layer",
                  "width": 100,
                  "height": 100,
                  "color": "#ffffff",
                  "effects": [
                    {
                      "file": "effects/opacity/effect.json",
                      "passes": [
                        {
                          "constantshadervalues": {
                            "alpha": {
                              "script": "'use strict'; const delayTime = 3; const fadeTime = 2; export function update(value) { return value; }",
                              "value": 1
                            }
                          }
                        }
                      ]
                    }
                  ]
                }
              ]
            }"##,
        );
        source.write_file("effects/opacity/effect.json", r#"{ "passes": [] }"#);
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Opacity Effect Scene",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        let node_id = scene["nodes"][0]["id"].as_str().unwrap();
        assert_eq!(scene["nodes"][0]["type"], "rectangle");
        assert_eq!(
            scene["nodes"][0]["effects"][0]["runtime"],
            "native-opacity-timeline"
        );
        assert_eq!(scene["timelines"][0]["target_node"], node_id);
        assert_eq!(scene["timelines"][0]["channels"][0]["property"], "opacity");
        assert_eq!(
            scene["timelines"][0]["channels"][0]["keyframes"][0]["time_ms"],
            0
        );
        assert_eq!(
            scene["timelines"][0]["channels"][0]["keyframes"][1]["time_ms"],
            3000
        );
        assert_eq!(
            scene["timelines"][0]["channels"][0]["keyframes"][2]["time_ms"],
            5000
        );
        assert_eq!(
            scene["timelines"][0]["channels"][0]["keyframes"][2]["value"],
            0.0
        );
        assert!(
            !scene["unsupported_features"]
                .as_array()
                .unwrap()
                .iter()
                .any(|feature| feature["feature"] == "we-effect-runtime")
        );
        assert!(
            !scene["resources"]
                .as_array()
                .unwrap()
                .iter()
                .any(|resource| resource["role"] == "we-effect")
        );
        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .converted_features
                .contains(&"scene-we-opacity-effect-timeline".to_owned())
        );
        assert!(
            report
                .converted_features
                .contains(&"scene-keyframe-timeline".to_owned())
        );
    }

    #[test]
    fn lowers_wallpaper_engine_opacity_effect_alias_script_to_native_timeline() {
        let source = TestDir::new("we-scene-opacity-effect-alias-source");
        let output = TestDir::new("we-scene-opacity-effect-alias-output");
        output.remove();
        source.write_file(
            "scene.json",
            r##"{
              "objects": [
                {
                  "id": 1,
                  "type": "rectangle",
                  "name": "Alpha Range Layer",
                  "width": 100,
                  "height": 100,
                  "color": "#ffffff",
                  "effects": [
                    {
                      "file": "effects/opacity/effect.json",
                      "passes": [
                        {
                          "constant_shader_values": {
                            "alpha": {
                              "script": "let startDelay = 1; let fadeDuration = 2; let fromAlpha = 0.25; let targetAlpha = 0.85;",
                              "value": 0
                            }
                          }
                        }
                      ]
                    }
                  ]
                }
              ]
            }"##,
        );
        source.write_file("effects/opacity/effect.json", r#"{ "passes": [] }"#);
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Opacity Effect Alias Scene",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        let node_id = scene["nodes"][0]["id"].as_str().unwrap();
        assert_eq!(
            scene["nodes"][0]["effects"][0]["runtime"],
            "native-opacity-timeline"
        );
        assert_eq!(scene["timelines"][0]["target_node"], node_id);
        assert_eq!(
            scene["timelines"][0]["channels"][0]["keyframes"],
            json!([
                { "time_ms": 0, "value": 0.25, "curve": "linear" },
                { "time_ms": 1000, "value": 0.25, "curve": "linear" },
                { "time_ms": 3000, "value": 0.85, "curve": "linear" }
            ])
        );
        assert!(
            !scene["unsupported_features"]
                .as_array()
                .unwrap()
                .iter()
                .any(|feature| feature["feature"] == "we-effect-runtime")
        );
    }

    #[test]
    fn lowers_wallpaper_engine_constant_opacity_effect_to_native_timeline() {
        let source = TestDir::new("we-scene-constant-opacity-effect-source");
        let output = TestDir::new("we-scene-constant-opacity-effect-output");
        output.remove();
        source.write_file(
            "scene.json",
            r##"{
              "objects": [
                {
                  "id": 1,
                  "type": "rectangle",
                  "name": "Tinted Layer",
                  "width": 100,
                  "height": 100,
                  "color": "#ffffff",
                  "effects": [
                    {
                      "file": "effects/opacity/effect.json",
                      "visible": true,
                      "passes": [
                        {
                          "constant_shader_values": {
                            "alpha": 0.35
                          }
                        }
                      ]
                    }
                  ]
                }
              ]
            }"##,
        );
        source.write_file(
            "effects/opacity/effect.json",
            r#"{ "passes": [{ "material": "materials/effects/opacity.json" }] }"#,
        );
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Constant Opacity Effect Scene",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        let node_id = scene["nodes"][0]["id"].as_str().unwrap();
        assert_eq!(
            scene["nodes"][0]["effects"][0]["runtime"],
            "native-opacity-timeline"
        );
        assert_eq!(scene["timelines"][0]["target_node"], node_id);
        assert_eq!(
            scene["timelines"][0]["channels"][0]["keyframes"][0]["value"],
            0.35
        );
        assert!(
            !scene["unsupported_features"]
                .as_array()
                .unwrap()
                .iter()
                .any(|feature| feature["feature"] == "we-effect-runtime")
        );
        assert!(
            !scene["resources"]
                .as_array()
                .unwrap()
                .iter()
                .any(|resource| resource["role"] == "we-effect")
        );

        let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
        assert_eq!(snapshot.layers[0].opacity, 0.35);
    }

    #[test]
    fn decodes_wallpaper_engine_scene_tex_material_to_renderable_frame_resource() {
        let rgba = vec![
            255, 0, 0, 255, 0, 255, 0, 255, 1, 1, 1, 255, 2, 2, 2, 255, 0, 0, 255, 255, 255, 255,
            0, 255, 3, 3, 3, 255, 4, 4, 4, 255,
        ];
        let tex = test_we_tex_rgba(4, 2, &rgba);
        let decoded = tex::decode_we_tex_image(&tex).unwrap();
        assert_eq!(decoded.width, 4);
        assert_eq!(decoded.height, 2);
        assert_eq!(decoded.rgba, rgba);
        let (frame, frame_count) = scene_we_tex_first_frame(
            decoded,
            Some(SceneWeModelFrameSize {
                width: 2,
                height: 2,
            }),
        )
        .unwrap();
        assert_eq!(frame_count, 2);
        assert_eq!(frame.width, 2);
        assert_eq!(frame.height, 2);
        assert_eq!(
            frame.rgba,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
            ]
        );

        let source = TestDir::new("we-scene-tex-renderable-model-source");
        let output = TestDir::new("we-scene-tex-renderable-model-output");
        output.remove();
        source.write_file(
            "scene.json",
            r#"{
              "objects": [
                {
                  "id": 1,
                  "name": "Renderable Tex",
                  "image": "models/renderable.json"
                }
              ]
            }"#,
        );
        source.write_file(
            "models/renderable.json",
            r#"{ "material": "materials/renderable.json", "width": 2, "height": 2 }"#,
        );
        source.write_file(
            "materials/renderable.json",
            r#"{ "passes": [{ "textures": ["atlas"], "combos": { "SPRITESHEET": 1 } }] }"#,
        );
        source.write_bytes("materials/atlas.tex", &tex);
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Renderable Tex Scene Model",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(scene["nodes"][0]["type"], "image");
        assert_eq!(scene["nodes"][0]["resource"], "resource-3-atlas-atlas");
        assert_eq!(
            scene["nodes"][0]["properties"]["spritesheet"]["type"],
            "atlas-grid"
        );
        assert_eq!(
            scene["nodes"][0]["properties"]["spritesheet"]["atlas_width"],
            4
        );
        assert_eq!(
            scene["nodes"][0]["properties"]["spritesheet"]["frame_width"],
            2
        );
        assert_eq!(
            scene["nodes"][0]["properties"]["spritesheet"]["frame_count"],
            2
        );
        assert_eq!(scene["resources"][2]["type"], "image");
        assert_eq!(
            scene["resources"][2]["source"],
            "assets/scene-resources/scene/resource-3-atlas-atlas.gtex"
        );
        assert_eq!(
            scene["resources"][2]["role"],
            "we-material-texture-decoded-atlas"
        );
        assert_eq!(
            scene["nodes"][0]["provenance"]["model"]["texture_resources"][0],
            "resource-3-atlas-atlas"
        );
        assert!(
            output
                .path()
                .join("assets/scene-resources/scene/resource-3-atlas-atlas.gtex")
                .exists()
        );
        assert!(scene["unsupported_features"].as_array().unwrap().is_empty());
        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .converted_features
                .contains(&"scene-we-tex-bc7-gpu-texture".to_owned())
        );
        assert!(
            report
                .converted_features
                .contains(&"scene-we-spritesheet-atlas-runtime".to_owned())
        );
    }

    #[test]
    fn extracts_wallpaper_engine_scene_tex_video_material_to_native_video_layer() {
        let video_payload = b"\0\0\0\x20ftypisom\0\0\x02\0isomiso2avc1mp41\0\0\0\x08free";
        let tex = test_we_tex_video(3840, 2160, video_payload);
        let source = TestDir::new("we-scene-tex-video-model-source");
        let output = TestDir::new("we-scene-tex-video-model-output");
        output.remove();
        source.write_file(
            "scene.json",
            r#"{
              "objects": [
                {
                  "id": 1,
                  "name": "Video Tex",
                  "image": "models/video.json"
                }
              ]
            }"#,
        );
        source.write_file(
            "models/video.json",
            r#"{ "material": "materials/video.json", "width": 3840, "height": 2160 }"#,
        );
        source.write_file(
            "materials/video.json",
            r#"{ "passes": [{ "textures": ["clip"] }] }"#,
        );
        source.write_bytes("materials/clip.tex", &tex);
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Video Tex Scene Model",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(scene["nodes"][0]["type"], "video");
        assert_eq!(scene["nodes"][0]["resource"], "resource-3-clip-video");
        assert_eq!(scene["nodes"][0]["width"], 3840.0);
        assert_eq!(scene["nodes"][0]["height"], 2160.0);
        assert_eq!(scene["resources"].as_array().unwrap().len(), 3);
        assert_eq!(scene["resources"][2]["type"], "video");
        assert_eq!(
            scene["resources"][2]["source"],
            "assets/scene-resources/scene/resource-3-clip-video.mp4"
        );
        assert_eq!(scene["resources"][2]["role"], "we-material-video-texture");
        assert_eq!(
            scene["resources"][2]["original_source"],
            "materials/clip.tex"
        );
        assert!(
            scene["resources"]
                .as_array()
                .unwrap()
                .iter()
                .all(|resource| !resource["source"].as_str().unwrap_or("").ends_with(".tex"))
        );
        assert_eq!(
            scene["nodes"][0]["provenance"]["model"]["texture_resources"][0],
            "resource-3-clip-video"
        );
        assert_eq!(
            fs::read(
                output
                    .path()
                    .join("assets/scene-resources/scene/resource-3-clip-video.mp4")
            )
            .unwrap(),
            video_payload
        );
        assert!(scene["unsupported_features"].as_array().unwrap().is_empty());
        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .converted_features
                .contains(&"scene-we-tex-video-layer-runtime".to_owned())
        );
        let full_scene = report.full_scene.unwrap();
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"wallpaper-engine-tex-video-layer-runtime".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"initial-visible-video-scene-composition".to_owned())
        );
        assert!(
            full_scene
                .completed_boundaries
                .contains(&"vulkan-video-scene-layer-composition".to_owned())
        );
        assert!(
            !full_scene
                .pending_boundaries
                .contains(&"mixed-video-scene-composition".to_owned())
        );
    }

    #[test]
    fn keeps_script_controlled_hidden_video_switching_out_of_mixed_video_boundary() {
        let video_payload = b"\0\0\0\x20ftypisom\0\0\x02\0isomiso2avc1mp41\0\0\0\x08free";
        let active_tex = test_we_tex_video(1920, 1080, video_payload);
        let hidden_tex = test_we_tex_video(1920, 1080, video_payload);
        let source = TestDir::new("we-scene-script-video-switch-source");
        let output = TestDir::new("we-scene-script-video-switch-output");
        output.remove();
        source.write_file(
            "scene.json",
            r#"{
              "script": "export function update(value) { return value; }",
              "objects": [
                {
                  "id": 1,
                  "name": "Loop",
                  "image": "models/loop.json"
                },
                {
                  "id": 2,
                  "name": "Interaction",
                  "visible": false,
                  "image": "models/interaction.json"
                },
                {
                  "id": 3,
                  "name": "Interaction Controller",
                  "image": "models/util/composelayer.json",
                  "visible": {
                    "value": true,
                    "scriptproperties": {
                      "targetLayerId": "Interaction",
                      "defaultHideTarget": true,
                      "togglePlay": true
                    }
                  }
                }
              ]
            }"#,
        );
        source.write_file(
            "models/loop.json",
            r#"{ "material": "materials/loop.json", "width": 1920, "height": 1080 }"#,
        );
        source.write_file(
            "models/interaction.json",
            r#"{ "material": "materials/interaction.json", "width": 1920, "height": 1080 }"#,
        );
        source.write_file(
            "materials/loop.json",
            r#"{ "passes": [{ "textures": ["loop"] }] }"#,
        );
        source.write_file(
            "materials/interaction.json",
            r#"{ "passes": [{ "textures": ["interaction"] }] }"#,
        );
        source.write_bytes("materials/loop.tex", &active_tex);
        source.write_bytes("materials/interaction.tex", &hidden_tex);
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Script Video Switch Scene",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(scene["nodes"][0]["type"], "video");
        assert_eq!(scene["nodes"][1]["type"], "video");
        assert_eq!(scene["nodes"][1]["visible"], true);
        assert_eq!(scene["nodes"][1]["opacity"], 0.0);
        assert_eq!(scene["nodes"][2]["type"], "script");
        assert_eq!(
            scene["nodes"][2]["properties"]["controller"]["kind"],
            "click-video-switch"
        );
        assert_eq!(
            scene["nodes"][2]["properties"]["controller"]["target_node"],
            scene["nodes"][1]["id"]
        );
        let controller_node = scene["nodes"][2]["id"].as_str().unwrap();
        assert!(
            scene["nodes"][2]["properties"]["controller"]["input_aliases"]
                .as_array()
                .unwrap()
                .contains(&json!(format!(
                    "scene.input.controller.{controller_node}.active"
                )))
        );
        assert!(
            scene["nodes"][2]["properties"]["controller"]["input_aliases"]
                .as_array()
                .unwrap()
                .contains(&json!(format!(
                    "scene.input.controller.{}.active",
                    scene["nodes"][1]["id"].as_str().unwrap()
                )))
        );
        assert!(
            scene["property_bindings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|binding| {
                    binding["target_node"] == scene["nodes"][1]["id"]
                        && binding["target"] == "opacity"
                        && binding["property"]
                            .as_str()
                            .is_some_and(|property| property.starts_with("scene.controller."))
                })
        );
        assert!(
            scene["native_lowering"]["completed_boundaries"]
                .as_array()
                .unwrap()
                .contains(&json!("initial-visible-video-scene-composition"))
        );
        assert!(
            scene["native_lowering"]["completed_boundaries"]
                .as_array()
                .unwrap()
                .contains(&json!("script-controlled-video-layer-switching"))
        );
        assert!(
            !scene["native_lowering"]["pending_boundaries"]
                .as_array()
                .unwrap()
                .contains(&json!("mixed-video-scene-composition"))
        );
        assert!(
            !scene["native_lowering"]["pending_boundaries"]
                .as_array()
                .unwrap()
                .contains(&json!("script-controlled-video-layer-switching"))
        );
        assert!(
            scene["native_lowering"]["pending_boundaries"]
                .as_array()
                .unwrap()
                .contains(&json!("scene-controller-input-source"))
        );
        let controller_property = scene["property_bindings"][0]["property"]
            .as_str()
            .unwrap()
            .to_owned();
        let target_node = scene["nodes"][1]["id"].as_str().unwrap().to_owned();
        let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
        document.validate().unwrap();
        let inactive = document.snapshot_at_with_property_resolver(0, |_| None);
        assert_eq!(
            inactive
                .layers
                .iter()
                .find(|layer| layer.id == target_node)
                .unwrap()
                .opacity,
            0.0
        );
        let active = document.snapshot_at_with_property_resolver(0, |property| {
            (property == controller_property).then_some(1.0)
        });
        assert_eq!(
            active
                .layers
                .iter()
                .find(|layer| layer.id == target_node)
                .unwrap()
                .opacity,
            1.0
        );
    }

    #[test]
    fn lowers_wallpaper_engine_timed_visibility_script_to_target_timeline() {
        let source = TestDir::new("we-scene-timed-visibility-controller-source");
        let output = TestDir::new("we-scene-timed-visibility-controller-output");
        output.remove();
        source.write_file(
            "scene.json",
            r##"{
              "general": {
                "orthogonalprojection": { "width": 1920, "height": 1080 }
              },
              "objects": [
                {
                  "id": 48,
                  "name": "Cloud",
                  "image": "models/util/fullscreenlayer.json",
                  "visible": false,
                  "color": "#ffffff"
                },
                {
                  "id": 63,
                  "name": "Intro Cloud Controller",
                  "solid": true,
                  "visible": {
                    "value": true,
                    "scriptproperties": {
                      "targetLayerName": "Cloud",
                      "enableAutoControl": { "value": true },
                      "startDelay": "0",
                      "showDuration": "2",
                      "fadeDuration": 0.5,
                      "hideOnStart": true,
                      "loopControl": false,
                      "loopInterval": 1
                    }
                  }
                }
              ]
            }"##,
        );
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Timed Visibility Controller Scene",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        let nodes = scene["nodes"].as_array().unwrap();
        let target = nodes.iter().find(|node| node["name"] == "Cloud").unwrap();
        let controller = nodes
            .iter()
            .find(|node| node["name"] == "Intro Cloud Controller")
            .unwrap();
        let target_node = target["id"].as_str().unwrap().to_owned();
        assert_eq!(target["type"], "rectangle");
        assert_eq!(target["visible"], true);
        assert_eq!(target["opacity"], 0.0);
        assert_eq!(target["width"], 1920.0);
        assert_eq!(target["height"], 1080.0);
        assert_eq!(
            controller["properties"]["controller"]["kind"],
            "timed-visibility"
        );
        assert_eq!(
            controller["properties"]["controller"]["target_node"],
            target_node
        );
        assert!(
            !scene["property_bindings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|binding| binding["target_node"] == target_node)
        );
        let timeline = scene["timelines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|timeline| timeline["target_node"] == target_node)
            .unwrap();
        assert_eq!(timeline["channels"][0]["property"], "opacity");
        assert_eq!(timeline["channels"][0]["loop"], false);
        assert_eq!(
            timeline["channels"][0]["keyframes"],
            json!([
                { "time_ms": 0, "value": 0.0 },
                { "time_ms": 500, "value": 1.0 },
                { "time_ms": 2500, "value": 1.0 },
                { "time_ms": 3000, "value": 0.0 }
            ])
        );
        assert!(
            scene["native_lowering"]["completed_boundaries"]
                .as_array()
                .unwrap()
                .contains(&json!(
                    "wallpaper-engine-timed-visibility-controller-lowering"
                ))
        );
        assert!(
            scene["native_lowering"]["completed_boundaries"]
                .as_array()
                .unwrap()
                .contains(&json!("scene-controller-fade-ramp-runtime"))
        );
        assert!(
            scene["native_lowering"]["completed_boundaries"]
                .as_array()
                .unwrap()
                .contains(&json!("timeline-animation-runtime"))
        );
        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .converted_features
                .contains(&"scene-we-timed-visibility-controller".to_owned())
        );
        assert!(
            report
                .converted_features
                .contains(&"scene-keyframe-timeline".to_owned())
        );

        let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
        document.validate().unwrap();
        for (time_ms, expected_opacity) in [(0, 0.0), (250, 0.5), (500, 1.0), (3000, 0.0)] {
            let snapshot = document.snapshot_at_with_property_resolver(time_ms, |_| None);
            let layer = snapshot
                .layers
                .iter()
                .find(|layer| layer.id == target_node)
                .unwrap();
            assert!(
                (layer.opacity - expected_opacity).abs() < 0.001,
                "opacity at {time_ms}ms was {}, expected {expected_opacity}",
                layer.opacity
            );
        }
    }

    #[test]
    fn lowers_wallpaper_engine_parent_ids_to_gscene_children() {
        let source = TestDir::new("we-scene-parent-graph-source");
        let output = TestDir::new("we-scene-parent-graph-output");
        output.remove();
        source.write_file(
            "scene.json",
            r#"{
              "objects": [
                {
                  "id": 10,
                  "name": "Parent",
                  "type": "image",
                  "path": "parent.png",
                  "origin": [10, 20, 0],
                  "alpha": 0.5
                },
                {
                  "id": 20,
                  "parent": 10,
                  "name": "Child",
                  "type": "image",
                  "path": "child.png",
                  "origin": [3, 4, 0],
                  "alpha": 0.5
                }
              ]
            }"#,
        );
        source.write_file("parent.png", "not real png");
        source.write_file("child.png", "not real png");
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Parented Scene",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(scene["nodes"].as_array().unwrap().len(), 1);
        let parent = &scene["nodes"][0];
        assert_eq!(parent["provenance"]["source_id"], "10");
        assert_eq!(parent["children"].as_array().unwrap().len(), 1);
        assert_eq!(parent["children"][0]["provenance"]["source_id"], "20");
        assert_eq!(parent["children"][0]["provenance"]["parent_id"], "10");

        let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
        assert_eq!(snapshot.layers.len(), 2);
        assert_eq!(snapshot.layers[0].id, "node-1-image");
        assert_eq!(snapshot.layers[0].transform.x, 10.0);
        assert_eq!(snapshot.layers[0].transform.y, 20.0);
        assert_eq!(snapshot.layers[0].opacity, 0.5);
        assert_eq!(snapshot.layers[1].id, "node-2-image");
        assert_eq!(snapshot.layers[1].transform.x, 13.0);
        assert_eq!(snapshot.layers[1].transform.y, 24.0);
        assert_eq!(snapshot.layers[1].opacity, 0.25);
    }

    #[test]
    fn converts_wallpaper_engine_scene_text_and_visible_property_binding() {
        let source = TestDir::new("we-scene-text-binding-source");
        let output = TestDir::new("we-scene-text-binding-output");
        output.remove();
        source.write_file(
            "scene.json",
            r#"{
              "objects": [
                {
                  "id": 30,
                  "name": "Title",
                  "type": "text",
                  "text": { "value": "Hello Scene" },
                  "pointsize": { "value": 48 },
                  "font": { "value": "fonts/Inter.ttf" },
                  "horizontalalign": "center",
                  "color": [1, 1, 1],
                  "visible": { "value": false, "user": "show_title" }
                }
              ]
            }"#,
        );
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Text Binding Scene",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        let node = &scene["nodes"][0];
        assert_eq!(node["type"], "text");
        assert_eq!(node["text"], "Hello Scene");
        assert_eq!(node["font_size"], 48.0);
        assert_eq!(node["font_family"], "fonts/Inter.ttf");
        assert_eq!(node["text_align"], "middle");
        assert_eq!(node["visible"], true);
        assert_eq!(node["opacity"], 0.0);
        assert_eq!(scene["property_bindings"][0]["property"], "show_title");
        assert_eq!(scene["property_bindings"][0]["target_node"], node["id"]);
        assert_eq!(scene["property_bindings"][0]["target"], "opacity");

        let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
        document.validate().unwrap();
        let hidden = document.snapshot_at_with_property_resolver(0, |_| None);
        assert_eq!(hidden.layers[0].opacity, 0.0);
        let visible = document.snapshot_at_with_property_resolver(0, |property| {
            if property == "show_title" {
                Some(1.0)
            } else {
                None
            }
        });
        assert_eq!(visible.layers[0].kind, crate::core::SceneNodeKind::Text);
        assert_eq!(visible.layers[0].text.as_deref(), Some("Hello Scene"));
        assert_eq!(visible.layers[0].opacity, 1.0);
    }

    #[test]
    fn converts_wallpaper_engine_scene_shape_objects_to_native_nodes() {
        let source = TestDir::new("we-scene-shape-source");
        let output = TestDir::new("we-scene-shape-output");
        output.remove();
        source.write_file(
            "scene.json",
            r##"{
              "objects": [
                {
                  "id": 40,
                  "shape": "rectangle",
                  "solid": true,
                  "backgroundcolor": "0.2 0.4 0.6",
                  "size": [200, 100, 0],
                  "radius": { "value": 12 }
                },
                {
                  "id": 41,
                  "shape": { "value": "ellipse" },
                  "color": [1, 0, 0],
                  "size": [50, 60, 0]
                },
                {
                  "id": 42,
                  "d": "M0 0 L100 0 L100 100 L0 100 Z M25 25 L75 25 L75 75 L25 75 Z",
                  "fillRule": "evenodd",
                  "color": "#22aa88"
                }
              ]
            }"##,
        );
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Shape Scene",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        let nodes = scene["nodes"].as_array().unwrap();
        assert_eq!(nodes[0]["type"], "rectangle");
        assert_eq!(nodes[0]["color"], "#336699");
        assert_eq!(nodes[0]["width"], 200.0);
        assert_eq!(nodes[0]["height"], 100.0);
        assert_eq!(nodes[0]["corner_radius"], 12.0);
        assert_eq!(nodes[1]["type"], "ellipse");
        assert_eq!(nodes[1]["color"], "#ff0000");
        assert_eq!(nodes[1]["width"], 50.0);
        assert_eq!(nodes[1]["height"], 60.0);
        assert_eq!(nodes[2]["type"], "path");
        assert_eq!(
            nodes[2]["path"],
            "M0 0 L100 0 L100 100 L0 100 Z M25 25 L75 25 L75 75 L25 75 Z"
        );
        assert_eq!(nodes[2]["path_fill_rule"], "evenodd");
        assert_eq!(nodes[2]["color"], "#22aa88");

        let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
        assert_eq!(snapshot.layers.len(), 3);
        assert_eq!(
            snapshot.layers[0].kind,
            crate::core::SceneNodeKind::Rectangle
        );
        assert_eq!(snapshot.layers[0].color.as_deref(), Some("#336699"));
        assert_eq!(snapshot.layers[0].corner_radius, Some(12.0));
        assert_eq!(snapshot.layers[1].kind, crate::core::SceneNodeKind::Ellipse);
        assert_eq!(snapshot.layers[1].color.as_deref(), Some("#ff0000"));
        assert_eq!(snapshot.layers[2].kind, crate::core::SceneNodeKind::Path);
        assert_eq!(
            snapshot.layers[2].path_fill_rule,
            crate::core::ScenePathFillRule::Evenodd
        );
    }

    #[test]
    fn converts_wallpaper_engine_scene_wrapped_geometry_properties() {
        let source = TestDir::new("we-scene-wrapped-geometry-source");
        let output = TestDir::new("we-scene-wrapped-geometry-output");
        output.remove();
        source.write_file(
            "scene.json",
            r##"{
              "general": {
                "cameraparallaxamount": 10
              },
              "objects": [
                {
                  "id": 45,
                  "shape": "rectangle",
                  "backgroundcolor": { "script": "return [0.2, 0.4, 0.6];", "value": [0.2, 0.4, 0.6] },
                  "x": { "value": 10, "user": "panel_x" },
                  "y": { "value": 20, "user": "panel_y" },
                  "width": { "value": 100, "user": "panel_width" },
                  "height": { "value": 50, "user": "panel_height" },
                  "radius": { "value": 6, "user": "panel_radius" },
                  "parallax_depth": { "script": "return 0.5;", "value": 0.5 },
                  "alpha": { "value": 0.4, "user": "panel_alpha" }
                }
              ]
            }"##,
        );
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Wrapped Geometry Scene",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        let node = &scene["nodes"][0];
        assert_eq!(node["type"], "rectangle");
        assert_eq!(node["color"], "#336699");
        assert_eq!(node["transform"]["x"], 10.0);
        assert_eq!(node["transform"]["y"], 20.0);
        assert_eq!(node["width"], 100.0);
        assert_eq!(node["height"], 50.0);
        assert_eq!(node["corner_radius"], 6.0);
        assert_eq!(node["parallax_depth"], 0.5);
        assert_eq!(node["opacity"], 0.4);
        let bindings = scene["property_bindings"].as_array().unwrap();
        for (property, target) in [
            ("panel_x", "x"),
            ("panel_y", "y"),
            ("panel_width", "width"),
            ("panel_height", "height"),
            ("panel_radius", "corner-radius"),
            ("panel_alpha", "opacity"),
        ] {
            assert!(
                bindings.iter().any(|binding| {
                    binding["property"] == property && binding["target"] == target
                }),
                "missing property binding {property} -> {target}: {bindings:?}"
            );
        }

        let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(0, |property| match property {
            "panel_x" => Some(30.0),
            "panel_y" => Some(40.0),
            "panel_width" => Some(220.0),
            "panel_height" => Some(90.0),
            "panel_radius" => Some(18.0),
            "panel_alpha" => Some(0.75),
            "scene.parallax.x" => Some(2.0),
            "scene.parallax.y" => Some(-1.0),
            _ => None,
        });
        assert_eq!(snapshot.layers[0].transform.x, 40.0);
        assert_eq!(snapshot.layers[0].transform.y, 35.0);
        assert_eq!(snapshot.layers[0].width, Some(220.0));
        assert_eq!(snapshot.layers[0].height, Some(90.0));
        assert_eq!(snapshot.layers[0].corner_radius, Some(18.0));
        assert_eq!(snapshot.layers[0].parallax_depth, Some(0.5));
        assert_eq!(snapshot.layers[0].opacity, 0.75);
    }

    #[test]
    fn lowers_wallpaper_engine_scene_linear_scenescript_bindings_without_js_engine() {
        let source = TestDir::new("we-scene-linear-script-binding-source");
        let output = TestDir::new("we-scene-linear-script-binding-output");
        output.remove();
        source.write_file(
            "scene.json",
            r##"{
              "objects": [
                {
                  "id": 46,
                  "shape": "rectangle",
                  "backgroundcolor": "#203040",
                  "x": {
                    "value": 10,
                    "user": "panel_x",
                    "script": "return value + user * 2 + 5;"
                  },
                  "width": {
                    "value": 100,
                    "script": "return this.user.panel_width.value / 2 + value;"
                  },
                  "height": {
                    "value": 50,
                    "script": "return getUserProperty(\"panel_height\") * 0.25 + value;"
                  },
                  "alpha": {
                    "value": 0.2,
                    "user": "panel_alpha",
                    "script": "return Number(user) * 0.5 + 0.1;"
                  }
                }
              ]
            }"##,
        );
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Linear SceneScript Scene",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        let bindings = scene["property_bindings"].as_array().unwrap();
        for (property, target, scale, offset) in [
            ("panel_x", "x", 2.0, 15.0),
            ("panel_width", "width", 0.5, 100.0),
            ("panel_height", "height", 0.25, 50.0),
            ("panel_alpha", "opacity", 0.5, 0.1),
        ] {
            let binding = bindings
                .iter()
                .find(|binding| binding["property"] == property && binding["target"] == target)
                .unwrap_or_else(|| panic!("missing binding {property} -> {target}: {bindings:?}"));
            assert_eq!(binding["scale"], scale);
            assert_eq!(binding["offset"], offset);
        }

        let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(0, |property| match property {
            "panel_x" => Some(7.0),
            "panel_width" => Some(80.0),
            "panel_height" => Some(40.0),
            "panel_alpha" => Some(1.0),
            _ => None,
        });
        assert_eq!(snapshot.layers[0].transform.x, 29.0);
        assert_eq!(snapshot.layers[0].width, Some(140.0));
        assert_eq!(snapshot.layers[0].height, Some(60.0));
        assert_eq!(snapshot.layers[0].opacity, 0.6);

        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .converted_features
                .contains(&"scene-deterministic-scenescript-expression".to_owned())
        );
    }

    #[test]
    fn lowers_wallpaper_engine_embedded_property_keyframes_to_gscene_timelines() {
        let source = TestDir::new("we-scene-embedded-property-timeline-source");
        let output = TestDir::new("we-scene-embedded-property-timeline-output");
        output.remove();
        source.write_file(
            "scene.json",
            r##"{
              "objects": [
                {
                  "id": 81,
                  "shape": "rectangle",
                  "backgroundcolor": "#203040",
                  "size": "100 50",
                  "origin": {
                    "value": "0 0 0",
                    "easing": "linear",
                    "keyframes": [
                      { "time": 0, "value": "0 0 0" },
                      { "time": { "value": 1 }, "value": { "value": "100 40 0" } }
                    ]
                  },
                  "alpha": {
                    "value": 1,
                    "frames": [
                      [0, 1],
                      [0.5, { "value": 0.25 }],
                      { "time": 1, "value": { "value": false } }
                    ]
                  }
                }
              ]
            }"##,
        );
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Embedded Property Timeline Scene",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        let timelines = scene["timelines"].as_array().unwrap();
        assert_eq!(timelines.len(), 2);
        assert!(timelines.iter().any(|timeline| {
            let channels = timeline["channels"].as_array().unwrap();
            channels.iter().any(|channel| channel["property"] == "x")
                && channels.iter().any(|channel| channel["property"] == "y")
        }));
        assert!(timelines.iter().any(|timeline| {
            timeline["channels"]
                .as_array()
                .unwrap()
                .iter()
                .any(|channel| channel["property"] == "opacity")
        }));

        let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(500, |_| None);
        assert_eq!(snapshot.layers[0].transform.x, 50.0);
        assert_eq!(snapshot.layers[0].transform.y, 20.0);
        assert_eq!(snapshot.layers[0].opacity, 0.25);

        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .converted_features
                .contains(&"scene-we-embedded-property-timeline".to_owned())
        );
    }

    #[test]
    fn converts_wallpaper_engine_scene_explicit_keyframes_to_gscene_timelines() {
        let source = TestDir::new("we-scene-keyframe-source");
        let output = TestDir::new("we-scene-keyframe-output");
        output.remove();
        source.write_file(
            "scene.json",
            r##"{
              "objects": [
                {
                  "id": 50,
                  "shape": "rectangle",
                  "backgroundcolor": "#203040",
                  "size": [320, 180, 0],
                  "timeline": [
                    {
                      "property": "scale",
                      "keyframes": [
                        { "time_ms": 0, "value": [1, 1, 0] },
                        { "time_ms": 1000, "value": [2, 3, 0] }
                      ]
                    }
                  ]
                }
              ],
              "timelines": [
                {
                  "name": "panel-motion",
                  "target": 50,
                  "channels": [
                    {
                      "property": "origin",
                      "keyframes": [
                        { "time_ms": 0, "value": [0, 0, 0] },
                        { "time_ms": 1000, "value": [100, 50, 0] }
                      ]
                    },
                    {
                      "property": "alpha",
                      "keyframes": [
                        { "time_ms": 0, "value": 0.25 },
                        { "time_ms": 1000, "value": 0.75 }
                      ]
                    }
                  ]
                }
              ]
            }"##,
        );
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Keyframe Scene",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(scene["timelines"].as_array().unwrap().len(), 2);
        assert_eq!(
            scene["timelines"][0]["target_node"],
            scene["nodes"][0]["id"]
        );
        assert_eq!(scene["timelines"][0]["channels"][0]["property"], "scale-x");
        assert_eq!(scene["timelines"][0]["channels"][1]["property"], "scale-y");
        assert_eq!(scene["timelines"][1]["id"], "timeline-2-panel-motion");
        assert_eq!(
            scene["timelines"][1]["target_node"],
            scene["nodes"][0]["id"]
        );
        assert_eq!(scene["timelines"][1]["channels"][0]["property"], "x");
        assert_eq!(scene["timelines"][1]["channels"][1]["property"], "y");
        assert_eq!(scene["timelines"][1]["channels"][2]["property"], "opacity");

        let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(500, |_| None);
        assert_eq!(snapshot.layers.len(), 1);
        assert_eq!(
            snapshot.layers[0].kind,
            crate::core::SceneNodeKind::Rectangle
        );
        assert_eq!(snapshot.layers[0].transform.x, 50.0);
        assert_eq!(snapshot.layers[0].transform.y, 25.0);
        assert_eq!(snapshot.layers[0].transform.scale_x, 1.5);
        assert_eq!(snapshot.layers[0].transform.scale_y, 2.0);
        assert_eq!(snapshot.layers[0].opacity, 0.5);

        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .converted_features
                .contains(&"scene-keyframe-timeline".to_owned())
        );
    }

    #[test]
    fn lowers_wallpaper_engine_animation_layer_keyframes_to_gscene_timelines() {
        let source = TestDir::new("we-scene-animation-layer-source");
        let output = TestDir::new("we-scene-animation-layer-output");
        output.remove();
        source.write_file(
            "scene.json",
            r##"{
              "objects": [
                {
                  "id": 70,
                  "type": "rectangle",
                  "name": "Animated Panel",
                  "width": 100,
                  "height": 60,
                  "color": "#203040",
                  "animationlayers": [
                    {
                      "name": "slide",
                      "property": "origin",
                      "keyframes": [
                        { "time_ms": 0, "value": [0, 0, 0] },
                        { "time_ms": 1000, "value": [120, 40, 0] }
                      ]
                    }
                  ]
                }
              ]
            }"##,
        );
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "scene",
              "title": "Animation Layer Scene",
              "file": "scene.json"
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/scene.gscene.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(scene["timelines"].as_array().unwrap().len(), 1);
        assert_eq!(scene["timelines"][0]["id"], "timeline-1-slide");
        assert_eq!(scene["timelines"][0]["channels"][0]["property"], "x");
        assert_eq!(scene["timelines"][0]["channels"][1]["property"], "y");
        assert!(
            !scene["unsupported_features"]
                .as_array()
                .unwrap()
                .iter()
                .any(|feature| feature["feature"] == "we-animation-layer-blending")
        );

        let document: crate::core::SceneDocument = serde_json::from_value(scene).unwrap();
        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(500, |_| None);
        assert_eq!(snapshot.layers[0].transform.x, 60.0);
        assert_eq!(snapshot.layers[0].transform.y, 20.0);

        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .converted_features
                .contains(&"scene-we-animation-layer-timeline".to_owned())
        );
        assert!(
            !report
                .unsupported_features
                .contains(&"we-animation-layer-blending".to_owned())
        );
    }

    #[test]
    fn converts_playlist_project_with_image_and_video_items() {
        let source = TestDir::new("we-playlist-source");
        let output = TestDir::new("we-playlist-output");
        output.remove();
        source.write_file("day.jpg", "not real jpg");
        source.write_file("night.mp4", "not real mp4");
        source.write_file("preview.jpg", "not real preview");
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "playlist",
              "title": "Playlist Example",
              "preview": "preview.jpg",
              "items": [
                {
                  "title": "Day Image",
                  "type": "image",
                  "file": "day.jpg",
                  "weight": 3
                },
                {
                  "title": "Night Video",
                  "type": "video",
                  "file": "night.mp4"
                }
              ]
            }"#,
        );

        let summary = convert_project(source.path(), output.path()).unwrap();
        assert_eq!(summary.source_type, "playlist");
        let manifest: Value =
            serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
                .unwrap();
        assert_eq!(manifest["kind"], "playlist");
        assert_eq!(manifest["entry"]["type"], "playlist");
        assert_eq!(manifest["entry"]["selection"], "first-match");
        let items = manifest["entry"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["id"], "item-0-day-image");
        assert_eq!(items[0]["weight"], 3);
        assert_eq!(items[0]["entry"]["type"], "static-image");
        assert_eq!(items[0]["entry"]["source"], "assets/playlist-0.jpg");
        assert_eq!(items[1]["id"], "item-1-night-video");
        assert_eq!(items[1]["entry"]["type"], "video");
        assert_eq!(items[1]["entry"]["source"], "assets/playlist-1.mp4");
        assert_eq!(items[1]["entry"]["muted"], true);
        assert!(output.path().join("assets/playlist-0.jpg").exists());
        assert!(output.path().join("assets/playlist-1.mp4").exists());
        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(report.detected_features.contains(&"playlist".to_owned()));
        assert!(report.converted_features.contains(&"playlist".to_owned()));
        assert!(
            report
                .converted_features
                .contains(&"playlist-item:image".to_owned())
        );
        assert!(
            report
                .converted_features
                .contains(&"playlist-item:video".to_owned())
        );
    }

    #[test]
    fn converts_playlist_project_with_web_and_scene_items() {
        let source = TestDir::new("we-playlist-web-scene-source");
        let output = TestDir::new("we-playlist-web-scene-output");
        output.remove();
        source.write_file(
            "web/index.html",
            "<!doctype html><script src=\"app.js\"></script>",
        );
        source.write_file("web/app.js", "window.wallpaperPropertyListener = {};");
        source.write_file(
            "scene.json",
            r#"{"objects":[{"type":"image","path":"background.png"}]}"#,
        );
        source.write_file("background.png", "not real png");
        source.write_file("preview.jpg", "not real preview");
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "playlist",
              "title": "Mixed Playlist",
              "preview": "preview.jpg",
              "items": [
                {
                  "title": "Web Item",
                  "type": "web",
                  "file": "web/index.html"
                },
                {
                  "title": "Scene Item",
                  "type": "scene",
                  "file": "scene.json"
                }
              ]
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let manifest: Value =
            serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
                .unwrap();
        let items = manifest["entry"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["id"], "item-0-web-item");
        assert_eq!(items[0]["entry"]["type"], "web");
        assert_eq!(items[0]["entry"]["root"], "assets/playlist-0-web");
        assert_eq!(items[0]["entry"]["index"], "web/index.html");
        assert_eq!(items[0]["entry"]["fallback"], "previews/poster.jpg");
        assert_eq!(items[1]["id"], "item-1-scene-item");
        assert_eq!(items[1]["entry"]["type"], "scene");
        assert_eq!(
            items[1]["entry"]["source"],
            "assets/playlist-1-scene.gscene.json"
        );
        assert!(items[1]["entry"].get("max_fps").is_none());
        assert!(items[1]["entry"].get("fallback").is_none());
        assert!(
            output
                .path()
                .join("assets/playlist-0-web/web/index.html")
                .exists()
        );
        assert!(
            output
                .path()
                .join("assets/playlist-0-web/gilder-bridge.js")
                .exists()
        );
        assert!(
            output
                .path()
                .join("metadata/playlist-1-source-scene.json")
                .exists()
        );
        let scene: Value = serde_json::from_str(
            &fs::read_to_string(output.path().join("assets/playlist-1-scene.gscene.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(scene["nodes"][0]["type"], "image");
        assert_eq!(
            scene["resources"][0]["source"],
            "assets/scene-resources/playlist-1-scene/resource-1-background.png"
        );
        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .converted_features
                .contains(&"playlist-item:web".to_owned())
        );
        assert!(
            report
                .converted_features
                .contains(&"playlist-item:scene".to_owned())
        );
        assert!(
            report
                .unsupported_features
                .contains(&"web-runtime".to_owned())
        );
        assert!(
            report
                .unsupported_features
                .contains(&"scene-runtime".to_owned())
        );
    }

    #[test]
    fn converts_playlist_project_with_shader_item() {
        let source = TestDir::new("we-playlist-shader-source");
        let output = TestDir::new("we-playlist-shader-output");
        output.remove();
        source.write_file("waves.wgsl", "@fragment fn main() {}");
        source.write_file("preview.jpg", "not real preview");
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "playlist",
              "title": "Shader Playlist",
              "preview": "preview.jpg",
              "items": [
                {
                  "title": "Waves",
                  "type": "shader",
                  "file": "waves.wgsl"
                }
              ]
            }"#,
        );

        convert_project(source.path(), output.path()).unwrap();
        let manifest: Value =
            serde_json::from_str(&fs::read_to_string(output.path().join(MANIFEST_FILE)).unwrap())
                .unwrap();
        let items = manifest["entry"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["id"], "item-0-waves");
        assert_eq!(items[0]["entry"]["type"], "shader");
        assert_eq!(items[0]["entry"]["source"], "shaders/playlist-0.wgsl");
        assert_eq!(items[0]["entry"]["fallback"], "previews/poster.jpg");
        assert_eq!(items[0]["entry"]["language"], "wgsl");
        assert!(output.path().join("shaders/playlist-0.wgsl").exists());

        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .detected_features
                .contains(&"playlist-item:shader".to_owned())
        );
        assert!(
            report
                .converted_features
                .contains(&"playlist-item:shader".to_owned())
        );
        assert!(
            report
                .unsupported_features
                .contains(&"shader-runtime".to_owned())
        );
    }

    #[test]
    fn reports_unsupported_playlist_items_when_none_can_convert() {
        let source = TestDir::new("we-playlist-unsupported-source");
        let output = TestDir::new("we-playlist-unsupported-output");
        output.remove();
        source.write_file("app.exe", "");
        source.write_file(
            PROJECT_FILE,
            r#"{
              "type": "playlist",
              "title": "Executable Playlist",
              "items": [
                {
                  "title": "Executable Item",
                  "type": "application",
                  "file": "app.exe"
                }
              ]
            }"#,
        );

        let error = convert_project(source.path(), output.path()).unwrap_err();
        assert!(matches!(error, ConversionError::MissingEntry(_)));
        let report: ConversionReport = serde_json::from_str(
            &fs::read_to_string(output.path().join("metadata/conversion-report.json")).unwrap(),
        )
        .unwrap();
        assert!(
            report
                .unsupported_features
                .contains(&"playlist-item:application".to_owned())
        );
        assert!(
            report
                .errors
                .contains(&"Playlist did not contain convertible items.".to_owned())
        );
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

    #[test]
    fn scene_tex_encoder_writes_native_bc7_gtex_payload() {
        let output = TestDir::new("scene-bc7-gtex-output");
        let image = SceneWeTexImage {
            width: 4,
            height: 4,
            rgba: vec![0; tex::rgba_len(4, 4).unwrap()],
        };
        let path = output.path().join("transparent.gtex");

        gtex::write_bc7_gtex(&path, &image).unwrap();
        let bytes = fs::read(&path).unwrap();

        assert_eq!(&bytes[0..8], gtex::GILDER_SCENE_TEXTURE_MAGIC);
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 4);
        assert_eq!(u32::from_le_bytes(bytes[12..16].try_into().unwrap()), 4);
        assert_eq!(
            u32::from_le_bytes(bytes[16..20].try_into().unwrap()),
            gtex::GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK
        );
        assert_eq!(u64::from_le_bytes(bytes[24..32].try_into().unwrap()), 16);
        assert_eq!(bytes.len(), 48);
        assert_eq!(bytes[32], 0x40);
    }

    #[test]
    fn converts_png_to_native_bc7_gtex_offline() {
        let output = TestDir::new("png-bc7-gtex-output");
        let png_path = output.path().join("source.png");
        let gtex_path = output.path().join("source.gtex");
        let rgba = [
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        {
            let file = fs::File::create(&png_path).unwrap();
            let writer = std::io::BufWriter::new(file);
            let mut encoder = png::Encoder::new(writer, 2, 2);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&rgba).unwrap();
        }

        let summary = convert_png_to_native_gtex(&png_path, &gtex_path).unwrap();
        let bytes = fs::read(&gtex_path).unwrap();

        assert_eq!(summary.width, 2);
        assert_eq!(summary.height, 2);
        assert_eq!(summary.format, "BC7_UNORM_BLOCK");
        assert_eq!(summary.payload_bytes, 16);
        assert_eq!(&bytes[0..8], gtex::GILDER_SCENE_TEXTURE_MAGIC);
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(bytes[12..16].try_into().unwrap()), 2);
        assert_eq!(
            u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
            summary.payload_bytes
        );
        assert_eq!(bytes.len(), 48);
    }

    #[test]
    fn png_to_native_gtex_uses_bottom_first_texture_rows() {
        let top_left = [255, 0, 0, 255];
        let top_right = [0, 255, 0, 255];
        let bottom_left = [0, 0, 255, 255];
        let bottom_right = [255, 255, 255, 255];
        let mut rgba = [top_left, top_right, bottom_left, bottom_right].concat();

        gtex::flip_rgba_rows_vertically(&mut rgba, 2, 2).unwrap();

        assert_eq!(&rgba[0..4], &bottom_left);
        assert_eq!(&rgba[4..8], &bottom_right);
        assert_eq!(&rgba[8..12], &top_left);
        assert_eq!(&rgba[12..16], &top_right);
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

        fn write_bytes(&self, relative_path: &str, contents: &[u8]) {
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

    fn write_executable_script(path: &Path, contents: &str) {
        use std::io::Write;

        let temporary_path = path.with_extension("tmp");
        if let Some(parent) = temporary_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        {
            let mut file = fs::File::create(&temporary_path).unwrap();
            file.write_all(contents.as_bytes()).unwrap();
            file.sync_all().unwrap();
        }
        make_executable(&temporary_path);
        fs::rename(&temporary_path, path).unwrap();
    }

    fn test_we_tex_rgba(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
        assert_eq!(rgba.len(), tex::rgba_len(width, height).unwrap());
        let compressed = test_lz4_literal_block(rgba);
        let mut bytes = vec![0; 91];
        bytes[0..8].copy_from_slice(b"TEXV0005");
        bytes[9..17].copy_from_slice(b"TEXI0001");
        test_write_u32_le(&mut bytes, 22, 7);
        test_write_u32_le(&mut bytes, 26, width);
        test_write_u32_le(&mut bytes, 30, height);
        test_write_u32_le(&mut bytes, 34, width);
        test_write_u32_le(&mut bytes, 38, height);
        bytes[46..54].copy_from_slice(b"TEXB0004");
        test_write_u32_le(&mut bytes, 55, 1);
        test_write_u32_le(&mut bytes, 67, 1);
        test_write_u32_le(&mut bytes, 71, width);
        test_write_u32_le(&mut bytes, 75, height);
        test_write_u32_le(&mut bytes, 79, 1);
        test_write_u32_le(&mut bytes, 83, u32::try_from(rgba.len()).unwrap());
        test_write_u32_le(&mut bytes, 87, u32::try_from(compressed.len()).unwrap());
        bytes.extend_from_slice(&compressed);
        bytes
    }

    fn test_we_tex_video(width: u32, height: u32, payload: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0; 91];
        bytes[0..8].copy_from_slice(b"TEXV0005");
        bytes[9..17].copy_from_slice(b"TEXI0001");
        test_write_u32_le(&mut bytes, 22, 7);
        test_write_u32_le(&mut bytes, 26, width);
        test_write_u32_le(&mut bytes, 30, height);
        test_write_u32_le(&mut bytes, 34, width);
        test_write_u32_le(&mut bytes, 38, height);
        bytes[46..54].copy_from_slice(b"TEXB0004");
        test_write_u32_le(&mut bytes, 55, 1);
        test_write_u32_le(&mut bytes, 67, 1);
        test_write_u32_le(&mut bytes, 71, width);
        test_write_u32_le(&mut bytes, 75, height);
        test_write_u32_le(&mut bytes, 79, 1);
        test_write_u32_le(&mut bytes, 83, 0);
        test_write_u32_le(&mut bytes, 87, u32::try_from(payload.len()).unwrap());
        bytes.extend_from_slice(payload);
        bytes
    }

    fn test_scene_pkg(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let version = b"PKGV0023";
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(version.len() as u32).to_le_bytes());
        bytes.extend_from_slice(version);
        bytes.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        let mut payload = Vec::new();
        for (path, contents) in entries {
            bytes.extend_from_slice(&(path.len() as u32).to_le_bytes());
            bytes.extend_from_slice(path.as_bytes());
            bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            bytes.extend_from_slice(&(contents.len() as u32).to_le_bytes());
            payload.extend_from_slice(contents);
        }
        bytes.extend_from_slice(&payload);
        bytes
    }

    fn test_lz4_literal_block(bytes: &[u8]) -> Vec<u8> {
        let mut output = Vec::with_capacity(bytes.len() + 8);
        let literal_len = bytes.len();
        if literal_len < 15 {
            output.push((literal_len as u8) << 4);
        } else {
            output.push(0xf0);
            let mut remaining = literal_len - 15;
            while remaining >= 255 {
                output.push(255);
                remaining -= 255;
            }
            output.push(remaining as u8);
        }
        output.extend_from_slice(bytes);
        output
    }

    fn test_write_u32_le(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
}
