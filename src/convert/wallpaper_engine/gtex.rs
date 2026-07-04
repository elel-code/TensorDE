use super::tex::{self, SceneWeTexImage};
use std::fs;
use std::io::{BufReader, Cursor, Write};
use std::path::Path;

pub(super) const GILDER_SCENE_TEXTURE_MAGIC: &[u8; 8] = b"GDTEX002";
pub(super) const GILDER_SCENE_TEXTURE_FORMAT_BC1_RGBA_UNORM_BLOCK: u32 = 1;
pub(super) const GILDER_SCENE_TEXTURE_FORMAT_BC3_UNORM_BLOCK: u32 = 3;
pub(super) const GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK: u32 = 7;
pub(super) const GILDER_SCENE_TEXTURE_FORMAT_R8_UNORM: u32 = 9;
pub(super) const GILDER_SCENE_TEXTURE_FORMAT_R8G8B8A8_UNORM: u32 = 37;

const BC_BLOCK_TEXELS: u32 = 4;
const BC1_BLOCK_BYTES: usize = 8;
const BC3_BLOCK_BYTES: usize = 16;
const BC7_BLOCK_BYTES: usize = 16;

pub(super) fn read_png_as_rgba(path: &Path) -> Result<SceneWeTexImage, String> {
    let file = fs::File::open(path).map_err(|err| format!("failed to open PNG: {err}"))?;
    let decoder = png::Decoder::new(BufReader::new(file));
    read_png_decoder_as_rgba(decoder)
}

pub(super) fn read_png_bytes_as_rgba(bytes: &[u8]) -> Result<SceneWeTexImage, String> {
    let decoder = png::Decoder::new(Cursor::new(bytes));
    read_png_decoder_as_rgba(decoder)
}

fn read_png_decoder_as_rgba<R: std::io::BufRead + std::io::Seek>(
    mut decoder: png::Decoder<R>,
) -> Result<SceneWeTexImage, String> {
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|err| format!("failed to read PNG metadata: {err}"))?;
    let output_size = reader
        .output_buffer_size()
        .ok_or_else(|| "PNG output buffer size overflowed".to_owned())?;
    let mut bytes = vec![0u8; output_size];
    let info = reader
        .next_frame(&mut bytes)
        .map_err(|err| format!("failed to decode PNG frame: {err}"))?;
    let frame = &bytes[..info.buffer_size()];
    let rgba = png_frame_to_rgba(frame, info.color_type, info.width, info.height)?;
    Ok(SceneWeTexImage {
        width: info.width,
        height: info.height,
        backing_width: info.width,
        backing_height: info.height,
        rgba,
        r8: None,
    })
}

pub(super) fn flip_rgba_rows_vertically(
    rgba: &mut [u8],
    width: u32,
    height: u32,
) -> Result<(), String> {
    let row_bytes = usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or_else(|| "RGBA row byte count overflowed".to_owned())?;
    let expected_len = row_bytes
        .checked_mul(usize::try_from(height).map_err(|_| "RGBA height exceeds usize")?)
        .ok_or_else(|| "RGBA byte count overflowed".to_owned())?;
    if rgba.len() != expected_len {
        return Err(format!(
            "RGBA payload has {} bytes, expected {expected_len}",
            rgba.len()
        ));
    }
    if height <= 1 {
        return Ok(());
    }
    let mut scratch = vec![0u8; row_bytes];
    for top_row in 0..height / 2 {
        let bottom_row = height - 1 - top_row;
        let top = usize::try_from(top_row)
            .ok()
            .and_then(|row| row.checked_mul(row_bytes))
            .ok_or_else(|| "RGBA top row offset overflowed".to_owned())?;
        let bottom = usize::try_from(bottom_row)
            .ok()
            .and_then(|row| row.checked_mul(row_bytes))
            .ok_or_else(|| "RGBA bottom row offset overflowed".to_owned())?;
        scratch.copy_from_slice(&rgba[top..top + row_bytes]);
        rgba.copy_within(bottom..bottom + row_bytes, top);
        rgba[bottom..bottom + row_bytes].copy_from_slice(&scratch);
    }
    Ok(())
}

