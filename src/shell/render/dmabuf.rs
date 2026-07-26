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
}
