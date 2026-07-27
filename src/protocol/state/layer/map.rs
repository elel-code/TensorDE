// Derived from Smithay's desktop layer implementation at commit c0aa71d.
// Smithay's copyright notice and MIT terms are in LICENSES/Smithay-MIT.txt.

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    num::Saturating,
    rc::Rc,
};

use smithay::{
    utils::{Logical, Physical, Point, Rectangle, Size},
    wayland::{
        compositor::{TraversalAction, with_states, with_surface_tree_downward},
        shell::wlr_layer::{
            Anchor, ExclusiveZone, KeyboardInteractivity, Layer as WlrLayer,
            LayerSurface as WlrLayerSurface, LayerSurfaceCachedState, LayerSurfaceData,
        },
    },
};
use wayland_server::{Resource, protocol::wl_surface::WlSurface};

use crate::protocol::globals::output::Output;

use super::super::{
    PopupManager,
    surfaces::surface_view,
    window::{surface_tree_bbox, surface_tree_under},
};

#[derive(Clone, Debug)]
pub(in crate::protocol::state) struct LayerSurface(Rc<LayerSurfaceInner>);

#[derive(Debug)]
struct LayerSurfaceInner {
    surface: WlrLayerSurface,
    _namespace: String,
    view_id: u64,
}

impl PartialEq for LayerSurface {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for LayerSurface {}

impl LayerSurface {
    pub(super) fn new(surface: WlrLayerSurface, namespace: String) -> Self {
        let mut hasher = DefaultHasher::new();
        surface.wl_surface().id().hash(&mut hasher);
        let view_id = 0xC000_0000_0000_0000 | (hasher.finish() & 0x0FFF_FFFF_FFFF_FFFF);
        Self(Rc::new(LayerSurfaceInner {
            surface,
            _namespace: namespace,
            view_id,
        }))
    }

    pub(super) fn protocol(&self) -> &WlrLayerSurface {
        &self.0.surface
    }

    pub(super) fn wl_surface(&self) -> &WlSurface {
        self.0.surface.wl_surface()
    }

    pub(super) fn alive(&self) -> bool {
        self.0.surface.alive()
    }

    pub(super) fn cached_state(&self) -> LayerSurfaceCachedState {
        self.0.surface.with_cached_state(Clone::clone)
    }

    pub(super) fn layer(&self) -> WlrLayer {
        self.0.surface.with_cached_state(|state| state.layer)
    }

    pub(super) fn can_receive_keyboard_focus(&self) -> bool {
        self.0.surface.with_cached_state(|state| {
            matches!(
                state.keyboard_interactivity,
                KeyboardInteractivity::Exclusive | KeyboardInteractivity::OnDemand
            )
        })
    }

    pub(super) fn geometry(&self) -> Rectangle<i32, Logical> {
        with_states(self.wl_surface(), |states| {
            surface_view(states).map(|view| Rectangle {
                loc: view.offset.into(),
                size: view.size.into(),
            })
        })
        .unwrap_or_default()
    }

    fn bbox_with_popups(&self, popups: &PopupManager) -> Rectangle<i32, Logical> {
        let mut bbox = surface_tree_bbox(self.wl_surface(), (0, 0));
        for (popup, location) in popups.popups_for_surface(self.wl_surface()) {
            bbox = bbox.merge(surface_tree_bbox(
                popup.wl_surface(),
                location - popup.geometry().loc,
            ));
        }
        bbox
    }

    pub(super) fn surface_under(
        &self,
        popups: &PopupManager,
        point: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<i32, Logical>)> {
        for (popup, location) in popups.popups_for_surface(self.wl_surface()) {
            if let Some(hit) =
                surface_tree_under(popup.wl_surface(), point, location - popup.geometry().loc)
            {
                return Some(hit);
            }
        }
        surface_tree_under(self.wl_surface(), point, (0, 0))
    }

    pub(super) fn view_id(&self) -> u64 {
        self.0.view_id
    }
}

#[derive(Debug)]
struct LayerEntry {
    surface: LayerSurface,
    location: Point<i32, Logical>,
}

#[derive(Debug)]
pub(super) struct LayerMap {
    output: Output,
    layers: Vec<LayerEntry>,
    zone: Rectangle<i32, Logical>,
}

impl LayerMap {
    fn new(output: Output) -> Self {
        let zone = output_rectangle(&output);
        Self {
            output,
            layers: Vec::new(),
            zone,
        }
    }