pub(super) fn write_rgba8_gtex(path: &Path, image: &SceneWeTexImage) -> Result<(), String> {
    let expected_len = tex::rgba_len(image.width, image.height)?;
    if image.rgba.len() != expected_len {
        return Err(format!(
            "RGBA payload has {} bytes, expected {expected_len}",
            image.rgba.len()
        ));
    }
    let mip_rgba = rgba_mip_chain(&image.rgba, image.width, image.height)?;
    let payloads = mip_rgba
        .into_iter()
        .map(|level| level.rgba)
        .collect::<Vec<_>>();
    write_uncompressed_mip_payload_gtex(
        path,
        image.width,
        image.height,
        GILDER_SCENE_TEXTURE_FORMAT_R8G8B8A8_UNORM,
        &payloads,
    )
}

pub(super) fn write_r8_gtex(
    path: &Path,
    width: u32,
    height: u32,
    payload: &[u8],
) -> Result<(), String> {
    write_uncompressed_mip_payload_gtex(
        path,
        width,
        height,
        GILDER_SCENE_TEXTURE_FORMAT_R8_UNORM,
        &r8_mip_chain(payload, width, height)?,
    )
}

fn write_uncompressed_mip_payload_gtex(
    path: &Path,
    width: u32,
    height: u32,
    format: u32,
    payloads: &[Vec<u8>],
) -> Result<(), String> {
    let format_label = gtex_format_label(format)?;
    validate_gtex_mip_payloads(
        format_label,
        width,
        height,
        payloads,
        |level_width, level_height| uncompressed_payload_len(format, level_width, level_height),
    )?;
    write_gtex_mip_payloads(path, width, height, format, payloads)
}

fn validate_gtex_mip_payloads(
    format_label: &str,
    width: u32,
    height: u32,
    payloads: &[Vec<u8>],
    mut expected_len: impl FnMut(u32, u32) -> Result<u64, String>,
) -> Result<(), String> {
    if payloads.is_empty() {
        return Err(format!("{format_label} mip payload list must not be empty"));
    }
    let expected_mip_count = gtex_mip_count(width, height)?;
    if payloads.len() > expected_mip_count as usize {
        return Err(format!(
            "{format_label} mip payload list has {} levels, but {width}x{height} supports at most {expected_mip_count}",
            payloads.len()
        ));
    }
    for (level, payload) in payloads.iter().enumerate() {
        let (level_width, level_height) = gtex_mip_extent(width, height, level as u32)?;
        let expected_len = usize::try_from(expected_len(level_width, level_height)?)
            .map_err(|_| format!("{format_label} mip {level} payload length exceeds usize"))?;
        if payload.len() != expected_len {
            return Err(format!(
                "{format_label} mip {level} payload has {} bytes, expected {expected_len}",
                payload.len()
            ));
        }
    }
    Ok(())
}

pub(super) fn rgba8_mip_chain_payload_len(width: u32, height: u32) -> Result<u64, String> {
    uncompressed_mip_chain_payload_len(
        GILDER_SCENE_TEXTURE_FORMAT_R8G8B8A8_UNORM,
        width,
        height,
        gtex_mip_count(width, height)?,
    )
}

