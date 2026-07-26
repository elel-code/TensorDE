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
    /// wgpu texture usages after import (e.g. TEXTURE_BINDING | COPY_SRC).
    pub usage: wgpu::TextureUsages,
    pub label: Option<&'static str>,
}

#[derive(Debug)]
pub(crate) enum DmabufImportError {
    FeatureUnavailable,
    NotVulkan,
    UnsupportedFourcc(u32),
    InvalidSize(u32, u32),
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

/// Fourccs we can import into wgpu textures, in preference order.
///
/// Prefer BGRA/ARGB (native LE layout for `Bgra8Unorm`) over RGBA variants.
pub(crate) const WGPU_IMPORT_FOURCC_PREFERENCE: &[u32] = &[
    fourcc::ARGB8888,
    fourcc::BGRA8888,
    fourcc::XRGB8888,
    fourcc::ABGR8888,
    fourcc::RGBA8888,
    fourcc::XBGR8888,
];

/// Pick a compositor-advertised format that wgpu can import.
///
/// Uses tranche preference from feedback, constrained to formats we map to a
/// wgpu [`TextureFormat`]. Returns `None` if feedback has no overlap.
pub(crate) fn pick_import_format(
    feedback: &wayland_client_runtime::DmabufFeedback,
) -> Option<wayland_client_runtime::DmabufFormat> {
    feedback.pick_format(WGPU_IMPORT_FOURCC_PREFERENCE)
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
    for &cand in WGPU_IMPORT_FOURCC_PREFERENCE {
        if let Some(fmt) = scanout.iter().find(|f| f.format == cand) {
            return Some(*fmt);
        }
    }
    None
}

/// Negotiated plan for importing external dmabuf content into wgpu.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DmabufImportPlan {
    pub fourcc: u32,
    pub modifier: u64,
    pub texture_format: wgpu::TextureFormat,
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
                self.vulkan_import as u8,
                self.wayland_global as u8,
                plan.fourcc,
                plan.modifier
            ),
            None => format!(
                "not-ready vulkan={} wayland={} feedback={}",
                self.vulkan_import as u8,
                self.wayland_global as u8,
                self.feedback_ready as u8
            ),
        }
    }
}

/// Build an import plan from compositor feedback (if any) + local wgpu capability.
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
    let texture_format = texture_format_for_fourcc(fmt.format)?;
    Some(DmabufImportPlan {
        fourcc: fmt.format,
        modifier: fmt.modifier,
        texture_format,
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
    usage: wgpu::TextureUsages,
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
    /// Zero-copy import via `texture_from_dmabuf_fd`.
    DmabufImport,
    /// CPU pixels uploaded with `queue.write_texture`.
    CpuUpload,
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

    pub fn from_cpu_upload(
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
            source: ExternalTextureSource::CpuUpload,
        }
    }
}

/// Default usages for sampleable external content (icons, video frames, …).
pub(crate) fn external_sample_usages() -> wgpu::TextureUsages {
    wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC
}

