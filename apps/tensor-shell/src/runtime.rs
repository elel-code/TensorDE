use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::Duration,
};

use wayland_client_runtime::{
    Event, LayerSurfaceEvent, OutputId, PointerEvent, PointerEventKind, Runtime, RuntimeError,
    SurfaceEvent, SurfaceId, TouchEvent, TouchEventKind,
};

use crate::config_reload::ShellConfigReloadHandle;
use crate::control_center_scene::{ControlCenterInteraction, ControlCenterScene};
use crate::media::MediaServiceHandle;
use crate::media_osd::MediaOsdState;
use crate::media_osd_scene::{MediaOsdInteraction, MediaOsdScene};
use crate::network::NetworkServiceHandle;
use crate::notification_scene::{NotificationInteraction, NotificationScene};
use crate::notification_service::NotificationServiceHandle;
use crate::overview::{OverviewServiceHandle, OverviewServiceSnapshot};
use crate::overview_scene::{OverviewInteraction, OverviewScene};
use crate::panel::PanelInteraction;
use crate::present::{RetainedSceneInput, ShellPresenter, SurfacePresentation};
use crate::session_lock_service::{SessionLockServiceError, SessionLockServiceHandle};
use crate::system_status::PowerServiceHandle;
use crate::{
    LauncherEndpoint, MediaActionState, MediaConfig, MediaServiceSnapshot,
    NotificationServiceError, NotificationStore, PanelAppletEmphasis, PanelAppletState,
    PanelAppletStore, PanelAppletUpdate, PanelConfig, PanelScene, PanelWidgetKind, ShellComponent,
    ShellConfig, ShellConfigError, ShellConfigReloadError, ShellModel, ShellPresentError,
    SurfaceKey, TensorlandConfigEndpoint,
};

const BTN_LEFT: u32 = 0x110;

mod config;
mod control_center;
mod media_osd;
mod notification;
mod overview;
mod session_lock;

/// Protocol runtime for the shell's model-driven layer surfaces.
pub struct ShellRuntime {
    wayland: Runtime,
    model: ShellModel,
    config_reload: Option<ShellConfigReloadHandle>,
    config_revision: u64,
    panel_config: PanelConfig,
    media_config: MediaConfig,
    launcher: LauncherEndpoint,
    tensorland: TensorlandConfigEndpoint,
    surfaces: BTreeMap<SurfaceKey, SurfaceId>,
    surface_keys: BTreeMap<SurfaceId, SurfaceKey>,
    configured_surfaces: BTreeSet<SurfaceId>,
    panel_scenes: BTreeMap<SurfaceId, PanelScene>,
    panel_input: BTreeMap<SurfaceId, PanelInteraction>,
    panel_touches: BTreeMap<i32, PanelTouch>,
    panel_applets: PanelAppletStore,
    overview: OverviewServiceHandle,
    overview_revision: u64,
    overview_snapshot: OverviewServiceSnapshot,
    overview_scenes: BTreeMap<SurfaceId, OverviewScene>,
    overview_input: BTreeMap<SurfaceId, OverviewInteraction>,
    notification_scenes: BTreeMap<SurfaceId, NotificationScene>,
    notification_input: BTreeMap<SurfaceId, NotificationInteraction>,
    control_center_scenes: BTreeMap<SurfaceId, ControlCenterScene>,
    control_center_input: BTreeMap<SurfaceId, ControlCenterInteraction>,
    control_center_revisions: (u64, u64, u64, u64, u64),
    events: Vec<Event>,
    notifications: NotificationServiceHandle,
    notification_revision: u64,
    power: PowerServiceHandle,
    power_revision: u64,
    media: MediaServiceHandle,
    media_revision: u64,
    media_snapshot: MediaServiceSnapshot,
    media_action: MediaActionState,
    media_osd: MediaOsdState,
    media_osd_observed_revision: u64,
    media_osd_revision: u64,
    media_osd_scenes: BTreeMap<SurfaceId, MediaOsdScene>,
    media_osd_input: BTreeMap<SurfaceId, MediaOsdInteraction>,
    network: NetworkServiceHandle,
    session_lock: SessionLockServiceHandle,
    session_lock_revision: u64,
    lock_surfaces: BTreeMap<OutputId, SurfaceId>,
    presenter: Option<ShellPresenter>,
}

impl ShellRuntime {
    pub fn connect() -> Result<Self, ShellRuntimeError> {
        let wayland = Self::connect_wayland()?;
        let (reload, config) =
            ShellConfigReloadHandle::start(ShellConfig::resolve_path(), wayland.wake_handle())?;
        Self::with_wayland(wayland, config, Some(reload))
    }

