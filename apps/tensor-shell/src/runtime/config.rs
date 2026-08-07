use wayland_client_runtime::SurfaceId;

use super::{ShellRuntime, ShellRuntimeError};
use crate::overview::{OverviewServiceHandle, OverviewServiceSnapshot};
use crate::{ShellComponent, ShellConfig, surface_plan};

impl ShellRuntime {
    pub(super) fn reconcile_config(&mut self) -> Result<(), ShellRuntimeError> {
        let update = self
            .config_reload
            .as_ref()
            .and_then(|reload| reload.read_if_changed(self.config_revision));
        let Some((revision, config)) = update else {
            return Ok(());
        };
        self.apply_config(config.as_ref())?;
        self.config_revision = revision;
        Ok(())
    }

    fn apply_config(&mut self, config: &ShellConfig) -> Result<(), ShellRuntimeError> {
        let layout_changed = self.model.layout() != config.layout;
        let panel_changed = self.panel_config != config.panel;
        let media_changed = self.media_config != config.media;
        let overview_changed = self.tensorland.ipc_socket != config.tensorland.ipc_socket;

        self.model.set_layout(config.layout);
        self.panel_config = config.panel.clone();
        self.launcher = config.launcher.clone();
        if media_changed {
            self.media_osd.update_policy(
                config.media.playback_osd,
                self.notifications.now_ms(),
                config.media.playback_osd_timeout_ms,
            );
        }
        self.media_config = config.media;
        if overview_changed {
            self.overview = OverviewServiceHandle::start(
                config.tensorland.ipc_socket.clone(),
                self.wayland.wake_handle(),
            );
            self.overview_revision = 0;
            self.overview_snapshot = OverviewServiceSnapshot::Pending;
        }
        self.tensorland = config.tensorland.clone();

        if layout_changed {
            let reconfigured = self.reconfigure_layer_surfaces()?;
            self.reset_all_scene_interactions();
            self.refresh_configured_scenes(&reconfigured)?;
        } else {
            if panel_changed {
                self.refresh_configured_panels()?;
            }
            if overview_changed {
                self.refresh_configured_overviews()?;
            }
        }
        Ok(())
    }

    fn reconfigure_layer_surfaces(&mut self) -> Result<Vec<SurfaceId>, ShellRuntimeError> {
        let surfaces = self
            .surface_keys
            .iter()
            .map(|(surface, key)| (*surface, *key))
            .collect::<Vec<_>>();
        let mut changed = Vec::new();
        for (surface, key) in surfaces {
            let Some(plan) = surface_plan(key.component, key.output, self.model.layout()) else {
                continue;
            };
            if self.wayland.layer_surface_state(surface)? == plan.attributes.state {
                continue;
            }
            self.wayland
                .set_layer_surface_state(surface, plan.attributes.state)?;
            self.wayland.commit(surface)?;
            self.configured_surfaces.remove(&surface);
            changed.push(surface);
        }
        if !changed.is_empty() {
            self.wayland.flush()?;
        }
        Ok(changed)
    }

    fn reset_all_scene_interactions(&mut self) {
        self.panel_input.clear();
        self.panel_touches.clear();
        self.overview_input.clear();
        self.notification_input.clear();
        self.media_osd_input.clear();
        self.control_center_input.clear();
    }

    fn refresh_configured_scenes(
        &mut self,
        reconfigured: &[SurfaceId],
    ) -> Result<(), ShellRuntimeError> {
        let surfaces = self
            .configured_surfaces
            .iter()
            .copied()
            .filter(|surface| !reconfigured.contains(surface))
            .collect::<Vec<_>>();
        for surface in surfaces {
            self.refresh_surface_scenes(surface)?;
            self.present_surface(surface)?;
        }
        Ok(())
    }

    fn refresh_configured_panels(&mut self) -> Result<(), ShellRuntimeError> {
        self.panel_input.clear();
        self.panel_touches.clear();
        let surfaces = self.configured_component_surfaces(ShellComponent::Panel);
        for surface in surfaces {
            self.refresh_panel_scene(surface)?;
            self.present_surface(surface)?;
        }
        Ok(())
    }

    fn refresh_configured_overviews(&mut self) -> Result<(), ShellRuntimeError> {
        self.overview_input.clear();
        let surfaces = self.configured_component_surfaces(ShellComponent::Overview);
        for surface in surfaces {
            self.refresh_overview_scene(surface)?;
            self.present_surface(surface)?;
        }
        Ok(())
    }

    fn configured_component_surfaces(&self, component: ShellComponent) -> Vec<SurfaceId> {
        self.surface_keys
            .iter()
            .filter(|(surface, key)| {
                key.component == component && self.configured_surfaces.contains(surface)
            })
            .map(|(surface, _)| *surface)
            .collect()
    }
}