fn uncompressed_mip_chain_payload_len(
    format: u32,
    width: u32,
    height: u32,
    mip_count: u32,
) -> Result<u64, String> {
    if mip_count == 0 {
        return Err("uncompressed mip chain must contain at least one level".to_owned());
    }
    let format_label = gtex_format_label(format)?;
    let max_mip_count = gtex_mip_count(width, height)?;
    if mip_count > max_mip_count {
        return Err(format!(
            "{format_label} mip count {mip_count} exceeds {width}x{height} maximum {max_mip_count}"
        ));
    }
    let mut total = 0u64;
    for level in 0..mip_count {
        let (level_width, level_height) = gtex_mip_extent(width, height, level)?;
        total = total
            .checked_add(uncompressed_payload_len(format, level_width, level_height)?)
            .ok_or_else(|| format!("{format_label} mip chain payload size overflowed"))?;
    }
    Ok(total)
}

pub(super) fn write_bc_payload_gtex(
    path: &Path,
    width: u32,
    height: u32,
    format: u32,
    payload: &[u8],
) -> Result<(), String> {
    let format_label = gtex_format_label(format)?;
    let expected_len = usize::try_from(bc_payload_len(format, width, height)?)
        .map_err(|_| format!("{format_label} payload length exceeds usize"))?;
    if payload.len() != expected_len {
        return Err(format!(
            "{format_label} payload has {} bytes, expected {expected_len}",
            payload.len()
        ));
    }
    write_gtex_mip_payloads(path, width, height, format, &[payload.to_vec()])
}

fn write_gtex_mip_payloads(
    path: &Path,
    width: u32,
    height: u32,
    format: u32,
    payloads: &[Vec<u8>],
) -> Result<(), String> {
    let mip_count = u32::try_from(payloads.len())
        .map_err(|_| "native .gtex mip count exceeds u32".to_owned())?;
    let payload_len = payloads.iter().try_fold(0u64, |total, payload| {
        total
            .checked_add(payload.len() as u64)
            .ok_or_else(|| "native .gtex payload size overflowed".to_owned())
    })?;
    let mut file = fs::File::create(path).map_err(|err| err.to_string())?;
    file.write_all(GILDER_SCENE_TEXTURE_MAGIC)
        .map_err(|err| err.to_string())?;
    file.write_all(&width.to_le_bytes())
        .map_err(|err| err.to_string())?;
    file.write_all(&height.to_le_bytes())
        .map_err(|err| err.to_string())?;
    file.write_all(&format.to_le_bytes())
        .map_err(|err| err.to_string())?;
    file.write_all(&mip_count.to_le_bytes())
        .map_err(|err| err.to_string())?;
    file.write_all(&payload_len.to_le_bytes())
        .map_err(|err| err.to_string())?;
    for payload in payloads {
        file.write_all(payload).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn uncompressed_payload_len(format: u32, width: u32, height: u32) -> Result<u64, String> {
    let format_label = gtex_format_label(format)?;
    if width == 0 || height == 0 {
        return Err(format!(
            "{format_label} texture dimensions must be non-zero"
        ));
    }
    let bytes_per_texel = match format {
        GILDER_SCENE_TEXTURE_FORMAT_R8_UNORM => 1u64,
        GILDER_SCENE_TEXTURE_FORMAT_R8G8B8A8_UNORM => 4u64,
        _ => {
            return Err(format!(
                "unsupported uncompressed native .gtex format id {format}"
            ));
        }
    };
    u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|texels| texels.checked_mul(bytes_per_texel))
        .ok_or_else(|| format!("{format_label} payload size overflowed"))
}

pub(super) fn bc_payload_len(format: u32, width: u32, height: u32) -> Result<u64, String> {
    let format_label = gtex_format_label(format)?;
    if width == 0 || height == 0 {
        return Err(format!(
            "{format_label} texture dimensions must be non-zero"
        ));
    }
    let block_bytes = u64::from(bc_block_bytes(format)?);
    let blocks_w = u64::from(width.div_ceil(BC_BLOCK_TEXELS));
    let blocks_h = u64::from(height.div_ceil(BC_BLOCK_TEXELS));
    blocks_w
        .checked_mul(blocks_h)
        .and_then(|blocks| blocks.checked_mul(block_bytes))
        .ok_or_else(|| format!("{format_label} payload size overflowed"))
}

pub(super) fn gtex_mip_count(width: u32, height: u32) -> Result<u32, String> {
    if width == 0 || height == 0 {
        return Err("native .gtex texture dimensions must be non-zero".to_owned());
    }
    let mut levels = 1u32;
    let mut level_width = width;
    let mut level_height = height;
    while level_width > 1 || level_height > 1 {
        level_width = (level_width / 2).max(1);
        level_height = (level_height / 2).max(1);
        levels = levels
            .checked_add(1)
            .ok_or_else(|| "native .gtex mip count overflowed".to_owned())?;
    }
    Ok(levels)
}

pub(super) fn gtex_mip_extent(width: u32, height: u32, level: u32) -> Result<(u32, u32), String> {
    if width == 0 || height == 0 {
        return Err("native .gtex mip extent requires non-zero base dimensions".to_owned());
    }
    let mut level_width = width;
    let mut level_height = height;
    for _ in 0..level {
        level_width = (level_width / 2).max(1);
        level_height = (level_height / 2).max(1);
    }
    Ok((level_width, level_height))
}

pub(super) fn bc_block_bytes(format: u32) -> Result<u32, String> {
    match format {
        GILDER_SCENE_TEXTURE_FORMAT_BC1_RGBA_UNORM_BLOCK => Ok(BC1_BLOCK_BYTES as u32),
        GILDER_SCENE_TEXTURE_FORMAT_BC3_UNORM_BLOCK => Ok(BC3_BLOCK_BYTES as u32),
        GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK => Ok(BC7_BLOCK_BYTES as u32),
        _ => Err(format!(
            "unsupported native .gtex block-compressed format id {format}"
        )),
    }
}

pub(super) fn gtex_format_label(format: u32) -> Result<&'static str, String> {
    match format {
        GILDER_SCENE_TEXTURE_FORMAT_BC1_RGBA_UNORM_BLOCK => Ok("BC1_RGBA_UNORM_BLOCK"),
        GILDER_SCENE_TEXTURE_FORMAT_BC3_UNORM_BLOCK => Ok("BC3_UNORM_BLOCK"),
        GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK => Ok("BC7_UNORM_BLOCK"),
        GILDER_SCENE_TEXTURE_FORMAT_R8_UNORM => Ok("R8_UNORM"),
        GILDER_SCENE_TEXTURE_FORMAT_R8G8B8A8_UNORM => Ok("R8G8B8A8_UNORM"),
        _ => Err(format!("unsupported native .gtex format id {format}")),
    }
}

