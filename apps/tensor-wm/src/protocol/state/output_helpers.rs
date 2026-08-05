use tensor_util::Rect;

use crate::{
    backend::OutputDescriptor,
    render::{NativeOutputTarget, RenderOutputId},
};

pub(super) fn rects_overlap(left: (i32, i32, i32, i32), right: (i32, i32, i32, i32)) -> bool {
    let (lx, ly, lw, lh) = left;
    let (rx, ry, rw, rh) = right;
    lx < rx.saturating_add(rw)
        && rx < lx.saturating_add(lw)
        && ly < ry.saturating_add(rh)
        && ry < ly.saturating_add(lh)
}

pub(super) fn renderer_target(descriptor: &OutputDescriptor) -> NativeOutputTarget {
    NativeOutputTarget {
        output: RenderOutputId {
            device_id: descriptor.id.device_id,
            connector_id: descriptor.id.connector_id,
        },
        viewport: Rect::new(
            0,
            0,
            u32::try_from(descriptor.mode.width).unwrap_or(0),
            u32::try_from(descriptor.mode.height).unwrap_or(0),
        ),
        format: descriptor.native_format,
        scale: descriptor.scale,
    }
}
