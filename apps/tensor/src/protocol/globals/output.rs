//! Tensor-owned `wl_output` and `zxdg_output_v1` wire state.

mod geometry;

use std::sync::{
    Arc, Mutex, Weak as ArcWeak,
    atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering},
};

use arc_swap::ArcSwap;
use tensor_host::{ConnectorId, PhysicalMode, SubpixelLayout};
use tensor_protocol::SurfaceTransform;
use tensor_util::OutputScale;
use wayland_protocols::xdg::xdg_output::zv1::server::{
    zxdg_output_manager_v1::{self, ZxdgOutputManagerV1},
    zxdg_output_v1::{self, ZxdgOutputV1},
};
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, New, Resource, Weak as ResourceWeak,
    backend::{ClientId, GlobalId},
    protocol::{
        wl_output::{self, Mode as WlMode, WlOutput},
        wl_surface::WlSurface,
    },
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

use geometry::{
    integer_scale, logical_length_round, subpixel_code, subpixel_from_code, transform_code,
    transform_from_code, transformed_dimensions, wl_subpixel, wl_transform,
};

const WL_OUTPUT_VERSION: u32 = 4;
const XDG_OUTPUT_VERSION: u32 = 3;
static NEXT_OUTPUT_INSTANCE: AtomicU64 = AtomicU64::new(1);

/// One compositor-side `wl_output` global lifetime.
///
/// A connector can disappear and later return with the same `ConnectorId`.
/// Protocol roles attached to the retired global must not migrate to the new
/// global, so wire identity is deliberately distinct from hardware identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct OutputInstanceId(u64);

pub(crate) struct OutputProtocol {
    _xdg_output_manager: GlobalId,
}

impl OutputProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _xdg_output_manager: display.create_global::<RuntimeState, ZxdgOutputManagerV1, _>(
                XDG_OUTPUT_VERSION,
                XdgOutputManagerGlobalData,
            ),
        }
    }

    pub(super) const fn xdg_output_enabled(&self) -> bool {
        true
    }
}

/// Coherent, lock-free output state used by layout, capture, and frame paths.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OutputSnapshot {
    pub(crate) mode: Option<PhysicalMode>,
    pub(crate) location: (i32, i32),
    pub(crate) physical_size: (i32, i32),
    pub(crate) subpixel: SubpixelLayout,
    pub(crate) scale: OutputScale,
    pub(crate) transform: SurfaceTransform,
}

#[derive(Debug)]
struct AtomicOutputSnapshot {
    sequence: AtomicU64,
    mode_valid: AtomicBool,
    mode_width: AtomicI32,
    mode_height: AtomicI32,
    refresh_millihertz: AtomicI32,
    location_x: AtomicI32,
    location_y: AtomicI32,
    physical_width: AtomicI32,
    physical_height: AtomicI32,
    subpixel: AtomicU32,
    scale_units: AtomicU32,
    transform: AtomicU32,
}

impl AtomicOutputSnapshot {
    fn new(snapshot: OutputSnapshot) -> Self {
        let mode = snapshot.mode.unwrap_or(PhysicalMode::new(0, 0, 0));
        Self {
            sequence: AtomicU64::new(0),
            mode_valid: AtomicBool::new(snapshot.mode.is_some()),
            mode_width: AtomicI32::new(mode.width),
            mode_height: AtomicI32::new(mode.height),
            refresh_millihertz: AtomicI32::new(mode.refresh_millihertz),
            location_x: AtomicI32::new(snapshot.location.0),
            location_y: AtomicI32::new(snapshot.location.1),
            physical_width: AtomicI32::new(snapshot.physical_size.0),
            physical_height: AtomicI32::new(snapshot.physical_size.1),
            subpixel: AtomicU32::new(subpixel_code(snapshot.subpixel)),
            scale_units: AtomicU32::new(snapshot.scale.units()),
            transform: AtomicU32::new(transform_code(snapshot.transform)),
        }
    }