fn png_frame_to_rgba(
    frame: &[u8],
    color_type: png::ColorType,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    let pixel_count = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or_else(|| "PNG pixel count overflowed".to_owned())?;
    let expected_rgba = pixel_count
        .checked_mul(4)
        .ok_or_else(|| "PNG RGBA byte count overflowed".to_owned())?;
    match color_type {
        png::ColorType::Rgba => {
            if frame.len() != expected_rgba {
                return Err(format!(
                    "PNG RGBA payload has {} bytes, expected {expected_rgba}",
                    frame.len()
                ));
            }
            Ok(frame.to_vec())
        }
        png::ColorType::Rgb => {
            let expected_rgb = pixel_count
                .checked_mul(3)
                .ok_or_else(|| "PNG RGB byte count overflowed".to_owned())?;
            if frame.len() != expected_rgb {
                return Err(format!(
                    "PNG RGB payload has {} bytes, expected {expected_rgb}",
                    frame.len()
                ));
            }
            let mut rgba = Vec::with_capacity(expected_rgba);
            for rgb in frame.chunks_exact(3) {
                rgba.extend_from_slice(rgb);
                rgba.push(255);
            }
            Ok(rgba)
        }
        png::ColorType::Grayscale => {
            if frame.len() != pixel_count {
                return Err(format!(
                    "PNG grayscale payload has {} bytes, expected {pixel_count}",
                    frame.len()
                ));
            }
            let mut rgba = Vec::with_capacity(expected_rgba);
            for value in frame {
                rgba.extend_from_slice(&[*value, *value, *value, 255]);
            }
            Ok(rgba)
        }
        png::ColorType::GrayscaleAlpha => {
            let expected_gray_alpha = pixel_count
                .checked_mul(2)
                .ok_or_else(|| "PNG grayscale-alpha byte count overflowed".to_owned())?;
            if frame.len() != expected_gray_alpha {
                return Err(format!(
                    "PNG grayscale-alpha payload has {} bytes, expected {expected_gray_alpha}",
                    frame.len()
                ));
            }
            let mut rgba = Vec::with_capacity(expected_rgba);
            for gray_alpha in frame.chunks_exact(2) {
                rgba.extend_from_slice(&[
                    gray_alpha[0],
                    gray_alpha[0],
                    gray_alpha[0],
                    gray_alpha[1],
                ]);
            }
            Ok(rgba)
        }
        png::ColorType::Indexed => Err(
            "indexed PNG was not expanded by the PNG decoder; native gtex conversion requires RGB/RGBA output"
                .to_owned(),
        ),
    }
}

