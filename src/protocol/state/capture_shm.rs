//! SHM client-content blit for image-copy-capture (idle path only).
//!
//! Uses the live buffer held by Tensor's surface state after commit consumes
//! the pending assignment. DMA-only clients keep the silhouette. Never called
//! from page-flip / submit.

use smithay::wayland::shm::{BufferAccessError, with_buffer_contents};
use tracing::trace;
use wayland_server::protocol::{wl_shm, wl_surface::WlSurface};

use crate::layout::Rect;

use super::surfaces::surface_buffer;

/// Blit one SHM surface into an XRGB/ARGB capture mapping.
///
/// 1:1 size only (no filter scale) so idle-turn cost stays O(src pixels).
pub(super) fn blit_surface_shm_into(
    surface: &WlSurface,
    dest: &mut [u8],
    dest_stride: i32,
    dest_w: i32,
    dest_h: i32,
    dest_rect: Rect,
    clip: Rect,
) -> bool {
    let Some(buffer) = surface_buffer(surface) else {
        return false;
    };
    match with_buffer_contents(&buffer, |ptr, len, data| {
        match data.format {
            wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888 => {}
            _ => return false,
        }
        if data.width <= 0 || data.height <= 0 || data.stride < data.width.saturating_mul(4) {
            return false;
        }
        let src_need = data
            .stride
            .checked_mul(data.height)
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(0);
        if src_need == 0 || src_need > len {
            return false;
        }
        // SAFETY: mapped for this closure only.
        #[allow(unsafe_code)]
        let src = unsafe { std::slice::from_raw_parts(ptr, src_need) };
        let Some(area) = dest_rect.intersection(clip) else {
            return false;
        };
        if data.width as u32 != dest_rect.width || data.height as u32 != dest_rect.height {
            trace!(
                src_w = data.width,
                src_h = data.height,
                dst_w = dest_rect.width,
                dst_h = dest_rect.height,
                "skip SHM blit: size mismatch"
            );
            return false;
        }
        let x0 = area.x.clamp(0, dest_w);
        let y0 = area.y.clamp(0, dest_h);
        let x1 = area.right().clamp(0, dest_w);
        let y1 = area.bottom().clamp(0, dest_h);
        if x0 >= x1 || y0 >= y1 {
            return false;
        }
        let src_off_x = (x0 - dest_rect.x).max(0) as usize;
        let src_off_y = (y0 - dest_rect.y).max(0) as usize;
        for (row, y) in (y0..y1).enumerate() {
            let src_y = src_off_y + row;
            if src_y >= data.height as usize {
                break;
            }
            let src_row = src_y * data.stride as usize;
            let dst_row = (y as usize).saturating_mul(dest_stride as usize);
            let width_px = (x1 - x0) as usize;
            let si = src_row + src_off_x * 4;
            let di = dst_row + (x0 as usize) * 4;
            let bytes = width_px * 4;
            if si + bytes <= src.len() && di + bytes <= dest.len() {
                dest[di..di + bytes].copy_from_slice(&src[si..si + bytes]);
            }
        }
        true
    }) {
        Ok(wrote) => wrote,
        Err(BufferAccessError::NotManaged) => false,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dest_rect_intersection_helper_exists() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(5, 5, 10, 10);
        assert_eq!(a.intersection(b).unwrap(), Rect::new(5, 5, 5, 5));
    }
}