    #[inline]
    fn load(&self) -> OutputSnapshot {
        loop {
            let before = self.sequence.load(Ordering::Acquire);
            if before & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            let mode = self.mode_valid.load(Ordering::Relaxed).then(|| {
                PhysicalMode::new(
                    self.mode_width.load(Ordering::Relaxed),
                    self.mode_height.load(Ordering::Relaxed),
                    self.refresh_millihertz.load(Ordering::Relaxed),
                )
            });
            let snapshot = OutputSnapshot {
                mode,
                location: (
                    self.location_x.load(Ordering::Relaxed),
                    self.location_y.load(Ordering::Relaxed),
                ),
                physical_size: (
                    self.physical_width.load(Ordering::Relaxed),
                    self.physical_height.load(Ordering::Relaxed),
                ),
                subpixel: subpixel_from_code(self.subpixel.load(Ordering::Relaxed)),
                scale: OutputScale::from_units(self.scale_units.load(Ordering::Relaxed))
                    .unwrap_or(OutputScale::ONE),
                transform: transform_from_code(self.transform.load(Ordering::Relaxed)),
            };
            let after = self.sequence.load(Ordering::Acquire);
            if before == after {
                return snapshot;
            }
        }
    }

    fn store(&self, snapshot: OutputSnapshot) {
        self.sequence.fetch_add(1, Ordering::AcqRel);
        if let Some(mode) = snapshot.mode {
            self.mode_width.store(mode.width, Ordering::Relaxed);
            self.mode_height.store(mode.height, Ordering::Relaxed);
            self.refresh_millihertz
                .store(mode.refresh_millihertz, Ordering::Relaxed);
        }
        self.mode_valid
            .store(snapshot.mode.is_some(), Ordering::Relaxed);
        self.location_x
            .store(snapshot.location.0, Ordering::Relaxed);
        self.location_y
            .store(snapshot.location.1, Ordering::Relaxed);
        self.physical_width
            .store(snapshot.physical_size.0, Ordering::Relaxed);
        self.physical_height
            .store(snapshot.physical_size.1, Ordering::Relaxed);
        self.subpixel
            .store(subpixel_code(snapshot.subpixel), Ordering::Relaxed);
        self.scale_units
            .store(snapshot.scale.units(), Ordering::Relaxed);
        self.transform
            .store(transform_code(snapshot.transform), Ordering::Relaxed);
        self.sequence.fetch_add(1, Ordering::Release);
    }
}

#[derive(Debug)]
struct OutputResources {
    modes: Arc<[PhysicalMode]>,
    preferred_mode: Option<PhysicalMode>,
    wl_outputs: Vec<ResourceWeak<WlOutput>>,
    xdg_outputs: Vec<ResourceWeak<ZxdgOutputV1>>,
    surfaces: Vec<ResourceWeak<WlSurface>>,
}

#[derive(Debug)]
struct OutputInner {
    id: ConnectorId,
    instance: OutputInstanceId,
    name: Arc<str>,
    description: Arc<str>,
    live: AtomicBool,
    snapshot: AtomicOutputSnapshot,
    /// RCU snapshot: presentation completion reads this without a mutex.
    wl_resource_snapshot: ArcSwap<Vec<WlOutput>>,
    resources: Mutex<OutputResources>,
}

#[derive(Clone, Debug)]
pub(crate) struct Output(Arc<OutputInner>);

impl PartialEq for Output {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Output {}

#[derive(Clone, Debug)]
pub(crate) struct WeakOutput(ArcWeak<OutputInner>);

impl WeakOutput {
    pub(crate) fn upgrade(&self) -> Option<Output> {
        let output = Output(self.0.upgrade()?);
        output.is_live().then_some(output)
    }
}

impl Output {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: ConnectorId,
        name: String,
        physical_size: (i32, i32),
        subpixel: SubpixelLayout,
        modes: Vec<PhysicalMode>,
        current_mode: PhysicalMode,
        preferred_mode: PhysicalMode,
        scale: OutputScale,
    ) -> Self {
        let name: Arc<str> = name.into();
        let instance = NEXT_OUTPUT_INSTANCE.fetch_add(1, Ordering::Relaxed);
        assert_ne!(instance, 0, "wl_output instance token exhausted");
        Self(Arc::new(OutputInner {
            id,
            instance: OutputInstanceId(instance),
            description: Arc::clone(&name),
            name,
            live: AtomicBool::new(true),
            snapshot: AtomicOutputSnapshot::new(OutputSnapshot {
                mode: Some(current_mode),
                location: (0, 0),
                physical_size,
                subpixel,
                scale,
                transform: SurfaceTransform::Normal,
            }),
            wl_resource_snapshot: ArcSwap::from_pointee(Vec::new()),
            resources: Mutex::new(OutputResources {
                modes: modes.into(),
                preferred_mode: Some(preferred_mode),
                wl_outputs: Vec::new(),
                xdg_outputs: Vec::new(),
                surfaces: Vec::new(),
            }),
        }))
    }

