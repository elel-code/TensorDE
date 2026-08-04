//! Compositor-thread protocol window adapter.
//!
//! Window identity, cached geometry, activation, and XDG roles are Tensor-owned.

use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    hash::{Hash, Hasher},
    rc::Rc,
    time::Duration,
};

use tensor_util::{LogicalPoint, LogicalRect};
use thiserror::Error;
use wayland_protocols::wp::presentation_time::server::wp_presentation_feedback;
use wayland_server::protocol::wl_surface::WlSurface;

#[cfg(feature = "xwayland")]
use crate::protocol::xwayland::X11Surface;

use super::{
    PopupManager,
    surface_tree::{
        OutputPresentationFeedback, for_each_surface_tree, send_frame_callbacks_surface_tree,
        take_presentation_feedback_surface_tree,
    },
    surfaces::surface_view,
};
use crate::ecs::ViewId;
use crate::protocol::globals::compositor::{
    SurfaceAttributes, SurfaceData, TraversalAction, with_surface_tree_downward,
};
use crate::protocol::globals::xdg_shell::Toplevel;

#[derive(Debug)]
// The enum already lives inside one Rc allocation. Boxing X11 would add a
// second allocation and pointer chase to every X11 window operation.
#[allow(clippy::large_enum_variant)]
enum ProtocolWindowSurface {
    Wayland(Toplevel),
    #[cfg(feature = "xwayland")]
    X11(X11Surface),
}

#[derive(Debug)]
struct ProtocolWindowInner {
    surface: ProtocolWindowSurface,
    bbox: Cell<LogicalRect<i32>>,
}

/// A compositor-thread window with stable identity and cached surface geometry.
#[derive(Clone, Debug)]
pub(crate) struct ProtocolWindow(Rc<ProtocolWindowInner>);

#[derive(Debug, Error)]
pub(crate) enum ViewCloseError {
    #[error("view {0:?} does not exist")]
    UnknownView(ViewId),
    #[error("view {0:?} has no retained protocol window")]
    UnmappedView(ViewId),
    #[error("view {view:?} close request failed: {message}")]
    Protocol { view: ViewId, message: String },
}

impl PartialEq for ProtocolWindow {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for ProtocolWindow {}

impl Hash for ProtocolWindow {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        Rc::as_ptr(&self.0).hash(state);
    }
}

impl ProtocolWindow {
    pub(crate) fn alive(&self) -> bool {
        match &self.0.surface {
            ProtocolWindowSurface::Wayland(surface) => surface.alive(),
            #[cfg(feature = "xwayland")]
            ProtocolWindowSurface::X11(surface) => surface.alive(),
        }
    }

    pub(crate) fn new_wayland(toplevel: Toplevel) -> Self {
        Self(Rc::new(ProtocolWindowInner {
            surface: ProtocolWindowSurface::Wayland(toplevel),
            bbox: Cell::new(LogicalRect::zero()),
        }))
    }

    #[cfg(feature = "xwayland")]
    pub(crate) fn new_x11(surface: X11Surface) -> Self {
        Self(Rc::new(ProtocolWindowInner {
            surface: ProtocolWindowSurface::X11(surface),
            bbox: Cell::new(LogicalRect::zero()),
        }))
    }

    pub(crate) fn geometry(&self) -> LogicalRect<i32> {
        match &self.0.surface {
            ProtocolWindowSurface::Wayland(surface) => {
                let bbox = self.bbox();
                surface
                    .geometry()
                    .map(|geometry| {
                        LogicalRect::new(
                            (geometry.x, geometry.y).into(),
                            (
                                i32::try_from(geometry.width).unwrap_or(i32::MAX),
                                i32::try_from(geometry.height).unwrap_or(i32::MAX),
                            )
                                .into(),
                        )
                    })
                    .and_then(|geometry| geometry.intersection(bbox))
                    .unwrap_or(bbox)
            }
            #[cfg(feature = "xwayland")]
            ProtocolWindowSurface::X11(surface) => surface.geometry(),
        }
    }

    fn bbox(&self) -> LogicalRect<i32> {
        match &self.0.surface {
            ProtocolWindowSurface::Wayland(_) => self.0.bbox.get(),
            #[cfg(feature = "xwayland")]
            ProtocolWindowSurface::X11(surface) => surface.bbox(),
        }
    }

