use smithay::{
    input::pointer::CursorImageStatus,
    utils::{IsAlive, Logical, Point, Rectangle},
};
use tensor_util::{OutputScale, Rect};

use crate::render::CursorOverlay;

/// Cursor state remains in the protocol boundary because a client cursor
/// surface is thread-affine Smithay state. The renderer receives only the
/// value-only `CursorOverlay` it needs for the current output frame.
pub(crate) struct CursorState {
    image: CursorImageStatus,
    logical_size: u32,
    hide_when_typing: bool,
    hidden_for_typing: bool,
}

impl Default for CursorState {
    fn default() -> Self {
        Self {
            image: CursorImageStatus::default_named(),
            logical_size: 24,
            hide_when_typing: false,
            hidden_for_typing: false,
        }
    }
}

impl CursorState {
    pub(crate) fn configure(&mut self, size: u32, hide_when_typing: bool) {
        self.logical_size = size.max(1);
        self.hide_when_typing = hide_when_typing;
        if !hide_when_typing {
            self.hidden_for_typing = false;
        }
    }

    pub(crate) fn set_image(&mut self, image: CursorImageStatus) -> bool {
        if self.image == image {
            return false;
        }
        self.image = image;
        true
    }

    /// Hide the software cursor after a keyboard press when configured.
    pub(crate) fn note_keyboard_activity(&mut self) -> bool {
        if !self.hide_when_typing || self.hidden_for_typing {
            return false;
        }
        self.hidden_for_typing = true;
        true
    }

    /// Reveal the cursor again after pointer motion.
    pub(crate) fn note_pointer_activity(&mut self) -> bool {
        if !self.hidden_for_typing {
            return false;
        }
        self.hidden_for_typing = false;
        true
    }

    /// Produce the universal software fallback for a visible pointer source.
    /// Keeping the source state here is intentional: named-theme and client
    /// cursor rasters will feed this same overlay contract without giving the
    /// renderer a Wayland surface or a second coordinate system.
    pub(crate) fn overlay_for_output(
        &mut self,
        pointer: Point<f64, Logical>,
        output: Rectangle<i32, Logical>,
        scale: OutputScale,
        viewport: Rect,
    ) -> Option<CursorOverlay> {
        self.normalize_surface_liveness();
        if self.hidden_for_typing || matches!(&self.image, CursorImageStatus::Hidden) {
            return None;
        }
        let local_x = pointer.x - f64::from(output.loc.x);
        let local_y = pointer.y - f64::from(output.loc.y);
        if output.size.w <= 0
            || output.size.h <= 0
            || local_x < 0.0
            || local_y < 0.0
            || local_x >= f64::from(output.size.w)
            || local_y >= f64::from(output.size.h)
        {
            return None;
        }
        let x = scale.physical_coordinate_round(local_x)?;
        let y = scale.physical_coordinate_round(local_y)?;
        let size = scale.physical_length_round(self.logical_size).max(1);
        CursorOverlay::new(Rect::new(x, y, size, size), viewport)
    }

    fn normalize_surface_liveness(&mut self) {
        if matches!(&self.image, CursorImageStatus::Surface(surface) if !surface.alive()) {
            self.image = CursorImageStatus::default_named();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output() -> Rectangle<i32, Logical> {
        Rectangle::new((10, 20).into(), (100, 80).into())
    }

    #[test]
    fn visible_cursor_uses_output_local_physical_coordinates() {
        let mut cursor = CursorState::default();
        let overlay = cursor
            .overlay_for_output(
                (20.4, 30.4).into(),
                output(),
                OutputScale::from_f64(1.25).unwrap(),
                Rect::new(0, 0, 125, 100),
            )
            .unwrap();

        assert_eq!(overlay.destination, Rect::new(13, 13, 30, 30));
        assert_eq!(overlay.clip, overlay.destination);
    }

    #[test]
    fn hidden_and_off_output_cursors_do_not_create_an_overlay() {
        let mut cursor = CursorState::default();
        assert!(!cursor.set_image(CursorImageStatus::default_named()));
        assert!(cursor.set_image(CursorImageStatus::Hidden));
        assert_eq!(
            cursor.overlay_for_output(
                (20.0, 30.0).into(),
                output(),
                OutputScale::ONE,
                Rect::new(0, 0, 100, 80),
            ),
            None
        );
        cursor.set_image(CursorImageStatus::default_named());
        assert_eq!(
            cursor.overlay_for_output(
                (110.0, 30.0).into(),
                output(),
                OutputScale::ONE,
                Rect::new(0, 0, 100, 80),
            ),
            None
        );
    }
}
