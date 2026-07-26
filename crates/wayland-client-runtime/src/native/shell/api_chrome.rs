//! Toplevel icon and background blur for [`NativeShell`].

use wayland_client::Proxy;
use wayland_protocols::xdg::decoration::zv1::client::zxdg_toplevel_decoration_v1::Mode as DecorationMode;

use super::api::NativeShell;
use super::types::NativeSurfaceId;
use crate::blur::{BlurRegion, BlurState};
use crate::native::connection::NativeError;
use crate::native::protocols::core::shm;
use crate::surface::DecorationPreference;
use crate::toplevel_icon::ToplevelIcon;

impl NativeShell {
    /// Set or clear the xdg-toplevel-icon for a surface.
    ///
    /// Named icons use the compositor XDG theme; pixel buffers are copied into
    /// SHM ARGB8888. Double-buffered — call [`Self::commit_surface`] after.
    pub fn set_toplevel_icon(
        &mut self,
        id: NativeSurfaceId,
        icon: Option<ToplevelIcon>,
    ) -> Result<(), NativeError> {
        let manager = self
            .state
            .toplevel_icon_manager
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("xdg_toplevel_icon_manager_v1 missing".into()))?
            .clone();
        let shm = self
            .state
            .shm
            .as_ref()
            .ok_or_else(|| NativeError::Registry("wl_shm".into()))?
            .clone();
        let qh = self.queue.handle();

        let record = self
            .state
            .toplevels
            .get_mut(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;

        // Drop previous SHM icon buffers.
        for (_file, pool, buffer) in record.icon_shm.drain(..) {
            buffer.destroy();
            pool.destroy();
        }

        let Some(icon) = icon else {
            manager.set_icon(&record.toplevel, None);
            self.connection.mark_dirty();
            return Ok(());
        };

        let mut kept = Vec::new();
        let protocol_icon = manager.create_icon(&qh, ());
        if let Some(name) = icon.name() {
            protocol_icon.set_name(name.to_string());
        }
        for source in icon.buffers() {
            let (file, pool, buffer) = shm::create_rgba_buffer(
                &shm,
                &qh,
                source.width(),
                source.height(),
                source.rgba(),
            )
            .map_err(|e| NativeError::Io(e.to_string()))?;
            protocol_icon.add_buffer(&buffer, source.scale());
            kept.push((file, pool, buffer));
        }
        manager.set_icon(&record.toplevel, Some(&protocol_icon));
        protocol_icon.destroy();
        record.icon_shm = kept;
        self.connection.mark_dirty();
        Ok(())
    }

    /// Enable / disable compositor background blur (ext-background-effect-v1).
    ///
    /// The blur region is double-buffered and applied on the next
    /// [`Self::commit_surface`]. This method commits immediately so startup and
    /// settings toggles take effect without waiting for an unrelated redraw.
    ///
    /// If the compositor has not yet advertised the blur capability (common
    /// right after bind), the request is remembered on the surface and applied
    /// when [`NativeShellState::background_blur_capable`] becomes true.
    pub fn set_blur(
        &mut self,
        id: NativeSurfaceId,
        state: BlurState,
    ) -> Result<(), NativeError> {
        // Pick up a late capabilities event before deciding unsupported.
        let _ = self.dispatch_pending();
        match state {
            BlurState::Disabled => {
                let record = self
                    .state
                    .toplevels
                    .get_mut(&id)
                    .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
                record.pending_blur = Some(BlurState::Disabled);
                if let Some(effect) = record.blur_effect.take() {
                    effect.destroy();
                }
                // destroy is double-buffered: commit so blur clears now.
                record.wl.commit();
                self.connection.mark_dirty();
                Ok(())
            }
            BlurState::Enabled(region) => {
                if !self.state.background_blur_capable {
                    // Remember without applying; capability handler will take it.
                    let record = self
                        .state
                        .toplevels
                        .get_mut(&id)
                        .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
                    record.pending_blur = Some(BlurState::Enabled(region));
                    return Ok(());
                }
                {
                    let record = self
                        .state
                        .toplevels
                        .get_mut(&id)
                        .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
                    // Keep a copy for capability re-advertise; apply by move.
                    record.pending_blur = Some(BlurState::Enabled(region.clone()));
                }
                self.apply_blur_enabled(id, region)
            }
        }
    }

