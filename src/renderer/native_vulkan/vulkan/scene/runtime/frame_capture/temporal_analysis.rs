//! In-process temporal artifact analysis for deterministic frame captures.

use std::path::{Path, PathBuf};

use image::GenericImageView as _;
use serde::Serialize;

use super::{SceneFrameCapturePixels, scene_frame_capture_output_path};

const CHANGE_THRESHOLD: u8 = 6;
const REFERENCE_EDGE_THRESHOLD: u16 = 24;
const SPARK_RESIDUAL_THRESHOLD: u8 = 12;
const SPARK_EXCESS_MOTION_THRESHOLD: u8 = 6;
const COHERENT_SWEEP_EDGE_RESIDUAL_THRESHOLD: f64 = 1.0;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneFrameTemporalAnalysisSnapshot {
    pub model: &'static str,
    pub frame_count: u64,
    pub adjacent_pair_count: u64,
    pub change_threshold: u8,
    pub adjacent_mean_abs_rgb: f64,
    pub adjacent_p95_abs_rgb: u8,
    pub adjacent_changed_pixel_ratio: f64,
    pub horizontal_motion: NativeVulkanSceneFrameHorizontalMotionSnapshot,
    pub reference: Option<NativeVulkanSceneFrameTemporalReferenceSnapshot>,
    pub verdict: &'static str,
    pub reasons: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneFrameHorizontalMotionSnapshot {
    pub strongest_window_pair_count: u64,
    pub slope_pixels_per_pair: f64,
    pub travel_pixels: f64,
    pub r_squared: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneFrameTemporalReferenceSnapshot {
    pub path: PathBuf,
    pub frame_rmse_rgb: f64,
    pub edge_sharpness_ratio: f64,
    pub temporal_residual_mean_abs_rgb: f64,
    pub edge_temporal_residual_mean_abs_rgb: f64,
    pub edge_spark_pixel_ratio: f64,
    pub edge_pixel_ratio: f64,
}

pub(super) fn analyze_scene_frame_sequence(
    frames: &[SceneFrameCapturePixels],
    width: u32,
    height: u32,
    reference_path: Option<&Path>,
    multiple_frames: bool,
) -> Result<Option<NativeVulkanSceneFrameTemporalAnalysisSnapshot>, String> {
    if frames.len() < 3 {
        return Ok(None);
    }
    validate_frames(frames, width, height)?;
    let adjacent = adjacent_motion(frames, width, height);
    let mut reasons = Vec::new();
    let reference = reference_path
        .map(|path| reference_analysis(frames, width, height, path, multiple_frames))
        .transpose()?;
    if let Some((reference, horizontal)) = &reference {
        if reference.edge_sharpness_ratio < 0.92 {
            reasons.push("edge-sharpness-regression");
        }
        if reference.edge_temporal_residual_mean_abs_rgb > 2.0 {
            reasons.push("edge-temporal-residual");
        }
        if reference.edge_spark_pixel_ratio > 0.005 {
            reasons.push("edge-spark-pixels");
        }
        if reference.edge_temporal_residual_mean_abs_rgb > COHERENT_SWEEP_EDGE_RESIDUAL_THRESHOLD
            && horizontal.r_squared >= 0.75
            && horizontal.travel_pixels.abs() >= f64::from(width) * 0.08
        {
            reasons.push("coherent-horizontal-sweep");
        }
    }
    let verdict = if reference.is_none() {
        "measurement-only"
    } else if reasons.is_empty() {
        "pass"
    } else {
        "fail"
    };
    Ok(Some(NativeVulkanSceneFrameTemporalAnalysisSnapshot {
        model: "deterministic-adjacent-rgb-reference-v1",
        frame_count: frames.len() as u64,
        adjacent_pair_count: frames.len().saturating_sub(1) as u64,
        change_threshold: CHANGE_THRESHOLD,
        adjacent_mean_abs_rgb: adjacent.mean,
        adjacent_p95_abs_rgb: adjacent.p95,
        adjacent_changed_pixel_ratio: adjacent.changed_ratio,
        horizontal_motion: reference
            .as_ref()
            .map(|(_, horizontal)| horizontal.clone())
            .unwrap_or(adjacent.horizontal_motion),
        reference: reference.map(|(reference, _)| reference),
        verdict,
        reasons,
    }))
}

struct AdjacentMotion {
    mean: f64,
    p95: u8,
    changed_ratio: f64,
    horizontal_motion: NativeVulkanSceneFrameHorizontalMotionSnapshot,
}

fn validate_frames(
    frames: &[SceneFrameCapturePixels],
    width: u32,
    height: u32,
) -> Result<(), String> {
    let expected = width as usize * height as usize * 4;
    for frame in frames {
        if frame.rgba.len() != expected {
            return Err(format!(
                "scene temporal analysis frame {} has {} bytes, expected {expected}",
                frame.frame_number,
                frame.rgba.len()
            ));
        }
    }
    Ok(())
}

fn adjacent_motion(frames: &[SceneFrameCapturePixels], width: u32, height: u32) -> AdjacentMotion {
    let mut histogram = [0u64; 256];
    let mut total = 0u64;
    let mut changed = 0u64;
    let mut samples = 0u64;
    let mut centroids = Vec::with_capacity(frames.len() - 1);
    for pair in frames.windows(2) {
        let mut columns = vec![0u64; width as usize];
        for pixel in 0..width as usize * height as usize {
            let offset = pixel * 4;
            let difference = rgb_abs_difference(&pair[0].rgba, &pair[1].rgba, offset);
            histogram[difference as usize] += 1;
            total += u64::from(difference);
            changed += u64::from(difference >= CHANGE_THRESHOLD);
            samples += 1;
            columns[pixel % width as usize] += u64::from(difference);
        }
        centroids.push(column_centroid(&columns, height));
    }
    AdjacentMotion {
        mean: ratio(total, samples),
        p95: histogram_percentile(&histogram, samples, 95),
        changed_ratio: ratio(changed, samples),
        horizontal_motion: strongest_monotonic_window(&centroids),
    }
}

fn reference_analysis(
    frames: &[SceneFrameCapturePixels],
    width: u32,
    height: u32,
    reference_path: &Path,
    multiple_frames: bool,
) -> Result<
    (
        NativeVulkanSceneFrameTemporalReferenceSnapshot,
        NativeVulkanSceneFrameHorizontalMotionSnapshot,
    ),
    String,
> {
    let references = frames
        .iter()
        .map(|frame| {
            load_reference_frame(
                reference_path,
                frame.frame_number,
                multiple_frames,
                width,
                height,
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    let pixel_count = width as usize * height as usize;
    let mut squared_error = 0u128;
    let mut rgb_samples = 0u64;
    let mut edge_pixels = 0u64;
    let mut candidate_gradient = 0u64;
    let mut reference_gradient = 0u64;
    for (frame, reference) in frames.iter().zip(&references) {
        for (candidate, expected) in frame.rgba.chunks_exact(4).zip(reference.chunks_exact(4)) {
            for channel in 0..3 {
                let difference = i32::from(candidate[channel]) - i32::from(expected[channel]);
                squared_error += (difference * difference) as u128;
                rgb_samples += 1;
            }
        }
        for y in 1..height.saturating_sub(1) {
            for x in 1..width.saturating_sub(1) {
                let expected_gradient = luma_gradient(reference, width, x, y);
                if expected_gradient < REFERENCE_EDGE_THRESHOLD {
                    continue;
                }
                edge_pixels += 1;
                reference_gradient += u64::from(expected_gradient);
                candidate_gradient += u64::from(luma_gradient(&frame.rgba, width, x, y));
            }
        }
    }

    let mut temporal_residual = 0u64;
    let mut temporal_samples = 0u64;
    let mut edge_temporal_residual = 0u64;
    let mut edge_temporal_samples = 0u64;
    let mut edge_sparks = 0u64;
    let mut residual_centroids = Vec::with_capacity(frames.len() - 1);
    for pair_index in 0..frames.len() - 1 {
        let candidate_previous = &frames[pair_index].rgba;
        let candidate_current = &frames[pair_index + 1].rgba;
        let reference_previous = &references[pair_index];
        let reference_current = &references[pair_index + 1];
        let mut columns = vec![0u64; width as usize];
        for pixel in 0..pixel_count {
            let offset = pixel * 4;
            let candidate_motion =
                rgb_abs_difference(candidate_previous, candidate_current, offset);
            let reference_motion =
                rgb_abs_difference(reference_previous, reference_current, offset);
            let residual = rgb_temporal_residual(
                candidate_previous,
                candidate_current,
                reference_previous,
                reference_current,
                offset,
            );
            temporal_residual += u64::from(residual);
            temporal_samples += 1;
            columns[pixel % width as usize] += u64::from(residual);
            let x = pixel as u32 % width;
            let y = pixel as u32 / width;
            let edge = x > 0
                && y > 0
                && x + 1 < width
                && y + 1 < height
                && (luma_gradient(reference_previous, width, x, y) >= REFERENCE_EDGE_THRESHOLD
                    || luma_gradient(reference_current, width, x, y) >= REFERENCE_EDGE_THRESHOLD);
            if edge {
                edge_temporal_residual += u64::from(residual);
                edge_temporal_samples += 1;
                edge_sparks += u64::from(
                    residual >= SPARK_RESIDUAL_THRESHOLD
                        && candidate_motion
                            >= reference_motion.saturating_add(SPARK_EXCESS_MOTION_THRESHOLD),
                );
            }
        }
        residual_centroids.push(column_centroid(&columns, height));
    }
    let horizontal_motion = strongest_monotonic_window(&residual_centroids);
    let rmse = if rgb_samples == 0 {
        0.0
    } else {
        (squared_error as f64 / rgb_samples as f64).sqrt()
    };
    let sharpness = if reference_gradient == 0 {
        1.0
    } else {
        candidate_gradient as f64 / reference_gradient as f64
    };
    let mut snapshot = NativeVulkanSceneFrameTemporalReferenceSnapshot {
        path: reference_path.to_path_buf(),
        frame_rmse_rgb: rmse,
        edge_sharpness_ratio: sharpness,
        temporal_residual_mean_abs_rgb: ratio(temporal_residual, temporal_samples),
        edge_temporal_residual_mean_abs_rgb: ratio(edge_temporal_residual, edge_temporal_samples),
        edge_spark_pixel_ratio: ratio(edge_sparks, edge_temporal_samples),
        edge_pixel_ratio: ratio(edge_pixels, pixel_count as u64 * frames.len() as u64),
    };
    normalize_reference_snapshot(&mut snapshot);
    Ok((snapshot, horizontal_motion))
}

fn normalize_reference_snapshot(snapshot: &mut NativeVulkanSceneFrameTemporalReferenceSnapshot) {
    for value in [
        &mut snapshot.frame_rmse_rgb,
        &mut snapshot.edge_sharpness_ratio,
        &mut snapshot.temporal_residual_mean_abs_rgb,
        &mut snapshot.edge_temporal_residual_mean_abs_rgb,
        &mut snapshot.edge_spark_pixel_ratio,
        &mut snapshot.edge_pixel_ratio,
    ] {
        if !value.is_finite() {
            *value = 0.0;
        }
    }
}

fn load_reference_frame(
    path: &Path,
    frame_number: u64,
    multiple_frames: bool,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    let path = scene_frame_capture_output_path(path, frame_number, multiple_frames);
    let image = image::open(&path)
        .map_err(|err| format!("open scene temporal reference {}: {err}", path.display()))?;
    if image.dimensions() != (width, height) {
        return Err(format!(
            "scene temporal reference {} is {}x{}, expected {width}x{height}",
            path.display(),
            image.width(),
            image.height()
        ));
    }
    Ok(image.to_rgba8().into_raw())
}

fn rgb_abs_difference(previous: &[u8], current: &[u8], offset: usize) -> u8 {
    let total = (0..3)
        .map(|channel| previous[offset + channel].abs_diff(current[offset + channel]) as u16)
        .sum::<u16>();
    (total / 3) as u8
}

fn rgb_temporal_residual(
    candidate_previous: &[u8],
    candidate_current: &[u8],
    reference_previous: &[u8],
    reference_current: &[u8],
    offset: usize,
) -> u8 {
    let total = (0..3)
        .map(|channel| {
            let candidate = i16::from(candidate_current[offset + channel])
                - i16::from(candidate_previous[offset + channel]);
            let reference = i16::from(reference_current[offset + channel])
                - i16::from(reference_previous[offset + channel]);
            candidate.abs_diff(reference)
        })
        .sum::<u16>();
    (total / 3).min(u16::from(u8::MAX)) as u8
}

fn luma_gradient(rgba: &[u8], width: u32, x: u32, y: u32) -> u16 {
    let left = luma(rgba, width, x - 1, y);
    let right = luma(rgba, width, x + 1, y);
    let top = luma(rgba, width, x, y - 1);
    let bottom = luma(rgba, width, x, y + 1);
    left.abs_diff(right) + top.abs_diff(bottom)
}

fn luma(rgba: &[u8], width: u32, x: u32, y: u32) -> u16 {
    let offset = (y as usize * width as usize + x as usize) * 4;
    (54 * u16::from(rgba[offset])
        + 183 * u16::from(rgba[offset + 1])
        + 19 * u16::from(rgba[offset + 2]))
        / 256
}

fn column_centroid(columns: &[u64], height: u32) -> f64 {
    let means = columns
        .iter()
        .map(|value| *value as f64 / f64::from(height.max(1)))
        .collect::<Vec<_>>();
    let baseline = means.iter().sum::<f64>() / means.len().max(1) as f64;
    let mut weight = 0.0;
    let mut weighted_position = 0.0;
    for (x, value) in means.into_iter().enumerate() {
        let value = (value - baseline).max(0.0);
        weight += value;
        weighted_position += value * x as f64;
    }
    if weight > f64::EPSILON {
        weighted_position / weight
    } else {
        0.0
    }
}

fn strongest_monotonic_window(positions: &[f64]) -> NativeVulkanSceneFrameHorizontalMotionSnapshot {
    if positions.len() < 3 {
        return NativeVulkanSceneFrameHorizontalMotionSnapshot {
            strongest_window_pair_count: positions.len() as u64,
            slope_pixels_per_pair: 0.0,
            travel_pixels: 0.0,
            r_squared: 0.0,
        };
    }
    let window = positions.len().min(12);
    let mut best = None::<(f64, NativeVulkanSceneFrameHorizontalMotionSnapshot)>;
    for sample in positions.windows(window) {
        let mean_x = (window - 1) as f64 * 0.5;
        let mean_y = sample.iter().sum::<f64>() / window as f64;
        let mut covariance = 0.0;
        let mut x_variance = 0.0;
        let mut y_variance = 0.0;
        for (index, value) in sample.iter().copied().enumerate() {
            let x = index as f64 - mean_x;
            let y = value - mean_y;
            covariance += x * y;
            x_variance += x * x;
            y_variance += y * y;
        }
        let slope = if x_variance > 0.0 {
            covariance / x_variance
        } else {
            0.0
        };
        let r_squared = if x_variance > 0.0 && y_variance > 0.0 {
            (covariance * covariance / (x_variance * y_variance)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let travel = sample[window - 1] - sample[0];
        let snapshot = NativeVulkanSceneFrameHorizontalMotionSnapshot {
            strongest_window_pair_count: window as u64,
            slope_pixels_per_pair: slope,
            travel_pixels: travel,
            r_squared,
        };
        let score = travel.abs() * r_squared;
        if best
            .as_ref()
            .is_none_or(|(best_score, _)| score > *best_score)
        {
            best = Some((score, snapshot));
        }
    }
    best.expect("at least one temporal window").1
}

fn histogram_percentile(histogram: &[u64; 256], samples: u64, percentile: u64) -> u8 {
    let target = samples.saturating_mul(percentile).div_ceil(100);
    let mut cumulative = 0u64;
    for (value, count) in histogram.iter().copied().enumerate() {
        cumulative = cumulative.saturating_add(count);
        if cumulative >= target {
            return value as u8;
        }
    }
    u8::MAX
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::renderer::native_vulkan::vulkan::scene::runtime::frame_capture::write_scene_frame_png;

    static NEXT_TEST: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn identical_deterministic_reference_passes() {
        let serial = NEXT_TEST.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "gilder-temporal-reference-{}-{serial}.png",
            std::process::id()
        ));
        let frames = synthetic_frames(false);
        write_reference_frames(&path, &frames);
        let analysis = analyze_scene_frame_sequence(&frames, 8, 6, Some(&path), true)
            .unwrap()
            .unwrap();
        assert_eq!(analysis.verdict, "pass");
        assert!(analysis.reasons.is_empty());
        let reference = analysis.reference.unwrap();
        assert_eq!(reference.frame_rmse_rgb, 0.0);
        assert_eq!(reference.temporal_residual_mean_abs_rgb, 0.0);
        remove_reference_frames(&path, &frames);
    }

    #[test]
    fn alternating_edge_sparks_fail_reference_gate() {
        let serial = NEXT_TEST.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "gilder-temporal-spark-reference-{}-{serial}.png",
            std::process::id()
        ));
        let reference_frames = synthetic_frames(false);
        let candidate_frames = synthetic_frames(true);
        write_reference_frames(&path, &reference_frames);
        let analysis = analyze_scene_frame_sequence(&candidate_frames, 8, 6, Some(&path), true)
            .unwrap()
            .unwrap();
        assert_eq!(analysis.verdict, "fail");
        assert!(analysis.reasons.contains(&"edge-temporal-residual"));
        remove_reference_frames(&path, &reference_frames);
    }

    fn synthetic_frames(sparks: bool) -> Vec<SceneFrameCapturePixels> {
        (1..=4)
            .map(|frame_number| {
                let mut rgba = vec![0u8; 8 * 6 * 4];
                for y in 0..6 {
                    for x in 0..8 {
                        let offset = (y * 8 + x) * 4;
                        let value = if x >= 4 { 220 } else { 20 };
                        rgba[offset..offset + 3].fill(value);
                        rgba[offset + 3] = 255;
                    }
                }
                if sparks {
                    let x = if frame_number % 2 == 0 { 3 } else { 4 };
                    let offset = (2 * 8 + x) * 4;
                    rgba[offset..offset + 3].fill(255);
                }
                SceneFrameCapturePixels {
                    frame_number,
                    scene_time_seconds: (frame_number - 1) as f32 / 60.0,
                    rgba,
                }
            })
            .collect()
    }

    fn write_reference_frames(path: &Path, frames: &[SceneFrameCapturePixels]) {
        for frame in frames {
            let frame_path = scene_frame_capture_output_path(path, frame.frame_number, true);
            write_scene_frame_png(&frame_path, 8, 6, &frame.rgba).unwrap();
        }
    }

    fn remove_reference_frames(path: &Path, frames: &[SceneFrameCapturePixels]) {
        for frame in frames {
            let frame_path = scene_frame_capture_output_path(path, frame.frame_number, true);
            std::fs::remove_file(frame_path).unwrap();
        }
    }
}
