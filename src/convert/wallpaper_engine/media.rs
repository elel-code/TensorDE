use super::*;

pub(super) fn copy_preview_or_generate(
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

pub(super) enum MissingPreviewFallback<'a> {
    None,
    StaticImage { source: &'a str },
    Video { source: &'a str },
    Scene { source: &'a str },
    Shader { source: &'a str },
}

#[derive(Debug, Clone, Copy)]
pub(super) struct StaticImageVariantTools<'a> {
    pub(super) ffmpeg: &'a Path,
    pub(super) ffprobe: &'a Path,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ImageDimensions {
    pub(super) width: u32,
    pub(super) height: u32,
}

impl ImageDimensions {
    fn can_generate(self, spec: StaticImageVariantSpec) -> bool {
        self.width >= spec.width
            && self.height >= spec.height
            && (self.width > spec.width || self.height > spec.height)
    }
}

pub(super) fn generate_static_image_variants(
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

pub(super) fn probe_static_image_dimensions_for_manifest(
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

pub(super) fn generate_static_image_variants_with_tools(
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

pub(super) fn generate_video_first_frame_preview_with_ffmpeg(
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

pub(super) fn find_executable_in_path_list(
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
