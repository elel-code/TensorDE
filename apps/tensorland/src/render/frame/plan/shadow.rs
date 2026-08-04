use tensor_util::Rect;

use crate::{
    ecs::ViewId,
    render::frame::NativeOutputTarget,
    scene::{LinearRgba16, SceneNode, UnitFraction},
};

/// One analytic rounded-rectangle shadow in physical output coordinates.
///
/// `box_rect` is the spread-adjusted opaque source inside `destination`;
/// `destination` includes the complete configured blur support. The Vulkan
/// shader integrates the Gaussian mask directly, so this remains a direct
/// output draw and needs no intermediate target or descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::render) struct ShadowDraw {
    pub(crate) view_id: ViewId,
    pub(crate) destination: Rect,
    pub(crate) clip: Rect,
    pub(crate) box_rect: Rect,
    pub(crate) color: LinearRgba16,
    pub(crate) opacity: UnitFraction,
    pub(crate) blur_radius: u32,
    pub(crate) corner_radius: u32,
}

pub(super) fn shadow_draw(
    node: &SceneNode,
    scene_viewport: Rect,
    target: NativeOutputTarget,
) -> Option<ShadowDraw> {
    let style = node.effects.shadow?;
    if style.color.alpha == 0
        || node.placement.geometry.width == 0
        || node.placement.geometry.height == 0
    {
        return None;
    }

    let geometry = node
        .placement
        .geometry
        .translated(-scene_viewport.x, -scene_viewport.y);
    let logical_box = geometry
        .translated(style.offset_x, style.offset_y)
        .inflated(style.spread);
    let logical_destination = logical_box.inflated(style.blur_radius);
    let destination = target.scale.physical_rect_cover(logical_destination);
    let box_physical = target.scale.physical_rect_round(logical_box);
    let clip = destination.intersection(target.viewport)?;
    let box_rect = Rect::new(
        box_physical.x.saturating_sub(destination.x),
        box_physical.y.saturating_sub(destination.y),
        box_physical.width,
        box_physical.height,
    );
    let blur_radius = target.scale.physical_length_round(style.blur_radius);
    let corner_radius = target
        .scale
        .physical_length_round(node.effects.corner_radius.saturating_add(style.spread))
        .min(box_rect.width.min(box_rect.height) / 2);

    Some(ShadowDraw {
        view_id: node.view_id,
        destination,
        clip,
        box_rect,
        color: style.color,
        opacity: node.effects.opacity,
        blur_radius,
        corner_radius,
    })
}
