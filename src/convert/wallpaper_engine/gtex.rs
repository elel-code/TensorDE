use super::tex::{self, SceneWeTexImage};
use std::fs;
use std::io::{BufReader, Cursor, Write};
use std::path::Path;

pub(super) const GILDER_SCENE_TEXTURE_MAGIC: &[u8; 8] = b"GDTEX002";
pub(super) const GILDER_SCENE_TEXTURE_FORMAT_BC1_RGBA_UNORM_BLOCK: u32 = 1;
pub(super) const GILDER_SCENE_TEXTURE_FORMAT_BC3_UNORM_BLOCK: u32 = 3;
pub(super) const GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK: u32 = 7;

const GILDER_SCENE_TEXTURE_MIP_COUNT: u32 = 1;
const BC_BLOCK_TEXELS: u32 = 4;
const BC1_BLOCK_BYTES: usize = 8;
const BC3_BLOCK_BYTES: usize = 16;
const BC7_BLOCK_BYTES: usize = 16;
const BC7_MODE6_INDEX_WEIGHTS: [u16; 16] =
    [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];

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
        rgba,
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

pub(super) fn write_bc7_gtex(path: &Path, image: &SceneWeTexImage) -> Result<(), String> {
    let expected_len = tex::rgba_len(image.width, image.height)?;
    if image.rgba.len() != expected_len {
        return Err(format!(
            "RGBA payload has {} bytes, expected {expected_len}",
            image.rgba.len()
        ));
    }
    let payload = encode_bc7(&image.rgba, image.width, image.height)?;
    write_bc7_payload_gtex(path, image.width, image.height, &payload)
}

