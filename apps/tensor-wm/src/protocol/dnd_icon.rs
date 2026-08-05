use tensor_util::{LogicalPoint, LogicalRect, OutputScale, Point, Rect};
use wayland_server::{Resource, protocol::wl_surface::WlSurface};

use crate::render::CursorOverlay;

use super::globals::output::OutputInstanceId;

pub(in crate::protocol) const DND_ICON_SOURCE: u64 = u64::MAX;

#[derive(Debug, Default)]
pub(crate) struct DndIconState {
    surface: Option<WlSurface>,
    offset: Point,
    pub(in crate::protocol) outputs: Vec<OutputInstanceId>,
}

impl DndIconState {
    pub(in crate::protocol) fn set(&mut self, surface: Option<WlSurface>) {
        self.surface = surface;
        self.offset = Point::default();
        self.outputs.clear();
    }

    pub(in crate::protocol) fn surface(&self) -> Option<&WlSurface> {
        self.surface.as_ref().filter(|surface| surface.is_alive())
    }

    pub(in crate::protocol) fn uses_surface(&self, surface: &WlSurface) -> bool {
        self.surface.as_ref() == Some(surface)
    }

    pub(in crate::protocol) fn apply_delta(&mut self, delta: Point) {
        self.offset.x = self.offset.x.saturating_add(delta.x);
        self.offset.y = self.offset.y.saturating_add(delta.y);
    }

    pub(in crate::protocol) fn clear(&mut self) -> Option<WlSurface> {
        self.offset = Point::default();
        self.outputs.clear();
        self.surface.take()
    }

    pub(in crate::protocol) fn logical_bounds(
        &self,
        pointer: LogicalPoint<f64>,
        size: (i32, i32),
    ) -> Option<(f64, f64, f64, f64)> {
        let width = f64::from(size.0);
        let height = f64::from(size.1);
        if !pointer.x.is_finite() || !pointer.y.is_finite() || width <= 0.0 || height <= 0.0 {
            return None;
        }
        let left = pointer.x + f64::from(self.offset.x);
        let top = pointer.y + f64::from(self.offset.y);
        Some((left, top, left + width, top + height))
    }

    pub(in crate::protocol) fn overlay(
        &self,
        pointer: Option<LogicalPoint<f64>>,
        output: LogicalRect<i32>,
        scale: OutputScale,
        viewport: Rect,
        resolve: impl FnOnce(&WlSurface, OutputScale, Point) -> Option<super::cursor::CursorRaster>,
    ) -> Option<CursorOverlay> {
        let pointer = pointer?;
        let surface = self.surface()?;
        let local_x = pointer.x - f64::from(output.loc.x);
        let local_y = pointer.y - f64::from(output.loc.y);
        let x = scale.physical_coordinate_round(local_x)?;
        let y = scale.physical_coordinate_round(local_y)?;
        let raster = resolve(
            surface,
            scale,
            Point::new(
                self.offset.x.saturating_neg(),
                self.offset.y.saturating_neg(),
            ),
        )?;
        CursorOverlay::new(
            DND_ICON_SOURCE,
            Rect::new(
                x.saturating_sub(raster.hotspot.x),
                y.saturating_sub(raster.hotspot.y),
                raster.size.width,
                raster.size.height,
            ),
            viewport,
        )
        .map(|overlay| {
            overlay
                .below_cursors()
                .with_texture(crate::render::CursorTexture {
                    buffer_id: raster.buffer_id,
                    sample_transform: raster.sample_transform,
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_offsets_move_the_icon_relative_to_the_pointer() {
        let mut icon = DndIconState::default();
        icon.apply_delta(Point::new(7, -3));
        icon.apply_delta(Point::new(-2, 5));

        assert_eq!(
            icon.logical_bounds((100.5, 40.25).into(), (20, 10)),
            Some((105.5, 42.25, 125.5, 52.25))
        );
    }

    #[test]
    fn invalid_or_empty_geometry_is_not_visible() {
        let icon = DndIconState::default();
        assert_eq!(icon.logical_bounds((f64::NAN, 0.0).into(), (20, 10)), None);
        assert_eq!(icon.logical_bounds((0.0, 0.0).into(), (0, 10)), None);
    }
}
