use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::Duration,
};

use wayland_client_runtime::{
    Event, LayerSurfaceEvent, OutputId, PointerEvent, PointerEventKind, Runtime, RuntimeError,
    SurfaceEvent, SurfaceId, TouchEvent, TouchEventKind,
};

use crate::notification_service::NotificationServiceHandle;
use crate::panel::PanelInteraction;
use crate::present::{PanelPresentation, ShellPresenter};
use crate::system_status::PowerServiceHandle;
use crate::{
    NotificationServiceError, NotificationStore, PanelAppletEmphasis, PanelAppletState,
    PanelAppletStore, PanelAppletUpdate, PanelConfig, PanelScene, PanelWidgetKind, ShellComponent,
    ShellConfig, ShellConfigError, ShellModel, ShellPresentError, SurfaceKey,
    TensorlandConfigEndpoint,
};

const BTN_LEFT: u32 = 0x110;

/// Protocol runtime for the shell's model-driven layer surfaces.
pub struct ShellRuntime {
    wayland: Runtime,
    model: ShellModel,
    panel_config: PanelConfig,
    tensorland: TensorlandConfigEndpoint,
    surfaces: BTreeMap<SurfaceKey, SurfaceId>,
    surface_keys: BTreeMap<SurfaceId, SurfaceKey>,
    configured_surfaces: BTreeSet<SurfaceId>,
    panel_scenes: BTreeMap<SurfaceId, PanelScene>,
    panel_input: BTreeMap<SurfaceId, PanelInteraction>,
    panel_touches: BTreeMap<i32, PanelTouch>,
    panel_applets: PanelAppletStore,
    events: Vec<Event>,
    notifications: NotificationServiceHandle,
    power: PowerServiceHandle,
    power_revision: u64,
    presenter: Option<ShellPresenter>,
}

impl ShellRuntime {
    pub fn connect() -> Result<Self, ShellRuntimeError> {
        Self::connect_with_config(ShellConfig::load_default_path()?)
    }