    fn push(&mut self, surface: LayerSurface) {
        self.layers.push(LayerEntry {
            surface,
            location: (0, 0).into(),
        });
    }

    fn remove(&mut self, surface: &WlrLayerSurface, popups: &PopupManager) -> bool {
        let Some(index) = self
            .layers
            .iter()
            .position(|entry| entry.surface.protocol() == surface)
        else {
            return false;
        };
        let entry = self.layers.remove(index);
        update_output_membership(&self.output, entry.surface.wl_surface(), false);
        for (popup, _) in popups.popups_for_surface(entry.surface.wl_surface()) {
            update_output_membership(&self.output, popup.wl_surface(), false);
        }
        self.arrange(popups);
        true
    }

    pub(super) fn non_exclusive_zone(&self) -> Rectangle<i32, Logical> {
        self.zone
    }

    pub(super) fn layer_geometry(&self, layer: &LayerSurface) -> Option<Rectangle<i32, Logical>> {
        let entry = self.layers.iter().find(|entry| entry.surface == *layer)?;
        let mut geometry = layer.geometry();
        geometry.loc += entry.location;
        Some(geometry)
    }

    pub(super) fn layer_under(
        &self,
        popups: &PopupManager,
        band: WlrLayer,
        point: Point<f64, Logical>,
    ) -> Option<&LayerSurface> {
        self.layers
            .iter()
            .rev()
            .filter(|entry| entry.surface.layer() == band)
            .find(|entry| {
                let mut bbox = entry.surface.bbox_with_popups(popups);
                bbox.loc += entry.location;
                bbox.to_f64().contains(point)
            })
            .map(|entry| &entry.surface)
    }

    pub(super) fn layers(&self) -> impl DoubleEndedIterator<Item = &LayerSurface> {
        self.layers.iter().map(|entry| &entry.surface)
    }

    pub(super) fn layers_on(
        &self,
        band: WlrLayer,
    ) -> impl DoubleEndedIterator<Item = &LayerSurface> {
        self.layers
            .iter()
            .map(|entry| &entry.surface)
            .filter(move |surface| surface.layer() == band)
    }

    pub(super) fn layer_for_root(&self, surface: &WlSurface) -> Option<&LayerSurface> {
        self.layers
            .iter()
            .find(|entry| entry.surface.wl_surface() == surface)
            .map(|entry| &entry.surface)
    }

