//! Import Linux dmabuf fds into wgpu textures (Vulkan path).
//!
//! Uses `wgpu_hal::vulkan::Device::texture_from_dmabuf_fd` +
//! `Device::create_texture_from_hal`. Present still goes through the normal
//! RWH/wgpu surface; this is for sampling or compositing external buffers
//! (thumbnails, video, multi-GPU, scan-out negotiation helpers).

use std::os::unix::io::OwnedFd;

use wayland_client_runtime::fourcc;

/// Plane layout for a single-plane dmabuf import (wgpu-hal limitation today).
#[derive(Debug)]
pub(crate) struct DmabufImportPlane {
    pub fd: OwnedFd,
    pub offset: u32,
    pub stride: u32,
    pub modifier: u64,
}

/// Description of a dmabuf-backed texture to wrap as a wgpu [`wgpu::Texture`].
#[derive(Debug)]
pub(crate) struct DmabufImportDesc {
    pub width: u32,
    pub height: u32,
    /// DRM fourcc (see [`wayland_client_runtime::fourcc`]).
    pub fourcc: u32,
    pub plane: DmabufImportPlane,
    /// Vulkan usages required after import. The legacy importer translates
    /// these only at its private wgpu boundary.
    pub usage: vulkan_renderer::vk::ImageUsageFlags,
    pub label: Option<&'static str>,
}

/// A Vulkan image whose memory is exported for direct compositor scanout.
pub(crate) struct ExportedDmabufTexture {
    pub texture: wgpu::Texture,
    pub plane: DmabufImportPlane,
    pub fourcc: u32,
}

#[derive(Debug)]
pub(crate) enum DmabufImportError {
    FeatureUnavailable,
    NotVulkan,
    UnsupportedFourcc(u32),
    InvalidSize(u32, u32),
    UnsupportedUsage(vulkan_renderer::vk::ImageUsageFlags),
    Hal(String),
}

impl std::fmt::Display for DmabufImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FeatureUnavailable => {
                write!(f, "adapter does not support VULKAN_EXTERNAL_MEMORY_DMA_BUF")
            }
            Self::NotVulkan => write!(f, "device is not a Vulkan wgpu-hal backend"),
            Self::UnsupportedFourcc(code) => {
                write!(f, "unsupported DRM fourcc 0x{code:08x}")
            }
            Self::InvalidSize(w, h) => write!(f, "invalid dimensions {w}x{h}"),
            Self::UnsupportedUsage(usage) => {
                write!(f, "unsupported dma-buf image usage {usage:?}")
            }
            Self::Hal(msg) => write!(f, "hal import failed: {msg}"),
        }
    }
}

impl std::error::Error for DmabufImportError {}

/// Map a DRM fourcc to a wgpu texture format when possible.
pub(crate) fn texture_format_for_fourcc(code: u32) -> Option<wgpu::TextureFormat> {
    // On little-endian, DRM ARGB8888 / XRGB8888 store as B,G,R,A in memory —
    // the same as Vulkan B8G8R8A8 / wgpu Bgra8Unorm.
    match code {
        fourcc::ARGB8888 | fourcc::XRGB8888 | fourcc::BGRA8888 => {
            Some(wgpu::TextureFormat::Bgra8Unorm)
        }
        fourcc::ABGR8888 | fourcc::XBGR8888 | fourcc::RGBA8888 => {
            Some(wgpu::TextureFormat::Rgba8Unorm)
        }
        _ => None,
    }
}

const fn sampled_fourcc_supported(code: u32) -> bool {
    matches!(
        code,
        fourcc::ARGB8888
            | fourcc::XRGB8888
            | fourcc::BGRA8888
            | fourcc::ABGR8888
            | fourcc::XBGR8888
            | fourcc::RGBA8888
    )
}

/// Whether the adapter can import dmabuf fds through wgpu-hal Vulkan.
pub(crate) fn adapter_supports_dmabuf_import(adapter: &wgpu::Adapter) -> bool {
    adapter
        .features()
        .contains(wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF)
}

/// Features to request so dmabuf import works when the adapter allows it.
pub(crate) fn optional_dmabuf_features(adapter: &wgpu::Adapter) -> wgpu::Features {
    let available = adapter.features();
    let mut want = wgpu::Features::empty();
    if available.contains(wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF) {
        want |= wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF;
    }
    want
}

