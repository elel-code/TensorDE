use tensor_util::Rect;

use crate::{ecs::ViewId, render::frame::NativeOutputTarget, scene::FocusOutline};

use super::FocusRingDraw;

pub(super) fn texture_format(
    format: tensor_host::Fourcc,
) -> Option<vulkan_renderer::TextureFormat> {
    match format {
        tensor_host::Fourcc::XRGB8888 | tensor_host::Fourcc::ARGB8888 => {
            Some(vulkan_renderer::TextureFormat::Bgra8Srgb)
        }
        tensor_host::Fourcc::XBGR8888 | tensor_host::Fourcc::ABGR8888 => {
            Some(vulkan_renderer::TextureFormat::Rgba8Srgb)
        }
        tensor_host::Fourcc::XRGB2101010 | tensor_host::Fourcc::ARGB2101010 => {
            Some(vulkan_renderer::TextureFormat::A2R10G10B10UnormPack32)
        }
        tensor_host::Fourcc::XBGR2101010 | tensor_host::Fourcc::ABGR2101010 => {
            Some(vulkan_renderer::TextureFormat::A2B10G10R10UnormPack32)
        }
        _ => None,
    }
}

pub(super) fn focus_ring_draw(
    view_id: ViewId,
    outline: FocusOutline,
    geometry: Rect,
    corner_radius: u32,
    scene_viewport: Rect,
    target: NativeOutputTarget,
) -> Option<FocusRingDraw> {
    if !outline.visible() || geometry.width == 0 || geometry.height == 0 {
        return None;
    }
    let logical_inner = geometry.translated(-scene_viewport.x, -scene_viewport.y);
    let inner = target.scale.physical_rect_round(logical_inner);
    if inner.width == 0 || inner.height == 0 {
        return None;
    }

    // Map expanded edges before guaranteeing at least one physical pixel on
    // each side, so fractional scaling cannot erase a one-logical-pixel ring.
    let minimum_width = target.scale.physical_length_round(outline.width).max(1);
    let outer = target
        .scale
        .physical_rect_round(logical_inner.inflated(outline.width))
        .union(inner.inflated(minimum_width));
    let clip = outer.intersection(target.viewport)?;
    let inner_radius = target
        .scale
        .physical_length_round(corner_radius)
        .min(inner.width.min(inner.height) / 2);
    let outer_radius = inner_radius
        .saturating_add(minimum_width)
        .min(outer.width.min(outer.height) / 2);
    Some(FocusRingDraw {
        view_id,
        destination: outer,
        clip,
        inner,
        color: outline.color,
        outer_radius,
        inner_radius,
    })
}
