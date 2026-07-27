//! Tensor-owned window/output mapping and stacking.
//!
//! Smithay remains the protocol-object adapter, but compositor geometry and
//! ordering live here. Refresh reuses per-window overlap storage and does not
//! allocate or clone the mapped-output list on each pass.

use smithay::{
    output::Output,
    utils::{IsAlive, Logical, Point, Rectangle, Transform},
    wayland::compositor::{TraversalAction, with_surface_tree_downward},
};
use wayland_server::protocol::wl_surface::WlSurface;

use super::{PopupManager, ProtocolWindow, surfaces::surface_view};

#[derive(Debug)]
struct MappedOutput {
    output: Output,
    location: Point<i32, Logical>,
    geometry: Option<Rectangle<i32, Logical>>,
}

#[derive(Debug)]
struct OutputOverlap {
    output: Output,
    region: Rectangle<i32, Logical>,
}

#[derive(Debug)]
struct MappedWindow {
    window: ProtocolWindow,
    location: Point<i32, Logical>,
    outputs: Vec<OutputOverlap>,
}

pub(super) struct WindowHit<'a> {
    pub(super) window: &'a ProtocolWindow,
    pub(super) window_location: Point<i32, Logical>,
    pub(super) surface: WlSurface,
    pub(super) surface_location: Point<i32, Logical>,
}

impl MappedWindow {
    fn geometry(&self) -> Rectangle<i32, Logical> {
        let mut geometry = self.window.geometry();
        geometry.loc = self.location;
        geometry
    }

    fn bbox(&self, popups: &PopupManager) -> Rectangle<i32, Logical> {
        let geometry = self.window.geometry();
        let mut bbox = self.window.bbox_with_popups(popups);
        bbox.loc += self.location - geometry.loc;
        bbox
    }

    fn render_location(&self) -> Point<i32, Logical> {
        self.location - self.window.geometry().loc
    }
}

/// Mapped protocol windows in back-to-front order plus logical outputs.
#[derive(Debug, Default)]
pub(crate) struct WindowSpace {
    elements: Vec<MappedWindow>,
    outputs: Vec<MappedOutput>,
}

impl WindowSpace {
    pub(crate) fn map_element<P>(&mut self, window: ProtocolWindow, location: P, activate: bool)
    where
        P: Into<Point<i32, Logical>>,
    {
        let location = location.into();
        let mut mapped = self
            .elements
            .iter()
            .position(|entry| entry.window == window)
            .map(|position| self.elements.remove(position))
            .unwrap_or_else(|| MappedWindow {
                window: window.clone(),
                location,
                outputs: Vec::new(),
            });
        mapped.window = window;
        mapped.location = location;
        self.insert_element(self.elements.len(), mapped, activate);
    }

    pub(crate) fn raise_element(&mut self, window: &ProtocolWindow, activate: bool) {
        let Some(position) = self
            .elements
            .iter()
            .position(|entry| &entry.window == window)
        else {
            return;
        };
        let mapped = self.elements.remove(position);
        self.insert_element(self.elements.len(), mapped, activate);
    }

    pub(crate) fn raise_element_above(
        &mut self,
        window: &ProtocolWindow,
        reference: &ProtocolWindow,
        activate: bool,
    ) {
        let Some(position) = self
            .elements
            .iter()
            .position(|entry| &entry.window == window)
        else {
            return;
        };
        let Some(reference_position) = self
            .elements
            .iter()
            .position(|entry| &entry.window == reference)
        else {
            return;
        };
        let insertion = if position > reference_position {
            reference_position + 1
        } else {
            reference_position
        };
        let mapped = self.elements.remove(position);
        self.insert_element(insertion, mapped, activate);
    }

    fn insert_element(&mut self, position: usize, mapped: MappedWindow, activate: bool) {
        if activate {
            mapped.window.set_activated(true);
            for entry in &self.elements {
                entry.window.set_activated(false);
            }
        }
        self.elements.insert(position, mapped);
    }

    pub(crate) fn relocate_element<P>(&mut self, window: &ProtocolWindow, location: P)
    where
        P: Into<Point<i32, Logical>>,
    {
        if let Some(mapped) = self
            .elements
            .iter_mut()
            .find(|entry| &entry.window == window)
        {
            mapped.location = location.into();
        }
    }

    pub(crate) fn unmap_elem(&mut self, window: &ProtocolWindow, popups: &PopupManager) {
        let Some(position) = self
            .elements
            .iter()
            .position(|entry| &entry.window == window)
        else {
            return;
        };
        let mapped = self.elements.remove(position);
        for overlap in &mapped.outputs {
            leave_window_output(&mapped.window, &overlap.output, popups);
        }
    }

    pub(crate) fn elements(
        &self,
    ) -> impl DoubleEndedIterator<Item = &ProtocolWindow> + ExactSizeIterator {
        self.elements.iter().map(|entry| &entry.window)
    }

