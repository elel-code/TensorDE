//! Bounded cursor-plane capability discovery for one Vulkan-selected DRM device.

use drm::{
    DriverCapability,
    control::{Device as ControlDevice, PlaneType, crtc, plane},
};
use tensor_host::{DrmFormat, Fourcc, OutputFormat};
use tracing::warn;

use crate::render::VulkanFormatCapability;

use super::formats::parse_format_modifier_blob;
use super::{AtomicError, PlaneProperties, PropertyKind, PropertySnapshot};

const MAX_CURSOR_PLANES: usize = 8;
const MAX_CURSOR_FORMATS: usize = 128;

#[derive(Debug)]
pub(in crate::backend::tty::kms) struct CursorPlaneCapabilities {
    max_width: u32,
    max_height: u32,
    planes: [Option<CursorPlaneCapability>; MAX_CURSOR_PLANES],
    len: usize,
}

impl CursorPlaneCapabilities {
    fn new(max_width: u32, max_height: u32) -> Self {
        Self {
            max_width,
            max_height,
            planes: std::array::from_fn(|_| None),
            len: 0,
        }
    }

    pub(in crate::backend::tty::kms) fn unavailable() -> Self {
        Self::new(0, 0)
    }

    pub(in crate::backend::tty::kms) fn max_size(&self) -> (u32, u32) {
        (self.max_width, self.max_height)
    }

    pub(in crate::backend::tty::kms) fn len(&self) -> usize {
        self.len
    }

    pub(in crate::backend::tty::kms) fn iter(
        &self,
    ) -> impl Iterator<Item = &CursorPlaneCapability> {
        self.planes[..self.len]
            .iter()
            .map(|plane| plane.as_ref().expect("populated cursor-plane prefix"))
    }

    #[cfg(test)]
    fn select_vulkan_target(
        &self,
        vulkan: &[VulkanFormatCapability],
    ) -> Option<CursorPlaneSelection> {
        self.select_vulkan_target_excluding(vulkan, &[])
    }

    pub(in crate::backend::tty::kms) fn select_vulkan_target_excluding(
        &self,
        vulkan: &[VulkanFormatCapability],
        claimed: &[plane::Handle],
    ) -> Option<CursorPlaneSelection> {
        self.iter().find_map(|plane| {
            if claimed.contains(&plane.handle) {
                return None;
            }
            plane
                .preferred_vulkan_format(vulkan)
                .map(|format| CursorPlaneSelection {
                    plane: plane.handle,
                    format,
                })
        })
    }

    fn push(&mut self, candidate: CursorPlaneCapability) -> Result<(), plane::Handle> {
        if self.len == MAX_CURSOR_PLANES {
            return Err(candidate.handle);
        }
        self.planes[self.len] = Some(candidate);
        self.len += 1;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::backend::tty::kms) struct CursorPlaneSelection {
    plane: plane::Handle,
    format: OutputFormat,
}

impl CursorPlaneSelection {
    pub(in crate::backend::tty::kms) fn plane(self) -> plane::Handle {
        self.plane
    }

    pub(in crate::backend::tty::kms) fn format(self) -> OutputFormat {
        self.format
    }
}

#[derive(Debug)]
pub(in crate::backend::tty::kms) struct CursorPlaneCapability {
    handle: plane::Handle,
    formats: ExplicitFormats,
    _properties: PlaneProperties,
}

impl CursorPlaneCapability {
    pub(in crate::backend::tty::kms) fn handle(&self) -> plane::Handle {
        self.handle
    }

    pub(in crate::backend::tty::kms) fn format_count(&self) -> usize {
        self.formats.entries.len()
    }

