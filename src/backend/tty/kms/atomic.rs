//! Tensor's atomic-only KMS surface.
//!
//! The property layout and mode lifecycle are adapted from Smithay's atomic
//! DRM backend. See `LICENSES/Smithay-MIT.txt`. Tensor keeps only its product
//! contract: one connector, one CRTC, one primary plane, explicit modifiers,
//! and mandatory input fences.

use std::{
    os::fd::{AsFd, AsRawFd, BorrowedFd},
    rc::Rc,
};

use drm::control::{
    AtomicCommitFlags, Device as ControlDevice, Mode, ResourceHandle, connector, crtc, framebuffer,
    plane, property,
};
use smithay::backend::drm::DrmDeviceFd;
use tensor_host::DrmFormat;
use thiserror::Error;
use tracing::warn;

const NO_INPUT_FENCE: u64 = u64::MAX;

mod formats;

pub(in crate::backend::tty) use formats::primary_plane_formats;
pub(super) use formats::select_primary_plane;

#[derive(Debug)]
pub(super) struct AtomicSurface {
    device: DrmDeviceFd,
    active: Rc<std::cell::Cell<bool>>,
    connector: connector::Handle,
    crtc: crtc::Handle,
    plane: plane::Handle,
    mode: Mode,
    mode_blob: u64,
    properties: AtomicProperties,
    modeset: ModesetRequest,
    needs_modeset: bool,
}

impl AtomicSurface {
    pub(super) fn new(
        device: DrmDeviceFd,
        active: Rc<std::cell::Cell<bool>>,
        connector: connector::Handle,
        crtc: crtc::Handle,
        plane: plane::Handle,
        mode: Mode,
        framebuffer: framebuffer::Handle,
    ) -> Result<Self, AtomicError> {
        let mode_blob = create_mode_blob(&device, mode)?;
        let initialized = (|| {
            let properties = AtomicProperties::load(&device, connector, crtc, plane)?;
            let mut modeset = ModesetRequest::new(
                &device,
                connector,
                crtc,
                plane,
                mode,
                mode_blob,
                &properties,
            )?;
            modeset.commit(
                device.as_fd(),
                AtomicCommitFlags::TEST_ONLY | AtomicCommitFlags::ALLOW_MODESET,
                framebuffer,
                None,
            )?;
            Ok((properties, modeset))
        })();
        let (properties, modeset) = match initialized {
            Ok(initialized) => initialized,
            Err(error) => {
                let _ = device.destroy_property_blob(mode_blob);
                return Err(error);
            }
        };

        Ok(Self {
            device,
            active,
            connector,
            crtc,
            plane,
            mode,
            mode_blob,
            properties,
            modeset,
            needs_modeset: true,
        })
    }

    #[inline]
    pub(super) fn crtc(&self) -> crtc::Handle {
        self.crtc
    }

    #[inline]
    pub(super) fn plane(&self) -> plane::Handle {
        self.plane
    }

    pub(super) fn submit(
        &mut self,
        framebuffer: framebuffer::Handle,
        fence: BorrowedFd<'_>,
    ) -> Result<(), AtomicError> {
        if !self.active.get() {
            return Err(AtomicError::DeviceInactive);
        }
        if self.needs_modeset {
            self.modeset.commit(
                self.device.as_fd(),
                AtomicCommitFlags::PAGE_FLIP_EVENT | AtomicCommitFlags::ALLOW_MODESET,
                framebuffer,
                Some(fence),
            )?;
            self.needs_modeset = false;
            return Ok(());
        }

        // Fixed two-property request: no allocation, lock, staging, or request clone
        // occurs on the steady-state present path.
        let mut objects = [u32::from(self.plane)];
        let mut property_counts = [2];
        let mut properties = [
            u32::from(self.properties.plane.framebuffer),
            u32::from(self.properties.plane.input_fence),
        ];
        let mut values = [
            u64::from(u32::from(framebuffer)),
            fence.as_raw_fd() as i64 as u64,
        ];
        drm_ffi::mode::atomic_commit(
            self.device.as_fd(),
            (AtomicCommitFlags::PAGE_FLIP_EVENT | AtomicCommitFlags::NONBLOCK).bits(),
            &mut objects,
            &mut property_counts,
            &mut properties,
            &mut values,
        )
        .map_err(AtomicError::Commit)
    }