    pub(crate) fn create_global(&self, display: &DisplayHandle) -> GlobalId {
        display.create_global::<RuntimeState, WlOutput, _>(
            WL_OUTPUT_VERSION,
            WlOutputGlobalData {
                output: self.downgrade(),
            },
        )
    }

    #[inline]
    pub(crate) fn id(&self) -> ConnectorId {
        self.0.id
    }

    #[inline]
    pub(crate) fn instance_id(&self) -> OutputInstanceId {
        self.0.instance
    }

    #[inline]
    pub(crate) fn name(&self) -> &str {
        &self.0.name
    }

    #[inline]
    pub(crate) fn snapshot(&self) -> OutputSnapshot {
        self.0.snapshot.load()
    }

    #[inline]
    pub(crate) fn current_mode(&self) -> Option<PhysicalMode> {
        self.snapshot().mode
    }

    #[inline]
    pub(crate) fn current_scale(&self) -> OutputScale {
        self.snapshot().scale
    }

    #[inline]
    #[cfg(test)]
    pub(crate) fn current_location(&self) -> (i32, i32) {
        self.snapshot().location
    }

    pub(crate) fn downgrade(&self) -> WeakOutput {
        WeakOutput(Arc::downgrade(&self.0))
    }

    pub(crate) fn from_resource(resource: &WlOutput) -> Option<Self> {
        let output = resource.data::<WlOutputData>()?.output.clone();
        output.is_live().then_some(output)
    }

    pub(crate) fn from_resource_including_inactive(resource: &WlOutput) -> Option<Self> {
        Some(resource.data::<WlOutputData>()?.output.clone())
    }

    pub(crate) fn logical_size(&self) -> (u32, u32) {
        let snapshot = self.snapshot();
        let Some(mode) = snapshot.mode else {
            return (0, 0);
        };
        let (width, height) = transformed_dimensions(
            logical_length_round(mode.width, snapshot.scale),
            logical_length_round(mode.height, snapshot.scale),
            snapshot.transform,
        );
        (
            u32::try_from(width).unwrap_or(0),
            u32::try_from(height).unwrap_or(0),
        )
    }

    pub(crate) fn reconfigure(
        &self,
        physical_size: (i32, i32),
        subpixel: SubpixelLayout,
        modes: Vec<PhysicalMode>,
        current_mode: PhysicalMode,
        preferred_mode: PhysicalMode,
        scale: OutputScale,
    ) {
        let mut resources = self.0.resources.lock().unwrap();
        let previous = self.snapshot();
        self.0.snapshot.store(OutputSnapshot {
            mode: Some(current_mode),
            physical_size,
            subpixel,
            scale,
            ..previous
        });
        resources.modes = modes.into();
        resources.preferred_mode = Some(preferred_mode);
        self.send_state(&mut resources);
    }

    pub(crate) fn set_location(&self, location: (i32, i32)) {
        let mut resources = self.0.resources.lock().unwrap();
        let previous = self.snapshot();
        if previous.location == location {
            return;
        }
        self.0.snapshot.store(OutputSnapshot {
            location,
            ..previous
        });
        self.send_state(&mut resources);
    }

    pub(crate) fn deactivate(&self) {
        if !self.0.live.swap(false, Ordering::AcqRel) {
            return;
        }
        // `WlOutputData` retains this protocol object until the client
        // releases its resource. Drop the RCU resource snapshot first so the
        // output and its `WlOutput` handles cannot retain each other.
        self.0.wl_resource_snapshot.store(Arc::new(Vec::new()));
        let mut resources = self.0.resources.lock().unwrap();
        let surfaces = std::mem::take(&mut resources.surfaces);
        for surface in surfaces {
            let Ok(surface) = surface.upgrade() else {
                continue;
            };
            for wl_output in &resources.wl_outputs {
                let Ok(wl_output) = wl_output.upgrade() else {
                    continue;
                };
                if wl_output.client() == surface.client() {
                    surface.leave(&wl_output);
                }
            }
        }
    }

    pub(crate) fn enter(&self, surface: &WlSurface) {
        if !self.is_live() {
            return;
        }
        let mut resources = self.0.resources.lock().unwrap();
        if resources
            .surfaces
            .iter()
            .any(|entry| entry.id() == surface.id())
        {
            return;
        }
        resources.surfaces.push(surface.downgrade());
        for output in &resources.wl_outputs {
            let Ok(output) = output.upgrade() else {
                continue;
            };
            if output.client() == surface.client() {
                surface.enter(&output);
            }
        }
    }

