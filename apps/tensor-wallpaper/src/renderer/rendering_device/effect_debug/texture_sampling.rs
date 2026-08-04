fn rendering_device_effect_debug_rgba_luma(color: [f64; 4]) -> f64 {
    color[0] * 0.2126 + color[1] * 0.7152 + color[2] * 0.0722
}

fn rendering_device_effect_debug_rgba_is_visible_dark(color: [f64; 4]) -> bool {
    color[3] > DEBUG_VISIBLE_ALPHA_THRESHOLD
        && rendering_device_effect_debug_rgba_luma(color) < DEBUG_DARK_LUMA_THRESHOLD
}

fn rendering_device_effect_debug_r8_at(width: u32, height: u32, payload: &[u8], x: u32, y: u32) -> u8 {
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

fn rendering_device_effect_debug_fraction_index(limit: u32, fraction: f32) -> u32 {
    if limit == 0 {
        return 0;
    }
    ((limit - 1) as f32 * fraction.clamp(0.0, 1.0)).round() as u32
}

fn rendering_device_effect_debug_read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn rendering_device_effect_debug_read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

fn rendering_device_effect_debug_mip_count(width: u32, height: u32) -> Result<u32, String> {
    if width == 0 || height == 0 {
        return Err("debug gtex mip count requires non-zero dimensions".to_owned());
    }
    let mut levels = 1u32;
    let mut level_width = width;
    let mut level_height = height;
    while level_width > 1 || level_height > 1 {
        level_width = (level_width / 2).max(1);
        level_height = (level_height / 2).max(1);
        levels = levels
            .checked_add(1)
            .ok_or_else(|| "debug gtex mip count overflowed".to_owned())?;
    }
    Ok(levels)
}

fn rendering_device_effect_debug_mip_extent(
    width: u32,
    height: u32,
    level: u32,
) -> Result<(u32, u32), String> {
    if width == 0 || height == 0 {
        return Err("debug gtex mip extent requires non-zero dimensions".to_owned());
    }
    let mut level_width = width;
    let mut level_height = height;
    for _ in 0..level {
        level_width = (level_width / 2).max(1);
        level_height = (level_height / 2).max(1);
    }
    Ok((level_width, level_height))
}

fn rendering_device_effect_debug_r8_mip_chain_len(
    width: u32,
    height: u32,
    mip_count: u32,
) -> Result<u64, String> {
    rendering_device_effect_debug_mip_chain_len(width, height, mip_count, |width, height| {
        u64::from(width).checked_mul(u64::from(height))
    })
}

fn rendering_device_effect_debug_bc7_mip_chain_len(
    width: u32,
    height: u32,
    mip_count: u32,
) -> Result<u64, String> {
    rendering_device_effect_debug_mip_chain_len(width, height, mip_count, |width, height| {
        u64::from(width.div_ceil(BC_BLOCK_TEXELS))
            .checked_mul(u64::from(height.div_ceil(BC_BLOCK_TEXELS)))
            .and_then(|blocks| blocks.checked_mul(BC7_BLOCK_BYTES as u64))
    })
}

fn rendering_device_effect_debug_rgba_base_len(
    width: u32,
    height: u32,
    format: u32,
) -> Result<u64, String> {
    match format {
        TENSOR_WALLPAPER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK => u64::from(width.div_ceil(BC_BLOCK_TEXELS))
            .checked_mul(u64::from(height.div_ceil(BC_BLOCK_TEXELS)))
            .and_then(|blocks| blocks.checked_mul(BC7_BLOCK_BYTES as u64))
            .ok_or_else(|| "debug BC7 base payload length overflowed".to_owned()),
        TENSOR_WALLPAPER_SCENE_TEXTURE_FORMAT_R8G8B8A8_UNORM => u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|texels| texels.checked_mul(4))
            .ok_or_else(|| "debug RGBA8 base payload length overflowed".to_owned()),
        _ => Err(format!("unsupported debug RGBA gtex format {format}")),
    }
}

fn rendering_device_effect_debug_rgba_mip_chain_len(
    width: u32,
    height: u32,
    mip_count: u32,
    format: u32,
) -> Result<u64, String> {
    match format {
        TENSOR_WALLPAPER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK => {
            rendering_device_effect_debug_bc7_mip_chain_len(width, height, mip_count)
        }
        TENSOR_WALLPAPER_SCENE_TEXTURE_FORMAT_R8G8B8A8_UNORM => {
            rendering_device_effect_debug_mip_chain_len(width, height, mip_count, |width, height| {
                u64::from(width)
                    .checked_mul(u64::from(height))
                    .and_then(|texels| texels.checked_mul(4))
            })
        }
        _ => Err(format!("unsupported debug RGBA gtex format {format}")),
    }
}

fn rendering_device_effect_debug_mip_chain_len(
    width: u32,
    height: u32,
    mip_count: u32,
    mut level_len: impl FnMut(u32, u32) -> Option<u64>,
) -> Result<u64, String> {
    if mip_count == 0 {
        return Err("debug gtex mip chain requires at least one level".to_owned());
    }
    let max_mip_count = rendering_device_effect_debug_mip_count(width, height)?;
    if mip_count > max_mip_count {
        return Err(format!(
            "debug gtex mip count {mip_count} exceeds {width}x{height} maximum {max_mip_count}"
        ));
    }
    let mut total = 0u64;
    for level in 0..mip_count {
        let (level_width, level_height) =
            rendering_device_effect_debug_mip_extent(width, height, level)?;
        let mip_len = level_len(level_width, level_height)
            .ok_or_else(|| "debug gtex mip payload length overflowed".to_owned())?;
        total = total
            .checked_add(mip_len)
            .ok_or_else(|| "debug gtex mip chain payload length overflowed".to_owned())?;
    }
    Ok(total)
}


pub(in crate::renderer::rendering_device) struct RenderingDeviceEffectDebugR8Texture {
    width: u32,
    height: u32,
    payload: Vec<u8>,
}

impl RenderingDeviceEffectDebugR8Texture {
    fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub(in crate::renderer::rendering_device) fn sample_linear(&self, uv: [f32; 2]) -> f64 {
        rendering_device_effect_debug_sample_r8_linear(self.width, self.height, self.payload(), uv)
    }
}

pub(in crate::renderer::rendering_device) struct RenderingDeviceEffectDebugRgbaTexture {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl RenderingDeviceEffectDebugRgbaTexture {
    fn payload(&self) -> &[u8] {
        &self.rgba
    }

    pub(in crate::renderer::rendering_device) fn sample_linear(&self, uv: [f32; 2]) -> [f64; 4] {
        rendering_device_effect_debug_sample_rgba_linear(self.width, self.height, self.payload(), uv)
    }
}

#[cfg(test)]
mod tests;