    pub(super) fn reset_after_session_resume(
        &mut self,
        framebuffer: framebuffer::Handle,
    ) -> Result<(), AtomicError> {
        if !self.active.get() {
            return Err(AtomicError::DeviceInactive);
        }
        let mode_blob = create_mode_blob(&self.device, self.mode)?;
        let refreshed = (|| {
            let properties =
                AtomicProperties::load(&self.device, self.connector, self.crtc, self.plane)?;
            let mut modeset = ModesetRequest::new(
                &self.device,
                self.connector,
                self.crtc,
                self.plane,
                self.mode,
                mode_blob,
                &properties,
            )?;
            modeset.commit(
                self.device.as_fd(),
                AtomicCommitFlags::TEST_ONLY | AtomicCommitFlags::ALLOW_MODESET,
                framebuffer,
                None,
            )?;
            Ok((properties, modeset))
        })();
        let (properties, modeset) = match refreshed {
            Ok(refreshed) => refreshed,
            Err(error) => {
                let _ = self.device.destroy_property_blob(mode_blob);
                return Err(error);
            }
        };

        let previous_blob = std::mem::replace(&mut self.mode_blob, mode_blob);
        self.properties = properties;
        self.modeset = modeset;
        self.needs_modeset = true;
        if let Err(error) = self.device.destroy_property_blob(previous_blob) {
            warn!(%error, "failed to destroy pre-resume KMS mode blob");
        }
        Ok(())
    }

    pub(super) fn clear(&mut self) -> Result<(), AtomicError> {
        if !self.active.get() {
            return Ok(());
        }
        let mut objects = [
            u32::from(self.connector),
            u32::from(self.crtc),
            u32::from(self.plane),
        ];
        let mut property_counts = [1, 2, 2];
        let mut properties = [
            u32::from(self.properties.connector_crtc),
            u32::from(self.properties.crtc.active),
            u32::from(self.properties.crtc.mode),
            u32::from(self.properties.plane.crtc),
            u32::from(self.properties.plane.framebuffer),
        ];
        let mut values = [0; 5];
        drm_ffi::mode::atomic_commit(
            self.device.as_fd(),
            AtomicCommitFlags::ALLOW_MODESET.bits(),
            &mut objects,
            &mut property_counts,
            &mut properties,
            &mut values,
        )
        .map_err(AtomicError::Clear)?;
        self.needs_modeset = true;
        Ok(())
    }
}

impl Drop for AtomicSurface {
    fn drop(&mut self) {
        if let Err(error) = self.device.destroy_property_blob(self.mode_blob)
            && self.active.get()
        {
            warn!(%error, "failed to destroy KMS mode blob");
        }
    }
}

#[derive(Debug)]
struct AtomicProperties {
    connector_crtc: property::Handle,
    crtc: CrtcProperties,
    plane: PlaneProperties,
}

impl AtomicProperties {
    fn load(
        device: &impl ControlDevice,
        connector: connector::Handle,
        crtc: crtc::Handle,
        plane: plane::Handle,
    ) -> Result<Self, AtomicError> {
        let connector_props = PropertySnapshot::load(device, connector)?;
        let crtc_props = PropertySnapshot::load(device, crtc)?;
        let plane_props = PropertySnapshot::load(device, plane)?;
        Ok(Self {
            connector_crtc: connector_props.required("CRTC_ID", PropertyKind::Crtc)?,
            crtc: CrtcProperties {
                active: crtc_props.required("ACTIVE", PropertyKind::Boolean)?,
                mode: crtc_props.required("MODE_ID", PropertyKind::Blob)?,
            },
            plane: PlaneProperties {
                crtc: plane_props.required("CRTC_ID", PropertyKind::Crtc)?,
                framebuffer: plane_props.required("FB_ID", PropertyKind::Framebuffer)?,
                source_x: plane_props.required("SRC_X", PropertyKind::Unsigned)?,
                source_y: plane_props.required("SRC_Y", PropertyKind::Unsigned)?,
                source_width: plane_props.required("SRC_W", PropertyKind::Unsigned)?,
                source_height: plane_props.required("SRC_H", PropertyKind::Unsigned)?,
                crtc_x: plane_props.required("CRTC_X", PropertyKind::Signed)?,
                crtc_y: plane_props.required("CRTC_Y", PropertyKind::Signed)?,
                crtc_width: plane_props.required("CRTC_W", PropertyKind::Unsigned)?,
                crtc_height: plane_props.required("CRTC_H", PropertyKind::Unsigned)?,
                input_fence: plane_props.required("IN_FENCE_FD", PropertyKind::Signed)?,
            },
        })
    }
}