    pub(crate) fn bbox_with_popups(&self, popups: &PopupManager) -> LogicalRect<i32> {
        let mut bbox = self.bbox();
        let Some(root) = self.wl_surface() else {
            return bbox;
        };
        let geometry_location = self.geometry().loc;
        for (popup, location) in popups.popups_for_surface(root.as_ref()) {
            let offset = geometry_location + location - popup.geometry().loc;
            bbox = bbox.union(surface_tree_bbox(popup.wl_surface(), offset));
        }
        bbox
    }

    pub(crate) fn set_activated(&self, active: bool) -> bool {
        match &self.0.surface {
            ProtocolWindowSurface::Wayland(surface) => surface.set_activated(active),
            #[cfg(feature = "xwayland")]
            ProtocolWindowSurface::X11(surface) => {
                let was_active = surface.is_activated();
                if surface.set_activated(active).is_ok() {
                    was_active != active
                } else {
                    false
                }
            }
        }
    }

    fn request_close(&self) -> Result<(), String> {
        match &self.0.surface {
            ProtocolWindowSurface::Wayland(surface) => surface
                .request_close()
                .then_some(())
                .ok_or_else(|| "xdg_toplevel is no longer alive".to_owned()),
            #[cfg(feature = "xwayland")]
            ProtocolWindowSurface::X11(surface) => {
                surface.request_close().map_err(|error| error.to_string())
            }
        }
    }

    pub(crate) fn on_commit(&self) {
        match &self.0.surface {
            ProtocolWindowSurface::Wayland(surface) => self
                .0
                .bbox
                .set(surface_tree_bbox(surface.wl_surface(), (0, 0))),
            #[cfg(feature = "xwayland")]
            ProtocolWindowSurface::X11(_) => {}
        }
    }

    pub(crate) fn with_surfaces<F>(&self, popups: &PopupManager, mut processor: F)
    where
        F: FnMut(&WlSurface, &SurfaceData),
    {
        let Some(root) = self.wl_surface() else {
            return;
        };
        for_each_surface_tree(root.as_ref(), &mut processor);
        for (popup, _) in popups.popups_for_surface(root.as_ref()) {
            for_each_surface_tree(popup.wl_surface(), &mut processor);
        }
    }

    pub(crate) fn send_frame<F>(&self, popups: &PopupManager, time: Duration, is_submitted: &mut F)
    where
        F: FnMut(&WlSurface, &SurfaceData) -> bool,
    {
        let Some(root) = self.wl_surface() else {
            return;
        };
        send_frame_callbacks_surface_tree(root.as_ref(), time, is_submitted);
        for (popup, _) in popups.popups_for_surface(root.as_ref()) {
            send_frame_callbacks_surface_tree(popup.wl_surface(), time, is_submitted);
        }
    }

    pub(crate) fn take_presentation_feedback<F1, F2>(
        &self,
        popups: &PopupManager,
        output_feedback: &mut OutputPresentationFeedback,
        is_submitted: &mut F1,
        presentation_feedback_flags: &mut F2,
    ) where
        F1: FnMut(&WlSurface, &SurfaceData) -> bool,
        F2: FnMut(&WlSurface, &SurfaceData) -> wp_presentation_feedback::Kind,
    {
        let Some(root) = self.wl_surface() else {
            return;
        };
        take_presentation_feedback_surface_tree(
            root.as_ref(),
            output_feedback,
            is_submitted,
            presentation_feedback_flags,
        );
        for (popup, _) in popups.popups_for_surface(root.as_ref()) {
            take_presentation_feedback_surface_tree(
                popup.wl_surface(),
                output_feedback,
                is_submitted,
                presentation_feedback_flags,
            );
        }
    }

