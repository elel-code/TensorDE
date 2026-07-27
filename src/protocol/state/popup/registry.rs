// Derived from Smithay's popup manager implementation at commit c0aa71d.
// Smithay's copyright notice and MIT terms are in LICENSES/Smithay-MIT.txt.

use smithay::utils::{Logical, Point, Rectangle};
use thiserror::Error;
use tracing::trace;
use wayland_server::{Resource, protocol::wl_surface::WlSurface};

use super::grab::{PopupGrab, PopupGrabError, PopupGrabInner};
use crate::protocol::globals::xdg_shell::Popup;
use crate::protocol::serial::Serial;

/// Protocol popup object retained by the compositor-thread topology owner.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PopupKind(pub(super) Popup);

impl PopupKind {
    pub(crate) fn alive(&self) -> bool {
        self.0.alive()
    }
    pub(crate) fn wl_surface(&self) -> &WlSurface {
        self.0.wl_surface()
    }

    pub(super) fn parent(&self) -> Option<WlSurface> {
        self.0.parent_surface()
    }

    pub(crate) fn geometry(&self) -> Rectangle<i32, Logical> {
        let geometry = self.0.window_geometry();
        Rectangle::new(
            (geometry.x, geometry.y).into(),
            (
                i32::try_from(geometry.width).unwrap_or(i32::MAX),
                i32::try_from(geometry.height).unwrap_or(i32::MAX),
            )
                .into(),
        )
    }

    fn location(&self) -> Point<i32, Logical> {
        let placement = self.0.placement();
        (placement.x, placement.y).into()
    }

    fn send_done(&self) {
        self.0.send_popup_done();
    }
}

#[derive(Clone, Copy, Debug, Error)]
#[error("popup resource is no longer alive")]
pub(crate) struct DeadResource;

impl From<PopupKind> for WlSurface {
    fn from(popup: PopupKind) -> Self {
        popup.wl_surface().clone()
    }
}

impl From<Popup> for PopupKind {
    fn from(popup: Popup) -> Self {
        Self(popup)
    }
}

#[derive(Debug)]
struct PopupNode {
    popup: PopupKind,
    parent: WlSurface,
    parent_index: Option<usize>,
    remove: bool,
}

#[derive(Debug)]
struct PopupTree {
    root: WlSurface,
    nodes: Vec<PopupNode>,
    /// Topmost-to-bottom order; rebuilt only when topology changes.
    order: Vec<usize>,
}

impl PopupTree {
    fn new(root: WlSurface) -> Self {
        Self {
            root,
            nodes: Vec::new(),
            order: Vec::new(),
        }
    }

    fn insert(&mut self, popup: PopupKind, parent: WlSurface) {
        self.nodes.push(PopupNode {
            popup,
            parent,
            parent_index: None,
            remove: false,
        });
        self.rebuild_order();
    }

    fn rebuild_order(&mut self) {
        for index in 0..self.nodes.len() {
            let parent_index = self
                .nodes
                .iter()
                .position(|candidate| candidate.popup.wl_surface() == &self.nodes[index].parent);
            self.nodes[index].parent_index = parent_index;
        }
        self.order.clear();
        append_popup_children(&self.root, &self.nodes, &mut self.order);
    }

    fn popup_location(&self, mut index: usize) -> Point<i32, Logical> {
        let mut location = Point::default();
        for _ in 0..self.nodes.len() {
            let node = &self.nodes[index];
            location += node.popup.location();
            let Some(parent) = node.parent_index else {
                break;
            };
            index = parent;
        }
        location
    }

    fn is_descendant_or_self(&self, mut index: usize, ancestor: &WlSurface) -> bool {
        for _ in 0..=self.nodes.len() {
            let node = &self.nodes[index];
            if node.popup.wl_surface() == ancestor {
                return true;
            }
            let Some(parent) = node.parent_index else {
                return false;
            };
            index = parent;
        }
        false
    }

    fn dismiss(&mut self, popup: &PopupKind) {
        let Some(target) = self.nodes.iter().position(|node| node.popup == *popup) else {
            return;
        };
        let target_surface = self.nodes[target].popup.wl_surface().clone();
        for index in self.order.iter().copied() {
            if self.is_descendant_or_self(index, &target_surface) {
                let node = &self.nodes[index];
                node.popup.send_done();
                self.nodes[index].remove = true;
            }
        }
        self.nodes.retain(|node| !node.remove);
        self.rebuild_order();
    }

    fn cleanup(&mut self) {
        for index in 0..self.nodes.len() {
            let popup = &self.nodes[index].popup;
            if popup.alive() && self.has_dead_ancestor(index) {
                popup.0.post_not_topmost();
            }
            let remove = !popup.alive() || self.has_dead_ancestor(index);
            self.nodes[index].remove = remove;
        }
        self.nodes.retain(|node| !node.remove);
        self.rebuild_order();
    }

    fn has_dead_ancestor(&self, mut index: usize) -> bool {
        for _ in 0..self.nodes.len() {
            let Some(parent) = self.nodes[index].parent_index else {
                return false;
            };
            if !self.nodes[parent].popup.alive() {
                return true;
            }
            index = parent;
        }
        false
    }
}

fn append_popup_children(parent: &WlSurface, nodes: &[PopupNode], order: &mut Vec<usize>) {
    for index in (0..nodes.len()).rev() {
        let node = &nodes[index];
        if node.parent != *parent || !node.popup.alive() {
            continue;
        }
        append_popup_children(node.popup.wl_surface(), nodes, order);
        order.push(index);
    }
}

