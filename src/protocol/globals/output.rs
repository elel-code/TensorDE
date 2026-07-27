//! Tensor-owned `wl_output` and `zxdg_output_v1` wire state.

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

const WL_OUTPUT_VERSION: u32 = 4;
const XDG_OUTPUT_VERSION: u32 = 3;

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
        Self(Arc::new(OutputInner {
            id,
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
    #[cfg(test)]
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
        resource.data::<WlOutputData>()?.output.upgrade()
    }

    pub(crate) fn owns(&self, resource: &WlOutput) -> bool {
        Self::from_resource(resource).is_some_and(|output| output == *self)
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

    fn is_live(&self) -> bool {
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
    output: WeakOutput,
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
        let weak = self.output.clone();
        let output_resource = data_init.init(resource, WlOutputData { output: weak });
        let Some(output) = self.output.upgrade() else {
            return;
        };
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
        if let Some(output) = self.output.upgrade() {
            let mut resources = output.0.resources.lock().unwrap();
            resources
                .wl_outputs
                .retain(|entry| entry.id() != resource.id());
            output.publish_wl_resources(&resources);
        }
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

fn integer_scale(scale: OutputScale) -> i32 {
    i32::try_from(scale.units().div_ceil(OutputScale::DENOMINATOR))
        .unwrap_or(i32::MAX)
        .max(1)
}

fn logical_length_round(value: i32, scale: OutputScale) -> i32 {
    (f64::from(value) / scale.as_f64()).round() as i32
}

pub(crate) fn transformed_dimensions(
    width: i32,
    height: i32,
    transform: SurfaceTransform,
) -> (i32, i32) {
    match transform {
        SurfaceTransform::Rotate90
        | SurfaceTransform::Rotate270
        | SurfaceTransform::Flipped90
        | SurfaceTransform::Flipped270 => (height, width),
        _ => (width, height),
    }
}

fn wl_subpixel(subpixel: SubpixelLayout) -> wl_output::Subpixel {
    match subpixel {
        SubpixelLayout::Unknown => wl_output::Subpixel::Unknown,
        SubpixelLayout::None => wl_output::Subpixel::None,
        SubpixelLayout::HorizontalRgb => wl_output::Subpixel::HorizontalRgb,
        SubpixelLayout::HorizontalBgr => wl_output::Subpixel::HorizontalBgr,
        SubpixelLayout::VerticalRgb => wl_output::Subpixel::VerticalRgb,
        SubpixelLayout::VerticalBgr => wl_output::Subpixel::VerticalBgr,
    }
}

fn wl_transform(transform: SurfaceTransform) -> wl_output::Transform {
    match transform {
        SurfaceTransform::Normal => wl_output::Transform::Normal,
        SurfaceTransform::Rotate90 => wl_output::Transform::_90,
        SurfaceTransform::Rotate180 => wl_output::Transform::_180,
        SurfaceTransform::Rotate270 => wl_output::Transform::_270,
        SurfaceTransform::Flipped => wl_output::Transform::Flipped,
        SurfaceTransform::Flipped90 => wl_output::Transform::Flipped90,
        SurfaceTransform::Flipped180 => wl_output::Transform::Flipped180,
        SurfaceTransform::Flipped270 => wl_output::Transform::Flipped270,
    }
}

const fn subpixel_code(subpixel: SubpixelLayout) -> u32 {
    match subpixel {
        SubpixelLayout::Unknown => 0,
        SubpixelLayout::None => 1,
        SubpixelLayout::HorizontalRgb => 2,
        SubpixelLayout::HorizontalBgr => 3,
        SubpixelLayout::VerticalRgb => 4,
        SubpixelLayout::VerticalBgr => 5,
    }
}

const fn subpixel_from_code(code: u32) -> SubpixelLayout {
    match code {
        1 => SubpixelLayout::None,
        2 => SubpixelLayout::HorizontalRgb,
        3 => SubpixelLayout::HorizontalBgr,
        4 => SubpixelLayout::VerticalRgb,
        5 => SubpixelLayout::VerticalBgr,
        _ => SubpixelLayout::Unknown,
    }
}

const fn transform_code(transform: SurfaceTransform) -> u32 {
    match transform {
        SurfaceTransform::Normal => 0,
        SurfaceTransform::Rotate90 => 1,
        SurfaceTransform::Rotate180 => 2,
        SurfaceTransform::Rotate270 => 3,
        SurfaceTransform::Flipped => 4,
        SurfaceTransform::Flipped90 => 5,
        SurfaceTransform::Flipped180 => 6,
        SurfaceTransform::Flipped270 => 7,
    }
}

const fn transform_from_code(code: u32) -> SurfaceTransform {
    match code {
        1 => SurfaceTransform::Rotate90,
        2 => SurfaceTransform::Rotate180,
        3 => SurfaceTransform::Rotate270,
        4 => SurfaceTransform::Flipped,
        5 => SurfaceTransform::Flipped90,
        6 => SurfaceTransform::Flipped180,
        7 => SurfaceTransform::Flipped270,
        _ => SurfaceTransform::Normal,
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
