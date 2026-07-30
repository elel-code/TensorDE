//! Retained icon frame data shared by native Vulkan and the legacy renderer.

#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use crate::{FileIconKind, ViewRect};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct IconFrameStats {
    pub(crate) icons: usize,
    pub(crate) quads: usize,
    pub(crate) fallbacks: usize,
    pub(crate) deferred: usize,
    pub(crate) thumbnails: usize,
    pub(crate) thumbnail_quads: usize,
    pub(crate) thumbnail_deferred: usize,
    pub(crate) thumbnail_read_ahead_queued: usize,
    pub(crate) thumbnail_ready_entries: usize,
    pub(crate) thumbnail_ready_bytes: usize,
    pub(crate) folder_previews: usize,
    pub(crate) folder_preview_quads: usize,
    pub(crate) folder_preview_deferred: usize,
    pub(crate) folder_preview_read_ahead_queued: usize,
    pub(crate) folder_preview_ready_entries: usize,
    pub(crate) folder_preview_ready_bytes: usize,
    pub(crate) atlas_uploads: usize,
    pub(crate) atlas_upload_skips: usize,
    pub(crate) atlas_width: u32,
    pub(crate) atlas_height: u32,
    pub(crate) atlas_bytes: usize,
    pub(crate) cache_hits: usize,
    pub(crate) cache_misses: usize,
    pub(crate) cache_entries: usize,
    pub(crate) cache_bytes: usize,
    pub(crate) content_hash: u64,
    pub(crate) geometry_hash: u64,
    pub(crate) vertex_hash: u64,
    pub(crate) slot_hash: u64,
    pub(crate) resolve_us: u128,
}

/// One retained frame of icon geometry and logical GPU resources.
pub(crate) struct IconFrame {
    /// Unique logical icons for this frame.
    pub(crate) slots: Vec<IconGpuSlot>,
    /// Content-layer draws grouped by slot.
    pub(crate) content_batches: Vec<IconSlotBatch>,
    /// Overlay-layer draws grouped by slot.
    pub(crate) overlay_batches: Vec<IconSlotBatch>,
    /// Packed vertex data for all content batches.
    pub(crate) content_vertices: Vec<IconVertex>,
    /// Packed vertex data for all overlay batches.
    pub(crate) overlay_vertices: Vec<IconVertex>,
    pub(crate) stats: IconFrameStats,
}

/// Optional single-plane dmabuf consumed when a cold GPU slot is populated.
pub(crate) struct IconDmabufSource {
    pub(crate) fourcc: u32,
    pub(crate) plane: crate::ui::render::dmabuf::DmabufImportPlane,
}

/// Encoded icon input consumed by a GPU renderer.
///
/// Paths stay encoded until their logical slot needs a resident image. Parsing
/// and decoding are cold preparation work; placement and compositing remain on
/// the GPU without readback.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum IconGpuSource {
    File {
        path: PathBuf,
        size_px: u16,
    },
    FolderPreview {
        children: Arc<[PathBuf]>,
        size_px: u16,
        seed: u64,
    },
}

impl IconGpuSource {
    pub(crate) fn file(path: PathBuf, size_px: u16) -> Self {
        Self::File { path, size_px }
    }

    pub(crate) fn size_px(&self) -> u16 {
        match self {
            Self::File { size_px, .. } | Self::FolderPreview { size_px, .. } => *size_px,
        }
    }

    #[cfg(test)]
    pub(crate) fn file_path(&self) -> Option<&Path> {
        match self {
            Self::File { path, .. } => Some(path),
            Self::FolderPreview { .. } => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn folder_preview_children(&self) -> Option<&[PathBuf]> {
        match self {
            Self::FolderPreview { children, .. } => Some(children),
            Self::File { .. } => None,
        }
    }

    pub(crate) fn memory_bytes(&self) -> usize {
        match self {
            Self::File { path, .. } => path.as_os_str().len(),
            Self::FolderPreview { children, .. } => {
                children.iter().map(|path| path.as_os_str().len()).sum()
            }
        }
        .saturating_add(std::mem::size_of::<Self>())
    }
}

/// One logical icon that maps to a resident GPU image.
pub(crate) struct IconGpuSlot {
    pub(crate) identity: IconGpuUploadKey,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) content_width: u32,
    pub(crate) content_height: u32,
    pub(crate) content_hash: u64,
    pub(crate) rounding: Option<IconRounding>,
    pub(crate) source: Option<IconGpuSource>,
    pub(crate) dmabuf: Option<IconDmabufSource>,
}

impl std::fmt::Debug for IconGpuSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IconGpuSlot")
            .field("identity", &self.identity)
            .field("w", &self.width)
            .field("h", &self.height)
            .field("content_hash", &self.content_hash)
            .field("has_gpu_source", &self.source.is_some())
            .field("has_dmabuf", &self.dmabuf.is_some())
            .finish()
    }
}

/// Draw range into a vertex buffer for one logical icon image.
#[derive(Clone, Debug)]
pub(crate) struct IconSlotBatch {
    pub(crate) slot: u32,
    pub(crate) vertex_start: u32,
    pub(crate) vertex_count: u32,
}

/// Size-independent identity for a resident GPU icon.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum IconGpuIdentity {
    Role(FileIconKind),
    NamedAsset { name: String },
    ThemeAsset { path: PathBuf },
    Content { path: PathBuf, stamp: u64 },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct IconGpuUploadKey {
    pub(crate) identity: IconGpuIdentity,
}

impl IconGpuUploadKey {
    pub(crate) fn role(kind: FileIconKind) -> Self {
        Self {
            identity: IconGpuIdentity::Role(kind),
        }
    }

    pub(crate) fn theme_asset(path: PathBuf) -> Self {
        Self {
            identity: IconGpuIdentity::ThemeAsset { path },
        }
    }

    pub(crate) fn named_asset(name: String) -> Self {
        Self {
            identity: IconGpuIdentity::NamedAsset { name },
        }
    }

    pub(crate) fn content(path: PathBuf, stamp: u64) -> Self {
        Self {
            identity: IconGpuIdentity::Content { path, stamp },
        }
    }

    pub(crate) fn from_slot(slot: &IconGpuSlot) -> Self {
        slot.identity.clone()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct IconDraw {
    pub(crate) screen: ViewRect,
    pub(crate) slot: u32,
    pub(crate) source: ViewRect,
    pub(crate) alpha: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct IconVertex {
    pub(crate) position: [f32; 2],
    pub(crate) uv: [f32; 2],
    pub(crate) rounding_bounds: [f32; 4],
    pub(crate) radius_alpha: [f32; 2],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IconDrawLayer {
    Content,
    Overlay,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct IconRounding {
    pub(crate) bounds: [f32; 4],
    pub(crate) radius_ratio: f32,
}