    pub(super) fn element_under<P, F>(
        &self,
        popups: &PopupManager,
        point: P,
        mut xwayland_dnd_active: F,
    ) -> Option<WindowHit<'_>>
    where
        P: Into<Point<f64, Logical>>,
        F: FnMut() -> bool,
    {
        let point = point.into();
        self.elements.iter().rev().find_map(|entry| {
            if !entry.bbox(popups).to_f64().contains(point) {
                return None;
            }
            let render_location = entry.render_location();
            let (surface, surface_location) = entry.window.surface_under(
                popups,
                point - render_location.to_f64(),
                &mut xwayland_dnd_active,
            )?;
            Some(WindowHit {
                window: &entry.window,
                window_location: render_location,
                surface,
                surface_location,
            })
        })
    }

    pub(crate) fn element_location(&self, window: &ProtocolWindow) -> Option<Point<i32, Logical>> {
        self.elements
            .iter()
            .find(|entry| &entry.window == window)
            .map(|entry| entry.location)
    }

    pub(crate) fn element_geometry(
        &self,
        window: &ProtocolWindow,
    ) -> Option<Rectangle<i32, Logical>> {
        self.elements
            .iter()
            .find(|entry| &entry.window == window)
            .map(MappedWindow::geometry)
    }

    pub(crate) fn map_output<P>(&mut self, output: &Output, location: P)
    where
        P: Into<Point<i32, Logical>>,
    {
        let location = location.into();
        let geometry = mapped_output_geometry(output, location);
        if let Some(mapped) = self
            .outputs
            .iter_mut()
            .find(|mapped| mapped.output == *output)
        {
            mapped.location = location;
            mapped.geometry = geometry;
        } else {
            self.outputs.push(MappedOutput {
                output: output.clone(),
                location,
                geometry,
            });
        }
    }

    pub(crate) fn refresh_output_geometry(&mut self, output: &Output) {
        if let Some(mapped) = self
            .outputs
            .iter_mut()
            .find(|mapped| mapped.output == *output)
        {
            mapped.geometry = mapped_output_geometry(output, mapped.location);
        }
    }

    pub(crate) fn outputs(&self) -> impl DoubleEndedIterator<Item = &Output> {
        self.outputs.iter().map(|mapped| &mapped.output)
    }

    pub(crate) fn unmap_output(&mut self, output: &Output, popups: &PopupManager) {
        let Some(position) = self
            .outputs
            .iter()
            .position(|mapped| mapped.output == *output)
        else {
            return;
        };
        self.outputs.remove(position);
        for mapped in &mut self.elements {
            if let Some(position) = mapped
                .outputs
                .iter()
                .position(|overlap| overlap.output == *output)
            {
                let overlap = mapped.outputs.remove(position);
                leave_window_output(&mapped.window, &overlap.output, popups);
            }
        }
    }

    pub(crate) fn output_geometry(&self, output: &Output) -> Option<Rectangle<i32, Logical>> {
        self.outputs
            .iter()
            .find(|mapped| mapped.output == *output)
            .and_then(|mapped| mapped.geometry)
    }

    pub(crate) fn output_under<P>(&self, point: P) -> impl Iterator<Item = &Output>
    where
        P: Into<Point<f64, Logical>>,
    {
        let point = point.into();
        self.outputs.iter().rev().filter_map(move |mapped| {
            mapped
                .geometry
                .is_some_and(|geometry| geometry.to_f64().contains(point))
                .then_some(&mapped.output)
        })
    }

    pub(crate) fn outputs_for_element<'a>(
        &'a self,
        window: &ProtocolWindow,
    ) -> impl Iterator<Item = &'a Output> {
        let overlaps = self
            .elements
            .iter()
            .find(|entry| &entry.window == window)
            .map(|entry| entry.outputs.as_slice())
            .unwrap_or_default();
        overlaps.iter().map(|overlap| &overlap.output)
    }

    pub(crate) fn refresh(&mut self, popups: &PopupManager) {
        self.elements.retain(|entry| entry.window.alive());
        let outputs = &self.outputs;
        for mapped in &mut self.elements {
            let bbox = mapped.bbox(popups);
            for output in outputs {
                let overlap = output
                    .geometry
                    .and_then(|geometry| geometry.intersection(bbox))
                    .map(|mut region| {
                        region.loc -= bbox.loc;
                        region
                    });
                let previous = mapped
                    .outputs
                    .iter()
                    .position(|entry| entry.output == output.output);
                match (previous, overlap) {
                    (Some(position), Some(region)) => mapped.outputs[position].region = region,
                    (None, Some(region)) => mapped.outputs.push(OutputOverlap {
                        output: output.output.clone(),
                        region,
                    }),
                    (Some(position), None) => {
                        let removed = mapped.outputs.remove(position);
                        leave_window_output(&mapped.window, &removed.output, popups);
                    }
                    (None, None) => {}
                }
            }
            mapped.outputs.retain(|overlap| {
                let retained = outputs.iter().any(|mapped| mapped.output == overlap.output);
                if !retained {
                    leave_window_output(&mapped.window, &overlap.output, popups);
                }
                retained
            });
            refresh_window_outputs(mapped, popups);
        }
        for output in outputs {
            output.output.cleanup();
        }
    }
}

