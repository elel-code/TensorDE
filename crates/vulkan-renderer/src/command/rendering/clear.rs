use vulkanalia::{prelude::v1_4::*, vk};

use super::RenderingEncoder;
use crate::{Error, Result};

impl RenderingEncoder<'_> {
    /// Clears bounded rectangles of one color attachment while preserving the
    /// rest of the attachment.
    ///
    /// This is the typed partial-repaint counterpart to `LoadOp::Clear`: the
    /// rendering scope must use `LoadOp::Load`, and callers clear only regions
    /// whose prior contents are invalid according to their buffer history.
    pub fn clear_color_attachment(
        &mut self,
        color_attachment: u32,
        color: [f32; 4],
        rects: &[crate::Rect2D],
    ) -> Result<()> {
        if !color.into_iter().all(f32::is_finite) {
            return Err(Error::Validation(
                "color attachment clear value must be finite".into(),
            ));
        }
        let attachment_index = usize::try_from(color_attachment).map_err(|_| {
            Error::Validation("color attachment clear index is not representable".into())
        })?;
        if self
            .color_formats
            .get(attachment_index)
            .is_none_or(Option::is_none)
        {
            return Err(Error::Validation(
                "color attachment clear index does not name an active attachment".into(),
            ));
        }
        for rect in rects {
            validate_clear_rect(*rect, self.render_area)?;
        }
        let attachment = vk::ClearAttachment::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .color_attachment(color_attachment)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue { float32: color },
            })
            .build();
        for rect in rects {
            let clear_rect = vk::ClearRect::builder()
                .rect(rect.to_vk())
                .base_array_layer(0)
                .layer_count(self.layer_count)
                .build();
            unsafe {
                self.encoder.owner.device.cmd_clear_attachments(
                    self.encoder.raw(),
                    &[attachment],
                    &[clear_rect],
                );
            }
        }
        Ok(())
    }
}

fn validate_clear_rect(rect: crate::Rect2D, render_area: crate::Rect2D) -> Result<()> {
    let rect_right = i64::from(rect.origin.x) + i64::from(rect.extent.width);
    let rect_bottom = i64::from(rect.origin.y) + i64::from(rect.extent.height);
    let area_right = i64::from(render_area.origin.x) + i64::from(render_area.extent.width);
    let area_bottom = i64::from(render_area.origin.y) + i64::from(render_area.extent.height);
    if rect.extent.width == 0
        || rect.extent.height == 0
        || rect.origin.x < render_area.origin.x
        || rect.origin.y < render_area.origin.y
        || rect_right > area_right
        || rect_bottom > area_bottom
    {
        return Err(Error::Validation(
            "color attachment clear rectangle must be non-empty and contained by the render area"
                .into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_clear_rectangles_must_stay_inside_the_render_area() {
        let area = crate::Rect2D::new(10, 20, 100, 80);
        assert!(validate_clear_rect(crate::Rect2D::new(10, 20, 100, 80), area).is_ok());
        assert!(validate_clear_rect(crate::Rect2D::new(20, 30, 10, 10), area).is_ok());
        assert!(validate_clear_rect(crate::Rect2D::new(9, 20, 10, 10), area).is_err());
        assert!(validate_clear_rect(crate::Rect2D::new(100, 90, 20, 20), area).is_err());
        assert!(validate_clear_rect(crate::Rect2D::new(10, 20, 0, 10), area).is_err());
    }
}
