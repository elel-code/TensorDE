fn rendering_device_effect_debug_decode_bc7_mode6_payload(
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
            let pixels = rendering_device_effect_debug_decode_bc7_mode6_block(&block).map_err(
                |err| {
                    format!(
                        "{err} at block {block_x},{block_y}; diagnostic currently decodes converter BC7 mode 6 blocks"
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

fn rendering_device_effect_debug_decode_bc7_mode6_block(
    block: &[u8; 16],
) -> Result<[[u8; 4]; 16], String> {
    let mut bit = 0usize;
    let mut mode = None;
    for candidate in 0..8 {
        let value = rendering_device_effect_debug_bc7_get_bits(block, &mut bit, 1);
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
            (rendering_device_effect_debug_bc7_get_bits(block, &mut bit, 7) as u8) << 1;
        endpoint_b[channel] =
            (rendering_device_effect_debug_bc7_get_bits(block, &mut bit, 7) as u8) << 1;
    }
    let pbit_a = rendering_device_effect_debug_bc7_get_bits(block, &mut bit, 1) as u8;
    let pbit_b = rendering_device_effect_debug_bc7_get_bits(block, &mut bit, 1) as u8;
    for channel in 0..4 {
        endpoint_a[channel] |= pbit_a;
        endpoint_b[channel] |= pbit_b;
    }

    let palette = rendering_device_effect_debug_bc7_mode6_palette(endpoint_a, endpoint_b);
    let mut pixels = [[0u8; 4]; 16];
    for (pixel_index, pixel) in pixels.iter_mut().enumerate() {
        let width = if pixel_index == 0 { 3 } else { 4 };
        let index = rendering_device_effect_debug_bc7_get_bits(block, &mut bit, width) as usize;
        *pixel = palette[index.min(palette.len() - 1)];
    }
    Ok(pixels)
}

fn rendering_device_effect_debug_bc7_mode6_palette(
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

fn rendering_device_effect_debug_bc7_get_bits(block: &[u8; 16], bit: &mut usize, width: usize) -> u32 {
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
