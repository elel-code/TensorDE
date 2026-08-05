#[cfg(feature = "tty")]
use tensor_util::OutputScale;
use tensor_util::Rect;

use super::RuntimeState;

#[derive(Clone, Copy, Debug)]
pub(super) struct ToplevelGpuTarget {
    pub(super) output: crate::protocol::globals::output::OutputInstanceId,
    pub(super) region: Rect,
}

impl RuntimeState {
    #[cfg(feature = "tty")]
    pub(super) fn toplevel_gpu_capture_target(&self, geometry: Rect) -> Option<ToplevelGpuTarget> {
        self.outputs.values().find_map(|managed| {
            let output_geometry = self.space.output_geometry(&managed.output)?;
            let output_rect = Rect::new(
                output_geometry.loc.x,
                output_geometry.loc.y,
                u32::try_from(output_geometry.size.w).ok()?,
                u32::try_from(output_geometry.size.h).ok()?,
            );
            let viewport = Rect::new(
                0,
                0,
                u32::try_from(managed.descriptor.mode.width).ok()?,
                u32::try_from(managed.descriptor.mode.height).ok()?,
            );
            let region = toplevel_region_on_output(
                geometry,
                output_rect,
                managed.descriptor.scale,
                viewport,
            )?;
            Some(ToplevelGpuTarget {
                output: managed.output.instance_id(),
                region,
            })
        })
    }
}

#[cfg(feature = "tty")]
fn toplevel_region_on_output(
    geometry: Rect,
    output_geometry: Rect,
    scale: OutputScale,
    viewport: Rect,
) -> Option<Rect> {
    if !output_geometry.contains_rect(geometry) {
        return None;
    }
    let local = Rect::new(
        geometry.x.saturating_sub(output_geometry.x),
        geometry.y.saturating_sub(output_geometry.y),
        geometry.width,
        geometry.height,
    );
    let physical = scale.physical_rect_round(local);
    (physical.width > 0 && physical.height > 0 && viewport.contains_rect(physical))
        .then_some(physical)
}

#[cfg(all(test, feature = "tty"))]
mod tests {
    use super::*;

    #[test]
    fn crop_uses_the_output_fractional_scale_boundary() {
        let region = toplevel_region_on_output(
            Rect::new(110, 60, 101, 51),
            Rect::new(100, 50, 1536, 864),
            OutputScale::from_f64(1.25).unwrap(),
            Rect::new(0, 0, 1920, 1080),
        )
        .unwrap();
        assert_eq!(region, Rect::new(13, 13, 126, 63));
    }

    #[test]
    fn crop_rejects_cross_output_geometry() {
        assert_eq!(
            toplevel_region_on_output(
                Rect::new(1500, 20, 100, 100),
                Rect::new(0, 0, 1536, 864),
                OutputScale::from_f64(1.25).unwrap(),
                Rect::new(0, 0, 1920, 1080),
            ),
            None
        );
    }
}