struct RgbaMipLevel {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn rgba_mip_chain(rgba: &[u8], width: u32, height: u32) -> Result<Vec<RgbaMipLevel>, String> {
    let expected_len = tex::rgba_len(width, height)?;
    if rgba.len() != expected_len {
        return Err(format!(
            "RGBA payload has {} bytes, expected {expected_len}",
            rgba.len()
        ));
    }
    let mut levels = Vec::with_capacity(gtex_mip_count(width, height)? as usize);
    levels.push(RgbaMipLevel {
        width,
        height,
        rgba: rgba.to_vec(),
    });
    while levels
        .last()
        .map(|level| level.width > 1 || level.height > 1)
        .unwrap_or(false)
    {
        let previous = levels.last().expect("mip level");
        let next_width = (previous.width / 2).max(1);
        let next_height = (previous.height / 2).max(1);
        let next = downsample_rgba_mip(
            &previous.rgba,
            previous.width,
            previous.height,
            next_width,
            next_height,
        )?;
        levels.push(RgbaMipLevel {
            width: next_width,
            height: next_height,
            rgba: next,
        });
    }
    Ok(levels)
}

fn downsample_rgba_mip(
    rgba: &[u8],
    width: u32,
    height: u32,
    next_width: u32,
    next_height: u32,
) -> Result<Vec<u8>, String> {
    let mut out = vec![0u8; tex::rgba_len(next_width, next_height)?];
    for y in 0..next_height {
        for x in 0..next_width {
            let (src_x0, src_x1) = mip_source_range(x, width, next_width)?;
            let (src_y0, src_y1) = mip_source_range(y, height, next_height)?;
            let mut sample_count = 0u32;
            let mut alpha_sum = 0u32;
            let mut premul = [0u32; 3];
            let mut raw_rgb = [0u32; 3];
            for src_y in src_y0..src_y1 {
                for src_x in src_x0..src_x1 {
                    let offset = rgba_offset(width, src_x, src_y)?;
                    let alpha = u32::from(rgba[offset + 3]);
                    for channel in 0..3 {
                        let value = u32::from(rgba[offset + channel]);
                        raw_rgb[channel] += value;
                        premul[channel] += value * alpha;
                    }
                    alpha_sum += alpha;
                    sample_count += 1;
                }
            }
            let dst = rgba_offset(next_width, x, y)?;
            let averaged_alpha = (alpha_sum + sample_count / 2) / sample_count;
            for channel in 0..3 {
                out[dst + channel] = if alpha_sum == 0 {
                    ((raw_rgb[channel] + sample_count / 2) / sample_count) as u8
                } else {
                    ((premul[channel] + alpha_sum / 2) / alpha_sum) as u8
                };
            }
            out[dst + 3] = averaged_alpha as u8;
        }
    }
    Ok(out)
}

fn r8_mip_chain(payload: &[u8], width: u32, height: u32) -> Result<Vec<Vec<u8>>, String> {
    let expected_len = usize::try_from(uncompressed_payload_len(
        GILDER_SCENE_TEXTURE_FORMAT_R8_UNORM,
        width,
        height,
    )?)
    .map_err(|_| "R8 payload length exceeds usize".to_owned())?;
    if payload.len() != expected_len {
        return Err(format!(
            "R8_UNORM payload has {} bytes, expected {expected_len}",
            payload.len()
        ));
    }
    let mut levels = Vec::with_capacity(gtex_mip_count(width, height)? as usize);
    levels.push(payload.to_vec());
    let mut level_width = width;
    let mut level_height = height;
    while level_width > 1 || level_height > 1 {
        let next_width = (level_width / 2).max(1);
        let next_height = (level_height / 2).max(1);
        let next = downsample_r8_mip(
            levels.last().expect("r8 mip level"),
            level_width,
            level_height,
            next_width,
            next_height,
        )?;
        levels.push(next);
        level_width = next_width;
        level_height = next_height;
    }
    Ok(levels)
}

fn downsample_r8_mip(
    bytes: &[u8],
    width: u32,
    height: u32,
    next_width: u32,
    next_height: u32,
) -> Result<Vec<u8>, String> {
    let out_len = usize::try_from(next_width)
        .ok()
        .and_then(|width| {
            usize::try_from(next_height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or_else(|| "R8 mip byte count overflowed".to_owned())?;
    let mut out = vec![0u8; out_len];
    for y in 0..next_height {
        for x in 0..next_width {
            let (src_x0, src_x1) = mip_source_range(x, width, next_width)?;
            let (src_y0, src_y1) = mip_source_range(y, height, next_height)?;
            let mut sample_count = 0u32;
            let mut sum = 0u32;
            for src_y in src_y0..src_y1 {
                for src_x in src_x0..src_x1 {
                    let offset = r8_offset(width, src_x, src_y)?;
                    sum += u32::from(bytes[offset]);
                    sample_count += 1;
                }
            }
            let dst = r8_offset(next_width, x, y)?;
            out[dst] = ((sum + sample_count / 2) / sample_count) as u8;
        }
    }
    Ok(out)
}

fn mip_source_range(index: u32, source_len: u32, target_len: u32) -> Result<(u32, u32), String> {
    if target_len == 0 {
        return Err("mip target dimension must be non-zero".to_owned());
    }
    let start = ((u64::from(index) * u64::from(source_len)) / u64::from(target_len)) as u32;
    let end =
        (((u64::from(index) + 1) * u64::from(source_len)).div_ceil(u64::from(target_len))) as u32;
    Ok((start.min(source_len), end.max(start + 1).min(source_len)))
}

fn rgba_offset(width: u32, x: u32, y: u32) -> Result<usize, String> {
    usize::try_from(y)
        .ok()
        .and_then(|row| {
            usize::try_from(width)
                .ok()
                .and_then(|stride| row.checked_mul(stride))
        })
        .and_then(|base| usize::try_from(x).ok().and_then(|x| base.checked_add(x)))
        .and_then(|pixel| pixel.checked_mul(4))
        .ok_or_else(|| "RGBA mip offset overflowed".to_owned())
}

fn r8_offset(width: u32, x: u32, y: u32) -> Result<usize, String> {
    usize::try_from(y)
        .ok()
        .and_then(|row| {
            usize::try_from(width)
                .ok()
                .and_then(|stride| row.checked_mul(stride))
        })
        .and_then(|base| usize::try_from(x).ok().and_then(|x| base.checked_add(x)))
        .ok_or_else(|| "R8 mip offset overflowed".to_owned())
}
