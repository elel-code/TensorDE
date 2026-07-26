use smithay_client_toolkit::dispatch2::Dispatch2;
use wayland_client::globals::{BindError, GlobalList};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::{
    Event as FractionalScaleEvent, WpFractionalScaleV1,
};
use wayland_protocols::wp::viewporter::client::wp_viewport::WpViewport;
use wayland_protocols::wp::viewporter::client::wp_viewporter::WpViewporter;

use crate::LogicalSize;

const SCALE_DENOMINATOR: f64 = 120.0;

pub(crate) trait FractionalScaleHandler {
    fn preferred_scale(&mut self, surface: &WlSurface, factor: f64);
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ManagerData;

impl<D> Dispatch2<WpFractionalScaleManagerV1, D> for ManagerData {
    fn event(
        &self,
        _: &mut D,
        _: &WpFractionalScaleManagerV1,
        _: <WpFractionalScaleManagerV1 as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
        unreachable!("wp_fractional_scale_manager_v1 has no events");
    }
}

#[derive(Debug)]
pub(crate) struct SurfaceData {
    surface: WlSurface,
}

impl<D> Dispatch2<WpFractionalScaleV1, D> for SurfaceData
where
    D: FractionalScaleHandler,
{
    fn event(
        &self,
        state: &mut D,
        _: &WpFractionalScaleV1,
        event: FractionalScaleEvent,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
        if let FractionalScaleEvent::PreferredScale { scale } = event {
            state.preferred_scale(&self.surface, decode_scale(scale));
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ViewporterData;

impl<D> Dispatch2<WpViewporter, D> for ViewporterData {
    fn event(
        &self,
        _: &mut D,
        _: &WpViewporter,
        _: <WpViewporter as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
        unreachable!("wp_viewporter has no events");
    }
}

impl<D> Dispatch2<WpViewport, D> for ViewporterData {
    fn event(
        &self,
        _: &mut D,
        _: &WpViewport,
        _: <WpViewport as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
        unreachable!("wp_viewport has no events");
    }
}

#[derive(Debug)]
pub(crate) struct FractionalScaleManager {
    manager: WpFractionalScaleManagerV1,
    viewporter: WpViewporter,
}

impl FractionalScaleManager {
    pub(crate) fn bind<D>(
        globals: &GlobalList,
        queue_handle: &QueueHandle<D>,
    ) -> Result<Self, BindError>
    where
        D: Dispatch<WpFractionalScaleManagerV1, ManagerData>
            + Dispatch<WpViewporter, ViewporterData>
            + 'static,
    {
        let manager = globals.bind(queue_handle, 1..=1, ManagerData)?;
        let viewporter = match globals.bind(queue_handle, 1..=1, ViewporterData) {
            Ok(viewporter) => viewporter,
            Err(error) => {
                manager.destroy();
                return Err(error);
            }
        };
        Ok(Self {
            manager,
            viewporter,
        })
    }

    pub(crate) fn create_surface<D>(
        &self,
        surface: &WlSurface,
        queue_handle: &QueueHandle<D>,
    ) -> FractionalScaleSurface
    where
        D: Dispatch<WpFractionalScaleV1, SurfaceData>
            + Dispatch<WpViewport, ViewporterData>
            + 'static,
    {
        FractionalScaleSurface {
            fractional_scale: self.manager.get_fractional_scale(
                surface,
                queue_handle,
                SurfaceData {
                    surface: surface.clone(),
                },
            ),
            viewport: self
                .viewporter
                .get_viewport(surface, queue_handle, ViewporterData),
        }
    }
}

impl Drop for FractionalScaleManager {
    fn drop(&mut self) {
        if self.manager.is_alive() {
            self.manager.destroy();
        }
        if self.viewporter.is_alive() {
            self.viewporter.destroy();
        }
    }
}

#[derive(Debug)]
pub(crate) struct FractionalScaleSurface {
    fractional_scale: WpFractionalScaleV1,
    viewport: WpViewport,
}

impl FractionalScaleSurface {
    pub(crate) fn set_destination(&self, size: Option<LogicalSize>) {
        match size {
            Some(size) => self.viewport.set_destination(
                size.width.min(i32::MAX as u32) as i32,
                size.height.min(i32::MAX as u32) as i32,
            ),
            None => self.viewport.set_destination(-1, -1),
        }
    }
}

impl Drop for FractionalScaleSurface {
    fn drop(&mut self) {
        if self.fractional_scale.is_alive() {
            self.fractional_scale.destroy();
        }
        if self.viewport.is_alive() {
            self.viewport.destroy();
        }
    }
}

fn decode_scale(scale: u32) -> f64 {
    f64::from(scale) / SCALE_DENOMINATOR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_numerators_use_the_fixed_denominator() {
        assert_eq!(decode_scale(120), 1.0);
        assert_eq!(decode_scale(150), 1.25);
        assert_eq!(decode_scale(180), 1.5);
    }
}
