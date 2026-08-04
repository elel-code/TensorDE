use vulkan_renderer::Rect2D as RendererRect2D;

use crate::render::{FrameSubmission, frame::ShadowDraw};

use super::{FrameRecordError, destination_to_ndc, validate_viewport};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(in crate::render::vulkan::frame) struct ShadowPushData {
    pub(super) destination: [f32; 4],
    pub(super) color: [f32; 4],
    pub(super) box_rect: [f32; 4],
    pub(super) shape: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub(in crate::render::vulkan::frame) struct PreparedShadowDraw {
    pub(super) view_id: crate::ecs::ViewId,
    pub(super) push: ShadowPushData,
    pub(super) scissor: RendererRect2D,
}

pub(in crate::render::vulkan::frame) fn prepare_shadow_draws(
    frame: &FrameSubmission,
) -> Result<Vec<PreparedShadowDraw>, FrameRecordError> {
    let viewport = validate_viewport(frame.target.viewport)?;
    frame
        .draw_plan
        .shadows()
        .iter()
        .map(|shadow| prepare_shadow(*shadow, viewport))
        .collect()
}

fn prepare_shadow(
    shadow: ShadowDraw,
    viewport: tensor_util::Rect,
) -> Result<PreparedShadowDraw, FrameRecordError> {
    let opacity = shadow.opacity.as_f32();
    let alpha = f32::from(shadow.color.alpha) / f32::from(u16::MAX) * opacity;
    Ok(PreparedShadowDraw {
        view_id: shadow.view_id,
        push: ShadowPushData {
            destination: destination_to_ndc(shadow.destination, viewport),
            color: [
                f32::from(shadow.color.red) / f32::from(u16::MAX),
                f32::from(shadow.color.green) / f32::from(u16::MAX),
                f32::from(shadow.color.blue) / f32::from(u16::MAX),
                alpha,
            ],
            box_rect: [
                shadow.box_rect.x as f32,
                shadow.box_rect.y as f32,
                shadow.box_rect.width as f32,
                shadow.box_rect.height as f32,
            ],
            shape: [
                shadow.blur_radius as f32 / 3.0,
                shadow.corner_radius as f32,
                shadow.destination.width as f32,
                shadow.destination.height as f32,
            ],
        },
        scissor: RendererRect2D::new(
            shadow.clip.x,
            shadow.clip.y,
            shadow.clip.width,
            shadow.clip.height,
        ),
    })
}

#[cfg(test)]
mod tests {
    use tensor_util::Rect;

    use crate::{
        ecs::ViewId,
        scene::{LinearRgba16, UnitFraction},
    };

    use super::*;

    #[test]
    fn push_is_premultiplication_ready_and_uses_bounded_gaussian_sigma() {
        let prepared = prepare_shadow(
            ShadowDraw {
                view_id: ViewId::new(7),
                destination: Rect::new(10, 5, 30, 20),
                clip: Rect::new(10, 5, 30, 20),
                box_rect: Rect::new(3, 3, 24, 14),
                color: LinearRgba16::new(u16::MAX, 0, 32_768, 32_768),
                opacity: UnitFraction::from_raw(32_768),
                blur_radius: 6,
                corner_radius: 4,
            },
            Rect::new(0, 0, 100, 50),
        )
        .unwrap();

        assert_eq!(prepared.view_id, ViewId::new(7));
        assert_eq!(prepared.push.box_rect, [3.0, 3.0, 24.0, 14.0]);
        assert_eq!(prepared.push.shape, [2.0, 4.0, 30.0, 20.0]);
        assert_eq!(prepared.push.color[0..3], [1.0, 0.0, 32_768.0 / 65_535.0]);
        assert_eq!(
            prepared.push.color[3],
            (32_768.0 / 65_535.0) * (32_768.0 / 65_535.0)
        );
    }
}