#[derive(Debug)]
struct CrtcProperties {
    active: property::Handle,
    mode: property::Handle,
}

#[derive(Debug)]
struct PlaneProperties {
    crtc: property::Handle,
    framebuffer: property::Handle,
    source_x: property::Handle,
    source_y: property::Handle,
    source_width: property::Handle,
    source_height: property::Handle,
    crtc_x: property::Handle,
    crtc_y: property::Handle,
    crtc_width: property::Handle,
    crtc_height: property::Handle,
    input_fence: property::Handle,
}

#[derive(Debug)]
struct ModesetRequest {
    objects: Vec<u32>,
    property_counts: Vec<u32>,
    properties: Vec<u32>,
    values: Vec<u64>,
    framebuffer_value: usize,
    fence_value: usize,
}

impl ModesetRequest {
    #[allow(clippy::too_many_arguments)]
    fn new(
        device: &impl ControlDevice,
        connector: connector::Handle,
        crtc: crtc::Handle,
        plane: plane::Handle,
        mode: Mode,
        mode_blob: u64,
        properties: &AtomicProperties,
    ) -> Result<Self, AtomicError> {
        let detached = connectors_to_detach(device, connector, crtc)?;
        let object_count = 3 + detached.len();
        let mut request = Self {
            objects: Vec::with_capacity(object_count),
            property_counts: Vec::with_capacity(object_count),
            properties: Vec::with_capacity(14 + detached.len()),
            values: Vec::with_capacity(14 + detached.len()),
            framebuffer_value: 0,
            fence_value: 0,
        };
        request.push_object(
            connector,
            &[(properties.connector_crtc, u64::from(u32::from(crtc)))],
        );
        for (connector, crtc_property) in detached {
            request.push_object(connector, &[(crtc_property, 0)]);
        }
        request.push_object(
            crtc,
            &[
                (properties.crtc.active, 1),
                (properties.crtc.mode, mode_blob),
            ],
        );
        let (width, height) = mode.size();
        let width = u64::from(width);
        let height = u64::from(height);
        request.push_object(
            plane,
            &[
                (properties.plane.crtc, u64::from(u32::from(crtc))),
                (properties.plane.framebuffer, 0),
                (properties.plane.source_x, 0),
                (properties.plane.source_y, 0),
                (properties.plane.source_width, width << 16),
                (properties.plane.source_height, height << 16),
                (properties.plane.crtc_x, 0),
                (properties.plane.crtc_y, 0),
                (properties.plane.crtc_width, width),
                (properties.plane.crtc_height, height),
                (properties.plane.input_fence, NO_INPUT_FENCE),
            ],
        );
        request.framebuffer_value = request
            .properties
            .iter()
            .position(|property| *property == u32::from(properties.plane.framebuffer))
            .expect("framebuffer property was inserted");
        request.fence_value = request
            .properties
            .iter()
            .position(|property| *property == u32::from(properties.plane.input_fence))
            .expect("input-fence property was inserted");
        Ok(request)
    }