    /// Finds the topmost toplevel, subsurface, or popup input surface.
    pub(crate) fn surface_under<P, F>(
        &self,
        popups: &PopupManager,
        point: P,
        _xwayland_dnd_active: &mut F,
    ) -> Option<(WlSurface, LogicalPoint<i32>)>
    where
        P: Into<LogicalPoint<f64>>,
        F: FnMut() -> bool,
    {
        let point = point.into();
        match &self.0.surface {
            ProtocolWindowSurface::Wayland(surface) => {
                let root = surface.wl_surface();
                let geometry_location = self.geometry().loc;
                for (popup, location) in popups.popups_for_surface(root) {
                    let offset = geometry_location + location - popup.geometry().loc;
                    if let Some(hit) = surface_tree_under(popup.wl_surface(), point, offset) {
                        return Some(hit);
                    }
                }
                surface_tree_under(root, point, (0, 0))
            }
            #[cfg(feature = "xwayland")]
            ProtocolWindowSurface::X11(surface) => {
                if surface.is_override_redirect() && _xwayland_dnd_active() {
                    return None;
                }
                surface
                    .wl_surface()
                    .and_then(|root| surface_tree_under(&root, point, (0, 0)))
            }
        }
    }

    pub(crate) fn toplevel(&self) -> Option<&Toplevel> {
        match &self.0.surface {
            ProtocolWindowSurface::Wayland(surface) => Some(surface),
            #[cfg(feature = "xwayland")]
            ProtocolWindowSurface::X11(_) => None,
        }
    }

    #[cfg(feature = "xwayland")]
    pub(crate) fn x11_surface(&self) -> Option<&X11Surface> {
        match &self.0.surface {
            ProtocolWindowSurface::Wayland(_) => None,
            ProtocolWindowSurface::X11(surface) => Some(surface),
        }
    }

    pub(crate) fn wl_surface(&self) -> Option<Cow<'_, WlSurface>> {
        match &self.0.surface {
            ProtocolWindowSurface::Wayland(surface) => Some(Cow::Borrowed(surface.wl_surface())),
            #[cfg(feature = "xwayland")]
            ProtocolWindowSurface::X11(surface) => surface.wl_surface().map(Cow::Owned),
        }
    }
}

impl super::RuntimeState {
    /// Ask the exact stable view selected by overview to close. Attached
    /// dialogs remain independent close targets; family teardown follows the
    /// client's normal protocol lifecycle.
    pub(crate) fn request_view_close(&self, view: ViewId) -> Result<(), ViewCloseError> {
        if self.world.view_workspace(view).is_none() {
            return Err(ViewCloseError::UnknownView(view));
        }
        let window = self
            .retained_window_for_view(view)
            .ok_or(ViewCloseError::UnmappedView(view))?;
        window
            .request_close()
            .map_err(|message| ViewCloseError::Protocol { view, message })
    }
}

pub(super) fn surface_tree_bbox<P>(surface: &WlSurface, location: P) -> LogicalRect<i32>
where
    P: Into<LogicalPoint<i32>>,
{
    let location = location.into();
    let mut bbox = LogicalRect::new(location, (0, 0).into());
    with_surface_tree_downward(
        surface,
        location,
        |_, states, location| {
            let Some(view) = surface_view(states) else {
                return TraversalAction::SkipChildren;
            };
            let location = *location + LogicalPoint::from(view.offset);
            bbox = bbox.union(LogicalRect::new(location, view.size.into()));
            TraversalAction::DoChildren(location)
        },
        |_, _, _| {},
        |_, _, _| true,
    );
    bbox
}

pub(in crate::protocol) fn surface_tree_under<P>(
    surface: &WlSurface,
    point: LogicalPoint<f64>,
    location: P,
) -> Option<(WlSurface, LogicalPoint<i32>)>
where
    P: Into<LogicalPoint<i32>>,
{
    let found = RefCell::new(None);
    with_surface_tree_downward(
        surface,
        location.into(),
        |_, states, location| {
            let Some(view) = surface_view(states) else {
                return TraversalAction::SkipChildren;
            };
            TraversalAction::DoChildren(*location + LogicalPoint::from(view.offset))
        },
        |surface, states, location| {
            let Some(view) = surface_view(states) else {
                return;
            };
            let location = *location + LogicalPoint::from(view.offset);
            let local = point - location.to_f64();
            let bounds = LogicalRect::from_size(view.size.into()).to_f64();
            if !bounds.contains(local) {
                return;
            }
            let local = local.floor_i32();
            let accepts_input = states
                .cached_state
                .get::<SurfaceAttributes>()
                .current()
                .input_region
                .as_ref()
                .is_none_or(|region| region.contains((local.x, local.y)));
            if accepts_input {
                *found.borrow_mut() = Some((surface.clone(), location));
            }
        },
        |_, _, _| found.borrow().is_none(),
    );
    found.into_inner()
}