    pub(super) fn arrange(&mut self, popups: &PopupManager) -> bool {
        let output_rect = output_rectangle(&self.output);
        let mut zone: Rectangle<Saturating<i32>, Logical> = Rectangle::new(
            Point::new(Saturating(output_rect.loc.x), Saturating(output_rect.loc.y)),
            Size::new(
                Saturating(output_rect.size.w),
                Saturating(output_rect.size.h),
            ),
        );
        let mut changed = false;

        self.layers.retain(|entry| entry.surface.alive());
        for exclusive_pass in [true, false] {
            for entry in &mut self.layers {
                let data = entry.surface.cached_state();
                let exclusive =
                    matches!(effective_exclusive_zone(&data), ExclusiveZone::Exclusive(_));
                if exclusive != exclusive_pass {
                    continue;
                }

                update_output_membership(&self.output, entry.surface.wl_surface(), true);
                for (popup, _) in popups.popups_for_surface(entry.surface.wl_surface()) {
                    update_output_membership(&self.output, popup.wl_surface(), true);
                }

                let mut source = match data.exclusive_zone {
                    ExclusiveZone::Exclusive(_) | ExclusiveZone::Neutral => zone,
                    ExclusiveZone::DontCare => Rectangle::new(
                        Point::new(Saturating(output_rect.loc.x), Saturating(output_rect.loc.y)),
                        Size::new(
                            Saturating(output_rect.size.w),
                            Saturating(output_rect.size.h),
                        ),
                    ),
                };

                if data.anchor.contains(Anchor::LEFT) {
                    source.size.w -= data.margin.left;
                }
                if data.anchor.contains(Anchor::RIGHT) {
                    source.size.w -= data.margin.right;
                }
                if data.anchor.contains(Anchor::TOP) {
                    source.size.h -= data.margin.top;
                }
                if data.anchor.contains(Anchor::BOTTOM) {
                    source.size.h -= data.margin.bottom;
                }

                let mut size: Size<Saturating<i32>, Logical> =
                    Size::new(Saturating(data.size.w), Saturating(data.size.h));
                size.w = size.w.min(source.size.w);
                size.h = size.h.min(source.size.h);
                if size.w.0 == 0 {
                    size.w.0 = source.size.w.0 / 2;
                }
                if size.h.0 == 0 {
                    size.h.0 = source.size.h.0 / 2;
                }
                if data.anchor.anchored_horizontally() {
                    size.w = source.size.w;
                }
                if data.anchor.anchored_vertically() {
                    size.h = source.size.h;
                }

                let x = if data.anchor.contains(Anchor::LEFT) {
                    source.loc.x + Saturating(data.margin.left)
                } else if data.anchor.contains(Anchor::RIGHT) {
                    source.loc.x + (source.size.w - size.w)
                } else {
                    source.loc.x + Saturating((source.size.w.0 / 2) - (size.w.0 / 2))
                };
                let y = if data.anchor.contains(Anchor::TOP) {
                    source.loc.y + Saturating(data.margin.top)
                } else if data.anchor.contains(Anchor::BOTTOM) {
                    source.loc.y + (source.size.h - size.h)
                } else {
                    source.loc.y + Saturating((source.size.h.0 / 2) - (size.h.0 / 2))
                };
                let location = Point::from((x.0, y.0));

                if let ExclusiveZone::Exclusive(amount) = data.exclusive_zone {
                    let amount = Saturating(amount as i32);
                    match effective_exclusive_edge(&data) {
                        Some(Anchor::TOP) => {
                            let amount = amount + Saturating(data.margin.top);
                            zone.loc.y += amount;
                            zone.size.h -= amount;
                        }
                        Some(Anchor::BOTTOM) => {
                            zone.size.h -= amount + Saturating(data.margin.bottom);
                        }
                        Some(Anchor::LEFT) => {
                            let amount = amount + Saturating(data.margin.left);
                            zone.loc.x += amount;
                            zone.size.w -= amount;
                        }
                        Some(Anchor::RIGHT) => {
                            zone.size.w -= amount + Saturating(data.margin.right);
                        }
                        Some(_) => unreachable!(),
                        None => {}
                    }
                }

                let size = Size::new(size.w.0.max(0), size.h.0.max(0));
                let size_changed = entry.surface.protocol().with_pending_state(|state| {
                    state.size.replace(size).is_none_or(|old| old != size)
                });
                let initial_configure_sent = with_states(entry.surface.wl_surface(), |states| {
                    states
                        .data_map
                        .get::<LayerSurfaceData>()
                        .is_some_and(|data| data.lock().unwrap().initial_configure_sent)
                });
                if size_changed && initial_configure_sent {
                    entry.surface.protocol().send_pending_configure();
                }
                changed |= size_changed || entry.location != location;
                entry.location = location;
            }
        }

        self.zone = Rectangle::new(
            Point::new(zone.loc.x.0, zone.loc.y.0),
            Size::new(zone.size.w.0.max(0), zone.size.h.0.max(0)),
        );
        changed
    }
}

#[derive(Debug, Default)]
pub(in crate::protocol::state) struct LayerMaps {
    maps: Vec<LayerMap>,
}

impl LayerMaps {
    pub(super) fn map(
        &mut self,
        output: &Output,
        surface: WlrLayerSurface,
        namespace: String,
        popups: &PopupManager,
    ) -> Result<(), &'static str> {
        if let Some(existing) = self
            .maps
            .iter()
            .find(|map| map.layer_for_root(surface.wl_surface()).is_some())
        {
            return if existing.output == *output {
                Ok(())
            } else {
                Err("layer surface is already mapped to another output")
            };
        }
        let map = if let Some(index) = self.maps.iter().position(|map| map.output == *output) {
            &mut self.maps[index]
        } else {
            self.maps.push(LayerMap::new(output.clone()));
            self.maps.last_mut().unwrap()
        };
        map.push(LayerSurface::new(surface, namespace));
        map.arrange(popups);
        Ok(())
    }

