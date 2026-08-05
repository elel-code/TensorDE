//! Primary-plane discovery and strict shared `IN_FORMATS` parsing.

use drm::{
    DriverCapability,
    control::{Device as ControlDevice, PlaneType, crtc, plane},
};
use tensor_host::{DrmFormat, Fourcc, Modifier};

use super::{AtomicError, PropertyKind, PropertySnapshot};

const FORMAT_BLOB_HEADER_SIZE: usize = 24;
const FORMAT_MODIFIER_SIZE: usize = 24;

pub(in crate::backend::tty) fn primary_plane_formats(
    device: &impl ControlDevice,
    crtc: crtc::Handle,
) -> Result<Vec<DrmFormat>, AtomicError> {
    let planes = primary_planes(device, crtc)?;
    let mut formats = Vec::new();
    for plane in planes {
        for format in plane.formats {
            if !formats.contains(&format) {
                formats.push(format);
            }
        }
    }
    Ok(formats)
}

pub(in crate::backend::tty::kms) fn select_primary_plane(
    device: &impl ControlDevice,
    crtc: crtc::Handle,
    format: DrmFormat,
    claimed: &[plane::Handle],
) -> Result<plane::Handle, AtomicError> {
    primary_planes(device, crtc)?
        .into_iter()
        .find(|candidate| {
            !claimed.contains(&candidate.handle) && candidate.formats.contains(&format)
        })
        .map(|candidate| candidate.handle)
        .ok_or(AtomicError::NoPrimaryPlaneForFormat { crtc, format })
}

pub(in crate::backend::tty) fn select_lease_primary_plane(
    device: &impl ControlDevice,
    crtc: crtc::Handle,
    claimed: &[plane::Handle],
) -> Result<plane::Handle, AtomicError> {
    primary_plane_handles(device, crtc)?
        .into_iter()
        .find(|handle| !claimed.contains(handle))
        .ok_or(AtomicError::NoPrimaryPlane(crtc))
}

fn primary_plane_handles(
    device: &impl ControlDevice,
    crtc: crtc::Handle,
) -> Result<Vec<plane::Handle>, AtomicError> {
    let resources = device.resource_handles().map_err(AtomicError::Resources)?;
    let handles = device.plane_handles().map_err(AtomicError::Planes)?;
    let mut primary = Vec::new();
    for handle in handles {
        let info = device.get_plane(handle).map_err(AtomicError::Plane)?;
        if !resources
            .filter_crtcs(info.possible_crtcs())
            .contains(&crtc)
        {
            continue;
        }
        let properties = PropertySnapshot::load(device, handle)?;
        let (_, plane_type) = properties.optional("type", PropertyKind::Enum)?.ok_or(
            AtomicError::MissingProperty {
                object: u32::from(handle),
                name: "type",
            },
        )?;
        if plane_type == PlaneType::Primary as u32 as u64 {
            primary.push(handle);
        }
    }
    primary.sort_unstable_by_key(|handle| u32::from(*handle));
    if primary.is_empty() {
        return Err(AtomicError::NoPrimaryPlane(crtc));
    }
    Ok(primary)
}

