//! Backend-neutral dma-buf negotiation and native Vulkan format mapping.

use std::os::fd::OwnedFd;

use wayland_client_runtime::fourcc;

#[derive(Debug)]
pub(crate) struct DmabufImportPlane {
    pub fd: OwnedFd,
    pub offset: u32,
    pub stride: u32,
    pub modifier: u64,
}

pub(crate) fn vulkan_format_for_fourcc(
    code: u32,
) -> Option<(
    vulkan_renderer::vk::Format,
    vulkan_renderer::vk::ComponentMapping,
)> {
    use vulkan_renderer::vk;

    let alpha = if matches!(code, fourcc::XRGB8888 | fourcc::XBGR8888) {
        vk::ComponentSwizzle::ONE
    } else {
        vk::ComponentSwizzle::A
    };
    let format = match code {
        fourcc::ARGB8888 | fourcc::XRGB8888 | fourcc::BGRA8888 => vk::Format::B8G8R8A8_UNORM,
        fourcc::ABGR8888 | fourcc::XBGR8888 | fourcc::RGBA8888 => vk::Format::R8G8B8A8_UNORM,
        _ => return None,
    };
    Some((
        format,
        vk::ComponentMapping {
            r: vk::ComponentSwizzle::R,
            g: vk::ComponentSwizzle::G,
            b: vk::ComponentSwizzle::B,
            a: alpha,
        },
    ))
}

pub(crate) const VULKAN_FOURCC_PREFERENCE: &[u32] = &[
    fourcc::ARGB8888,
    fourcc::BGRA8888,
    fourcc::XRGB8888,
    fourcc::ABGR8888,
    fourcc::RGBA8888,
    fourcc::XBGR8888,
];

pub(crate) fn pick_import_format(
    feedback: &wayland_client_runtime::DmabufFeedback,
) -> Option<wayland_client_runtime::DmabufFormat> {
    feedback.pick_format(VULKAN_FOURCC_PREFERENCE)
}

pub(crate) fn pick_export_format(
    feedback: &wayland_client_runtime::DmabufFeedback,
    exportable: &[wayland_client_runtime::DmabufFormat],
) -> Option<wayland_client_runtime::DmabufFormat> {
    let preferred = feedback.preferred_formats();
    VULKAN_FOURCC_PREFERENCE
        .iter()
        .flat_map(|fourcc| {
            preferred
                .iter()
                .chain(feedback.formats().iter())
                .filter(move |format| format.format == *fourcc)
        })
        .find(|format| {
            format.modifier != fourcc::MOD_INVALID
                && exportable.iter().any(|candidate| {
                    candidate.format == format.format && candidate.modifier == format.modifier
                })
        })
        .copied()
}

pub(crate) fn vulkan_exportable_formats(
    device: &vulkan_renderer::Device,
) -> Result<Vec<wayland_client_runtime::DmabufFormat>, String> {
    let mut formats = Vec::new();
    for &fourcc in VULKAN_FOURCC_PREFERENCE {
        let Some((format, _)) = vulkan_format_for_fourcc(fourcc) else {
            continue;
        };
        let capabilities = device
            .drm_format_modifier_capabilities(
                format,
                vulkan_renderer::vk::ImageUsageFlags::COLOR_ATTACHMENT,
            )
            .map_err(|error| {
                format!("query Vulkan dma-buf export modifiers for fourcc 0x{fourcc:08x}: {error}")
            })?;
        formats.extend(
            capabilities
                .into_iter()
                .filter(|capability| capability.exportable)
                .map(|capability| {
                    wayland_client_runtime::DmabufFormat::new(fourcc, capability.modifier)
                }),
        );
    }
    Ok(formats)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DmabufExportPlan {
    pub fourcc: u32,
    pub modifier: u64,
    pub main_device: u64,
    pub scanout_preferred: bool,
}

#[cfg(test)]
pub(crate) fn try_allocate_udmabuf_argb8888(width: u32, height: u32) -> Option<(OwnedFd, u32)> {
    use rustix::fs::{MemfdFlags, SealFlags, fcntl_add_seals, memfd_create};
    use std::fs::{File, OpenOptions};
    use std::io::{Seek, SeekFrom, Write};
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::path::Path;

    if !Path::new("/dev/udmabuf").exists() {
        return None;
    }
    let stride = width.checked_mul(4)?;
    let size = (u64::from(stride))
        .checked_mul(u64::from(height))?
        .max(4096)
        .next_multiple_of(4096);
    let memfd = memfd_create(
        "fika-dmabuf",
        MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
    )
    .ok()?;
    let mut file = File::from(memfd);
    file.set_len(size).ok()?;
    let row = [0_u8, 0, 255, 255].repeat(width as usize);
    for y in 0..height {
        file.seek(SeekFrom::Start(u64::from(y) * u64::from(stride)))
            .ok()?;
        file.write_all(&row).ok()?;
    }
    fcntl_add_seals(&file, SealFlags::SHRINK | SealFlags::SEAL).ok()?;

    #[repr(C)]
    struct UdmabufCreate {
        memfd: u32,
        flags: u32,
        offset: u64,
        size: u64,
    }
    const UDMABUF_CREATE: std::os::raw::c_ulong = 0x4018_7542;
    let device = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/udmabuf")
        .ok()?;
    let request = UdmabufCreate {
        memfd: file.as_raw_fd() as u32,
        flags: 0,
        offset: 0,
        size,
    };
    unsafe extern "C" {
        fn ioctl(
            fd: std::os::raw::c_int,
            request: std::os::raw::c_ulong,
            ...
        ) -> std::os::raw::c_int;
    }
    let fd = unsafe { ioctl(device.as_raw_fd(), UDMABUF_CREATE, &request) };
    if fd < 0 {
        return None;
    }
    Some((unsafe { OwnedFd::from_raw_fd(fd) }, stride))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) static GPU_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
