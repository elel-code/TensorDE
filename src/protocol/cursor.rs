use cursor_icon::CursorIcon;
use tensor_util::{LogicalPoint, LogicalRect, OutputScale, Rect};
use wayland_server::{Resource, protocol::wl_surface::WlSurface};

use crate::render::{CursorOverlay, CursorOverlays};

const MAX_TABLET_CURSORS: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::protocol) enum CursorImage {
    Hidden,
    Named(CursorIcon),
    Surface(WlSurface),
}

impl CursorImage {
    pub(in crate::protocol) const fn default_named() -> Self {
        Self::Named(CursorIcon::Default)
    }
}

/// Cursor state remains in the protocol boundary because a client cursor
/// surface is a thread-affine Wayland object. The renderer receives only the
/// fixed-capacity value-only cursor batch needed for the current output frame.
pub(crate) struct CursorState {
    image: CursorImage,
    tablets: Vec<TabletCursor>,
    logical_size: u32,
    hide_when_typing: bool,
    hidden_for_typing: bool,
}

#[derive(Clone)]
struct TabletCursor {
    tool: tensor_event::TabletToolId,
    image: CursorImage,
    location: LogicalPoint<f64>,
}

impl Default for CursorState {
    fn default() -> Self {
        Self {
            image: CursorImage::default_named(),
            tablets: Vec::with_capacity(MAX_TABLET_CURSORS),
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

    pub(in crate::protocol) fn set_image(&mut self, image: CursorImage) -> bool {
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

    pub(in crate::protocol) fn note_tablet_activity(
        &mut self,
        tool: tensor_event::TabletToolId,
        location: LogicalPoint<f64>,
    ) -> bool {
        if let Some(tablet) = self.tablets.iter_mut().find(|tablet| tablet.tool == tool) {
            let changed = tablet.location != location;
            tablet.location = location;
            return changed;
        }
        if self.tablets.len() == MAX_TABLET_CURSORS {
            tracing::warn!(tool = tool.get(), "tablet cursor capacity exceeded");
            return false;
        }
        let index = self.tablets.partition_point(|tablet| tablet.tool < tool);
        self.tablets.insert(
            index,
            TabletCursor {
                tool,
                image: CursorImage::default_named(),
                location,
            },
        );
        true
    }

    pub(in crate::protocol) fn set_tablet_image(
        &mut self,
        tool: tensor_event::TabletToolId,
        image: CursorImage,
    ) -> bool {
        let Some(tablet) = self.tablets.iter_mut().find(|tablet| tablet.tool == tool) else {
            return false;
        };
        if tablet.image == image {
            return false;
        }
        tablet.image = image;
        true
    }

    pub(in crate::protocol) fn clear_tablet(&mut self, tool: tensor_event::TabletToolId) -> bool {
        let Some(index) = self.tablets.iter().position(|tablet| tablet.tool == tool) else {
            return false;
        };
        self.tablets.remove(index);
        true
    }

    /// Produce the universal software fallback for a visible pointer source.
    /// Keeping the source state here is intentional: named-theme and client
    /// cursor rasters will feed this same overlay contract without giving the
    /// renderer a Wayland surface or a second coordinate system.
    pub(crate) fn overlays_for_output(
        &mut self,
        pointer: Option<LogicalPoint<f64>>,
        output: LogicalRect<i32>,
        scale: OutputScale,
        viewport: Rect,
    ) -> CursorOverlays {
        self.normalize_surface_liveness();
        let mut overlays = CursorOverlays::default();
        if !self.hidden_for_typing
            && let Some(pointer) = pointer
            && let Some(overlay) = self.overlay(0, pointer, &self.image, output, scale, viewport)
        {
            assert!(overlays.push(overlay), "pointer cursor has a reserved slot");
        }
        for tablet in &self.tablets {
            if let Some(overlay) = self.overlay(
                tablet.tool.get(),
                tablet.location,
                &tablet.image,
                output,
                scale,
                viewport,
            ) {
                assert!(
                    overlays.push(overlay),
                    "tablet cursor capacity matches tools"
                );
            }
        }
        overlays
    }

    fn overlay(
        &self,
        source: u64,
        location: LogicalPoint<f64>,
        image: &CursorImage,
        output: LogicalRect<i32>,
        scale: OutputScale,
        viewport: Rect,
    ) -> Option<CursorOverlay> {
        if matches!(image, CursorImage::Hidden) {
            return None;
        }
        let local_x = location.x - f64::from(output.loc.x);
        let local_y = location.y - f64::from(output.loc.y);
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
        CursorOverlay::new(source, Rect::new(x, y, size, size), viewport)
    }

    fn normalize_surface_liveness(&mut self) {
        if matches!(&self.image, CursorImage::Surface(surface) if !surface.is_alive()) {
            self.image = CursorImage::default_named();
        }
        for tablet in &mut self.tablets {
            if matches!(&tablet.image, CursorImage::Surface(surface) if !surface.is_alive()) {
                tablet.image = CursorImage::default_named();
            }
        }
    }

    #[cfg(test)]
    pub(in crate::protocol) fn image(&self) -> &CursorImage {
        &self.image
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output() -> LogicalRect<i32> {
        LogicalRect::new((10, 20).into(), (100, 80).into())
    }

    #[test]
    fn visible_cursor_uses_output_local_physical_coordinates() {
        let mut cursor = CursorState::default();
        let overlays = cursor.overlays_for_output(
            Some((20.4, 30.4).into()),
            output(),
            OutputScale::from_f64(1.25).unwrap(),
            Rect::new(0, 0, 125, 100),
        );
        let overlay = overlays.as_slice()[0];

        assert_eq!(overlay.destination, Rect::new(13, 13, 30, 30));
        assert_eq!(overlay.clip, overlay.destination);
    }

    #[test]
    fn hidden_and_off_output_cursors_do_not_create_an_overlay() {
        let mut cursor = CursorState::default();
        assert!(!cursor.set_image(CursorImage::default_named()));
        assert!(cursor.set_image(CursorImage::Hidden));
        assert_eq!(
            cursor
                .overlays_for_output(
                    Some((20.0, 30.0).into()),
                    output(),
                    OutputScale::ONE,
                    Rect::new(0, 0, 100, 80),
                )
                .as_slice(),
            []
        );
        cursor.set_image(CursorImage::default_named());
        assert_eq!(
            cursor
                .overlays_for_output(
                    Some((110.0, 30.0).into()),
                    output(),
                    OutputScale::ONE,
                    Rect::new(0, 0, 100, 80),
                )
                .as_slice(),
            []
        );
    }

    #[test]
    fn pointer_and_tablet_cursors_remain_independent() {
        let mut cursor = CursorState::default();
        assert!(
            cursor.note_tablet_activity(tensor_event::TabletToolId::new(1), (40.0, 50.0).into())
        );
        let overlays = cursor.overlays_for_output(
            Some((20.0, 30.0).into()),
            output(),
            OutputScale::ONE,
            Rect::new(0, 0, 100, 80),
        );
        assert_eq!(overlays.as_slice().len(), 2);
        assert_eq!(
            overlays.as_slice()[0].destination,
            Rect::new(10, 10, 24, 24)
        );
        assert_eq!(
            overlays.as_slice()[1].destination,
            Rect::new(30, 30, 24, 24)
        );
        assert!(cursor.clear_tablet(tensor_event::TabletToolId::new(1)));
        assert_eq!(
            cursor
                .overlays_for_output(
                    Some((20.0, 30.0).into()),
                    output(),
                    OutputScale::ONE,
                    Rect::new(0, 0, 100, 80),
                )
                .as_slice()
                .len(),
            1
        );
    }
}
