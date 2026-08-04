use wayland_server::protocol::{wl_buffer::WlBuffer, wl_shm};

use crate::{
    protocol::globals::{
        image_copy_capture::CaptureFailureReason,
        shm::{BufferAccessError, with_buffer_contents_mut},
    },
    render::OutputCapturePixels,
};

pub(super) fn write_gpu_capture_shm(
    buffer: &WlBuffer,
    capture: &OutputCapturePixels,
) -> Result<(), CaptureFailureReason> {
    let width =
        i32::try_from(capture.size.width).map_err(|_| CaptureFailureReason::BufferConstraints)?;
    let height =
        i32::try_from(capture.size.height).map_err(|_| CaptureFailureReason::BufferConstraints)?;
    let source_stride = usize::try_from(capture.size.width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or(CaptureFailureReason::BufferConstraints)?;
    let source_len = source_stride
        .checked_mul(usize::try_from(capture.size.height).unwrap_or(usize::MAX))
        .ok_or(CaptureFailureReason::BufferConstraints)?;
    if capture.bytes.len() != source_len {
        return Err(CaptureFailureReason::Unknown);
    }
    with_buffer_contents_mut(buffer, |ptr, len, data| {
        if data.width < width || data.height < height || data.stride < width.saturating_mul(4) {
            return Err(CaptureFailureReason::BufferConstraints);
        }
        let opaque = match data.format {
            wl_shm::Format::Xrgb8888 => true,
            wl_shm::Format::Argb8888 => false,
            _ => return Err(CaptureFailureReason::BufferConstraints),
        };
        let destination_len = usize::try_from(data.stride)
            .ok()
            .and_then(|stride| stride.checked_mul(usize::try_from(height).ok()?))
            .ok_or(CaptureFailureReason::BufferConstraints)?;
        if destination_len > len {
            return Err(CaptureFailureReason::BufferConstraints);
        }
        #[allow(unsafe_code)]
        let destination = unsafe { std::slice::from_raw_parts_mut(ptr, destination_len) };
        for y in 0..usize::try_from(height).unwrap_or(0) {
            let source_row = &capture.bytes[y * source_stride..(y + 1) * source_stride];
            let destination_start = y * usize::try_from(data.stride).unwrap_or(0);
            let destination_row =
                &mut destination[destination_start..destination_start + source_stride];
            for (source, target) in source_row
                .chunks_exact(4)
                .zip(destination_row.chunks_exact_mut(4))
            {
                let [b, g, r, a] = capture_bgra8_pixel(capture.format, source)?;
                target.copy_from_slice(&[b, g, r, if opaque { 0xff } else { a }]);
            }
        }
        Ok(())
    })
    .map_err(|error| match error {
        BufferAccessError::NotManaged => CaptureFailureReason::BufferConstraints,
        _ => CaptureFailureReason::Unknown,
    })?
}

fn capture_bgra8_pixel(
    format: vulkan_renderer::TextureFormat,
    bytes: &[u8],
) -> Result<[u8; 4], CaptureFailureReason> {
    let pixel: [u8; 4] = bytes
        .try_into()
        .map_err(|_| CaptureFailureReason::Unknown)?;
    Ok(match format {
        vulkan_renderer::TextureFormat::Bgra8Unorm | vulkan_renderer::TextureFormat::Bgra8Srgb => {
            pixel
        }
        vulkan_renderer::TextureFormat::Rgba8Unorm | vulkan_renderer::TextureFormat::Rgba8Srgb => {
            [pixel[2], pixel[1], pixel[0], pixel[3]]
        }
        vulkan_renderer::TextureFormat::A2R10G10B10UnormPack32 => {
            packed_10_bit_bgra(u32::from_le_bytes(pixel), false)
        }
        vulkan_renderer::TextureFormat::A2B10G10R10UnormPack32 => {
            packed_10_bit_bgra(u32::from_le_bytes(pixel), true)
        }
        _ => return Err(CaptureFailureReason::BufferConstraints),
    })
}

fn packed_10_bit_bgra(pixel: u32, red_low: bool) -> [u8; 4] {
    let low = pixel & 0x3ff;
    let green = (pixel >> 10) & 0x3ff;
    let high = (pixel >> 20) & 0x3ff;
    let (red, blue) = if red_low { (low, high) } else { (high, low) };
    let to_u8 = |channel: u32| ((channel * 255 + 511) / 1023) as u8;
    [
        to_u8(blue),
        to_u8(green),
        to_u8(red),
        (((pixel >> 30) & 0x3) * 85) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_and_ten_bit_formats_lower_to_shm_bgra() {
        assert_eq!(
            capture_bgra8_pixel(vulkan_renderer::TextureFormat::Rgba8Srgb, &[1, 2, 3, 4]).unwrap(),
            [3, 2, 1, 4]
        );
        assert_eq!(packed_10_bit_bgra(0xC00F_FC00, false)[3], 0xff);
    }
}
