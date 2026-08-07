use wayland_client_runtime::{PointerEvent, PointerEventKind, SurfaceId};

use super::{BTN_LEFT, ShellRuntime, ShellRuntimeError};
use crate::{
    ShellComponent,
    media_osd_scene::{MediaOsdHit, MediaOsdInteraction, MediaOsdScene},
};

impl ShellRuntime {
    pub(super) fn reconcile_media_osd(&mut self) -> Result<(), ShellRuntimeError> {
        let now_ms = self.notifications.now_ms();
        if self.media_osd_observed_revision != self.media_revision {
            self.media_osd.observe(
                &self.media_snapshot,
                now_ms,
                self.media_config.playback_osd,
                self.media_config.playback_osd_timeout_ms,
            );
            self.media_osd_observed_revision = self.media_revision;
        }
        self.media_osd.advance(now_ms);
        self.media_osd.expire(now_ms);
        self.sync_media_osd_visibility();

        let revision = self.media_osd.revision();
        if revision == self.media_osd_revision {
            return Ok(());
        }
        self.media_osd_revision = revision;
        let surfaces = self
            .surface_keys
            .iter()
            .filter(|(_, key)| key.component == ShellComponent::Osd)
            .map(|(surface, _)| *surface)
            .collect::<Vec<_>>();
        for surface in surfaces {
            self.refresh_media_osd_scene(surface)?;
            if self.configured_surfaces.contains(&surface) {
                self.present_surface(surface)?;
            }
        }
        Ok(())
    }

    fn sync_media_osd_visibility(&mut self) {
        let visible = self.media_osd.visible();
        let outputs = self.model.output_ids().collect::<Vec<_>>();
        for output in outputs {
            self.model.set_visible(output, ShellComponent::Osd, visible);
        }
    }

    pub(super) fn refresh_media_osd_scene(
        &mut self,
        surface: SurfaceId,
    ) -> Result<(), ShellRuntimeError> {
        let is_osd = self
            .surface_keys
            .get(&surface)
            .is_some_and(|key| key.component == ShellComponent::Osd);
        let Some(content) = is_osd.then(|| self.media_osd.content()).flatten() else {
            self.media_osd_scenes.remove(&surface);
            self.media_osd_input.remove(&surface);
            return Ok(());
        };
        let extent = self
            .wayland
            .logical_size(surface)
            .ok_or(ShellRuntimeError::MissingLogicalExtent(surface))?;
        if let Some(scene) = self.media_osd_scenes.get_mut(&surface)
            && scene.update_progress(content)
        {
            self.media_osd_input.entry(surface).or_default();
            return Ok(());
        }
        self.media_osd_scenes
            .insert(surface, MediaOsdScene::build(extent, content));
        self.media_osd_input.entry(surface).or_default();
        Ok(())
    }

    pub(super) fn handle_media_osd_pointer(
        &mut self,
        event: &PointerEvent,
    ) -> Result<(), ShellRuntimeError> {
        let Some(scene) = self.media_osd_scenes.get(&event.surface) else {
            return Ok(());
        };
        let hit = scene.hit_test(event.position);
        let state = self.media_osd_input.entry(event.surface).or_default();
        let previous = *state;
        let activation = match event.kind {
            PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                state.hovered = hit;
                None
            }
            PointerEventKind::Leave => {
                *state = MediaOsdInteraction::default();
                None
            }
            PointerEventKind::Press {
                button: BTN_LEFT, ..
            } => {
                state.hovered = hit;
                state.pressed = hit;
                None
            }
            PointerEventKind::Release {
                button: BTN_LEFT, ..
            } => {
                state.hovered = hit;
                let pressed = state.pressed.take();
                (pressed == hit).then_some(hit).flatten()
            }
            PointerEventKind::Press { .. }
            | PointerEventKind::Release { .. }
            | PointerEventKind::Axis { .. } => None,
        };
        let hovered = !matches!(event.kind, PointerEventKind::Leave) && hit.is_some();
        self.media_osd.set_hovered(
            hovered,
            self.notifications.now_ms(),
            self.media_config.playback_osd_timeout_ms,
        );
        if let Some(hit) = activation {
            self.activate_media_osd_hit(hit)?;
        } else if previous != *state {
            self.present_surface(event.surface)?;
        }
        Ok(())
    }

    fn activate_media_osd_hit(&mut self, hit: MediaOsdHit) -> Result<(), ShellRuntimeError> {
        if let Some(action) = hit.action() {
            self.media.request(action)?;
        }
        self.media_osd.dismiss();
        self.sync_media_osd_visibility();
        Ok(())
    }
}
