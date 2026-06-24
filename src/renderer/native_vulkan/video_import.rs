//! Decoded video import telemetry for the native Vulkan video path.
//!
//! Import implementations still live next to the Vulkan/CUDA/VA/DMABuf code.
//! This module owns the stable status surface that runtime JSON and smoke tests
//! consume.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanDmabufImportSnapshot {
    pub source: String,
    pub format: &'static str,
    pub drm_fourcc: u32,
    pub drm_fourcc_hex: String,
    pub modifier: u64,
    pub modifier_hex: String,
    pub available_plane_count: u32,
    pub drm_object_count: u32,
    pub y_uv_same_fd: bool,
    pub driver_modifier_plane_count: Option<u32>,
    pub y_offset: u64,
    pub y_stride: u32,
    pub uv_offset: u64,
    pub uv_stride: u32,
    pub image_memory_type_bits: Option<u32>,
    pub image_memory_type_bits_hex: Option<String>,
    pub fd_memory_type_bits: Option<u32>,
    pub fd_memory_type_bits_hex: Option<String>,
    pub compatible_memory_type_bits: Option<u32>,
    pub compatible_memory_type_bits_hex: Option<String>,
    pub selected_memory_type_index: Option<u32>,
    pub memory_allocation_size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeVulkanVideoImportSnapshot {
    pub(super) texture_import_status: &'static str,
    pub(super) frames_imported: u64,
    pub(super) last_import_size: Option<(u32, u32)>,
    pub(super) last_import_memory_path: Option<String>,
    pub(super) last_import_error: Option<String>,
    pub(super) last_import_elapsed_us: Option<u64>,
    pub(super) max_import_elapsed_us: Option<u64>,
    pub(super) last_dmabuf_import: Option<NativeVulkanDmabufImportSnapshot>,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeVulkanVideoImportReport {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) memory_path: String,
    pub(super) elapsed_us: u64,
    pub(super) dmabuf_contract: Option<NativeVulkanDmabufImportSnapshot>,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct NativeVulkanVideoImportStatus {
    frames_imported: u64,
    last_import_size: Option<(u32, u32)>,
    last_import_memory_path: Option<String>,
    last_import_error: Option<String>,
    last_import_elapsed_us: Option<u64>,
    max_import_elapsed_us: Option<u64>,
    last_dmabuf_import: Option<NativeVulkanDmabufImportSnapshot>,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanVideoImportStatus {
    pub(super) fn record_import(&mut self, report: NativeVulkanVideoImportReport) {
        self.frames_imported = self.frames_imported.saturating_add(1);
        self.last_import_size = Some((report.width, report.height));
        self.last_import_memory_path = Some(report.memory_path);
        self.last_import_error = None;
        self.last_import_elapsed_us = Some(report.elapsed_us);
        self.last_dmabuf_import = report.dmabuf_contract;
        self.max_import_elapsed_us = Some(
            self.max_import_elapsed_us
                .map(|current| current.max(report.elapsed_us))
                .unwrap_or(report.elapsed_us),
        );
    }

    pub(super) fn clear_dmabuf_contract(&mut self) {
        self.last_dmabuf_import = None;
    }

    pub(super) fn record_dmabuf_contract(&mut self, contract: NativeVulkanDmabufImportSnapshot) {
        self.last_dmabuf_import = Some(contract);
    }

    pub(super) fn record_error(&mut self, error: String) {
        self.last_import_error = Some(error);
    }

    pub(super) fn snapshot(&self) -> NativeVulkanVideoImportSnapshot {
        let texture_import_status = if self.frames_imported > 0 {
            match self.last_import_memory_path.as_deref() {
                Some(path) if path.contains("GstDmaBufMemory") => "importing-dmabuf-vulkan-image",
                _ => "importing-cuda-vulkan-image-planes",
            }
        } else if self.last_import_error.is_some() {
            "waiting-for-supported-importer"
        } else {
            "waiting-for-importable-sample"
        };
        NativeVulkanVideoImportSnapshot {
            texture_import_status,
            frames_imported: self.frames_imported,
            last_import_size: self.last_import_size,
            last_import_memory_path: self.last_import_memory_path.clone(),
            last_import_error: self.last_import_error.clone(),
            last_import_elapsed_us: self.last_import_elapsed_us,
            max_import_elapsed_us: self.max_import_elapsed_us,
            last_dmabuf_import: self.last_dmabuf_import.clone(),
        }
    }
}
