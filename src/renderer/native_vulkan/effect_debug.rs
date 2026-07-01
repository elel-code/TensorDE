use std::fmt;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

const GILDER_SCENE_TEXTURE_MAGIC: &[u8; 8] = b"GDTEX002";
const GILDER_SCENE_TEXTURE_HEADER_BYTES: usize = 32;
const GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK: u32 = 7;
const GILDER_SCENE_TEXTURE_FORMAT_R8_UNORM: u32 = 9;
const BC_BLOCK_TEXELS: u32 = 4;
const BC7_BLOCK_BYTES: usize = 16;
const BC7_MODE6_INDEX_WEIGHTS: [u16; 16] =
    [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];
const DEBUG_DARK_LUMA_THRESHOLD: f64 = 48.0;
const DEBUG_VISIBLE_ALPHA_THRESHOLD: f64 = 16.0;

pub(in crate::renderer::native_vulkan) struct NativeVulkanEffectDebugR8UvGroup<'a> {
    pub label: &'a str,
    pub sample_uvs: &'a [[f32; 2]],
    pub coverage_uvs: &'a [[f32; 2]],
}

pub(in crate::renderer::native_vulkan) struct NativeVulkanEffectDebugRgbaUvGroup<'a> {
    pub label: &'a str,
    pub sample_uvs: &'a [[f32; 2]],
    pub coverage_uvs: &'a [[f32; 2]],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_effect_debug_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("GILDER_NATIVE_VULKAN_EFFECT_DEBUG")
            .map(|value| {
                let value = value.trim().to_ascii_lowercase();
                !value.is_empty()
                    && value != "0"
                    && value != "false"
                    && value != "off"
                    && value != "no"
            })
            .unwrap_or(false)
    })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_effect_debug_log(
    scope: &str,
    args: fmt::Arguments<'_>,
) {
    if native_vulkan_effect_debug_enabled() {
        eprintln!("[gilder-effect-debug][{scope}] {args}");
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_effect_debug_log_limited(
    counter: &AtomicUsize,
    limit: usize,
    scope: &str,
    args: fmt::Arguments<'_>,
) {
    if !native_vulkan_effect_debug_enabled() {
        return;
    }
    let index = counter.fetch_add(1, Ordering::Relaxed);
    if index < limit {
        eprintln!("[gilder-effect-debug][{scope}] {args}");
    } else if index == limit {
        eprintln!("[gilder-effect-debug][{scope}] further logs suppressed after {limit} entries");
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_effect_debug_r8_gtex_report(
    path: &Path,
    sample_uvs: &[[f32; 2]],
) -> Result<String, String> {
    let texture = native_vulkan_effect_debug_read_r8_gtex(path)?;
    Ok(native_vulkan_effect_debug_r8_payload_report(
        texture.width,
        texture.height,
        texture.payload(),
        sample_uvs,
    ))
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_effect_debug_r8_gtex_group_report(
    path: &Path,
    groups: &[NativeVulkanEffectDebugR8UvGroup<'_>],
) -> Result<String, String> {
    let texture = native_vulkan_effect_debug_read_r8_gtex(path)?;
    Ok(native_vulkan_effect_debug_r8_payload_group_report(
        texture.width,
        texture.height,
        texture.payload(),
        groups,
    ))
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_effect_debug_bc7_mode6_gtex_group_report(
    path: &Path,
    groups: &[NativeVulkanEffectDebugRgbaUvGroup<'_>],
) -> Result<String, String> {
    let texture = native_vulkan_effect_debug_read_bc7_mode6_gtex_cached(path)?;
    Ok(native_vulkan_effect_debug_rgba_payload_group_report(
        texture.width,
        texture.height,
        texture.payload(),
        groups,
    ))
}

fn native_vulkan_effect_debug_read_r8_gtex(
    path: &Path,
) -> Result<NativeVulkanEffectDebugR8Texture, String> {
    let bytes =
        fs::read(path).map_err(|err| format!("read R8 debug gtex {}: {err}", path.display()))?;
    if bytes.len() < GILDER_SCENE_TEXTURE_HEADER_BYTES {
        return Err(format!("{} is shorter than a gtex header", path.display()));
    }
    if bytes.get(0..8) != Some(GILDER_SCENE_TEXTURE_MAGIC.as_slice()) {
        return Err(format!("{} is not a native gtex", path.display()));
    }
    let width = native_vulkan_effect_debug_read_u32(&bytes, 8)
        .ok_or_else(|| format!("{} has no width", path.display()))?;
    let height = native_vulkan_effect_debug_read_u32(&bytes, 12)
        .ok_or_else(|| format!("{} has no height", path.display()))?;
    let format = native_vulkan_effect_debug_read_u32(&bytes, 16)
        .ok_or_else(|| format!("{} has no format", path.display()))?;
    let payload_len = native_vulkan_effect_debug_read_u64(&bytes, 24)
        .ok_or_else(|| format!("{} has no payload length", path.display()))?;
    if format != GILDER_SCENE_TEXTURE_FORMAT_R8_UNORM {
        return Err(format!(
            "{} is gtex format {}, not R8_UNORM",
            path.display(),
            format
        ));
    }
    let expected_len = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| format!("{} R8 dimensions overflow", path.display()))?;
    if payload_len != expected_len {
        return Err(format!(
            "{} declares R8 payload {payload_len}, expected {expected_len}",
            path.display()
        ));
    }
    let payload_byte_len = bytes
        .len()
        .saturating_sub(GILDER_SCENE_TEXTURE_HEADER_BYTES);
    if payload_byte_len as u64 != payload_len {
        return Err(format!(
            "{} contains {} R8 payload bytes, expected {payload_len}",
            path.display(),
            payload_byte_len
        ));
    }
    Ok(NativeVulkanEffectDebugR8Texture {
        width,
        height,
        bytes,
    })
}

fn native_vulkan_effect_debug_read_bc7_mode6_gtex_cached(
    path: &Path,
) -> Result<Arc<NativeVulkanEffectDebugRgbaTexture>, String> {
    static CACHE: OnceLock<
        Mutex<std::collections::BTreeMap<PathBuf, Arc<NativeVulkanEffectDebugRgbaTexture>>>,
    > = OnceLock::new();
    let key = path.to_path_buf();
    let cache = CACHE.get_or_init(|| Mutex::new(std::collections::BTreeMap::new()));
    if let Ok(cache) = cache.lock()
        && let Some(texture) = cache.get(&key)
    {
        return Ok(Arc::clone(texture));
    }

    let texture = Arc::new(native_vulkan_effect_debug_read_bc7_mode6_gtex(path)?);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(key, Arc::clone(&texture));
    }
    Ok(texture)
}

fn native_vulkan_effect_debug_read_bc7_mode6_gtex(
    path: &Path,
) -> Result<NativeVulkanEffectDebugRgbaTexture, String> {
    let bytes =
        fs::read(path).map_err(|err| format!("read BC7 debug gtex {}: {err}", path.display()))?;
    if bytes.len() < GILDER_SCENE_TEXTURE_HEADER_BYTES {
        return Err(format!("{} is shorter than a gtex header", path.display()));
    }
    if bytes.get(0..8) != Some(GILDER_SCENE_TEXTURE_MAGIC.as_slice()) {
        return Err(format!("{} is not a native gtex", path.display()));
    }
    let width = native_vulkan_effect_debug_read_u32(&bytes, 8)
        .ok_or_else(|| format!("{} has no width", path.display()))?;
    let height = native_vulkan_effect_debug_read_u32(&bytes, 12)
        .ok_or_else(|| format!("{} has no height", path.display()))?;
    let format = native_vulkan_effect_debug_read_u32(&bytes, 16)
        .ok_or_else(|| format!("{} has no format", path.display()))?;
    let payload_len = native_vulkan_effect_debug_read_u64(&bytes, 24)
        .ok_or_else(|| format!("{} has no payload length", path.display()))?;
    if format != GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK {
        return Err(format!(
            "{} is gtex format {}, not BC7_UNORM_BLOCK",
            path.display(),
            format
        ));
    }
    let blocks_w = width.div_ceil(BC_BLOCK_TEXELS);
    let blocks_h = height.div_ceil(BC_BLOCK_TEXELS);
    let expected_len = u64::from(blocks_w)
        .checked_mul(u64::from(blocks_h))
        .and_then(|blocks| blocks.checked_mul(BC7_BLOCK_BYTES as u64))
        .ok_or_else(|| format!("{} BC7 dimensions overflow", path.display()))?;
    if payload_len != expected_len {
        return Err(format!(
            "{} declares BC7 payload {payload_len}, expected {expected_len}",
            path.display()
        ));
    }
    let payload_byte_len = bytes
        .len()
        .saturating_sub(GILDER_SCENE_TEXTURE_HEADER_BYTES);
    if payload_byte_len as u64 != payload_len {
        return Err(format!(
            "{} contains {} BC7 payload bytes, expected {payload_len}",
            path.display(),
            payload_byte_len
        ));
    }
    let rgba = native_vulkan_effect_debug_decode_bc7_mode6_payload(
        width,
        height,
        &bytes[GILDER_SCENE_TEXTURE_HEADER_BYTES..],
    )?;
    Ok(NativeVulkanEffectDebugRgbaTexture {
        width,
        height,
        rgba,
    })
}

fn native_vulkan_effect_debug_decode_bc7_mode6_payload(
    width: u32,
    height: u32,
    payload: &[u8],
) -> Result<Vec<u8>, String> {
    let pixel_count = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or_else(|| "BC7 debug pixel count overflowed".to_owned())?;
    let mut rgba = vec![
        0u8;
        pixel_count
            .checked_mul(4)
            .ok_or_else(|| "BC7 debug RGBA byte count overflowed".to_owned())?
    ];
    let blocks_w = width.div_ceil(BC_BLOCK_TEXELS);
    let blocks_h = height.div_ceil(BC_BLOCK_TEXELS);
    for block_y in 0..blocks_h {
        for block_x in 0..blocks_w {
            let block_index = usize::try_from(block_y)
                .ok()
                .and_then(|y| {
                    usize::try_from(blocks_w)
                        .ok()
                        .and_then(|stride| y.checked_mul(stride))
                })
                .and_then(|base| {
                    usize::try_from(block_x)
                        .ok()
                        .and_then(|x| base.checked_add(x))
                })
                .ok_or_else(|| "BC7 debug block index overflowed".to_owned())?;
            let offset = block_index
                .checked_mul(BC7_BLOCK_BYTES)
                .ok_or_else(|| "BC7 debug block byte offset overflowed".to_owned())?;
            let block: [u8; 16] = payload
                .get(offset..offset + BC7_BLOCK_BYTES)
                .ok_or_else(|| "BC7 debug block range exceeded payload".to_owned())?
                .try_into()
                .map_err(|_| "BC7 debug block size mismatch".to_owned())?;
            let pixels = native_vulkan_effect_debug_decode_bc7_mode6_block(&block).map_err(
                |err| {
                    format!(
                        "{err} at block {block_x},{block_y}; diagnostic currently decodes native converter BC7 mode 6 blocks"
                    )
                },
            )?;
            for y in 0..BC_BLOCK_TEXELS {
                for x in 0..BC_BLOCK_TEXELS {
                    let dst_x = block_x * BC_BLOCK_TEXELS + x;
                    let dst_y = block_y * BC_BLOCK_TEXELS + y;
                    if dst_x >= width || dst_y >= height {
                        continue;
                    }
                    let dst = usize::try_from(dst_y)
                        .ok()
                        .and_then(|row| {
                            usize::try_from(width)
                                .ok()
                                .and_then(|stride| row.checked_mul(stride))
                        })
                        .and_then(|base| {
                            usize::try_from(dst_x)
                                .ok()
                                .and_then(|x| base.checked_add(x))
                        })
                        .and_then(|pixel| pixel.checked_mul(4))
                        .ok_or_else(|| "BC7 debug destination offset overflowed".to_owned())?;
                    let src = usize::try_from(y * BC_BLOCK_TEXELS + x)
                        .map_err(|_| "BC7 debug source pixel index overflowed".to_owned())?;
                    rgba[dst..dst + 4].copy_from_slice(&pixels[src]);
                }
            }
        }
    }
    Ok(rgba)
}

fn native_vulkan_effect_debug_decode_bc7_mode6_block(
    block: &[u8; 16],
) -> Result<[[u8; 4]; 16], String> {
    let mut bit = 0usize;
    let mut mode = None;
    for candidate in 0..8 {
        let value = native_vulkan_effect_debug_bc7_get_bits(block, &mut bit, 1);
        if value == 1 {
            mode = Some(candidate);
            break;
        }
    }
    if mode != Some(6) {
        return Err(format!("unsupported BC7 mode {:?}", mode));
    }

    let mut endpoint_a = [0u8; 4];
    let mut endpoint_b = [0u8; 4];
    for channel in 0..4 {
        endpoint_a[channel] =
            (native_vulkan_effect_debug_bc7_get_bits(block, &mut bit, 7) as u8) << 1;
        endpoint_b[channel] =
            (native_vulkan_effect_debug_bc7_get_bits(block, &mut bit, 7) as u8) << 1;
    }
    let pbit_a = native_vulkan_effect_debug_bc7_get_bits(block, &mut bit, 1) as u8;
    let pbit_b = native_vulkan_effect_debug_bc7_get_bits(block, &mut bit, 1) as u8;
    for channel in 0..4 {
        endpoint_a[channel] |= pbit_a;
        endpoint_b[channel] |= pbit_b;
    }

    let palette = native_vulkan_effect_debug_bc7_mode6_palette(endpoint_a, endpoint_b);
    let mut pixels = [[0u8; 4]; 16];
    for (pixel_index, pixel) in pixels.iter_mut().enumerate() {
        let width = if pixel_index == 0 { 3 } else { 4 };
        let index = native_vulkan_effect_debug_bc7_get_bits(block, &mut bit, width) as usize;
        *pixel = palette[index.min(palette.len() - 1)];
    }
    Ok(pixels)
}

fn native_vulkan_effect_debug_bc7_mode6_palette(
    endpoint_a: [u8; 4],
    endpoint_b: [u8; 4],
) -> [[u8; 4]; 16] {
    let mut palette = [[0u8; 4]; 16];
    for (index, weight) in BC7_MODE6_INDEX_WEIGHTS.iter().copied().enumerate() {
        for channel in 0..4 {
            let a = u16::from(endpoint_a[channel]);
            let b = u16::from(endpoint_b[channel]);
            palette[index][channel] = (((64 - weight) * a + weight * b + 32) >> 6) as u8;
        }
    }
    palette
}

fn native_vulkan_effect_debug_bc7_get_bits(block: &[u8; 16], bit: &mut usize, width: usize) -> u32 {
    let mut value = 0u32;
    for offset in 0..width {
        let bit_index = *bit + offset;
        let byte = block[bit_index / 8];
        let mask = 1u8 << (bit_index % 8);
        if byte & mask != 0 {
            value |= 1u32 << offset;
        }
    }
    *bit += width;
    value
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_effect_debug_r8_payload_report(
    width: u32,
    height: u32,
    payload: &[u8],
    sample_uvs: &[[f32; 2]],
) -> String {
    let stats = native_vulkan_effect_debug_r8_payload_stats(width, height, payload);
    let samples = native_vulkan_effect_debug_r8_sample_label(width, height, payload, sample_uvs);
    let row_spans = native_vulkan_effect_debug_r8_probe_spans(width, height, payload, false);
    let column_spans = native_vulkan_effect_debug_r8_probe_spans(width, height, payload, true);
    format!(
        "{stats} samples={samples} row_spans_gt127={row_spans} column_spans_gt127={column_spans}"
    )
}

fn native_vulkan_effect_debug_r8_payload_group_report(
    width: u32,
    height: u32,
    payload: &[u8],
    groups: &[NativeVulkanEffectDebugR8UvGroup<'_>],
) -> String {
    let stats = native_vulkan_effect_debug_r8_payload_stats(width, height, payload);
    let row_spans = native_vulkan_effect_debug_r8_probe_spans(width, height, payload, false);
    let column_spans = native_vulkan_effect_debug_r8_probe_spans(width, height, payload, true);
    let mut report =
        format!("{stats} row_spans_gt127={row_spans} column_spans_gt127={column_spans}");
    for group in groups {
        report.push(' ');
        report.push_str(group.label);
        report.push_str("_samples=");
        report.push_str(&native_vulkan_effect_debug_r8_sample_label(
            width,
            height,
            payload,
            group.sample_uvs,
        ));
        report.push(' ');
        report.push_str(group.label);
        report.push_str("_coverage=");
        report.push_str(&native_vulkan_effect_debug_r8_coverage_label(
            width,
            height,
            payload,
            group.coverage_uvs,
        ));
    }
    report
}

fn native_vulkan_effect_debug_rgba_payload_group_report(
    width: u32,
    height: u32,
    rgba: &[u8],
    groups: &[NativeVulkanEffectDebugRgbaUvGroup<'_>],
) -> String {
    let mut report = native_vulkan_effect_debug_rgba_payload_stats(width, height, rgba);
    for group in groups {
        report.push(' ');
        report.push_str(group.label);
        report.push_str("_samples=");
        report.push_str(&native_vulkan_effect_debug_rgba_sample_label(
            width,
            height,
            rgba,
            group.sample_uvs,
        ));
        report.push(' ');
        report.push_str(group.label);
        report.push_str("_coverage=");
        report.push_str(&native_vulkan_effect_debug_rgba_coverage_label(
            width,
            height,
            rgba,
            group.coverage_uvs,
        ));
    }
    report
}

fn native_vulkan_effect_debug_rgba_payload_stats(width: u32, height: u32, rgba: &[u8]) -> String {
    let mut min_luma = f64::INFINITY;
    let mut max_luma = f64::NEG_INFINITY;
    let mut sum_luma = 0.0;
    let mut min_alpha = f64::INFINITY;
    let mut max_alpha = f64::NEG_INFINITY;
    let mut sum_alpha = 0.0;
    let mut visible_count = 0usize;
    let mut dark_visible_count = 0usize;
    let mut dark_bbox = NativeVulkanEffectDebugBbox::default();
    let width_usize = width as usize;
    let mut pixel_count = 0usize;
    for (pixel_index, pixel) in rgba.chunks_exact(4).enumerate() {
        let color = [
            f64::from(pixel[0]),
            f64::from(pixel[1]),
            f64::from(pixel[2]),
            f64::from(pixel[3]),
        ];
        let luma = native_vulkan_effect_debug_rgba_luma(color);
        let alpha = color[3];
        min_luma = min_luma.min(luma);
        max_luma = max_luma.max(luma);
        sum_luma += luma;
        min_alpha = min_alpha.min(alpha);
        max_alpha = max_alpha.max(alpha);
        sum_alpha += alpha;
        if alpha > DEBUG_VISIBLE_ALPHA_THRESHOLD {
            visible_count += 1;
            if luma < DEBUG_DARK_LUMA_THRESHOLD {
                dark_visible_count += 1;
                let x = (pixel_index % width_usize) as u32;
                let y = (pixel_index / width_usize) as u32;
                dark_bbox.include(x, y);
            }
        }
        pixel_count += 1;
    }
    let count = pixel_count.max(1);
    format!(
        "extent={}x{} pixels={} luma_min={:.1} luma_max={:.1} luma_mean={:.1} alpha_min={:.1} alpha_max={:.1} alpha_mean={:.1} visible_alpha_gt{:.0}={}/{} dark_visible_luma_lt{:.0}={}/{} dark_bbox={}",
        width,
        height,
        pixel_count,
        min_luma,
        max_luma,
        sum_luma / count as f64,
        min_alpha,
        max_alpha,
        sum_alpha / count as f64,
        DEBUG_VISIBLE_ALPHA_THRESHOLD,
        visible_count,
        pixel_count,
        DEBUG_DARK_LUMA_THRESHOLD,
        dark_visible_count,
        pixel_count,
        dark_bbox.label(),
    )
}

fn native_vulkan_effect_debug_rgba_sample_label(
    width: u32,
    height: u32,
    rgba: &[u8],
    sample_uvs: &[[f32; 2]],
) -> String {
    let mut label = String::new();
    label.push('[');
    let default_uvs = [
        [0.0, 0.0],
        [0.25, 0.25],
        [0.5, 0.5],
        [0.75, 0.75],
        [1.0, 1.0],
    ];
    let uvs = if sample_uvs.is_empty() {
        &default_uvs[..]
    } else {
        sample_uvs
    };
    for (index, uv) in uvs.iter().enumerate() {
        if index > 0 {
            label.push_str(", ");
        }
        let color = native_vulkan_effect_debug_sample_rgba_linear(width, height, rgba, *uv);
        let luma = native_vulkan_effect_debug_rgba_luma(color);
        label.push_str(&format!(
            "({:.3},{:.3})->rgba({:.0},{:.0},{:.0},{:.0})/luma={:.1}/dark={}",
            uv[0],
            uv[1],
            color[0],
            color[1],
            color[2],
            color[3],
            luma,
            native_vulkan_effect_debug_rgba_is_visible_dark(color)
        ));
    }
    label.push(']');
    label
}

fn native_vulkan_effect_debug_rgba_coverage_label(
    width: u32,
    height: u32,
    rgba: &[u8],
    sample_uvs: &[[f32; 2]],
) -> String {
    if sample_uvs.is_empty() {
        return "n=0".to_owned();
    }
    let mut min_luma = f64::INFINITY;
    let mut max_luma = f64::NEG_INFINITY;
    let mut sum_luma = 0.0;
    let mut min_alpha = f64::INFINITY;
    let mut max_alpha = f64::NEG_INFINITY;
    let mut sum_alpha = 0.0;
    let mut visible_count = 0usize;
    let mut dark_visible_count = 0usize;
    let mut outside_count = 0usize;
    for uv in sample_uvs {
        if uv[0] < 0.0 || uv[0] > 1.0 || uv[1] < 0.0 || uv[1] > 1.0 {
            outside_count += 1;
        }
        let color = native_vulkan_effect_debug_sample_rgba_linear(width, height, rgba, *uv);
        let luma = native_vulkan_effect_debug_rgba_luma(color);
        let alpha = color[3];
        min_luma = min_luma.min(luma);
        max_luma = max_luma.max(luma);
        sum_luma += luma;
        min_alpha = min_alpha.min(alpha);
        max_alpha = max_alpha.max(alpha);
        sum_alpha += alpha;
        if alpha > DEBUG_VISIBLE_ALPHA_THRESHOLD {
            visible_count += 1;
            if luma < DEBUG_DARK_LUMA_THRESHOLD {
                dark_visible_count += 1;
            }
        }
    }
    let count = sample_uvs.len();
    format!(
        "n={} outside={} luma_min={:.1} luma_max={:.1} luma_mean={:.1} alpha_min={:.1} alpha_max={:.1} alpha_mean={:.1} visible_alpha_gt{:.0}={}/{} dark_visible_luma_lt{:.0}={}/{}",
        count,
        outside_count,
        min_luma,
        max_luma,
        sum_luma / count as f64,
        min_alpha,
        max_alpha,
        sum_alpha / count as f64,
        DEBUG_VISIBLE_ALPHA_THRESHOLD,
        visible_count,
        count,
        DEBUG_DARK_LUMA_THRESHOLD,
        dark_visible_count,
        count
    )
}

fn native_vulkan_effect_debug_r8_payload_stats(width: u32, height: u32, payload: &[u8]) -> String {
    let mut min_value = u8::MAX;
    let mut max_value = u8::MIN;
    let mut sum = 0u64;
    let mut zero_count = 0usize;
    let mut full_count = 0usize;
    let mut nonzero_bbox = NativeVulkanEffectDebugBbox::default();
    let mut solid_bbox = NativeVulkanEffectDebugBbox::default();
    let width_usize = width as usize;
    for (index, value) in payload.iter().copied().enumerate() {
        min_value = min_value.min(value);
        max_value = max_value.max(value);
        sum += u64::from(value);
        if value == 0 {
            zero_count += 1;
        }
        if value == 255 {
            full_count += 1;
        }
        let x = (index % width_usize) as u32;
        let y = (index / width_usize) as u32;
        if value > 0 {
            nonzero_bbox.include(x, y);
        }
        if value > 127 {
            solid_bbox.include(x, y);
        }
    }
    let len = payload.len().max(1);
    let mean = sum as f64 / len as f64;
    let nonzero_count = payload.len().saturating_sub(zero_count);
    let solid_count = payload.iter().filter(|value| **value > 127).count();
    format!(
        "extent={}x{} bytes={} min={} max={} mean={:.2} zero={}/{} nonzero={}/{} gt127={}/{} full={}/{} bbox_gt0={} bbox_gt127={}",
        width,
        height,
        payload.len(),
        min_value,
        max_value,
        mean,
        zero_count,
        payload.len(),
        nonzero_count,
        payload.len(),
        solid_count,
        payload.len(),
        full_count,
        payload.len(),
        nonzero_bbox.label(),
        solid_bbox.label(),
    )
}

fn native_vulkan_effect_debug_r8_coverage_label(
    width: u32,
    height: u32,
    payload: &[u8],
    sample_uvs: &[[f32; 2]],
) -> String {
    if sample_uvs.is_empty() {
        return "n=0".to_owned();
    }
    let mut min_value = f64::INFINITY;
    let mut max_value = f64::NEG_INFINITY;
    let mut sum = 0.0;
    let mut gt0_count = 0usize;
    let mut gt127_count = 0usize;
    let mut full_count = 0usize;
    let mut outside_count = 0usize;
    for uv in sample_uvs {
        if uv[0] < 0.0 || uv[0] > 1.0 || uv[1] < 0.0 || uv[1] > 1.0 {
            outside_count += 1;
        }
        let value = native_vulkan_effect_debug_sample_r8_linear(width, height, payload, *uv);
        min_value = min_value.min(value);
        max_value = max_value.max(value);
        sum += value;
        if value > 0.0 {
            gt0_count += 1;
        }
        if value > 127.0 {
            gt127_count += 1;
        }
        if value >= 254.5 {
            full_count += 1;
        }
    }
    let count = sample_uvs.len();
    let mean = sum / count as f64;
    format!(
        "n={} outside={} min={:.1} max={:.1} mean={:.1} gt0={}/{} gt127={}/{} full={}/{}",
        count,
        outside_count,
        min_value,
        max_value,
        mean,
        gt0_count,
        count,
        gt127_count,
        count,
        full_count,
        count
    )
}

fn native_vulkan_effect_debug_r8_sample_label(
    width: u32,
    height: u32,
    payload: &[u8],
    sample_uvs: &[[f32; 2]],
) -> String {
    let mut label = String::new();
    label.push('[');
    let default_uvs = [
        [0.0, 0.0],
        [0.25, 0.25],
        [0.5, 0.5],
        [0.75, 0.75],
        [1.0, 1.0],
    ];
    let uvs = if sample_uvs.is_empty() {
        &default_uvs[..]
    } else {
        sample_uvs
    };
    for (index, uv) in uvs.iter().enumerate() {
        if index > 0 {
            label.push_str(", ");
        }
        let value = native_vulkan_effect_debug_sample_r8_linear(width, height, payload, *uv);
        label.push_str(&format!("({:.3},{:.3})->{:.1}", uv[0], uv[1], value));
    }
    label.push(']');
    label
}

fn native_vulkan_effect_debug_r8_probe_spans(
    width: u32,
    height: u32,
    payload: &[u8],
    columns: bool,
) -> String {
    let mut label = String::new();
    label.push('[');
    let probes = [0.0_f32, 0.25, 0.5, 0.75, 1.0];
    for (index, fraction) in probes.iter().copied().enumerate() {
        if index > 0 {
            label.push_str(", ");
        }
        let line_index = if columns {
            native_vulkan_effect_debug_fraction_index(width, fraction)
        } else {
            native_vulkan_effect_debug_fraction_index(height, fraction)
        };
        let span = if columns {
            native_vulkan_effect_debug_r8_axis_span(width, height, payload, line_index, true)
        } else {
            native_vulkan_effect_debug_r8_axis_span(width, height, payload, line_index, false)
        };
        label.push_str(&format!("{:.2}@{}:{}", fraction, line_index, span));
    }
    label.push(']');
    label
}

fn native_vulkan_effect_debug_r8_axis_span(
    width: u32,
    height: u32,
    payload: &[u8],
    line_index: u32,
    column: bool,
) -> String {
    let limit = if column { height } else { width };
    let mut first = None;
    let mut last = None;
    let mut count = 0u32;
    for offset in 0..limit {
        let (x, y) = if column {
            (line_index, offset)
        } else {
            (offset, line_index)
        };
        let value = native_vulkan_effect_debug_r8_at(width, height, payload, x, y);
        if value > 127 {
            first.get_or_insert(offset);
            last = Some(offset);
            count += 1;
        }
    }
    match (first, last) {
        (Some(first), Some(last)) => format!("{first}..{last}/{}", count),
        _ => "none".to_owned(),
    }
}

fn native_vulkan_effect_debug_sample_r8_linear(
    width: u32,
    height: u32,
    payload: &[u8],
    uv: [f32; 2],
) -> f64 {
    if width == 0 || height == 0 {
        return 0.0;
    }
    let u = uv[0].clamp(0.0, 1.0) as f64;
    let v = uv[1].clamp(0.0, 1.0) as f64;
    let x = u * f64::from(width.saturating_sub(1));
    let y = v * f64::from(height.saturating_sub(1));
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = x0.saturating_add(1).min(width.saturating_sub(1));
    let y1 = y0.saturating_add(1).min(height.saturating_sub(1));
    let tx = x - f64::from(x0);
    let ty = y - f64::from(y0);
    let v00 = f64::from(native_vulkan_effect_debug_r8_at(
        width, height, payload, x0, y0,
    ));
    let v10 = f64::from(native_vulkan_effect_debug_r8_at(
        width, height, payload, x1, y0,
    ));
    let v01 = f64::from(native_vulkan_effect_debug_r8_at(
        width, height, payload, x0, y1,
    ));
    let v11 = f64::from(native_vulkan_effect_debug_r8_at(
        width, height, payload, x1, y1,
    ));
    let top = v00 * (1.0 - tx) + v10 * tx;
    let bottom = v01 * (1.0 - tx) + v11 * tx;
    top * (1.0 - ty) + bottom * ty
}

fn native_vulkan_effect_debug_sample_rgba_linear(
    width: u32,
    height: u32,
    rgba: &[u8],
    uv: [f32; 2],
) -> [f64; 4] {
    if width == 0 || height == 0 {
        return [0.0; 4];
    }
    let u = uv[0].clamp(0.0, 1.0) as f64;
    let v = uv[1].clamp(0.0, 1.0) as f64;
    let x = u * f64::from(width.saturating_sub(1));
    let y = v * f64::from(height.saturating_sub(1));
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = x0.saturating_add(1).min(width.saturating_sub(1));
    let y1 = y0.saturating_add(1).min(height.saturating_sub(1));
    let tx = x - f64::from(x0);
    let ty = y - f64::from(y0);
    let c00 = native_vulkan_effect_debug_rgba_at(width, height, rgba, x0, y0);
    let c10 = native_vulkan_effect_debug_rgba_at(width, height, rgba, x1, y0);
    let c01 = native_vulkan_effect_debug_rgba_at(width, height, rgba, x0, y1);
    let c11 = native_vulkan_effect_debug_rgba_at(width, height, rgba, x1, y1);
    let mut color = [0.0; 4];
    for channel in 0..4 {
        let top = c00[channel] * (1.0 - tx) + c10[channel] * tx;
        let bottom = c01[channel] * (1.0 - tx) + c11[channel] * tx;
        color[channel] = top * (1.0 - ty) + bottom * ty;
    }
    color
}

fn native_vulkan_effect_debug_rgba_at(
    width: u32,
    height: u32,
    rgba: &[u8],
    x: u32,
    y: u32,
) -> [f64; 4] {
    if width == 0 || height == 0 {
        return [0.0; 4];
    }
    let x = x.min(width - 1);
    let y = y.min(height - 1);
    let Some(offset) = usize::try_from(y)
        .ok()
        .and_then(|y| {
            usize::try_from(width)
                .ok()
                .and_then(|width| y.checked_mul(width))
        })
        .and_then(|base| usize::try_from(x).ok().and_then(|x| base.checked_add(x)))
        .and_then(|pixel| pixel.checked_mul(4))
    else {
        return [0.0; 4];
    };
    [
        f64::from(rgba.get(offset).copied().unwrap_or(0)),
        f64::from(rgba.get(offset + 1).copied().unwrap_or(0)),
        f64::from(rgba.get(offset + 2).copied().unwrap_or(0)),
        f64::from(rgba.get(offset + 3).copied().unwrap_or(0)),
    ]
}

fn native_vulkan_effect_debug_rgba_luma(color: [f64; 4]) -> f64 {
    color[0] * 0.2126 + color[1] * 0.7152 + color[2] * 0.0722
}

fn native_vulkan_effect_debug_rgba_is_visible_dark(color: [f64; 4]) -> bool {
    color[3] > DEBUG_VISIBLE_ALPHA_THRESHOLD
        && native_vulkan_effect_debug_rgba_luma(color) < DEBUG_DARK_LUMA_THRESHOLD
}

fn native_vulkan_effect_debug_r8_at(width: u32, height: u32, payload: &[u8], x: u32, y: u32) -> u8 {
    if width == 0 || height == 0 {
        return 0;
    }
    let x = x.min(width - 1);
    let y = y.min(height - 1);
    let Some(offset) = usize::try_from(y)
        .ok()
        .and_then(|y| {
            usize::try_from(width)
                .ok()
                .and_then(|width| y.checked_mul(width))
        })
        .and_then(|base| usize::try_from(x).ok().and_then(|x| base.checked_add(x)))
    else {
        return 0;
    };
    payload.get(offset).copied().unwrap_or(0)
}

fn native_vulkan_effect_debug_fraction_index(limit: u32, fraction: f32) -> u32 {
    if limit == 0 {
        return 0;
    }
    ((limit - 1) as f32 * fraction.clamp(0.0, 1.0)).round() as u32
}

fn native_vulkan_effect_debug_read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn native_vulkan_effect_debug_read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

#[derive(Debug, Clone, Copy)]
struct NativeVulkanEffectDebugBbox {
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
    initialized: bool,
}

struct NativeVulkanEffectDebugR8Texture {
    width: u32,
    height: u32,
    bytes: Vec<u8>,
}

impl NativeVulkanEffectDebugR8Texture {
    fn payload(&self) -> &[u8] {
        &self.bytes[GILDER_SCENE_TEXTURE_HEADER_BYTES..]
    }
}

struct NativeVulkanEffectDebugRgbaTexture {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl NativeVulkanEffectDebugRgbaTexture {
    fn payload(&self) -> &[u8] {
        &self.rgba
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r8_group_report_includes_sample_and_coverage_labels() {
        let payload = [255, 0, 64, 128, 192, 255];
        let samples = [[0.0, 0.0], [1.0, 1.0]];
        let coverage = [[0.0, 0.0], [0.5, 0.5], [1.0, 1.0]];
        let group = NativeVulkanEffectDebugR8UvGroup {
            label: "current",
            sample_uvs: &samples,
            coverage_uvs: &coverage,
        };

        let report = native_vulkan_effect_debug_r8_payload_group_report(3, 2, &payload, &[group]);

        assert!(report.contains("current_samples=["));
        assert!(report.contains("current_coverage=n=3"));
        assert!(report.contains("gt127="));
    }
}

impl Default for NativeVulkanEffectDebugBbox {
    fn default() -> Self {
        Self {
            min_x: u32::MAX,
            min_y: u32::MAX,
            max_x: 0,
            max_y: 0,
            initialized: false,
        }
    }
}

impl NativeVulkanEffectDebugBbox {
    fn include(&mut self, x: u32, y: u32) {
        if !self.initialized {
            self.min_x = x;
            self.min_y = y;
            self.max_x = x;
            self.max_y = y;
            self.initialized = true;
            return;
        }
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }

    fn label(self) -> String {
        if !self.initialized {
            return "none".to_owned();
        }
        format!(
            "{}..{}x{}..{}",
            self.min_x, self.max_x, self.min_y, self.max_y
        )
    }
}
