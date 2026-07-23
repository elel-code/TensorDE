//! Conservative normalized alpha coverage generated before GPU block compression.

use crate::engine::scene::{
    SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE, SCENE_TEXTURE_ALPHA_COVERAGE_GUARD_CELLS,
    SceneTextureFormat,
};

use super::{TexUpload, TexUploadMip};

pub(in crate::convert::we_ingest) fn texture_alpha_coverage_rows(
    upload: &TexUpload,
) -> [u32; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE] {
    if upload.format != SceneTextureFormat::Rgba8Unorm {
        return [u32::MAX; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE];
    }
    let mut rows = [0u32; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE];
    for mip in &upload.mips {
        let Some(payload) = mip_payload(upload, mip) else {
            return [u32::MAX; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE];
        };
        let expected_len = mip.width as usize * mip.height as usize * 4;
        if mip.width == 0 || mip.height == 0 || payload.len() != expected_len {
            return [u32::MAX; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE];
        }
        let mip_width = mip.width as usize;
        let mip_height = mip.height as usize;
        for y in 0..mip_height {
            let row_start = y * SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE / mip_height;
            let row_end = ((y + 1) * SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE)
                .div_ceil(mip_height)
                .min(SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE);
            for x in 0..mip_width {
                if payload[(y * mip.width as usize + x) * 4 + 3] == 0 {
                    continue;
                }
                let column_start = x * SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE / mip_width;
                let column_end = ((x + 1) * SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE)
                    .div_ceil(mip_width)
                    .min(SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE);
                let width = column_end - column_start;
                let mask = if width == SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE {
                    u32::MAX
                } else {
                    ((1u32 << width) - 1) << column_start
                };
                for row in &mut rows[row_start..row_end] {
                    *row |= mask;
                }
            }
        }
    }
    dilate_filter_guard(rows)
}

fn mip_payload<'a>(upload: &'a TexUpload, mip: &TexUploadMip) -> Option<&'a [u8]> {
    let start = usize::try_from(mip.payload_offset).ok()?;
    let len = usize::try_from(mip.payload_len).ok()?;
    upload.payload.get(start..start.checked_add(len)?)
}

fn dilate_filter_guard(
    rows: [u32; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE],
) -> [u32; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE] {
    let mut expanded = [0u32; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE];
    for (row, bits) in rows.into_iter().enumerate() {
        let mut horizontal = bits;
        for shift in 1..=SCENE_TEXTURE_ALPHA_COVERAGE_GUARD_CELLS {
            horizontal |= bits.wrapping_shl(shift as u32) | bits.wrapping_shr(shift as u32);
        }
        let start = row.saturating_sub(SCENE_TEXTURE_ALPHA_COVERAGE_GUARD_CELLS);
        let end = (row + SCENE_TEXTURE_ALPHA_COVERAGE_GUARD_CELLS)
            .min(SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE - 1);
        for target in expanded.iter_mut().take(end + 1).skip(start) {
            *target |= horizontal;
        }
    }
    expanded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::we_ingest::tex::TexMetadata;

    #[test]
    fn rgba_coverage_maps_all_mips_and_expands_filter_footprint() {
        let mut pixels = vec![0u8; 8 * 8 * 4];
        pixels[(3 * 8 + 4) * 4 + 3] = 255;
        let upload = TexUpload {
            metadata: metadata(8, 8),
            format: SceneTextureFormat::Rgba8Unorm,
            mips: vec![TexUploadMip {
                width: 8,
                height: 8,
                payload_offset: 0,
                payload_len: pixels.len() as u64,
            }],
            payload: pixels,
        };

        let rows = texture_alpha_coverage_rows(&upload);
        let center_row = 3 * SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE / 8;
        let center_column = 4 * SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE / 8;
        for row in center_row - SCENE_TEXTURE_ALPHA_COVERAGE_GUARD_CELLS
            ..=center_row + SCENE_TEXTURE_ALPHA_COVERAGE_GUARD_CELLS
        {
            assert_ne!(rows[row] & (1 << center_column), 0);
        }
    }

    #[test]
    fn compressed_source_without_provable_alpha_is_fully_covered() {
        let upload = TexUpload {
            metadata: metadata(4, 4),
            format: SceneTextureFormat::Bc7UnormBlock,
            mips: vec![TexUploadMip {
                width: 4,
                height: 4,
                payload_offset: 0,
                payload_len: 16,
            }],
            payload: vec![0; 16],
        };

        assert_eq!(
            texture_alpha_coverage_rows(&upload),
            [u32::MAX; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE]
        );
    }

    #[test]
    fn low_resolution_mip_covers_its_full_normalized_cell_span() {
        let pixels = vec![255u8; 2 * 2 * 4];
        let upload = TexUpload {
            metadata: metadata(2, 2),
            format: SceneTextureFormat::Rgba8Unorm,
            mips: vec![TexUploadMip {
                width: 2,
                height: 2,
                payload_offset: 0,
                payload_len: pixels.len() as u64,
            }],
            payload: pixels,
        };

        assert_eq!(
            texture_alpha_coverage_rows(&upload),
            [u32::MAX; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE]
        );
    }

    fn metadata(width: u32, height: u32) -> TexMetadata {
        TexMetadata {
            texv_tag: "TEXV0005".to_owned(),
            texi_tag: "TEXI0001".to_owned(),
            texb_tag: "TEXB0004".to_owned(),
            runtime_format: 0,
            payload_format: 0,
            sampler_seed: 0,
            sampler_filter: crate::engine::scene::SceneTextureSamplerFilter::Anisotropic8,
            sampler_address_mode: crate::engine::scene::SceneTextureSamplerAddressMode::Repeat,
            width,
            height,
            storage_width: width,
            storage_height: height,
            mip_count: 1,
        }
    }
}