    fn preferred_vulkan_format(&self, vulkan: &[VulkanFormatCapability]) -> Option<OutputFormat> {
        let mut compatible = vulkan
            .iter()
            .copied()
            .filter(|candidate| candidate.supports_output_export())
            .filter(|candidate| candidate.plane_count > 0)
            .filter(|candidate| !candidate.format.modifier.is_invalid())
            .filter(|candidate| cursor_format_rank(candidate.format.code).is_some())
            .filter(|candidate| self.formats.entries.contains(&candidate.format))
            .map(|candidate| OutputFormat {
                format: candidate.format,
                plane_count: candidate.plane_count,
            })
            .collect::<Vec<_>>();
        compatible.sort_unstable_by_key(|candidate| {
            (
                cursor_format_rank(candidate.format.code).expect("alpha format was filtered"),
                candidate.format.modifier.is_linear(),
                candidate.format.modifier.raw(),
                candidate.plane_count,
            )
        });
        compatible.dedup();
        compatible.into_iter().next()
    }
}

const fn cursor_format_rank(format: Fourcc) -> Option<u8> {
    if format.raw() == Fourcc::ARGB8888.raw() {
        Some(0)
    } else if format.raw() == Fourcc::ABGR8888.raw() {
        Some(1)
    } else if format.raw() == Fourcc::ARGB2101010.raw() {
        Some(2)
    } else if format.raw() == Fourcc::ABGR2101010.raw() {
        Some(3)
    } else {
        None
    }
}

#[derive(Debug)]
struct ExplicitFormats {
    entries: Box<[DrmFormat]>,
}

impl ExplicitFormats {
    fn new(plane: plane::Handle, mut formats: Vec<DrmFormat>) -> Result<Self, AtomicError> {
        formats.sort_unstable();
        formats.dedup();
        if formats.len() > MAX_CURSOR_FORMATS {
            return Err(AtomicError::CursorFormatCapacity {
                plane,
                count: formats.len(),
                limit: MAX_CURSOR_FORMATS,
            });
        }
        Ok(Self {
            entries: formats.into_boxed_slice(),
        })
    }