    pub fn connect_with_config(config: ShellConfig) -> Result<Self, ShellRuntimeError> {
        let wayland = Self::connect_wayland()?;
        Self::with_wayland(wayland, config, None)
    }

    fn connect_wayland() -> Result<Runtime, ShellRuntimeError> {
        let wayland = Runtime::connect()?;
        if !wayland.capabilities().layer_shell_v1 {
            return Err(RuntimeError::Unsupported("layer-shell-v1").into());
        }
        if !wayland.capabilities().session_lock_v1 {
            return Err(RuntimeError::Unsupported("ext-session-lock-v1").into());
        }
        Ok(wayland)
    }

    fn with_wayland(
        wayland: Runtime,
        config: ShellConfig,
        config_reload: Option<ShellConfigReloadHandle>,
    ) -> Result<Self, ShellRuntimeError> {
        let session_lock = SessionLockServiceHandle::start(wayland.wake_handle())?;
        let notifications =
            NotificationServiceHandle::start(Arc::new(Mutex::new(NotificationStore::default())))?;
        let power = PowerServiceHandle::start();
        let media = MediaServiceHandle::start(wayland.wake_handle());
        let network = NetworkServiceHandle::start(wayland.wake_handle());
        let overview = OverviewServiceHandle::start(
            config.tensorland.ipc_socket.clone(),
            wayland.wake_handle(),
        );
        Ok(Self {
            wayland,
            model: ShellModel::new(config.layout),
            config_reload,
            config_revision: 0,
            panel_config: config.panel,
            media_config: config.media,
            launcher: config.launcher,
            tensorland: config.tensorland,
            surfaces: BTreeMap::new(),
            surface_keys: BTreeMap::new(),
            configured_surfaces: BTreeSet::new(),
            panel_scenes: BTreeMap::new(),
            panel_input: BTreeMap::new(),
            panel_touches: BTreeMap::new(),
            panel_applets: PanelAppletStore::default(),
            overview,
            overview_revision: 0,
            overview_snapshot: OverviewServiceSnapshot::Pending,
            overview_scenes: BTreeMap::new(),
            overview_input: BTreeMap::new(),
            notification_scenes: BTreeMap::new(),
            notification_input: BTreeMap::new(),
            control_center_scenes: BTreeMap::new(),
            control_center_input: BTreeMap::new(),
            control_center_revisions: (0, 0, 0, 0, 0),
            events: Vec::with_capacity(128),
            notifications,
            notification_revision: 0,
            power,
            power_revision: 0,
            media,
            media_revision: 0,
            media_snapshot: MediaServiceSnapshot::Pending,
            media_action: MediaActionState::Idle,
            media_osd: MediaOsdState::default(),
            media_osd_observed_revision: 0,
            media_osd_revision: 0,
            media_osd_scenes: BTreeMap::new(),
            media_osd_input: BTreeMap::new(),
            network,
            session_lock,
            session_lock_revision: 0,
            lock_surfaces: BTreeMap::new(),
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
            self.reconcile_config()?;
            self.reconcile_session_lock_service()?;
            self.reconcile_overview_service()?;
            self.reconcile_panel_services()?;
            self.reconcile_media_osd()?;
            self.reconcile_notification_scenes()?;
            self.reconcile_control_center_scenes()?;
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
        let (media_revision, media, media_action) = self.media.read();
        if media_revision != self.media_revision {
            self.panel_applets.apply(PanelAppletUpdate::new(
                PanelWidgetKind::Media,
                media.panel_state(),
            ));
            self.media_snapshot = media;
            self.media_action = media_action;
            self.media_revision = media_revision;
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
                self.refresh_surface_scenes(*surface)?;
                self.present_surface(*surface)?;
            }
            Event::LayerSurface(LayerSurfaceEvent::Closed { surface }) => {
                if let Some(key) = self.surface_keys.remove(surface) {
                    self.surfaces.remove(&key);
                    self.configured_surfaces.remove(surface);
                    self.remove_surface_state(*surface);
                    self.remove_presented_surface(*surface)?;
                    self.wayland.destroy_surface(*surface)?;
                }
            }
            Event::SessionLock(event) => self.handle_session_lock_event(event)?,
            Event::Surface(SurfaceEvent::ScaleFactorChanged { surface, .. })
                if self.configured_surfaces.contains(surface) =>
            {
                self.present_surface(*surface)?;
            }
            Event::Pointer(event) => {
                self.handle_panel_pointer(event)?;
                self.handle_overview_pointer(event)?;
                self.handle_notification_pointer(event)?;
                self.handle_media_osd_pointer(event)?;
                self.handle_control_center_pointer(event)?;
            }
            Event::Keyboard(event) => {
                self.handle_overview_keyboard(event)?;
                self.handle_notification_keyboard(event)?;
                self.handle_control_center_keyboard(event)?;
            }
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
        let panel_interaction = self.panel_interaction(surface);
        let overview_interaction = self
            .overview_input
            .get(&surface)
            .copied()
            .unwrap_or_default();
        let notification_interaction = self
            .notification_input
            .get(&surface)
            .copied()
            .unwrap_or_default();
        let media_osd_interaction = self
            .media_osd_input
            .get(&surface)
            .copied()
            .unwrap_or_default();
        let control_center_interaction = self
            .control_center_input
            .get(&surface)
            .copied()
            .unwrap_or_default();
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
                SurfacePresentation::new(
                    RetainedSceneInput::new(self.panel_scenes.get(&surface), panel_interaction),
                    &self.panel_applets,
                    RetainedSceneInput::new(
                        self.overview_scenes.get(&surface),
                        overview_interaction,
                    ),
                    RetainedSceneInput::new(
                        self.notification_scenes.get(&surface),
                        notification_interaction,
                    ),
                    RetainedSceneInput::new(
                        self.media_osd_scenes.get(&surface),
                        media_osd_interaction,
                    ),
                    RetainedSceneInput::new(
                        self.control_center_scenes.get(&surface),
                        control_center_interaction,
                    ),
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

    fn refresh_surface_scenes(&mut self, surface: SurfaceId) -> Result<(), ShellRuntimeError> {
        self.refresh_panel_scene(surface)?;
        self.refresh_overview_scene(surface)?;
        self.refresh_notification_scene(surface)?;
        self.refresh_media_osd_scene(surface)?;
        self.refresh_control_center_scene(surface)
    }

    fn panel_interaction(&self, surface: SurfaceId) -> PanelInteraction {
        let mut interaction = self.panel_input.get(&surface).copied().unwrap_or_default();
        let Some(key) = self.surface_keys.get(&surface) else {
            return interaction;
        };
        interaction.active = [
            (ShellComponent::Overview, PanelWidgetKind::Workspaces),
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
            self.activate_panel_widget(event.surface, widget)?;
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
                    self.activate_panel_widget(touch.surface, touch.pressed)?;
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

    fn activate_panel_widget(
        &mut self,
        surface: SurfaceId,
        widget: PanelWidgetKind,
    ) -> Result<(), ShellRuntimeError> {
        if widget == PanelWidgetKind::Launcher {
            self.overview
                .spawn(self.launcher.command())
                .map_err(|error| ShellRuntimeError::OverviewCommand(error.to_string()))?;
            return Ok(());
        }
        let Some(component) = widget.activation() else {
            return Ok(());
        };
        let Some(key) = self.surface_keys.get(&surface).copied() else {
            return Ok(());
        };
        let visible = self.model.visible(key.output, component);
        self.model.set_visible(key.output, component, !visible);
        Ok(())
    }

    fn remove_surface_state(&mut self, surface: SurfaceId) {
        self.panel_scenes.remove(&surface);
        self.panel_input.remove(&surface);
        self.panel_touches
            .retain(|_, touch| touch.surface != surface);
        self.overview_scenes.remove(&surface);
        self.overview_input.remove(&surface);
        self.notification_scenes.remove(&surface);
        self.notification_input.remove(&surface);
        self.media_osd_scenes.remove(&surface);
        self.media_osd_input.remove(&surface);
        self.control_center_scenes.remove(&surface);
        self.control_center_input.remove(&surface);
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
                self.remove_surface_state(surface);
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
        if component == ShellComponent::LockScreen {
            return Err(ShellRuntimeError::SecureLockControlledByLogind);
        }
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
    ConfigReload(#[from] ShellConfigReloadError),
    #[error(transparent)]
    Wayland(#[from] RuntimeError),
    #[error(transparent)]
    NotificationService(#[from] NotificationServiceError),
    #[error(transparent)]
    SessionLockService(#[from] SessionLockServiceError),
    #[error(transparent)]
    MediaService(#[from] crate::MediaServiceError),
    #[error(transparent)]
    NetworkService(#[from] crate::NetworkServiceError),
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
    #[error("Tensor Shell overview command failed: {0}")]
    OverviewCommand(String),
    #[error("Tensor Shell secure lock is controlled by logind, not layer-surface visibility")]
    SecureLockControlledByLogind,
    #[error("Tensor Shell received conflicting session-lock surfaces for output {output:?}")]
    ConflictingLockSurface { output: OutputId },
    #[error("Tensor Shell's logind lock monitor stopped while the session was unlocked")]
    SessionLockMonitorStopped,
    #[error("the compositor finished Tensor Shell's session-lock request (locked={was_locked})")]
    SessionLockFinished { was_locked: bool },
}