    fn push_object(&mut self, object: impl ResourceHandle, properties: &[(property::Handle, u64)]) {
        let object: drm::control::RawResourceHandle = object.into();
        self.objects.push(u32::from(object));
        self.property_counts.push(properties.len() as u32);
        self.properties
            .extend(properties.iter().map(|(property, _)| u32::from(*property)));
        self.values
            .extend(properties.iter().map(|(_, value)| *value));
    }

    fn commit(
        &mut self,
        device: BorrowedFd<'_>,
        flags: AtomicCommitFlags,
        framebuffer: framebuffer::Handle,
        fence: Option<BorrowedFd<'_>>,
    ) -> Result<(), AtomicError> {
        self.values[self.framebuffer_value] = u64::from(u32::from(framebuffer));
        self.values[self.fence_value] = fence
            .map(|fence| fence.as_raw_fd() as i64 as u64)
            .unwrap_or(NO_INPUT_FENCE);
        drm_ffi::mode::atomic_commit(
            device,
            flags.bits(),
            &mut self.objects,
            &mut self.property_counts,
            &mut self.properties,
            &mut self.values,
        )
        .map_err(if flags.contains(AtomicCommitFlags::TEST_ONLY) {
            AtomicError::Test
        } else {
            AtomicError::Commit
        })
    }
}

fn connectors_to_detach(
    device: &impl ControlDevice,
    selected: connector::Handle,
    crtc: crtc::Handle,
) -> Result<Vec<(connector::Handle, property::Handle)>, AtomicError> {
    let resources = device.resource_handles().map_err(AtomicError::Resources)?;
    let mut detached = Vec::new();
    for &connector in resources.connectors() {
        if connector == selected {
            continue;
        }
        let properties = PropertySnapshot::load(device, connector)?;
        let Some((crtc_property, value)) = properties.optional("CRTC_ID", PropertyKind::Crtc)?
        else {
            continue;
        };
        if value == u64::from(u32::from(crtc)) {
            detached.push((connector, crtc_property));
        }
    }
    Ok(detached)
}

struct PropertySnapshot {
    object: u32,
    entries: Vec<(property::Info, u64)>,
}

impl PropertySnapshot {
    fn load(
        device: &impl ControlDevice,
        resource: impl ResourceHandle,
    ) -> Result<Self, AtomicError> {
        let values = device
            .get_properties(resource)
            .map_err(AtomicError::Properties)?;
        let mut entries = Vec::with_capacity(values.as_props_and_values().0.len());
        for (handle, value) in values {
            entries.push((
                device.get_property(handle).map_err(AtomicError::Property)?,
                value,
            ));
        }
        Ok(Self {
            object: {
                let resource: drm::control::RawResourceHandle = resource.into();
                u32::from(resource)
            },
            entries,
        })
    }

    fn required(
        &self,
        name: &'static str,
        kind: PropertyKind,
    ) -> Result<property::Handle, AtomicError> {
        self.optional(name, kind)?
            .map(|(handle, _)| handle)
            .ok_or(AtomicError::MissingProperty {
                object: self.object,
                name,
            })
    }

    fn optional(
        &self,
        name: &'static str,
        kind: PropertyKind,
    ) -> Result<Option<(property::Handle, u64)>, AtomicError> {
        let Some((info, value)) = self
            .entries
            .iter()
            .find(|(info, _)| info.name().to_bytes() == name.as_bytes())
        else {
            return Ok(None);
        };
        if !kind.matches(&info.value_type()) {
            return Err(AtomicError::PropertyType {
                object: self.object,
                name,
                expected: kind.name(),
                actual: format!("{:?}", info.value_type()),
            });
        }
        Ok(Some((info.handle(), *value)))
    }
}

#[derive(Clone, Copy)]
enum PropertyKind {
    Boolean,
    Unsigned,
    Signed,
    Enum,
    Blob,
    Crtc,
    Framebuffer,
}

impl PropertyKind {
    fn matches(self, actual: &property::ValueType) -> bool {
        matches!(
            (self, actual),
            (Self::Boolean, property::ValueType::Boolean)
                | (Self::Unsigned, property::ValueType::UnsignedRange(_, _))
                | (Self::Signed, property::ValueType::SignedRange(_, _))
                | (Self::Enum, property::ValueType::Enum(_))
                | (Self::Blob, property::ValueType::Blob)
                | (Self::Crtc, property::ValueType::CRTC)
                | (Self::Framebuffer, property::ValueType::Framebuffer)
        )
    }