    pub fn connect_with_config(config: ShellConfig) -> Result<Self, ShellRuntimeError> {
        let wayland = Runtime::connect()?;
        if !wayland.capabilities().layer_shell_v1 {
            return Err(RuntimeError::Unsupported("layer-shell-v1").into());
        }
        let notifications =
            NotificationServiceHandle::start(Arc::new(Mutex::new(NotificationStore::default())))?;
        let power = PowerServiceHandle::start();
        Ok(Self {
            wayland,
            model: ShellModel::new(config.layout),
            panel_config: config.panel,
            tensorland: config.tensorland,
            surfaces: BTreeMap::new(),
            surface_keys: BTreeMap::new(),
            configured_surfaces: BTreeSet::new(),
            panel_scenes: BTreeMap::new(),
            panel_input: BTreeMap::new(),
            panel_touches: BTreeMap::new(),
            panel_applets: PanelAppletStore::default(),
            events: Vec::with_capacity(128),
            notifications,
            power,
            power_revision: 0,
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
            self.reconcile_panel_services()?;
            self.reconcile_surfaces()?;
            self.events = events;
        }
    }

    fn reconcile_panel_services(&mut self) -> Result<(), ShellRuntimeError> {
        let now_ms = self.notifications.now_ms();
        let (show_popups, notification_state) = {
            let mut store = self
                .notifications
                .store()
                .lock()
                .map_err(|_| ShellRuntimeError::NotificationStorePoisoned)?;
            for closed in store.expire(now_ms) {
                self.notifications.emit_closed(closed)?;
            }
            let show_popups = store.visible_popups().next().is_some();
            let emphasis = if store.has_active_critical() {
                PanelAppletEmphasis::Critical
            } else if store.do_not_disturb() {
                PanelAppletEmphasis::Active
            } else {
                PanelAppletEmphasis::Normal
            };
            let state = PanelAppletState::ready()
                .with_badge(store.active_count())
                .with_emphasis(emphasis);
            (show_popups, state)
        };
        let outputs = self.model.output_ids().collect::<Vec<_>>();
        for output in outputs {
            self.model
                .set_visible(output, ShellComponent::NotificationPopups, show_popups);
        }
        self.panel_applets.apply(PanelAppletUpdate::new(
            PanelWidgetKind::Notifications,
            notification_state,
        ));
        let (power_revision, power) = self.power.read();
        if power_revision != self.power_revision {
            self.panel_applets.apply(PanelAppletUpdate::new(
                PanelWidgetKind::SystemStatus,
                power.panel_state(),
            ));
            self.power_revision = power_revision;
        }
        if self.panel_applets.take_dirty() {
            self.present_configured_panels()?;
        }
        Ok(())
    }

    fn handle_event(&mut self, event: &Event) -> Result<(), ShellRuntimeError> {
        match event {
            Event::Output(output) => self.model.apply_output_event(output.clone()),
            Event::LayerSurface(LayerSurfaceEvent::Configure { surface, .. }) => {
                self.configured_surfaces.insert(*surface);
                self.refresh_panel_scene(*surface)?;
                self.present_surface(*surface)?;
            }
            Event::LayerSurface(LayerSurfaceEvent::Closed { surface }) => {
                if let Some(key) = self.surface_keys.remove(surface) {
                    self.surfaces.remove(&key);
                    self.configured_surfaces.remove(surface);
                    self.remove_panel_state(*surface);
                    self.remove_presented_surface(*surface)?;
                    self.wayland.destroy_surface(*surface)?;
                }
            }
            Event::Surface(SurfaceEvent::ScaleFactorChanged { surface, .. })
                if self.configured_surfaces.contains(surface) =>
            {
                self.present_surface(*surface)?;
            }
            Event::Pointer(event) => self.handle_panel_pointer(event)?,
            Event::Touch(event) => self.handle_panel_touch(event)?,
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
        let interaction = self.panel_interaction(surface);
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
            .present(
                surface,
                extent,
                PanelPresentation::new(
                    self.panel_scenes.get(&surface),
                    interaction,
                    &self.panel_applets,
                ),
            )?;
        Ok(())
    }

    fn present_configured_panels(&mut self) -> Result<(), ShellRuntimeError> {
        let surfaces = self
            .surface_keys
            .iter()
            .filter(|(surface, key)| {
                key.component == ShellComponent::Panel && self.configured_surfaces.contains(surface)
            })
            .map(|(surface, _)| *surface)
            .collect::<Vec<_>>();
        for surface in surfaces {
            self.present_surface(surface)?;
        }
        Ok(())
    }

    fn refresh_panel_scene(&mut self, surface: SurfaceId) -> Result<(), ShellRuntimeError> {
        let is_panel = self
            .surface_keys
            .get(&surface)
            .is_some_and(|key| key.component == ShellComponent::Panel);
        if !is_panel {
            self.panel_scenes.remove(&surface);
            self.panel_input.remove(&surface);
            return Ok(());
        }
        let extent = self
            .wayland
            .logical_size(surface)
            .ok_or(ShellRuntimeError::MissingLogicalExtent(surface))?;
        self.panel_scenes
            .insert(surface, PanelScene::build(extent, &self.panel_config));
        self.panel_input.entry(surface).or_default();
        Ok(())
    }

    fn panel_interaction(&self, surface: SurfaceId) -> PanelInteraction {
        let mut interaction = self.panel_input.get(&surface).copied().unwrap_or_default();
        let Some(key) = self.surface_keys.get(&surface) else {
            return interaction;
        };
        interaction.active = [
            (ShellComponent::Launcher, PanelWidgetKind::Launcher),
            (
                ShellComponent::NotificationCenter,
                PanelWidgetKind::Notifications,
            ),
            (
                ShellComponent::ControlCenter,
                PanelWidgetKind::ControlCenter,
            ),
        ]
        .into_iter()
        .find_map(|(component, widget)| {
            self.model.visible(key.output, component).then_some(widget)
        });
        interaction
    }

    fn handle_panel_pointer(&mut self, event: &PointerEvent) -> Result<(), ShellRuntimeError> {
        let Some(scene) = self.panel_scenes.get(&event.surface) else {
            return Ok(());
        };
        let hit = scene
            .hit_test(event.position)
            .filter(|widget| widget.is_interactive());
        let state = self.panel_input.entry(event.surface).or_default();
        let previous = *state;
        let activation = match event.kind {
            PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                state.hovered = hit;
                None
            }
            PointerEventKind::Leave => {
                state.hovered = None;
                state.pressed = None;
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
        let changed = previous != *state;
        if let Some(widget) = activation {
            self.activate_panel_widget(event.surface, widget);
        }
        if changed || activation.is_some() {
            self.present_surface(event.surface)?;
        }
        Ok(())
    }

    fn handle_panel_touch(&mut self, event: &TouchEvent) -> Result<(), ShellRuntimeError> {
        match event.kind {
            TouchEventKind::Down { id, position, .. } => {
                let Some(surface) = event.surface else {
                    return Ok(());
                };
                let Some(hit) = self
                    .panel_scenes
                    .get(&surface)
                    .and_then(|scene| scene.hit_test(position))
                    .filter(|widget| widget.is_interactive())
                else {
                    return Ok(());
                };
                self.panel_touches.insert(
                    id,
                    PanelTouch {
                        surface,
                        pressed: hit,
                        current: Some(hit),
                    },
                );
                self.panel_input.entry(surface).or_default().pressed = Some(hit);
                self.present_surface(surface)?;
            }
            TouchEventKind::Motion { id, position, .. } => {
                let Some(touch) = self.panel_touches.get_mut(&id) else {
                    return Ok(());
                };
                let current = self
                    .panel_scenes
                    .get(&touch.surface)
                    .and_then(|scene| scene.hit_test(position))
                    .filter(|widget| widget.is_interactive());
                if touch.current != current {
                    touch.current = current;
                    self.panel_input.entry(touch.surface).or_default().pressed =
                        (current == Some(touch.pressed)).then_some(touch.pressed);
                    let surface = touch.surface;
                    self.present_surface(surface)?;
                }
            }
            TouchEventKind::Up { id, .. } => {
                let Some(touch) = self.panel_touches.remove(&id) else {
                    return Ok(());
                };
                self.panel_input.entry(touch.surface).or_default().pressed = None;
                if touch.current == Some(touch.pressed) {
                    self.activate_panel_widget(touch.surface, touch.pressed);
                }
                self.present_surface(touch.surface)?;
            }
            TouchEventKind::Cancelled => {
                let surfaces = self
                    .panel_touches
                    .values()
                    .map(|touch| touch.surface)
                    .collect::<BTreeSet<_>>();
                self.panel_touches.clear();
                for surface in surfaces {
                    self.panel_input.entry(surface).or_default().pressed = None;
                    self.present_surface(surface)?;
                }
            }
            TouchEventKind::Shape { .. } | TouchEventKind::Orientation { .. } => {}
        }
        Ok(())
    }

    fn activate_panel_widget(&mut self, surface: SurfaceId, widget: PanelWidgetKind) {
        let Some(component) = widget.activation() else {
            return;
        };
        let Some(key) = self.surface_keys.get(&surface).copied() else {
            return;
        };
        let visible = self.model.visible(key.output, component);
        self.model.set_visible(key.output, component, !visible);
    }

    fn remove_panel_state(&mut self, surface: SurfaceId) {
        self.panel_scenes.remove(&surface);
        self.panel_input.remove(&surface);
        self.panel_touches
            .retain(|_, touch| touch.surface != surface);
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
                self.remove_panel_state(surface);
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

    pub fn tensorland_endpoint(&self) -> &TensorlandConfigEndpoint {
        &self.tensorland
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PanelTouch {
    surface: SurfaceId,
    pressed: PanelWidgetKind,
    current: Option<PanelWidgetKind>,
}

#[derive(Debug, thiserror::Error)]
pub enum ShellRuntimeError {
    #[error(transparent)]
    Config(#[from] ShellConfigError),
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
    #[error("Tensor Shell panel surface {0:?} has no configured logical extent")]
    MissingLogicalExtent(SurfaceId),
    #[error("Tensor Shell notification store lock is poisoned")]
    NotificationStorePoisoned,
}