fn primary_planes(
    device: &impl ControlDevice,
    crtc: crtc::Handle,
) -> Result<Vec<PrimaryPlane>, AtomicError> {
    if device
        .get_driver_capability(DriverCapability::AddFB2Modifiers)
        .map_err(AtomicError::Capability)?
        != 1
    {
        return Err(AtomicError::ExplicitModifiersUnsupported);
    }
    let resources = device.resource_handles().map_err(AtomicError::Resources)?;
    let handles = device.plane_handles().map_err(AtomicError::Planes)?;
    let mut primary = Vec::new();
    for handle in handles {
        let info = device.get_plane(handle).map_err(AtomicError::Plane)?;
        if !resources
            .filter_crtcs(info.possible_crtcs())
            .contains(&crtc)
        {
            continue;
        }
        let properties = PropertySnapshot::load(device, handle)?;
        let (_, plane_type) = properties.optional("type", PropertyKind::Enum)?.ok_or(
            AtomicError::MissingProperty {
                object: u32::from(handle),
                name: "type",
            },
        )?;
        if plane_type != PlaneType::Primary as u32 as u64 {
            continue;
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
        let data = device
            .get_property_blob(blob)
            .map_err(AtomicError::FormatBlob)?;
        let formats = parse_format_modifier_blob(&data)?;
        if formats.is_empty() {
            return Err(AtomicError::NoExplicitFormats(handle));
        }
        primary.push(PrimaryPlane { handle, formats });
    }
    primary.sort_unstable_by_key(|plane| u32::from(plane.handle));
    if primary.is_empty() {
        return Err(AtomicError::NoPrimaryPlane(crtc));
    }
    Ok(primary)
}

struct PrimaryPlane {
    handle: plane::Handle,
    formats: Vec<DrmFormat>,
}

pub(super) fn parse_format_modifier_blob(data: &[u8]) -> Result<Vec<DrmFormat>, AtomicError> {
    if data.len() < FORMAT_BLOB_HEADER_SIZE {
        return Err(AtomicError::MalformedFormatBlob("header is truncated"));
    }
    let version = read_u32(data, 0)?;
    let flags = read_u32(data, 4)?;
    if version != 1 {
        return Err(AtomicError::FormatBlobVersion(version));
    }
    if flags != 0 {
        return Err(AtomicError::FormatBlobFlags(flags));
    }
    let format_count = usize::try_from(read_u32(data, 8)?)
        .map_err(|_| AtomicError::MalformedFormatBlob("format count overflows usize"))?;
    let formats_offset = usize::try_from(read_u32(data, 12)?)
        .map_err(|_| AtomicError::MalformedFormatBlob("format offset overflows usize"))?;
    let modifier_count = usize::try_from(read_u32(data, 16)?)
        .map_err(|_| AtomicError::MalformedFormatBlob("modifier count overflows usize"))?;
    let modifiers_offset = usize::try_from(read_u32(data, 20)?)
        .map_err(|_| AtomicError::MalformedFormatBlob("modifier offset overflows usize"))?;
    let formats = checked_region(data, formats_offset, format_count, 4, "formats")?;
    let modifiers = checked_region(
        data,
        modifiers_offset,
        modifier_count,
        FORMAT_MODIFIER_SIZE,
        "modifiers",
    )?;
    if regions_overlap(
        formats_offset,
        formats.len(),
        modifiers_offset,
        modifiers.len(),
    ) {
        return Err(AtomicError::MalformedFormatBlob(
            "format and modifier tables overlap",
        ));
    }

    let mut result = Vec::new();
    for index in 0..modifier_count {
        let base = index * FORMAT_MODIFIER_SIZE;
        let mask = read_u64(modifiers, base)?;
        let offset = usize::try_from(read_u32(modifiers, base + 8)?)
            .map_err(|_| AtomicError::MalformedFormatBlob("modifier offset overflows usize"))?;
        let padding = read_u32(modifiers, base + 12)?;
        if padding != 0 {
            return Err(AtomicError::MalformedFormatBlob(
                "modifier padding is non-zero",
            ));
        }
        let modifier = Modifier::from_raw(read_u64(modifiers, base + 16)?);
        if modifier.is_invalid() {
            return Err(AtomicError::ImplicitFormatModifier(index));
        }
        for bit in 0..64 {
            if mask & (1_u64 << bit) == 0 {
                continue;
            }
            let format_index = offset
                .checked_add(bit)
                .ok_or(AtomicError::MalformedFormatBlob(
                    "modifier format index overflows usize",
                ))?;
            if format_index >= format_count {
                return Err(AtomicError::ModifierFormatOutOfBounds {
                    modifier: index,
                    format: format_index,
                    count: format_count,
                });
            }
            let code = Fourcc::from_raw(read_u32(formats, format_index * 4)?);
            let format = DrmFormat::new(code, modifier);
            if !result.contains(&format) {
                result.push(format);
            }
        }
    }
    Ok(result)
}

fn checked_region<'a>(
    data: &'a [u8],
    offset: usize,
    count: usize,
    stride: usize,
    name: &'static str,
) -> Result<&'a [u8], AtomicError> {
    if offset < FORMAT_BLOB_HEADER_SIZE {
        return Err(AtomicError::BlobRegionBeforeHeader(name));
    }
    let len = count
        .checked_mul(stride)
        .ok_or(AtomicError::BlobRegionOverflow(name))?;
    let end = offset
        .checked_add(len)
        .ok_or(AtomicError::BlobRegionOverflow(name))?;
    data.get(offset..end)
        .ok_or(AtomicError::BlobRegionOutOfBounds(name))
}