    pub(crate) fn leave(&self, surface: &WlSurface) {
        let mut resources = self.0.resources.lock().unwrap();
        let Some(index) = resources
            .surfaces
            .iter()
            .position(|entry| entry.id() == surface.id())
        else {
            return;
        };
        resources.surfaces.swap_remove(index);
        for output in &resources.wl_outputs {
            let Ok(output) = output.upgrade() else {
                continue;
            };
            if output.client() == surface.client() {
                surface.leave(&output);
            }
        }
    }

    /// Forget a destroyed surface without attempting to send a leave event
    /// to the already-dead protocol object.
    pub(crate) fn forget_surface(&self, surface: &WlSurface) {
        self.0
            .resources
            .lock()
            .unwrap()
            .surfaces
            .retain(|entry| entry.id() != surface.id());
    }

    #[cfg(test)]
    pub(crate) fn contains_surface(&self, surface: &WlSurface) -> bool {
        self.0
            .resources
            .lock()
            .unwrap()
            .surfaces
            .iter()
            .any(|current| current.id() == surface.id())
    }

    pub(crate) fn cleanup(&self) {
        let mut resources = self.0.resources.lock().unwrap();
        resources.surfaces.retain(|surface| surface.is_alive());
        let previous_outputs = resources.wl_outputs.len();
        resources.wl_outputs.retain(|output| output.is_alive());
        resources.xdg_outputs.retain(|output| output.is_alive());
        if resources.wl_outputs.len() != previous_outputs {
            self.publish_wl_resources(&resources);
        }
    }

    pub(crate) fn for_each_client_resource(
        &self,
        client: &Client,
        mut visit: impl FnMut(&WlOutput),
    ) {
        let resources = self.0.wl_resource_snapshot.load();
        for output in resources.iter() {
            if output.client().as_ref() == Some(client) {
                visit(output);
            }
        }
    }

    pub(crate) fn is_live(&self) -> bool {
        self.0.live.load(Ordering::Acquire)
    }

    fn send_state(&self, resources: &mut OutputResources) {
        resources.xdg_outputs.retain(|output| output.is_alive());
        resources.wl_outputs.retain(|output| output.is_alive());
        let snapshot = self.snapshot();
        for output in &resources.xdg_outputs {
            if let Ok(output) = output.upgrade() {
                send_xdg_state(&output, self, snapshot);
            }
        }
        for output in &resources.wl_outputs {
            if let Ok(output) = output.upgrade() {
                send_wl_state(&output, self, resources, snapshot, true);
            }
        }
    }

    fn publish_wl_resources(&self, resources: &OutputResources) {
        if !self.is_live() {
            self.0.wl_resource_snapshot.store(Arc::new(Vec::new()));
            return;
        }
        let live = resources
            .wl_outputs
            .iter()
            .filter_map(|output| output.upgrade().ok())
            .collect();
        self.0.wl_resource_snapshot.store(Arc::new(live));
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct WlOutputGlobalData {
    output: WeakOutput,
}

#[derive(Debug)]
pub(in crate::protocol) struct WlOutputData {
    output: Output,
}

#[derive(Debug)]
pub(in crate::protocol) struct XdgOutputManagerGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct XdgOutputManagerData;

#[derive(Debug)]
pub(in crate::protocol) struct XdgOutputData {
    output: Option<WeakOutput>,
}

impl<D> GlobalDispatchDelegate<WlOutput, D> for WlOutputGlobalData
where
    D: Dispatch<WlOutput, WlOutputData> + 'static,
{
    fn bind(
        &self,
        _state: &mut D,
        _display: &DisplayHandle,
        client: &Client,
        resource: New<WlOutput>,
        data_init: &mut DataInit<'_, D>,
    ) {
        let output = self
            .output
            .upgrade()
            .expect("a published wl_output global retains a live output");
        let output_resource = data_init.init(
            resource,
            WlOutputData {
                output: output.clone(),
            },
        );
        let mut resources = output.0.resources.lock().unwrap();
        send_wl_state(
            &output_resource,
            &output,
            &resources,
            output.snapshot(),
            false,
        );
        for surface in &resources.surfaces {
            let Ok(surface) = surface.upgrade() else {
                continue;
            };
            if surface.client().as_ref() == Some(client) {
                surface.enter(&output_resource);
            }
        }
        resources.wl_outputs.push(output_resource.downgrade());
        output.publish_wl_resources(&resources);
    }
}

impl<D> DispatchDelegate<WlOutput, D> for WlOutputData
where
    D: 'static,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        _resource: &WlOutput,
        _request: wl_output::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
    }

