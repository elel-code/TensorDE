use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::Duration,
};

use wayland_client_runtime::{
    Event, LayerSurfaceEvent, OutputId, Runtime, RuntimeError, SurfaceEvent, SurfaceId,
};

use crate::notification_service::NotificationServiceHandle;
use crate::present::ShellPresenter;
use crate::{
    NotificationServiceError, NotificationStore, ShellComponent, ShellLayout, ShellModel,
    ShellPresentError, SurfaceKey,
};

/// Protocol runtime for the shell's model-driven layer surfaces.
pub struct ShellRuntime {
    wayland: Runtime,
    model: ShellModel,
    surfaces: BTreeMap<SurfaceKey, SurfaceId>,
    surface_keys: BTreeMap<SurfaceId, SurfaceKey>,
    configured_surfaces: BTreeSet<SurfaceId>,
    events: Vec<Event>,
    notifications: NotificationServiceHandle,
    presenter: Option<ShellPresenter>,
}

impl ShellRuntime {
    pub fn connect() -> Result<Self, ShellRuntimeError> {
        let wayland = Runtime::connect()?;
        if !wayland.capabilities().layer_shell_v1 {
            return Err(RuntimeError::Unsupported("layer-shell-v1").into());
        }
        let notifications =
            NotificationServiceHandle::start(Arc::new(Mutex::new(NotificationStore::default())))?;
        Ok(Self {
            wayland,
            model: ShellModel::new(ShellLayout::default()),
            surfaces: BTreeMap::new(),
            surface_keys: BTreeMap::new(),
            configured_surfaces: BTreeSet::new(),
            events: Vec::with_capacity(128),
            notifications,
            presenter: None,
        })
    }

    pub fn run(mut self) -> Result<(), ShellRuntimeError> {
        loop {
            self.wayland.dispatch(Some(Duration::from_millis(20)))?;
            self.events.clear();
            self.wayland.drain_events_into(&mut self.events);
            let events = std::mem::take(&mut self.events);
            for event in &events {
                self.handle_event(event)?;
            }
            self.reconcile_notification_state()?;
            self.reconcile_surfaces()?;
            self.events = events;
        }
    }

    fn reconcile_notification_state(&mut self) -> Result<(), ShellRuntimeError> {
        let now_ms = self.notifications.now_ms();
        let show_popups = {
            let mut store = self
                .notifications
                .store()
                .lock()
                .map_err(|_| ShellRuntimeError::NotificationStorePoisoned)?;
            for closed in store.expire(now_ms) {
                self.notifications.emit_closed(closed)?;
            }
            store.visible_popups().next().is_some()
        };
        let outputs = self.model.output_ids().collect::<Vec<_>>();
        for output in outputs {
            self.model
                .set_visible(output, ShellComponent::NotificationPopups, show_popups);
        }
        Ok(())
    }

    fn handle_event(&mut self, event: &Event) -> Result<(), ShellRuntimeError> {
        match event {
            Event::Output(output) => self.model.apply_output_event(output.clone()),
            Event::LayerSurface(LayerSurfaceEvent::Configure { surface, .. }) => {
                self.configured_surfaces.insert(*surface);
                self.present_surface(*surface)?;
            }
            Event::LayerSurface(LayerSurfaceEvent::Closed { surface }) => {
                if let Some(key) = self.surface_keys.remove(surface) {
                    self.surfaces.remove(&key);
                    self.configured_surfaces.remove(surface);
                    self.remove_presented_surface(*surface)?;
                    self.wayland.destroy_surface(*surface)?;
                }
            }
            Event::Surface(SurfaceEvent::ScaleFactorChanged { surface, .. })
                if self.configured_surfaces.contains(surface) =>
            {
                self.present_surface(*surface)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn present_surface(&mut self, surface: SurfaceId) -> Result<(), ShellRuntimeError> {
        let key = self
            .surface_keys
            .get(&surface)
            .copied()
            .ok_or(ShellRuntimeError::UnknownConfiguredSurface(surface))?;
        let host = self
            .wayland
            .surface_handle(surface)
            .ok_or(ShellRuntimeError::MissingSurfaceHandle(surface))?;
        let (width, height) = self
            .wayland
            .buffer_size(surface)
            .ok_or(ShellRuntimeError::MissingBufferExtent(surface))?;
        let extent = vulkan_renderer::Extent2D::new(width, height);
        match self.presenter.as_mut() {
            Some(presenter) => presenter.ensure_surface(surface, key, Arc::new(host), extent)?,
            None => {
                self.presenter = Some(ShellPresenter::new(surface, key, Arc::new(host), extent)?);
            }
        }
        self.wayland.arm_present_notify(surface)?;
        self.wayland.flush()?;
        self.presenter
            .as_mut()
            .expect("Tensor Shell presenter was initialized above")
            .present(surface, extent)?;
        Ok(())
    }

    fn remove_presented_surface(&mut self, surface: SurfaceId) -> Result<(), ShellPresentError> {
        if let Some(presenter) = self.presenter.as_mut() {
            presenter.remove_surface(surface)?;
        }
        Ok(())
    }

    fn reconcile_surfaces(&mut self) -> Result<(), ShellRuntimeError> {
        let desired = self
            .model
            .plans()
            .map(|plan| (plan.key, plan))
            .collect::<BTreeMap<_, _>>();
        let stale = self
            .surfaces
            .keys()
            .copied()
            .filter(|key| !desired.contains_key(key))
            .collect::<Vec<_>>();
        for key in stale {
            if let Some(surface) = self.surfaces.remove(&key) {
                self.surface_keys.remove(&surface);
                self.configured_surfaces.remove(&surface);
                self.remove_presented_surface(surface)?;
                self.wayland.destroy_surface(surface)?;
            }
        }
        for (key, plan) in desired {
            if self.surfaces.contains_key(&key) {
                continue;
            }
            let surface = self.wayland.create_layer_surface_gpu(plan.attributes)?;
            self.surface_keys.insert(surface, key);
            self.surfaces.insert(key, surface);
        }
        Ok(())
    }

    pub fn set_visible(
        &mut self,
        output: OutputId,
        component: ShellComponent,
        visible: bool,
    ) -> Result<(), ShellRuntimeError> {
        self.model.set_visible(output, component, visible);
        self.reconcile_surfaces()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ShellRuntimeError {
    #[error(transparent)]
    Wayland(#[from] RuntimeError),
    #[error(transparent)]
    NotificationService(#[from] NotificationServiceError),
    #[error(transparent)]
    Present(#[from] ShellPresentError),
    #[error("Tensor Shell received configure for unknown surface {0:?}")]
    UnknownConfiguredSurface(SurfaceId),
    #[error("Tensor Shell surface {0:?} has no Vulkan-compatible host handle")]
    MissingSurfaceHandle(SurfaceId),
    #[error("Tensor Shell surface {0:?} has no configured physical buffer extent")]
    MissingBufferExtent(SurfaceId),
    #[error("Tensor Shell notification store lock is poisoned")]
    NotificationStorePoisoned,
}