    fn name(self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::Unsigned => "unsigned range",
            Self::Signed => "signed range",
            Self::Enum => "enum",
            Self::Blob => "blob",
            Self::Crtc => "CRTC object",
            Self::Framebuffer => "framebuffer object",
        }
    }
}

fn create_mode_blob(device: &impl ControlDevice, mode: Mode) -> Result<u64, AtomicError> {
    let blob: property::RawValue = device
        .create_property_blob(&mode)
        .map_err(AtomicError::CreateModeBlob)?
        .into();
    if blob == 0 {
        return Err(AtomicError::ZeroModeBlob);
    }
    Ok(blob)
}

#[derive(Debug, Error)]
pub(in crate::backend::tty) enum AtomicError {
    #[error("atomic KMS device is inactive")]
    DeviceInactive,
    #[error("DRM resource enumeration failed: {0}")]
    Resources(std::io::Error),
    #[error("DRM plane enumeration failed: {0}")]
    Planes(std::io::Error),
    #[error("DRM plane query failed: {0}")]
    Plane(std::io::Error),
    #[error("DRM property query failed: {0}")]
    Properties(std::io::Error),
    #[error("DRM property metadata query failed: {0}")]
    Property(std::io::Error),
    #[error("DRM capability query failed: {0}")]
    Capability(std::io::Error),
    #[error("DRM device does not support explicit framebuffer modifiers")]
    ExplicitModifiersUnsupported,
    #[error("DRM object {object} is missing required property {name}")]
    MissingProperty { object: u32, name: &'static str },
    #[error("DRM object {object} property {name} has type {actual}, expected {expected}")]
    PropertyType {
        object: u32,
        name: &'static str,
        expected: &'static str,
        actual: String,
    },
    #[error("CRTC {0:?} has no explicit-modifier primary plane")]
    NoPrimaryPlane(crtc::Handle),
    #[error("CRTC {crtc:?} has no unclaimed primary plane for {format:?}")]
    NoPrimaryPlaneForFormat {
        crtc: crtc::Handle,
        format: DrmFormat,
    },
    #[error("primary plane {0:?} has a zero IN_FORMATS blob")]
    EmptyFormatBlob(plane::Handle),
    #[error("primary plane {0:?} advertises no explicit formats")]
    NoExplicitFormats(plane::Handle),
    #[error("failed to read the IN_FORMATS property blob: {0}")]
    FormatBlob(std::io::Error),
    #[error("unsupported IN_FORMATS blob version {0}")]
    FormatBlobVersion(u32),
    #[error("unsupported IN_FORMATS blob flags {0:#x}")]
    FormatBlobFlags(u32),
    #[error("malformed IN_FORMATS blob: {0}")]
    MalformedFormatBlob(&'static str),
    #[error("IN_FORMATS {0} table starts inside its header")]
    BlobRegionBeforeHeader(&'static str),
    #[error("IN_FORMATS {0} table size overflows")]
    BlobRegionOverflow(&'static str),
    #[error("IN_FORMATS {0} table lies outside the blob")]
    BlobRegionOutOfBounds(&'static str),
    #[error("IN_FORMATS modifier {modifier} references format {format}, but only {count} exist")]
    ModifierFormatOutOfBounds {
        modifier: usize,
        format: usize,
        count: usize,
    },
    #[error("failed to create the KMS mode blob: {0}")]
    CreateModeBlob(std::io::Error),
    #[error("DRM returned a zero KMS mode blob")]
    ZeroModeBlob,
    #[error("atomic KMS TEST_ONLY commit failed: {0}")]
    Test(std::io::Error),
    #[error("atomic KMS commit failed: {0}")]
    Commit(std::io::Error),
    #[error("atomic KMS clear failed: {0}")]
    Clear(std::io::Error),
}