    fn destroyed(&self, _state: &mut D, _client: ClientId, resource: &WlOutput) {
        let mut resources = self.output.0.resources.lock().unwrap();
        resources
            .wl_outputs
            .retain(|entry| entry.id() != resource.id());
        self.output.publish_wl_resources(&resources);
    }
}

impl<D> GlobalDispatchDelegate<ZxdgOutputManagerV1, D> for XdgOutputManagerGlobalData
where
    D: Dispatch<ZxdgOutputManagerV1, XdgOutputManagerData> + 'static,
{
    fn bind(
        &self,
        _state: &mut D,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ZxdgOutputManagerV1>,
        data_init: &mut DataInit<'_, D>,
    ) {
        data_init.init(resource, XdgOutputManagerData);
    }
}

impl<D> DispatchDelegate<ZxdgOutputManagerV1, D> for XdgOutputManagerData
where
    D: Dispatch<ZxdgOutputV1, XdgOutputData> + 'static,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        _manager: &ZxdgOutputManagerV1,
        request: zxdg_output_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            zxdg_output_manager_v1::Request::GetXdgOutput {
                id,
                output: wl_output,
            } => {
                let output = Output::from_resource(&wl_output);
                let xdg_output = data_init.init(
                    id,
                    XdgOutputData {
                        output: output.as_ref().map(Output::downgrade),
                    },
                );
                if let Some(output) = output {
                    let mut resources = output.0.resources.lock().unwrap();
                    send_xdg_state(&xdg_output, &output, output.snapshot());
                    if wl_output.version() >= 2 {
                        wl_output.done();
                    }
                    resources.xdg_outputs.push(xdg_output.downgrade());
                }
            }
            zxdg_output_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl<D> DispatchDelegate<ZxdgOutputV1, D> for XdgOutputData
where
    D: 'static,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        _resource: &ZxdgOutputV1,
        _request: zxdg_output_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
    }

    fn destroyed(&self, _state: &mut D, _client: ClientId, resource: &ZxdgOutputV1) {
        if let Some(output) = self.output.as_ref().and_then(WeakOutput::upgrade) {
            output
                .0
                .resources
                .lock()
                .unwrap()
                .xdg_outputs
                .retain(|entry| entry.id() != resource.id());
        }
    }
}

fn send_wl_state(
    resource: &WlOutput,
    output: &Output,
    resources: &OutputResources,
    snapshot: OutputSnapshot,
    update: bool,
) {
    resource.geometry(
        snapshot.location.0,
        snapshot.location.1,
        snapshot.physical_size.0,
        snapshot.physical_size.1,
        wl_subpixel(snapshot.subpixel),
        "Tensor".to_owned(),
        output.name().to_owned(),
        wl_transform(snapshot.transform),
    );
    for mode in resources.modes.iter().copied() {
        let mut flags = WlMode::empty();
        if Some(mode) == snapshot.mode {
            flags |= WlMode::Current;
        }
        if Some(mode) == resources.preferred_mode {
            flags |= WlMode::Preferred;
        }
        resource.mode(flags, mode.width, mode.height, mode.refresh_millihertz);
    }
    if !update && resource.version() >= 4 {
        resource.name(output.name().to_owned());
        resource.description(output.0.description.to_string());
    }
    if resource.version() >= 2 {
        resource.scale(integer_scale(snapshot.scale));
        resource.done();
    }
}

fn send_xdg_state(resource: &ZxdgOutputV1, output: &Output, snapshot: OutputSnapshot) {
    resource.logical_position(snapshot.location.0, snapshot.location.1);
    if let Some(mode) = snapshot.mode {
        let (width, height) = transformed_dimensions(
            logical_length_round(mode.width, snapshot.scale),
            logical_length_round(mode.height, snapshot.scale),
            snapshot.transform,
        );
        resource.logical_size(width, height);
    }
    if resource.version() >= 2 {
        resource.name(output.name().to_owned());
        resource.description(output.0.description.to_string());
    }
    if resource.version() < 3 {
        resource.done();
    }
}

delegate_global_dispatch!(RuntimeState, WlOutput, WlOutputGlobalData);
delegate_dispatch!(RuntimeState, WlOutput, WlOutputData);
delegate_global_dispatch!(
    RuntimeState,
    ZxdgOutputManagerV1,
    XdgOutputManagerGlobalData
);
delegate_dispatch!(RuntimeState, ZxdgOutputManagerV1, XdgOutputManagerData);
delegate_dispatch!(RuntimeState, ZxdgOutputV1, XdgOutputData);

#[cfg(test)]
mod tests;
