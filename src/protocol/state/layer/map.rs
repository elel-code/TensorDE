// Derived from Smithay's desktop layer implementation at commit c0aa71d.
// Smithay's copyright notice and MIT terms are in LICENSES/Smithay-MIT.txt.

use std::{collections::HashMap, num::Saturating};

use smithay::utils::{Logical, Physical, Point, Rectangle, Size};
use wayland_server::{Resource, backend::ObjectId, protocol::wl_surface::WlSurface};

use crate::protocol::globals::{
    compositor::{TraversalAction, with_states, with_surface_tree_downward},
    output::{Output, OutputInstanceId},
};

use super::super::{
    PopupManager,
    surfaces::surface_view,
    window::{surface_tree_bbox, surface_tree_under},
};
use super::{Anchor, ExclusiveZone, LayerSurface, LayerSurfaceState, WlrLayer};

impl LayerSurface {
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

    fn remove(&mut self, surface: &LayerSurface, popups: &PopupManager) -> bool {
        let Some(index) = self
            .layers
            .iter()
            .position(|entry| entry.surface == *surface)
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

        for exclusive_pass in [true, false] {
            for entry in &mut self.layers {
                let data = entry.surface.current();
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
                let size_changed = entry.surface.set_pending_server_size(size);
                let initial_configure_sent = entry.surface.initial_configure_sent();
                if size_changed && initial_configure_sent {
                    entry.surface.send_pending_configure();
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
    maps: HashMap<OutputInstanceId, LayerMap>,
    roots: HashMap<ObjectId, LayerRoot>,
}

#[derive(Debug)]
struct LayerRoot {
    surface: LayerSurface,
    output: Output,
}

impl LayerMaps {
    pub(super) fn map(
        &mut self,
        output: &Output,
        surface: LayerSurface,
        popups: &PopupManager,
    ) -> Result<(), &'static str> {
        let root = surface.wl_surface().id();
        if let Some(existing) = self.roots.get(&root) {
            return if existing.output == *output {
                Ok(())
            } else {
                Err("layer surface is already mapped to another output")
            };
        }
        let map = self
            .maps
            .entry(output.instance_id())
            .or_insert_with(|| LayerMap::new(output.clone()));
        map.push(surface.clone());
        map.arrange(popups);
        self.roots.insert(
            root,
            LayerRoot {
                surface,
                output: output.clone(),
            },
        );
        Ok(())
    }

    pub(super) fn unmap(&mut self, surface: &LayerSurface, popups: &PopupManager) -> bool {
        let Some(root) = self.roots.remove(&surface.wl_surface().id()) else {
            return false;
        };
        let output = root.output.instance_id();
        let Some(map) = self.maps.get_mut(&output) else {
            return false;
        };
        let removed = map.remove(&root.surface, popups);
        if map.layers.is_empty() {
            self.maps.remove(&output);
        }
        removed
    }

    pub(super) fn for_output(&self, output: &Output) -> Option<&LayerMap> {
        self.maps.get(&output.instance_id())
    }

    pub(super) fn for_output_mut(&mut self, output: &Output) -> Option<&mut LayerMap> {
        self.maps.get_mut(&output.instance_id())
    }

    pub(super) fn layer_and_output_for_root(
        &self,
        root: &WlSurface,
    ) -> Option<(&LayerSurface, &Output)> {
        let root = self.roots.get(&root.id())?;
        Some((&root.surface, &root.output))
    }

    pub(super) fn arrange_for_root(
        &mut self,
        root: &WlSurface,
        popups: &PopupManager,
    ) -> Option<(&LayerSurface, &Output)> {
        let root = self.roots.get(&root.id())?;
        self.maps
            .get_mut(&root.output.instance_id())?
            .arrange(popups);
        Some((&root.surface, &root.output))
    }

    pub(super) fn remove_output(
        &mut self,
        output: &Output,
        popups: &PopupManager,
    ) -> Vec<LayerSurface> {
        let Some(map) = self.maps.remove(&output.instance_id()) else {
            return Vec::new();
        };
        let mut removed = Vec::with_capacity(map.layers.len());
        for entry in map.layers {
            self.roots.remove(&entry.surface.wl_surface().id());
            update_output_membership(output, entry.surface.wl_surface(), false);
            for (popup, _) in popups.popups_for_surface(entry.surface.wl_surface()) {
                update_output_membership(output, popup.wl_surface(), false);
            }
            removed.push(entry.surface);
        }
        removed
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

fn effective_exclusive_zone(state: &LayerSurfaceState) -> ExclusiveZone {
    if matches!(state.exclusive_zone, ExclusiveZone::Exclusive(_))
        && effective_exclusive_edge(state).is_none()
    {
        ExclusiveZone::Neutral
    } else {
        state.exclusive_zone
    }
}

fn effective_exclusive_edge(state: &LayerSurfaceState) -> Option<Anchor> {
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
    use super::{Anchor, implied_exclusive_edge_for_anchor};

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