fn mapped_output_geometry(
    output: &Output,
    location: Point<i32, Logical>,
) -> Option<Rectangle<i32, Logical>> {
    let transform: Transform = output.current_transform();
    output.current_mode().map(|mode| {
        Rectangle::new(
            location,
            transform
                .transform_size(mode.size)
                .to_f64()
                .to_logical(output.current_scale().fractional_scale())
                .to_i32_ceil(),
        )
    })
}

fn refresh_window_outputs(mapped: &MappedWindow, popups: &PopupManager) {
    let Some(root) = mapped.window.wl_surface() else {
        return;
    };
    for overlap in &mapped.outputs {
        update_surface_tree_output(&overlap.output, Some(overlap.region), &root);
        for (popup, location) in popups.popups_for_surface(&root) {
            let mut region = overlap.region;
            region.loc -= location;
            update_surface_tree_output(&overlap.output, Some(region), popup.wl_surface());
        }
    }
}

fn leave_window_output(window: &ProtocolWindow, output: &Output, popups: &PopupManager) {
    let Some(root) = window.wl_surface() else {
        return;
    };
    update_surface_tree_output(output, None, &root);
    for (popup, _) in popups.popups_for_surface(&root) {
        update_surface_tree_output(output, None, popup.wl_surface());
    }
}

fn update_surface_tree_output(
    output: &Output,
    output_overlap: Option<Rectangle<i32, Logical>>,
    surface: &WlSurface,
) {
    with_surface_tree_downward(
        surface,
        (Point::from((0, 0)), false),
        |_, states, (location, parent_unmapped)| {
            let mut location = *location;
            if *parent_unmapped {
                TraversalAction::DoChildren((location, true))
            } else if let Some(surface_view) = surface_view(states) {
                location += Point::from(surface_view.offset);
                TraversalAction::DoChildren((location, false))
            } else {
                TraversalAction::DoChildren((location, true))
            }
        },
        |surface, states, (location, parent_unmapped)| {
            let mut location = *location;
            if *parent_unmapped {
                output.leave(surface);
                return;
            }
            let Some(output_overlap) = output_overlap else {
                output.leave(surface);
                return;
            };
            if let Some(surface_view) = surface_view(states) {
                location += Point::from(surface_view.offset);
                let surface_rectangle = Rectangle::new(location, surface_view.size.into());
                if output_overlap.overlaps(surface_rectangle) {
                    output.enter(surface);
                } else {
                    output.leave(surface);
                }
            } else {
                output.leave(surface);
            }
        },
        |_, _, _| true,
    );
}

#[cfg(test)]
mod tests {
    use smithay::{
        output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
        utils::Transform,
    };

    use super::{PopupManager, WindowSpace};

    fn output(name: &str, size: (i32, i32), scale: f64) -> Output {
        let output = Output::new(
            name.to_owned(),
            PhysicalProperties {
                size: (0, 0).into(),
                subpixel: Subpixel::Unknown,
                make: "Tensor".to_owned(),
                model: name.to_owned(),
                serial_number: name.to_owned(),
            },
        );
        let mode = Mode {
            size: size.into(),
            refresh: 60_000,
        };
        output.add_mode(mode);
        output.change_current_state(
            Some(mode),
            Some(Transform::Normal),
            Some(Scale::Fractional(scale)),
            None,
        );
        output
    }

    #[test]
    fn output_geometry_uses_tensor_location_and_fractional_scale() {
        let output = output("one", (1920, 1080), 1.5);
        let mut space = WindowSpace::default();
        space.map_output(&output, (10, 20));

        let geometry = space.output_geometry(&output).unwrap();
        assert_eq!(geometry.loc, (10, 20).into());
        assert_eq!(geometry.size, (1280, 720).into());

        output.change_current_state(None, None, Some(Scale::Fractional(2.0)), None);
        space.refresh_output_geometry(&output);
        assert_eq!(
            space.output_geometry(&output).unwrap().size,
            (960, 540).into()
        );
    }

    #[test]
    fn last_mapped_overlapping_output_wins_hit_test_order() {
        let first = output("first", (100, 100), 1.0);
        let second = output("second", (100, 100), 1.0);
        let mut space = WindowSpace::default();
        space.map_output(&first, (0, 0));
        space.map_output(&second, (0, 0));

        assert_eq!(space.output_under((5.0, 5.0)).next(), Some(&second));
        space.unmap_output(&second, &PopupManager::default());
        assert_eq!(space.output_under((5.0, 5.0)).next(), Some(&first));
    }
}
