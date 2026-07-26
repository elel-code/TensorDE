//! Toplevel icon and background blur for [`NativeShell`].

use super::api::NativeShell;
use super::types::NativeSurfaceId;
use crate::blur::{BlurRegion, BlurState};
use crate::native::connection::NativeError;
use crate::native::protocols::core::shm;
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
            self.connection.flush()?;
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
        self.connection.flush()?;
        Ok(())
    }

    /// Enable / disable compositor background blur (ext-background-effect-v1).
    ///
    /// Requires the manager to advertise the blur capability. Double-buffered —
    /// commit the surface after changing.
    pub fn set_blur(
        &mut self,
        id: NativeSurfaceId,
        state: BlurState,
    ) -> Result<(), NativeError> {
        match state {
            BlurState::Disabled => {
                let record = self
                    .state
                    .toplevels
                    .get_mut(&id)
                    .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
                if let Some(effect) = record.blur_effect.take() {
                    effect.destroy();
                }
                self.connection.flush()?;
                Ok(())
            }
            BlurState::Enabled(region) => {
                if !self.state.background_blur_capable {
                    return Err(NativeError::Protocol(
                        "ext_background_effect blur capability missing".into(),
                    ));
                }
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
                self.connection.flush()?;
                Ok(())
            }
        }
    }
}