    /// Apply any remembered blur requests once the compositor is capable.
    pub(crate) fn apply_pending_blur_all(&mut self) -> Result<(), NativeError> {
        if !self.state.background_blur_capable {
            return Ok(());
        }
        // Take ownership of each pending request (no clone of the whole table).
        let pending: Vec<_> = self
            .state
            .toplevels
            .iter_mut()
            .filter_map(|(&id, record)| record.pending_blur.take().map(|state| (id, state)))
            .collect();
        for (id, state) in pending {
            match state {
                BlurState::Disabled => {}
                BlurState::Enabled(region) => {
                    // Keep desired state for capability flaps. EntireSurface is
                    // the only region Fika uses today (clone is free); general
                    // rectangles clone only the rect list.
                    let desired = BlurState::Enabled(region.clone());
                    if let Err(error) = self.apply_blur_enabled(id, region) {
                        eprintln!("[fika-wayland] deferred blur apply failed: {error}");
                    }
                    if let Some(record) = self.state.toplevels.get_mut(&id) {
                        record.pending_blur = Some(desired);
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_blur_enabled(
        &mut self,
        id: NativeSurfaceId,
        region: BlurRegion,
    ) -> Result<(), NativeError> {
        let manager = self
            .state
            .background_effect_manager
            .as_ref()
            .ok_or_else(|| {
                NativeError::Protocol("ext_background_effect_manager_v1 missing".into())
            })?
            .clone();
        let compositor = self
            .state
            .compositor
            .as_ref()
            .ok_or_else(|| NativeError::Registry("wl_compositor".into()))?
            .clone();
        let qh = self.queue.handle();
        let record = self
            .state
            .toplevels
            .get_mut(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        if record.blur_effect.is_none() {
            let effect = manager.get_background_effect(&record.wl, &qh, ());
            record.blur_effect = Some(effect);
        }
        let effect = record
            .blur_effect
            .as_ref()
            .expect("blur effect just set");
        let wl_region = compositor.create_region(&qh, ());
        match region {
            BlurRegion::EntireSurface => {
                // NULL disables blur in this protocol; use a huge region.
                wl_region.add(0, 0, i32::MAX, i32::MAX);
            }
            BlurRegion::Rectangles(rects) => {
                for rect in rects.into_iter().filter(|r| !r.is_empty()) {
                    wl_region.add(
                        rect.origin.x,
                        rect.origin.y,
                        rect.size.width.max(1) as i32,
                        rect.size.height.max(1) as i32,
                    );
                }
            }
        }
        effect.set_blur_region(Some(&wl_region));
        wl_region.destroy();
        // Protocol: blur region is double-buffered; commit so it applies now.
        record.wl.commit();
        self.connection.mark_dirty();
        Ok(())
    }

    /// Request server/client/none decorations via `zxdg_decoration_manager_v1`.
    ///
    /// When the preference is [`DecorationPreference::Client`] (or the
    /// compositor refuses SSD), the native shell draws a full CSD frame
    /// (titlebar, borders, buttons, move/resize/menu hit-testing).
    ///
    /// When the global is missing, Client/None still enable the CSD path.
    /// Double-buffered with the next surface commit.
    pub fn set_decorations(
        &mut self,
        id: NativeSurfaceId,
        preference: DecorationPreference,
    ) -> Result<(), NativeError> {
        {
            let record = self
                .state
                .toplevels
                .get_mut(&id)
                .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
            record.decorations_preference = preference;
        }

        if let Some(manager) = self.state.decoration_manager.clone() {
            let qh = self.queue.handle();
            let record = self
                .state
                .toplevels
                .get_mut(&id)
                .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
            if record.decoration.is_none() {
                let deco = manager.get_toplevel_decoration(&record.toplevel, &qh, ());
                record.decoration = Some(deco);
            }
            let deco = record
                .decoration
                .as_ref()
                .expect("decoration just created");
            match preference {
                DecorationPreference::Server => {
                    deco.set_mode(DecorationMode::ServerSide);
                }
                DecorationPreference::Client | DecorationPreference::None => {
                    deco.set_mode(DecorationMode::ClientSide);
                }
            }
            let deco_id = deco.id().protocol_id();
            self.state.decoration_objects.insert(deco_id, id);
        }

        self.sync_csd_for(id)?;
        self.connection.mark_dirty();
        Ok(())
    }
}