/// Fourccs the Vulkan import path accepts, in preference order.
///
/// Prefer BGRA/ARGB (native LE layout for `Bgra8Unorm`) over RGBA variants.
pub(crate) const VULKAN_IMPORT_FOURCC_PREFERENCE: &[u32] = &[
    fourcc::ARGB8888,
    fourcc::BGRA8888,
    fourcc::XRGB8888,
    fourcc::ABGR8888,
    fourcc::RGBA8888,
    fourcc::XBGR8888,
];

/// Pick a compositor-advertised format that the Vulkan path can import.
///
/// Uses tranche preference from feedback, constrained to formats we map to a
/// sampleable format. Returns `None` if feedback has no overlap.
pub(crate) fn pick_import_format(
    feedback: &wayland_client_runtime::DmabufFeedback,
) -> Option<wayland_client_runtime::DmabufFormat> {
    feedback.pick_format(VULKAN_IMPORT_FOURCC_PREFERENCE)
}

/// Same as [`pick_import_format`], but only consider scan-out tranches when
/// the compositor advertises any (zero-copy direct scan-out path).
#[allow(dead_code)] // ready for scan-out-aware import paths
pub(crate) fn pick_scanout_import_format(
    feedback: &wayland_client_runtime::DmabufFeedback,
) -> Option<wayland_client_runtime::DmabufFormat> {
    let scanout = feedback.scanout_formats();
    if scanout.is_empty() {
        return pick_import_format(feedback);
    }
    for &cand in VULKAN_IMPORT_FOURCC_PREFERENCE {
        if let Some(fmt) = scanout.iter().find(|f| f.format == cand) {
            return Some(*fmt);
        }
    }
    None
}

/// Backend-neutral plan for importing external dma-buf content on Vulkan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DmabufImportPlan {
    pub fourcc: u32,
    pub modifier: u64,
    /// Compositor main device (`dev_t`) from feedback, when known.
    pub main_device: u64,
    pub scanout_preferred: bool,
}

/// Full readiness snapshot for diagnostics and feature gating.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DmabufReadiness {
    pub vulkan_import: bool,
    pub wayland_global: bool,
    pub feedback_ready: bool,
    pub plan: Option<DmabufImportPlan>,
}

impl DmabufReadiness {
    pub fn import_ready(&self) -> bool {
        self.vulkan_import && self.plan.is_some()
    }

    pub fn summary(&self) -> String {
        match self.plan {
            Some(plan) => format!(
                "ready vulkan={} wayland={} fourcc=0x{:08x} mod=0x{:x}",
                self.vulkan_import as u8, self.wayland_global as u8, plan.fourcc, plan.modifier
            ),
            None => format!(
                "not-ready vulkan={} wayland={} feedback={}",
                self.vulkan_import as u8, self.wayland_global as u8, self.feedback_ready as u8
            ),
        }
    }
}

/// Build an import plan from compositor feedback and local Vulkan capability.
pub(crate) fn build_import_plan(
    vulkan_import: bool,
    feedback: Option<&wayland_client_runtime::DmabufFeedback>,
    prefer_scanout: bool,
) -> Option<DmabufImportPlan> {
    if !vulkan_import {
        return None;
    }
    let feedback = feedback?;
    let (fmt, scanout_preferred) = if prefer_scanout {
        match pick_scanout_import_format(feedback) {
            Some(f) => (f, !feedback.scanout_formats().is_empty()),
            None => (pick_import_format(feedback)?, false),
        }
    } else {
        (pick_import_format(feedback)?, false)
    };
    sampled_fourcc_supported(fmt.format).then_some(())?;
    Some(DmabufImportPlan {
        fourcc: fmt.format,
        modifier: fmt.modifier,
        main_device: feedback.main_device(),
        scanout_preferred,
    })
}

/// Combine device + protocol + feedback into a single readiness view.
pub(crate) fn assess_readiness(
    vulkan_import: bool,
    wayland_global: bool,
    feedback: Option<&wayland_client_runtime::DmabufFeedback>,
) -> DmabufReadiness {
    let plan = build_import_plan(vulkan_import, feedback, false);
    DmabufReadiness {
        vulkan_import,
        wayland_global,
        feedback_ready: feedback.is_some(),
        plan,
    }
}