struct PopupIter<'a> {
    tree: Option<&'a PopupTree>,
    front: usize,
    back: usize,
}

impl<'a> PopupIter<'a> {
    fn new(tree: Option<&'a PopupTree>) -> Self {
        let back = tree.map(|tree| tree.order.len()).unwrap_or_default();
        Self {
            tree,
            front: 0,
            back,
        }
    }

    fn item(&self, order_index: usize) -> Option<(&'a PopupKind, Point<i32, Logical>)> {
        let tree = self.tree?;
        let node_index = *tree.order.get(order_index)?;
        Some((
            &tree.nodes[node_index].popup,
            tree.popup_location(node_index),
        ))
    }
}

impl<'a> Iterator for PopupIter<'a> {
    type Item = (&'a PopupKind, Point<i32, Logical>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let index = self.front;
        self.front += 1;
        self.item(index)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.back - self.front;
        (remaining, Some(remaining))
    }
}

impl DoubleEndedIterator for PopupIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        self.item(self.back)
    }
}

impl ExactSizeIterator for PopupIter<'_> {}

/// Compositor-thread popup topology and explicit-grab owner.
#[derive(Debug, Default)]
pub(crate) struct PopupManager {
    unmapped: Vec<PopupKind>,
    trees: Vec<PopupTree>,
    popup_grabs: Vec<PopupGrabInner>,
    seat_grab: PopupGrabInner,
}

impl PopupManager {
    pub(crate) fn track_popup(&mut self, popup: PopupKind) -> Result<(), DeadResource> {
        if popup.parent().is_some() {
            self.add_popup(popup)
        } else {
            trace!(?popup, "tracking popup until its parent commit");
            self.unmapped.push(popup);
            Ok(())
        }
    }

    pub(crate) fn commit(&mut self, popup: &PopupKind) {
        let Some(index) = self
            .unmapped
            .iter()
            .position(|candidate| candidate == popup)
        else {
            return;
        };
        let popup = self.unmapped.swap_remove(index);
        let _ = self.add_popup(popup);
    }

    fn add_popup(&mut self, popup: PopupKind) -> Result<(), DeadResource> {
        let parent = popup.parent().ok_or(DeadResource)?;
        let root = find_popup_root_surface(&popup)?;
        let tree = if let Some(index) = self.trees.iter().position(|tree| tree.root == root) {
            &mut self.trees[index]
        } else {
            self.trees.push(PopupTree::new(root));
            self.trees.last_mut().unwrap()
        };
        tree.insert(popup, parent);
        Ok(())
    }

    pub(crate) fn find_popup(&self, surface: &WlSurface) -> Option<PopupKind> {
        self.unmapped
            .iter()
            .find(|popup| popup.wl_surface() == surface && popup.alive())
            .cloned()
            .or_else(|| {
                self.trees
                    .iter()
                    .flat_map(|tree| &tree.nodes)
                    .find(|node| node.popup.wl_surface() == surface && node.popup.alive())
                    .map(|node| node.popup.clone())
            })
    }

    pub(crate) fn popups_for_surface<'a>(
        &'a self,
        surface: &WlSurface,
    ) -> impl DoubleEndedIterator<Item = (&'a PopupKind, Point<i32, Logical>)> + ExactSizeIterator
    {
        PopupIter::new(self.trees.iter().find(|tree| tree.root == *surface))
    }

    pub(crate) fn dismiss_popup(
        &mut self,
        root: &WlSurface,
        popup: &PopupKind,
    ) -> Result<(), DeadResource> {
        if !root.is_alive() {
            return Err(DeadResource);
        }
        if let Some(tree) = self.trees.iter_mut().find(|tree| tree.root == *root) {
            tree.dismiss(popup);
        }
        Ok(())
    }

    pub(crate) fn cleanup(&mut self) {
        self.popup_grabs.retain_mut(|grabs| {
            grabs.cleanup();
            grabs.has_any_grabs()
        });
        for tree in &mut self.trees {
            tree.cleanup();
        }
        self.trees.retain(|tree| !tree.nodes.is_empty());
        self.unmapped.retain(PopupKind::alive);
    }

    pub(crate) fn grab_popup(
        &mut self,
        root: WlSurface,
        popup: PopupKind,
        serial: Serial,
    ) -> Result<PopupGrab, PopupGrabError> {
        let root_surface =
            find_popup_root_surface(&popup).map_err(|_| PopupGrabError::DeadResource)?;
        assert_eq!(root, root_surface);

        let toplevel_popups = self.seat_grab.clone();
        if !toplevel_popups.has_any_grabs() {
            self.popup_grabs.push(toplevel_popups.clone());
        }

        let previous_serial = match toplevel_popups.grab(&popup, serial) {
            Ok(serial) => serial,
            Err(error) => {
                match error {
                    PopupGrabError::ParentDismissed => {
                        let _ = self.dismiss_popup(&root_surface, &popup);
                    }
                    PopupGrabError::NotTheTopmostPopup => {
                        popup.0.post_not_topmost();
                    }
                    _ => {}
                }
                return Err(error);
            }
        };

        Ok(PopupGrab::new(
            toplevel_popups,
            root,
            serial,
            previous_serial,
        ))
    }
}

/// Finds the non-popup surface at the root of a popup parent chain.
pub(crate) fn find_popup_root_surface(popup: &PopupKind) -> Result<WlSurface, DeadResource> {
    popup.0.root_surface().ok_or(DeadResource)
}

pub(crate) fn get_popup_toplevel_coords(popup: &PopupKind) -> Point<i32, Logical> {
    let point = popup.0.toplevel_coords();
    (point.x, point.y).into()
}