    pub(super) fn unmap(&mut self, surface: &WlrLayerSurface, popups: &PopupManager) -> bool {
        let Some(index) = self
            .maps
            .iter()
            .position(|map| map.layer_for_root(surface.wl_surface()).is_some())
        else {
            return false;
        };
        let removed = self.maps[index].remove(surface, popups);
        if self.maps[index].layers.is_empty() {
            self.maps.remove(index);
        }
        removed
    }

    pub(super) fn for_output(&self, output: &Output) -> Option<&LayerMap> {
        self.maps.iter().find(|map| map.output == *output)
    }

    pub(super) fn for_output_mut(&mut self, output: &Output) -> Option<&mut LayerMap> {
        self.maps.iter_mut().find(|map| map.output == *output)
    }

    pub(super) fn layer_and_output_for_root(
        &self,
        root: &WlSurface,
    ) -> Option<(&LayerSurface, &Output)> {
        self.maps
            .iter()
            .find_map(|map| map.layer_for_root(root).map(|layer| (layer, &map.output)))
    }

    pub(super) fn remove_output(&mut self, output: &Output, popups: &PopupManager) {
        let Some(index) = self.maps.iter().position(|map| map.output == *output) else {
            return;
        };
        let map = self.maps.remove(index);
        for entry in map.layers {
            update_output_membership(output, entry.surface.wl_surface(), false);
            for (popup, _) in popups.popups_for_surface(entry.surface.wl_surface()) {
                update_output_membership(output, popup.wl_surface(), false);
            }
        }
    }
}

fn output_rectangle(output: &Output) -> Rectangle<i32, Logical> {
    let snapshot = output.snapshot();
    Rectangle::from_size(
        snapshot
            .mode
            .map(|mode| {
                let logical = Size::<i32, Physical>::from(mode.size())
                    .to_f64()
                    .to_logical(snapshot.scale.as_f64())
                    .to_i32_round();
                super::super::smithay_transform(snapshot.transform).transform_size(logical)
            })
            .unwrap_or_default(),
    )
}

fn effective_exclusive_zone(state: &LayerSurfaceCachedState) -> ExclusiveZone {
    if matches!(state.exclusive_zone, ExclusiveZone::Exclusive(_))
        && effective_exclusive_edge(state).is_none()
    {
        ExclusiveZone::Neutral
    } else {
        state.exclusive_zone
    }
}

fn effective_exclusive_edge(state: &LayerSurfaceCachedState) -> Option<Anchor> {
    state
        .exclusive_edge
        .or_else(|| implied_exclusive_edge_for_anchor(state.anchor))
}

fn implied_exclusive_edge_for_anchor(anchor: Anchor) -> Option<Anchor> {
    match anchor.bits().count_ones() {
        0 | 2 | 4 => None,
        1 => Some(anchor),
        3 => Some(match anchor.complement() {
            Anchor::TOP => Anchor::BOTTOM,
            Anchor::BOTTOM => Anchor::TOP,
            Anchor::LEFT => Anchor::RIGHT,
            Anchor::RIGHT => Anchor::LEFT,
            _ => unreachable!(),
        }),
        _ => unreachable!(),
    }
}

fn update_output_membership(output: &Output, root: &WlSurface, enter: bool) {
    with_surface_tree_downward(
        root,
        (),
        |_, _, _| TraversalAction::DoChildren(()),
        |surface, _, _| {
            if enter {
                output.enter(surface);
            } else {
                output.leave(surface);
            }
        },
        |_, _, _| true,
    );
}

#[cfg(test)]
mod tests {
    use super::implied_exclusive_edge_for_anchor;
    use smithay::wayland::shell::wlr_layer::Anchor;

    #[test]
    fn implied_edge_matches_wlr_anchor_shapes() {
        assert_eq!(
            implied_exclusive_edge_for_anchor(Anchor::TOP),
            Some(Anchor::TOP)
        );
        assert_eq!(
            implied_exclusive_edge_for_anchor(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT),
            Some(Anchor::TOP)
        );
        assert_eq!(
            implied_exclusive_edge_for_anchor(Anchor::TOP | Anchor::BOTTOM),
            None
        );
        assert_eq!(implied_exclusive_edge_for_anchor(Anchor::empty()), None);
    }
}