/// Build a [`DmabufImportDesc`] for a plane using a negotiated plan.
pub(crate) fn import_desc_from_plan(
    plan: DmabufImportPlan,
    width: u32,
    height: u32,
    plane: DmabufImportPlane,
    usage: vulkan_renderer::vk::ImageUsageFlags,
    label: Option<&'static str>,
) -> DmabufImportDesc {
    DmabufImportDesc {
        width,
        height,
        fourcc: plan.fourcc,
        plane,
        usage,
        label,
    }
}

/// How an external GPU buffer was obtained for rendering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExternalTextureSource {
    /// Rendered directly into the resident Vulkan image.
    GpuRender,
    /// Zero-copy import via `texture_from_dmabuf_fd`.
    DmabufImport,
}

/// A sampleable external texture with its provenance.
#[allow(dead_code)] // fields consumed by future video/thumbnail GPU producers
pub(crate) struct ExternalTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
    pub format: wgpu::TextureFormat,
    pub source: ExternalTextureSource,
}

impl ExternalTexture {
    pub fn from_imported(
        texture: wgpu::Texture,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Self {
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            texture,
            view,
            width,
            height,
            format,
            source: ExternalTextureSource::DmabufImport,
        }
    }
}

/// Default usages for sampleable external content (icons, video frames, …).
pub(crate) fn external_sample_usages() -> vulkan_renderer::vk::ImageUsageFlags {
    vulkan_renderer::vk::ImageUsageFlags::SAMPLED
        | vulkan_renderer::vk::ImageUsageFlags::TRANSFER_SRC
}