    #[cfg(test)]
    fn as_slice(&self) -> &[DrmFormat] {
        &self.entries
    }
}

pub(in crate::backend::tty::kms) fn discover_cursor_planes(
    device: &impl ControlDevice,
    crtc: crtc::Handle,
) -> Result<CursorPlaneCapabilities, AtomicError> {
    let max_width = device
        .get_driver_capability(DriverCapability::CursorWidth)
        .map_err(AtomicError::Capability)?;
    let max_height = device
        .get_driver_capability(DriverCapability::CursorHeight)
        .map_err(AtomicError::Capability)?;
    let (max_width, max_height) = cursor_dimensions(max_width, max_height)?;
    let resources = device.resource_handles().map_err(AtomicError::Resources)?;
    let mut handles = device.plane_handles().map_err(AtomicError::Planes)?;
    handles.sort_unstable_by_key(|handle| u32::from(*handle));

    let mut capabilities = CursorPlaneCapabilities::new(max_width, max_height);
    for handle in handles {
        let info = match device.get_plane(handle) {
            Ok(info) => info,
            Err(error) => {
                warn!(plane = u32::from(handle), %error, "cannot query DRM cursor-plane candidate");
                continue;
            }
        };
        if !resources
            .filter_crtcs(info.possible_crtcs())
            .contains(&crtc)
        {
            continue;
        }
        match discover_candidate(device, handle) {
            Ok(Some(candidate)) => {
                if let Err(dropped) = capabilities.push(candidate) {
                    warn!(
                        plane = u32::from(dropped),
                        limit = MAX_CURSOR_PLANES,
                        "DRM cursor-plane capacity reached; ignoring higher plane ID"
                    );
                }
            }
            Ok(None) => {}
            Err(error) => {
                warn!(plane = u32::from(handle), %error, "rejecting ineligible DRM cursor plane");
            }
        }
    }
    Ok(capabilities)
}

fn discover_candidate(
    device: &impl ControlDevice,
    handle: plane::Handle,
) -> Result<Option<CursorPlaneCapability>, AtomicError> {
    let properties = PropertySnapshot::load(device, handle)?;
    let Some((_, plane_type)) = properties.optional("type", PropertyKind::Enum)? else {
        return Err(AtomicError::MissingProperty {
            object: u32::from(handle),
            name: "type",
        });
    };
    if plane_type != PlaneType::Cursor as u32 as u64 {
        return Ok(None);
    }
    let (_, blob) = properties
        .optional("IN_FORMATS", PropertyKind::Blob)?
        .ok_or(AtomicError::MissingProperty {
            object: u32::from(handle),
            name: "IN_FORMATS",
        })?;
    if blob == 0 {
        return Err(AtomicError::EmptyFormatBlob(handle));
    }
    let formats = parse_format_modifier_blob(
        &device
            .get_property_blob(blob)
            .map_err(AtomicError::FormatBlob)?,
    )?;
    if formats.is_empty() {
        return Err(AtomicError::NoExplicitFormats(handle));
    }
    Ok(Some(CursorPlaneCapability {
        handle,
        formats: ExplicitFormats::new(handle, formats)?,
        _properties: PlaneProperties::load(&properties)?,
    }))
}

fn cursor_dimensions(width: u64, height: u64) -> Result<(u32, u32), AtomicError> {
    let valid =
        width != 0 && height != 0 && u32::try_from(width).is_ok() && u32::try_from(height).is_ok();
    if !valid {
        return Err(AtomicError::InvalidCursorDimensions { width, height });
    }
    Ok((width as u32, height as u32))
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use tensor_host::{Fourcc, Modifier};

    use super::*;

    fn plane_handle(raw: u32) -> plane::Handle {
        NonZeroU32::new(raw).unwrap().into()
    }

    fn capability_with_formats(raw: u32, formats: Vec<DrmFormat>) -> CursorPlaneCapability {
        CursorPlaneCapability {
            handle: plane_handle(raw),
            formats: ExplicitFormats::new(plane_handle(raw), formats).unwrap(),
            _properties: PlaneProperties::resolve(|_, _| Ok(NonZeroU32::new(1).unwrap().into()))
                .unwrap(),
        }
    }

    fn capability(raw: u32) -> CursorPlaneCapability {
        capability_with_formats(
            raw,
            vec![DrmFormat::new(Fourcc::ARGB8888, Modifier::from_raw(7))],
        )
    }

    fn vulkan_format(format: DrmFormat) -> VulkanFormatCapability {
        VulkanFormatCapability {
            format,
            plane_count: 1,
            renderable: true,
            importable: true,
            exportable: true,
        }
    }

    #[test]
    fn cursor_capabilities_have_fixed_capacity_and_stable_prefix_order() {
        let mut capabilities = CursorPlaneCapabilities::new(256, 128);
        for id in 1..=(MAX_CURSOR_PLANES as u32) {
            capabilities.push(capability(id)).unwrap();
        }
        assert_eq!(capabilities.push(capability(99)), Err(plane_handle(99)));
        assert_eq!(capabilities.max_size(), (256, 128));
        assert_eq!(
            capabilities
                .iter()
                .map(|candidate| u32::from(candidate.handle()))
                .collect::<Vec<_>>(),
            (1..=(MAX_CURSOR_PLANES as u32)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn explicit_formats_are_sorted_and_deduplicated() {
        let argb = DrmFormat::new(Fourcc::ARGB8888, Modifier::from_raw(8));
        let xrgb = DrmFormat::new(Fourcc::XRGB8888, Modifier::from_raw(3));
        let formats = ExplicitFormats::new(plane_handle(3), vec![xrgb, argb, xrgb]).unwrap();
        assert_eq!(formats.as_slice(), &[argb, xrgb]);
    }

    #[test]
    fn cursor_target_selection_requires_exact_alpha_export_and_stable_plane_order() {
        let tiled_argb = DrmFormat::new(Fourcc::ARGB8888, Modifier::from_raw(9));
        let linear_argb = DrmFormat::linear(Fourcc::ARGB8888);
        let tiled_abgr = DrmFormat::new(Fourcc::ABGR8888, Modifier::from_raw(3));
        let xrgb = DrmFormat::new(Fourcc::XRGB8888, Modifier::from_raw(4));
        let mut capabilities = CursorPlaneCapabilities::new(256, 256);
        capabilities
            .push(capability_with_formats(2, vec![linear_argb, tiled_argb]))
            .unwrap();
        capabilities
            .push(capability_with_formats(3, vec![tiled_abgr, xrgb]))
            .unwrap();

        let selection = capabilities
            .select_vulkan_target(&[
                vulkan_format(xrgb),
                vulkan_format(tiled_abgr),
                vulkan_format(linear_argb),
                vulkan_format(tiled_argb),
            ])
            .unwrap();
        assert_eq!(selection.plane(), plane_handle(2));
        assert_eq!(selection.format().format, tiled_argb);
    }

    #[test]
    fn cursor_target_selection_skips_planes_claimed_by_other_outputs() {
        let format = DrmFormat::new(Fourcc::ARGB8888, Modifier::from_raw(9));
        let mut capabilities = CursorPlaneCapabilities::new(256, 256);
        capabilities
            .push(capability_with_formats(2, vec![format]))
            .unwrap();
        capabilities
            .push(capability_with_formats(3, vec![format]))
            .unwrap();

        let selection = capabilities
            .select_vulkan_target_excluding(&[vulkan_format(format)], &[plane_handle(2)])
            .unwrap();
        assert_eq!(selection.plane(), plane_handle(3));
    }

    #[test]
    fn cursor_target_selection_rejects_non_exportable_and_unspecified_formats() {
        let explicit = DrmFormat::new(Fourcc::ARGB8888, Modifier::from_raw(5));
        let mut capabilities = CursorPlaneCapabilities::new(64, 64);
        capabilities
            .push(capability_with_formats(1, vec![explicit]))
            .unwrap();
        let mut not_exportable = vulkan_format(explicit);
        not_exportable.exportable = false;
        let unspecified = vulkan_format(DrmFormat::new(Fourcc::ARGB8888, Modifier::INVALID));
        assert!(
            capabilities
                .select_vulkan_target(&[not_exportable, unspecified])
                .is_none()
        );
    }

    #[test]
    fn explicit_format_storage_rejects_capacity_overflow() {
        let formats = (0..=MAX_CURSOR_FORMATS)
            .map(|modifier| DrmFormat::new(Fourcc::ARGB8888, Modifier::from_raw(modifier as u64)))
            .collect();
        assert!(matches!(
            ExplicitFormats::new(plane_handle(3), formats),
            Err(AtomicError::CursorFormatCapacity {
                count,
                limit: MAX_CURSOR_FORMATS,
                ..
            }) if count == MAX_CURSOR_FORMATS + 1
        ));
    }

    #[test]
    fn required_cursor_properties_reject_a_missing_input_fence() {
        let error = PlaneProperties::resolve(|name, _| {
            if name == "IN_FENCE_FD" {
                Err(AtomicError::MissingProperty { object: 77, name })
            } else {
                Ok(NonZeroU32::new(1).unwrap().into())
            }
        })
        .unwrap_err();
        assert!(matches!(
            error,
            AtomicError::MissingProperty {
                object: 77,
                name: "IN_FENCE_FD"
            }
        ));
    }

    #[test]
    fn cursor_dimensions_reject_zero_and_u32_overflow() {
        assert_eq!(cursor_dimensions(64, 128).unwrap(), (64, 128));
        assert!(cursor_dimensions(0, 64).is_err());
        assert!(cursor_dimensions(u64::from(u32::MAX) + 1, 64).is_err());
    }
}