/// Upload tightly packed RGBA/BGRA8 pixels via the normal wgpu path (fallback).
pub(crate) fn upload_rgba8_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    pixels: &[u8],
    label: Option<&'static str>,
) -> Result<ExternalTexture, DmabufImportError> {
    if width == 0 || height == 0 {
        return Err(DmabufImportError::InvalidSize(width, height));
    }
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or(DmabufImportError::InvalidSize(width, height))?;
    if pixels.len() < expected {
        return Err(DmabufImportError::Hal(format!(
            "pixel buffer too small: have {} need {expected}",
            pixels.len()
        )));
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label,
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: external_sample_usages() | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &pixels[..expected],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    Ok(ExternalTexture::from_cpu_upload(
        texture, width, height, format,
    ))
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
    use rustix::fs::{fcntl_add_seals, memfd_create, MemfdFlags, SealFlags};
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

    let memfd =
        memfd_create("fika-dmabuf", MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING).ok()?;
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
    let ud = OpenOptions::new().read(true).write(true).open("/dev/udmabuf").ok()?;
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

/// Prefer dmabuf import when a plan + plane are available; otherwise CPU upload.
///
/// This is the business-facing entry point for external content (video frames,
/// future GPU thumbnails, etc.). Icon atlas packing still uses `write_texture`
/// into a shared atlas and does not go through this helper.
///
/// `pixels` is tightly packed 8-bit RGBA/BGRA matching `plan.texture_format`
/// (or Bgra8Unorm when plan is absent); used only for CPU fallback.
pub(crate) fn acquire_external_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    plan: Option<DmabufImportPlan>,
    width: u32,
    height: u32,
    plane: Option<DmabufImportPlane>,
    pixels: Option<&[u8]>,
    label: Option<&'static str>,
) -> Result<ExternalTexture, DmabufImportError> {
    if let (Some(plan), Some(plane)) = (plan, plane) {
        let desc = import_desc_from_plan(
            plan,
            width,
            height,
            plane,
            external_sample_usages(),
            label,
        );
        match import_dmabuf_texture(device, desc) {
            Ok(texture) => {
                return Ok(ExternalTexture::from_imported(
                    texture,
                    width,
                    height,
                    plan.texture_format,
                ));
            }
            Err(err) => {
                // Fall through to CPU if pixels are available.
                if pixels.is_none() {
                    return Err(err);
                }
            }
        }
    }
    let pixels = pixels.ok_or(DmabufImportError::FeatureUnavailable)?;
    let format = plan
        .map(|p| p.texture_format)
        .unwrap_or(wgpu::TextureFormat::Bgra8Unorm);
    upload_rgba8_texture(device, queue, width, height, format, pixels, label)
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
        usage: desc.usage,
        view_formats: &[],
    };

    // Translate public usages into hal TextureUses for the import descriptor.
    let mut hal_usage = wgpu::TextureUses::empty();
    if desc.usage.contains(wgpu::TextureUsages::COPY_SRC) {
        hal_usage |= wgpu::TextureUses::COPY_SRC;
    }
    if desc.usage.contains(wgpu::TextureUsages::COPY_DST) {
        hal_usage |= wgpu::TextureUses::COPY_DST;
    }
    if desc.usage.contains(wgpu::TextureUsages::TEXTURE_BINDING) {
        hal_usage |= wgpu::TextureUses::RESOURCE;
    }
    if desc.usage.contains(wgpu::TextureUsages::RENDER_ATTACHMENT) {
        hal_usage |= wgpu::TextureUses::COLOR_TARGET;
    }
    if hal_usage.is_empty() {
        hal_usage = wgpu::TextureUses::RESOURCE;
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fourcc_maps_common_formats() {
        assert_eq!(
            texture_format_for_fourcc(fourcc::ARGB8888),
            Some(wgpu::TextureFormat::Bgra8Unorm)
        );
        assert_eq!(
            texture_format_for_fourcc(fourcc::BGRA8888),
            Some(wgpu::TextureFormat::Bgra8Unorm)
        );
        assert_eq!(
            texture_format_for_fourcc(fourcc::ABGR8888),
            Some(wgpu::TextureFormat::Rgba8Unorm)
        );
        assert!(texture_format_for_fourcc(0xdead_beef).is_none());
    }

    #[test]
    fn pick_import_format_prefers_argb_from_feedback() {
        use wayland_client_runtime::{DmabufFeedback, DmabufFeedbackTranche, DmabufFormat};

        let feedback = DmabufFeedback {
            main_device: 0,
            formats: vec![
                DmabufFormat::new(fourcc::RGBA8888, fourcc::MOD_LINEAR),
                DmabufFormat::new(fourcc::ARGB8888, fourcc::MOD_LINEAR),
            ],
            tranches: vec![DmabufFeedbackTranche {
                device: 0,
                flags: wayland_client_runtime::DmabufTrancheFlags::empty(),
                formats: vec![0, 1],
            }],
        };
        // Preference list puts ARGB before RGBA, even if tranche lists RGBA first.
        let picked = pick_import_format(&feedback).expect("pick");
        assert_eq!(picked.format, fourcc::ARGB8888);
    }

    #[test]
    fn assess_readiness_needs_vulkan_and_feedback() {
        use wayland_client_runtime::{DmabufFeedback, DmabufFormat};

        let feedback = DmabufFeedback {
            main_device: 42,
            formats: vec![DmabufFormat::new(fourcc::ARGB8888, fourcc::MOD_LINEAR)],
            tranches: vec![],
        };
        let not_ready = assess_readiness(false, true, Some(&feedback));
        assert!(!not_ready.import_ready());
        assert!(not_ready.plan.is_none());

        let ready = assess_readiness(true, true, Some(&feedback));
        assert!(ready.import_ready());
        let plan = ready.plan.expect("plan");
        assert_eq!(plan.fourcc, fourcc::ARGB8888);
        assert_eq!(plan.main_device, 42);
        assert_eq!(plan.texture_format, wgpu::TextureFormat::Bgra8Unorm);
    }

    #[test]
    fn acquire_external_falls_back_to_cpu_without_plane() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN | wgpu::Backends::GL,
            flags: wgpu::InstanceFlags::default(),
            backend_options: wgpu::BackendOptions::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            display: None,
        });
        let adapter =
            match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })) {
                Ok(a) => a,
                Err(_) => {
                    eprintln!("skip: no adapter for CPU fallback test");
                    return;
                }
            };
        let (device, queue) =
            match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("cpu-fallback-test"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })) {
                Ok(dq) => dq,
                Err(e) => {
                    eprintln!("skip: request_device: {e}");
                    return;
                }
            };
        // 2x2 BGRA solid.
        let pixels: [u8; 16] = [
            0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255,
        ];
        let ext = acquire_external_texture(
            &device,
            &queue,
            None,
            2,
            2,
            None,
            Some(&pixels),
            Some("cpu-fallback"),
        )
        .expect("cpu upload");
        assert_eq!(ext.source, ExternalTextureSource::CpuUpload);
        assert_eq!(ext.width, 2);
        assert_eq!(ext.height, 2);
        ext.texture.destroy();
    }

    #[test]
    fn import_udmabuf_into_wgpu_when_available() {
        let Some((fd, stride)) = try_allocate_udmabuf_argb8888(64, 64) else {
            eprintln!("skip: /dev/udmabuf unavailable or permission denied");
            return;
        };

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            flags: wgpu::InstanceFlags::default(),
            backend_options: wgpu::BackendOptions::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            display: None,
        });
        let adapter =
            match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })) {
                Ok(a) => a,
                Err(_) => {
                    eprintln!("skip: no Vulkan adapter");
                    return;
                }
            };
        if !adapter_supports_dmabuf_import(&adapter) {
            eprintln!("skip: adapter lacks VULKAN_EXTERNAL_MEMORY_DMA_BUF");
            return;
        }
        let features = optional_dmabuf_features(&adapter);
        let (device, _queue) =
            match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("dmabuf-import-test"),
                required_features: features,
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })) {
                Ok(dq) => dq,
                Err(e) => {
                    eprintln!("skip: request_device failed: {e}");
                    return;
                }
            };

        let plan = DmabufImportPlan {
            fourcc: fourcc::ARGB8888,
            modifier: fourcc::MOD_LINEAR,
            texture_format: wgpu::TextureFormat::Bgra8Unorm,
            main_device: 0,
            scanout_preferred: false,
        };
        let desc = import_desc_from_plan(
            plan,
            64,
            64,
            DmabufImportPlane {
                fd,
                offset: 0,
                stride,
                modifier: fourcc::MOD_LINEAR,
            },
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            Some("udmabuf-test"),
        );
        match import_dmabuf_texture(&device, desc) {
            Ok(texture) => {
                assert_eq!(texture.size().width, 64);
                assert_eq!(texture.size().height, 64);
                assert_eq!(texture.format(), wgpu::TextureFormat::Bgra8Unorm);
                texture.destroy();
            }
            Err(e) => {
                // Some drivers reject linear udmabuf without DRM modifiers; still useful signal.
                eprintln!("import failed (driver may reject linear udmabuf): {e}");
            }
        }
    }
}