/// Allocate a single-plane linear ARGB8888 dmabuf via `/dev/udmabuf` when available.
///
/// Used by smoke tests and diagnostics. Returns `(fd, stride_bytes)`.
/// Fails (returns `None`) if the node is missing, sealed memfd fails, or
/// the process lacks permission.
#[cfg(test)]
pub(crate) fn try_allocate_udmabuf_argb8888(
    width: u32,
    height: u32,
) -> Option<(std::os::fd::OwnedFd, u32)> {
    use rustix::fs::{MemfdFlags, SealFlags, fcntl_add_seals, memfd_create};
    use std::fs::{File, OpenOptions};
    use std::io::{Seek, SeekFrom, Write};
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::os::unix::io::AsRawFd;
    use std::path::Path;

    if !Path::new("/dev/udmabuf").exists() {
        return None;
    }
    let stride = width.checked_mul(4)?;
    let size = (stride as u64)
        .checked_mul(height as u64)?
        .max(4096)
        .next_multiple_of(4096);

    let memfd = memfd_create(
        "fika-dmabuf",
        MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
    )
    .ok()?;
    let mut file = File::from(memfd);
    file.set_len(size).ok()?;
    // Solid opaque red in little-endian ARGB8888 (B,G,R,A).
    let pixel = [0u8, 0, 255, 255];
    let row = pixel.repeat(width as usize);
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
    // Linux uapi: _IOW('u', 0x42, struct udmabuf_create) → 0x40187542.
    const UDMABUF_CREATE: std::os::raw::c_ulong = 0x4018_7542;
    let ud = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/udmabuf")
        .ok()?;
    let arg = UdmabufCreate {
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
    // SAFETY: UDMABUF_CREATE returns a new dmabuf fd on success.
    let dmabuf_raw = unsafe { ioctl(ud.as_raw_fd(), UDMABUF_CREATE, &arg) };
    drop(file);
    drop(ud);
    if dmabuf_raw < 0 {
        return None;
    }
    // SAFETY: kernel returned a fresh owned fd.
    Some((unsafe { OwnedFd::from_raw_fd(dmabuf_raw) }, stride))
}

/// Import external content from a negotiated dmabuf plane.
///
/// There is deliberately no CPU pixel fallback: a rejected or unavailable
/// import remains unavailable until the producer supplies a usable GPU buffer.
pub(crate) fn acquire_external_texture(
    device: &wgpu::Device,
    plan: Option<DmabufImportPlan>,
    width: u32,
    height: u32,
    plane: Option<DmabufImportPlane>,
    label: Option<&'static str>,
) -> Result<ExternalTexture, DmabufImportError> {
    let plan = plan.ok_or(DmabufImportError::FeatureUnavailable)?;
    let plane = plane.ok_or(DmabufImportError::FeatureUnavailable)?;
    let desc = import_desc_from_plan(plan, width, height, plane, external_sample_usages(), label);
    let texture = import_dmabuf_texture(device, desc)?;
    let format = texture_format_for_fourcc(plan.fourcc)
        .ok_or(DmabufImportError::UnsupportedFourcc(plan.fourcc))?;
    Ok(ExternalTexture::from_imported(
        texture, width, height, format,
    ))
}

/// Import a single-plane dmabuf as a wgpu texture.
///
/// Requires the device to have been created with
/// [`wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF`] (enabled when the adapter
/// advertises it).
///
/// # Safety notes
///
/// Caller must ensure the fd layout matches `desc` and that the buffer remains
/// valid for the lifetime of the returned texture (or until the compositor
/// signals release if it was shared).
pub(crate) fn import_dmabuf_texture(
    device: &wgpu::Device,
    desc: DmabufImportDesc,
) -> Result<wgpu::Texture, DmabufImportError> {
    if desc.width == 0 || desc.height == 0 {
        return Err(DmabufImportError::InvalidSize(desc.width, desc.height));
    }
    let format = texture_format_for_fourcc(desc.fourcc)
        .ok_or(DmabufImportError::UnsupportedFourcc(desc.fourcc))?;

    let (wgpu_usage, hal_usage) = translate_image_usage(desc.usage)?;
    let wgpu_desc = wgpu::TextureDescriptor {
        label: desc.label,
        size: wgpu::Extent3d {
            width: desc.width,
            height: desc.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu_usage,
        view_formats: &[],
    };

    let hal_desc = wgpu::hal::TextureDescriptor {
        label: desc.label,
        size: wgpu_desc.size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: hal_usage,
        memory_flags: wgpu::hal::MemoryFlags::empty(),
        view_formats: vec![],
    };

    // SAFETY: as_hal guard is held only for the import call; the resulting
    // texture is immediately wrapped by create_texture_from_hal on this device.
    let hal_texture = unsafe {
        let Some(hal_device) = device.as_hal::<wgpu::hal::api::Vulkan>() else {
            return Err(DmabufImportError::NotVulkan);
        };
        match hal_device.texture_from_dmabuf_fd(
            desc.plane.fd,
            &hal_desc,
            desc.plane.modifier,
            u64::from(desc.plane.stride),
            u64::from(desc.plane.offset),
        ) {
            Ok(tex) => tex,
            Err(err) => {
                // wgpu-hal returns Unexpected when the DMA_BUF feature is missing.
                let msg = format!("{err:?}");
                if msg.contains("Unexpected") {
                    return Err(DmabufImportError::FeatureUnavailable);
                }
                return Err(DmabufImportError::Hal(msg));
            }
        }
    };

    // SAFETY: texture was created from this device with matching descriptor;
    // UNINITIALIZED is correct for freshly imported external memory until the
    // first barrier (create_texture_from_hal documents this).
    let texture = unsafe {
        device.create_texture_from_hal::<wgpu::hal::api::Vulkan>(
            hal_texture,
            &wgpu_desc,
            wgpu::TextureUses::UNINITIALIZED,
        )
    };
    Ok(texture)
}

fn translate_image_usage(
    usage: vulkan_renderer::vk::ImageUsageFlags,
) -> Result<(wgpu::TextureUsages, wgpu::TextureUses), DmabufImportError> {
    use vulkan_renderer::vk::ImageUsageFlags as VkUsage;

    let supported = VkUsage::TRANSFER_SRC
        | VkUsage::TRANSFER_DST
        | VkUsage::SAMPLED
        | VkUsage::COLOR_ATTACHMENT;
    if usage.is_empty() || usage & supported != usage {
        return Err(DmabufImportError::UnsupportedUsage(usage));
    }

    let mut public = wgpu::TextureUsages::empty();
    let mut hal = wgpu::TextureUses::empty();
    if usage.contains(VkUsage::TRANSFER_SRC) {
        public |= wgpu::TextureUsages::COPY_SRC;
        hal |= wgpu::TextureUses::COPY_SRC;
    }
    if usage.contains(VkUsage::TRANSFER_DST) {
        public |= wgpu::TextureUsages::COPY_DST;
        hal |= wgpu::TextureUses::COPY_DST;
    }
    if usage.contains(VkUsage::SAMPLED) {
        public |= wgpu::TextureUsages::TEXTURE_BINDING;
        hal |= wgpu::TextureUses::RESOURCE;
    }
    if usage.contains(VkUsage::COLOR_ATTACHMENT) {
        public |= wgpu::TextureUsages::RENDER_ATTACHMENT;
        hal |= wgpu::TextureUses::COLOR_TARGET;
    }
    Ok((public, hal))
}

/// Allocate a compositor-compatible GBM buffer and import it into wgpu.
///
/// wgpu-hal owns the Vulkan import. Fika only negotiates the dma-buf layout and
/// submits normal wgpu commands; no raw Vulkan allocation is needed here.
pub(crate) fn create_exportable_dmabuf_texture(
    device: &wgpu::Device,
    plan: DmabufImportPlan,
    width: u32,
    height: u32,
    label: Option<&'static str>,
) -> Result<ExportedDmabufTexture, DmabufImportError> {
    if width == 0 || height == 0 {
        return Err(DmabufImportError::InvalidSize(width, height));
    }
    let usage = vulkan_renderer::vk::ImageUsageFlags::COLOR_ATTACHMENT
        | vulkan_renderer::vk::ImageUsageFlags::SAMPLED;
    let format = gbm::Format::try_from(plan.fourcc)
        .map_err(|_| DmabufImportError::UnsupportedFourcc(plan.fourcc))?;
    let modifier = gbm::Modifier::from(plan.modifier);
    let mut errors = Vec::new();
    for path in gbm_device_candidates(plan.main_device) {
        let file = match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) => {
                errors.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        let gbm = match gbm::Device::new(file) {
            Ok(gbm) => gbm,
            Err(error) => {
                errors.push(format!("{}: gbm device: {error}", path.display()));
                continue;
            }
        };
        let bo = match gbm.create_buffer_object_with_modifiers2::<()>(
            width,
            height,
            format,
            std::iter::once(modifier),
            gbm::BufferObjectFlags::RENDERING,
        ) {
            Ok(bo) => bo,
            Err(error) => {
                errors.push(format!("{}: allocate: {error}", path.display()));
                continue;
            }
        };
        if bo.plane_count() != 1 {
            errors.push(format!("{}: {} planes", path.display(), bo.plane_count()));
            continue;
        }
        let import_fd = match bo.fd_for_plane(0) {
            Ok(fd) => fd,
            Err(error) => {
                errors.push(format!("{}: import fd: {error}", path.display()));
                continue;
            }
        };
        let compositor_fd = match bo.fd_for_plane(0) {
            Ok(fd) => fd,
            Err(error) => {
                errors.push(format!("{}: compositor fd: {error}", path.display()));
                continue;
            }
        };
        let offset = bo.offset(0);
        let stride = bo.stride_for_plane(0);
        let actual_modifier = u64::from(bo.modifier());
        let desc = DmabufImportDesc {
            width,
            height,
            fourcc: plan.fourcc,
            plane: DmabufImportPlane {
                fd: import_fd,
                offset,
                stride,
                modifier: actual_modifier,
            },
            usage,
            label,
        };
        match import_dmabuf_texture(device, desc) {
            Ok(texture) => {
                return Ok(ExportedDmabufTexture {
                    texture,
                    plane: DmabufImportPlane {
                        fd: compositor_fd,
                        offset,
                        stride,
                        modifier: actual_modifier,
                    },
                    fourcc: plan.fourcc,
                });
            }
            Err(error) => errors.push(format!("{}: wgpu import: {error}", path.display())),
        }
    }
    Err(DmabufImportError::Hal(format!(
        "GBM allocation/import failed: {}",
        errors.join("; ")
    )))
}

fn gbm_device_candidates(main_device: u64) -> Vec<std::path::PathBuf> {
    use std::os::unix::fs::MetadataExt as _;

    let mut candidates = std::fs::read_dir("/dev/dri")
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("renderD") || name.starts_with("card"))
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|path| {
        let exact_device = path
            .metadata()
            .ok()
            .is_some_and(|metadata| metadata.rdev() == main_device);
        let render_node = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("renderD"));
        (!exact_device, !render_node, path.clone())
    });
    candidates
}

#[cfg(test)]
mod tests;