pub(super) fn write_bc7_payload_gtex(
    path: &Path,
    width: u32,
    height: u32,
    payload: &[u8],
) -> Result<(), String> {
    write_bc_payload_gtex(
        path,
        width,
        height,
        GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK,
        payload,
    )
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
    let mut file = fs::File::create(path).map_err(|err| err.to_string())?;
    file.write_all(GILDER_SCENE_TEXTURE_MAGIC)
        .map_err(|err| err.to_string())?;
    file.write_all(&width.to_le_bytes())
        .map_err(|err| err.to_string())?;
    file.write_all(&height.to_le_bytes())
        .map_err(|err| err.to_string())?;
    file.write_all(&format.to_le_bytes())
        .map_err(|err| err.to_string())?;
    file.write_all(&GILDER_SCENE_TEXTURE_MIP_COUNT.to_le_bytes())
        .map_err(|err| err.to_string())?;
    file.write_all(&(payload.len() as u64).to_le_bytes())
        .map_err(|err| err.to_string())?;
    file.write_all(&payload).map_err(|err| err.to_string())
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

pub(super) fn bc7_payload_len(width: u32, height: u32) -> Result<u64, String> {
    bc_payload_len(GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK, width, height)
}

pub(super) fn bc_block_bytes(format: u32) -> Result<u32, String> {
    match format {
        GILDER_SCENE_TEXTURE_FORMAT_BC1_RGBA_UNORM_BLOCK => Ok(BC1_BLOCK_BYTES as u32),
        GILDER_SCENE_TEXTURE_FORMAT_BC3_UNORM_BLOCK => Ok(BC3_BLOCK_BYTES as u32),
        GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK => Ok(BC7_BLOCK_BYTES as u32),
        _ => Err(format!("unsupported native .gtex BC format id {format}")),
    }
}

pub(super) fn gtex_format_label(format: u32) -> Result<&'static str, String> {
    match format {
        GILDER_SCENE_TEXTURE_FORMAT_BC1_RGBA_UNORM_BLOCK => Ok("BC1_RGBA_UNORM_BLOCK"),
        GILDER_SCENE_TEXTURE_FORMAT_BC3_UNORM_BLOCK => Ok("BC3_UNORM_BLOCK"),
        GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK => Ok("BC7_UNORM_BLOCK"),
        _ => Err(format!("unsupported native .gtex BC format id {format}")),
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

fn encode_bc7(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    if width == 0 || height == 0 {
        return Err("BC7 texture dimensions must be non-zero".to_owned());
    }
    let expected_len = tex::rgba_len(width, height)?;
    if rgba.len() != expected_len {
        return Err(format!(
            "RGBA payload has {} bytes, expected {expected_len}",
            rgba.len()
        ));
    }
    let blocks_w = width.div_ceil(BC_BLOCK_TEXELS);
    let blocks_h = height.div_ceil(BC_BLOCK_TEXELS);
    let block_count = usize::try_from(blocks_w)
        .ok()
        .and_then(|w| {
            usize::try_from(blocks_h)
                .ok()
                .and_then(|h| w.checked_mul(h))
        })
        .ok_or_else(|| "BC7 block count overflowed".to_owned())?;
    let mut out = Vec::with_capacity(
        block_count
            .checked_mul(BC7_BLOCK_BYTES)
            .ok_or_else(|| "BC7 payload size overflowed".to_owned())?,
    );
    for block_y in 0..blocks_h {
        for block_x in 0..blocks_w {
            encode_bc7_block(rgba, width, height, block_x, block_y, &mut out)?;
        }
    }
    Ok(out)
}

fn encode_bc7_block(
    rgba: &[u8],
    width: u32,
    height: u32,
    block_x: u32,
    block_y: u32,
    out: &mut Vec<u8>,
) -> Result<(), String> {
    let mut pixels = [[0u8; 4]; 16];
    for y in 0..BC_BLOCK_TEXELS {
        for x in 0..BC_BLOCK_TEXELS {
            let src_x = (block_x * BC_BLOCK_TEXELS + x).min(width - 1);
            let src_y = (block_y * BC_BLOCK_TEXELS + y).min(height - 1);
            let src = usize::try_from(src_y)
                .ok()
                .and_then(|row| {
                    usize::try_from(width)
                        .ok()
                        .and_then(|stride| row.checked_mul(stride))
                })
                .and_then(|base| {
                    usize::try_from(src_x)
                        .ok()
                        .and_then(|x| base.checked_add(x))
                })
                .and_then(|pixel| pixel.checked_mul(4))
                .ok_or_else(|| "BC7 source pixel offset overflowed".to_owned())?;
            let dst = usize::try_from(y * BC_BLOCK_TEXELS + x)
                .map_err(|_| "BC7 block pixel index overflowed".to_owned())?;
            pixels[dst].copy_from_slice(
                rgba.get(src..src + 4)
                    .ok_or_else(|| "BC7 source pixel range exceeded RGBA payload".to_owned())?,
            );
        }
    }
    let (mut endpoint_a, mut endpoint_b) = bc7_mode6_endpoints(&pixels);
    let palette = bc7_mode6_palette(endpoint_a, endpoint_b);
    let mut indices = bc7_mode6_indices(&pixels, &palette);
    if indices[0] >= 8 {
        std::mem::swap(&mut endpoint_a, &mut endpoint_b);
        for index in &mut indices {
            *index = 15 - *index;
        }
    }
    pack_bc7_mode6_block(endpoint_a, endpoint_b, &indices, out);
    Ok(())
}

fn bc7_mode6_endpoints(pixels: &[[u8; 4]; 16]) -> ([u8; 4], [u8; 4]) {
    let mut min_rgba = [255u8; 4];
    let mut max_rgba = [0u8; 4];
    for pixel in pixels {
        for channel in 0..4 {
            min_rgba[channel] = min_rgba[channel].min(pixel[channel]);
            max_rgba[channel] = max_rgba[channel].max(pixel[channel]);
        }
    }
    let endpoint_a = bc7_endpoint_with_majority_pbit(min_rgba);
    let endpoint_b = bc7_endpoint_with_majority_pbit(max_rgba);
    (endpoint_a, endpoint_b)
}

fn bc7_endpoint_with_majority_pbit(endpoint: [u8; 4]) -> [u8; 4] {
    let pbit = u8::from(
        endpoint
            .iter()
            .filter(|component| **component & 1 != 0)
            .count()
            >= 2,
    );
    [
        bc7_quantize_7_with_pbit(endpoint[0], pbit),
        bc7_quantize_7_with_pbit(endpoint[1], pbit),
        bc7_quantize_7_with_pbit(endpoint[2], pbit),
        bc7_quantize_7_with_pbit(endpoint[3], pbit),
    ]
}

fn bc7_quantize_7_with_pbit(value: u8, pbit: u8) -> u8 {
    let adjusted = value.saturating_add(1);
    ((adjusted >> 1) << 1) | (pbit & 1)
}

fn bc7_mode6_palette(endpoint_a: [u8; 4], endpoint_b: [u8; 4]) -> [[u8; 4]; 16] {
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

fn bc7_mode6_indices(pixels: &[[u8; 4]; 16], palette: &[[u8; 4]; 16]) -> [u8; 16] {
    let mut indices = [0u8; 16];
    for (pixel_index, pixel) in pixels.iter().enumerate() {
        indices[pixel_index] = palette
            .iter()
            .enumerate()
            .min_by_key(|(_, candidate)| rgba_distance_squared(pixel, candidate))
            .map(|(index, _)| index as u8)
            .unwrap_or(0);
    }
    indices
}

fn rgba_distance_squared(lhs: &[u8; 4], rhs: &[u8; 4]) -> u32 {
    (0..4)
        .map(|channel| {
            let delta = i32::from(lhs[channel]) - i32::from(rhs[channel]);
            (delta * delta) as u32
        })
        .sum()
}

fn pack_bc7_mode6_block(
    endpoint_a: [u8; 4],
    endpoint_b: [u8; 4],
    indices: &[u8; 16],
    out: &mut Vec<u8>,
) {
    let mut block = [0u8; 16];
    let mut bit = 0usize;
    bc7_set_bits(&mut block, &mut bit, 6, 0);
    bc7_set_bits(&mut block, &mut bit, 1, 1);
    for channel in 0..4 {
        bc7_set_bits(&mut block, &mut bit, 7, endpoint_a[channel] >> 1);
        bc7_set_bits(&mut block, &mut bit, 7, endpoint_b[channel] >> 1);
    }
    bc7_set_bits(&mut block, &mut bit, 1, endpoint_a[0] & 1);
    bc7_set_bits(&mut block, &mut bit, 1, endpoint_b[0] & 1);
    for (pixel_index, index) in indices.iter().copied().enumerate() {
        let width = if pixel_index == 0 { 3 } else { 4 };
        bc7_set_bits(&mut block, &mut bit, width, index);
    }
    debug_assert_eq!(bit, 128);
    out.extend_from_slice(&block);
}

fn bc7_set_bits(block: &mut [u8; 16], bit: &mut usize, width: usize, value: u8) {
    if width == 0 {
        return;
    }
    debug_assert!(*bit + width <= 128);
    debug_assert!(width <= 8);
    debug_assert!(u16::from(value) < (1u16 << width));
    for offset in 0..width {
        if value & (1u8 << offset) != 0 {
            let absolute = *bit + offset;
            block[absolute >> 3] |= 1u8 << (absolute & 7);
        }
    }
    *bit += width;
}
