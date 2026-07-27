//! Client-side decoration lifecycle for [`NativeShell`].

use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::Shape as CursorShape;

use super::api::NativeShell;
use super::csd::{ClientSideFrame, FrameAction, FrameCursor, FramePartKind};
use super::types::NativeSurfaceId;
use crate::native::connection::NativeError;
use crate::surface::DecorationPreference;

impl NativeShell {
    /// Create or refresh the CSD frame for a toplevel after preference / mode change.
    pub(crate) fn sync_csd_for(
        &mut self,
        id: NativeSurfaceId,
    ) -> Result<(), NativeError> {
        let (pref, mode, title, w, h, states, scale, parent_wl) = {
            let Some(record) = self.state.toplevels.get(&id) else {
                return Ok(());
            };
            (
                record.decorations_preference,
                record.decoration_mode,
                record.title.clone(),
                record.logical_w,
                record.logical_h,
                record.pending_states,
                record.scale_factor,
                record.wl.clone(),
            )
        };

        // Effective mode: compositor configure wins; else user preference.
        // Missing decoration manager ⇒ client must draw its own frame.
        let effective = match mode {
            Some(m) => m,
            None if self.state.decoration_manager.is_none() => match pref {
                DecorationPreference::None => DecorationPreference::None,
                _ => DecorationPreference::Client,
            },
            None => pref,
        };

        let want_csd = matches!(effective, DecorationPreference::Client)
            || (matches!(pref, DecorationPreference::Client)
                && !matches!(mode, Some(DecorationPreference::Server)));
        let hide = matches!(pref, DecorationPreference::None)
            || matches!(
                effective,
                DecorationPreference::Server | DecorationPreference::None
            );

        if hide && !want_csd {
            if let Some(mut frame) = self.state.csd_frames.remove(&id) {
                frame.destroy_parts(&mut self.state);
            }
            return Ok(());
        }

        if !want_csd {
            if let Some(mut frame) = self.state.csd_frames.remove(&id) {
                frame.destroy_parts(&mut self.state);
            }
            return Ok(());
        }

        self.state.csd_frames.entry(id).or_insert_with(|| ClientSideFrame::new(id, w, h, title.clone()));

        {
            let frame = self
                .state
                .csd_frames
                .get_mut(&id)
                .expect("csd frame just inserted");
            frame.set_title(title);
            frame.set_content_size(w, h);
            frame.set_toplevel_state(states);
            frame.set_scale(scale);
            frame.set_enabled(true);
            frame.set_hidden(matches!(pref, DecorationPreference::None));
        }

        let compositor = self
            .state
            .compositor
            .clone()
            .ok_or_else(|| NativeError::Registry("wl_compositor".into()))?;
        let Some(subcompositor) = self.state.subcompositor.clone() else {
            // Without subcompositor we cannot draw CSD; leave state ready for later.
            return Ok(());
        };
        let qh = self.queue.handle();

        // Split borrow: take the frame out, mutate, put back.
        let mut frame = self
            .state
            .csd_frames
            .remove(&id)
            .expect("csd frame present");
        frame.ensure_parts(
            &parent_wl,
            &compositor,
            &subcompositor,
            &qh,
            &mut self.state,
        );
        let shm = self
            .state
            .shm
            .clone()
            .ok_or_else(|| NativeError::Registry("wl_shm".into()))?;
        frame.redraw(&shm, &qh)?;
        self.state.csd_frames.insert(id, frame);
        self.connection.mark_dirty();
        Ok(())
    }

    /// Redraw all dirty CSD frames (call after configure / scale / title).
    pub fn redraw_csd(&mut self, id: NativeSurfaceId) -> Result<(), NativeError> {
        let shm = self
            .state
            .shm
            .clone()
            .ok_or_else(|| NativeError::Registry("wl_shm".into()))?;
        let qh = self.queue.handle();
        if let Some(frame) = self.state.csd_frames.get_mut(&id) {
            frame.redraw(&shm, &qh)?;
            self.connection.mark_dirty();
        }
        Ok(())
    }

    /// Redraw every CSD frame that is dirty.
    pub fn redraw_all_csd(&mut self) -> Result<(), NativeError> {
        let ids: Vec<_> = self.state.csd_frames.keys().copied().collect();
        for id in ids {
            self.redraw_csd(id)?;
        }
        Ok(())
    }

    pub(crate) fn destroy_csd(&mut self, id: NativeSurfaceId) {
        if let Some(mut frame) = self.state.csd_frames.remove(&id) {
            frame.destroy_parts(&mut self.state);
        }
    }

    pub(crate) fn apply_frame_action(
        &mut self,
        id: NativeSurfaceId,
        action: FrameAction,
    ) -> Result<(), NativeError> {
        match action {
            FrameAction::Move => self.begin_interactive_move(id),
            FrameAction::Resize(edge) => self.begin_interactive_resize(id, edge),
            FrameAction::Close => {
                self.state
                    .push(super::types::NativeShellEvent::ToplevelClose { surface: id });
                Ok(())
            }
            FrameAction::Maximize => self.set_maximized(id, true),
            FrameAction::UnMaximize => self.set_maximized(id, false),
            FrameAction::Minimize => self.set_minimized(id),
            FrameAction::ShowMenu { x, y } => self.show_window_menu(
                id,
                crate::geometry::LogicalPosition::new(x, y),
            ),
        }
    }

    pub(crate) fn set_csd_cursor(&mut self, cursor: FrameCursor) {
        let shape = match cursor {
            FrameCursor::Default => CursorShape::Default,
            FrameCursor::Pointer => CursorShape::Pointer,
            FrameCursor::NResize => CursorShape::NResize,
            FrameCursor::SResize => CursorShape::SResize,
            FrameCursor::EResize => CursorShape::EResize,
            FrameCursor::WResize => CursorShape::WResize,
            FrameCursor::NeResize => CursorShape::NeResize,
            FrameCursor::NwResize => CursorShape::NwResize,
            FrameCursor::SeResize => CursorShape::SeResize,
            FrameCursor::SwResize => CursorShape::SwResize,
        };
        let _ = self.set_cursor_shape(shape);
    }

    /// Whether `surface` is a CSD decoration part; returns parent + kind.
    #[allow(dead_code)]
    pub(crate) fn csd_part_of(
        &self,
        surface: NativeSurfaceId,
    ) -> Option<(NativeSurfaceId, FramePartKind)> {
        self.state.csd_part_owners.get(&surface).copied()
    }

    pub fn has_subcompositor(&self) -> bool {
        self.state.subcompositor.is_some()
    }

    /// Number of active CSD frames (debug / tests).
    pub fn csd_frame_count(&self) -> usize {
        self.state.csd_frames.len()
    }

    /// Whether a toplevel currently has a live CSD frame.
    #[allow(dead_code)]
    pub fn has_csd_frame(&self, id: NativeSurfaceId) -> bool {
        self.state.csd_frames.contains_key(&id)
    }
}