fn regions_overlap(a_start: usize, a_len: usize, b_start: usize, b_len: usize) -> bool {
    let a_end = a_start + a_len;
    let b_end = b_start + b_len;
    a_start < b_end && b_start < a_end
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, AtomicError> {
    let end = offset
        .checked_add(4)
        .ok_or(AtomicError::MalformedFormatBlob("u32 offset overflows"))?;
    let bytes: [u8; 4] = data
        .get(offset..end)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(AtomicError::MalformedFormatBlob("u32 is truncated"))?;
    Ok(u32::from_ne_bytes(bytes))
}

fn read_u64(data: &[u8], offset: usize) -> Result<u64, AtomicError> {
    let end = offset
        .checked_add(8)
        .ok_or(AtomicError::MalformedFormatBlob("u64 offset overflows"))?;
    let bytes: [u8; 8] = data
        .get(offset..end)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(AtomicError::MalformedFormatBlob("u64 is truncated"))?;
    Ok(u64::from_ne_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_ne_bytes());
    }

    fn push_u64(bytes: &mut Vec<u8>, value: u64) {
        bytes.extend_from_slice(&value.to_ne_bytes());
    }

    fn format_blob(mask: u64, offset: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        push_u32(&mut bytes, 1);
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 2);
        push_u32(&mut bytes, 24);
        push_u32(&mut bytes, 1);
        push_u32(&mut bytes, 32);
        push_u32(&mut bytes, Fourcc::XRGB8888.raw());
        push_u32(&mut bytes, Fourcc::ARGB8888.raw());
        push_u64(&mut bytes, mask);
        push_u32(&mut bytes, offset);
        push_u32(&mut bytes, 0);
        push_u64(&mut bytes, 9);
        bytes
    }

    #[test]
    fn format_blob_maps_modifier_bits_without_alignment_casts() {
        let formats = parse_format_modifier_blob(&format_blob(0b11, 0)).unwrap();
        assert_eq!(
            formats,
            vec![
                DrmFormat::new(Fourcc::XRGB8888, Modifier::from_raw(9)),
                DrmFormat::new(Fourcc::ARGB8888, Modifier::from_raw(9)),
            ]
        );
    }

    #[test]
    fn format_blob_rejects_modifier_bits_past_the_format_table() {
        assert!(matches!(
            parse_format_modifier_blob(&format_blob(0b10, 1)),
            Err(AtomicError::ModifierFormatOutOfBounds {
                modifier: 0,
                format: 2,
                count: 2,
            })
        ));
    }

    #[test]
    fn format_blob_rejects_the_unspecified_modifier_sentinel() {
        let mut blob = format_blob(1, 0);
        blob[48..56].copy_from_slice(&u64::MAX.to_ne_bytes());
        assert!(matches!(
            parse_format_modifier_blob(&blob),
            Err(AtomicError::ImplicitFormatModifier(0))
        ));
    }

    #[test]
    fn format_blob_rejects_truncated_and_overlapping_tables() {
        assert!(parse_format_modifier_blob(&[0; 23]).is_err());
        let mut overlapping = format_blob(1, 0);
        overlapping[20..24].copy_from_slice(&28_u32.to_ne_bytes());
        assert!(matches!(
            parse_format_modifier_blob(&overlapping),
            Err(AtomicError::MalformedFormatBlob(
                "format and modifier tables overlap"
            ))
        ));
    }
}
