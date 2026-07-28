//! Renderer-independent icon resolution and residency snapshot state.

use std::collections::HashMap;

use crate::{FileIconResolver, IconGpuUploadKey, IconRounding, ThumbnailSourceResolver};

/// Snapshot of resident GPU icon sizes at frame start.
#[derive(Clone, Debug, Default)]
pub(crate) struct IconGpuResidentIndex {
    pub(crate) entries: HashMap<IconGpuUploadKey, IconGpuResidentEntry>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct IconGpuResidentEntry {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) content_width: u32,
    pub(crate) content_height: u32,
    pub(crate) content_hash: u64,
    pub(crate) rounding: Option<IconRounding>,
}

impl IconGpuResidentIndex {
    pub(crate) fn get(&self, key: &IconGpuUploadKey) -> Option<IconGpuResidentEntry> {
        self.entries.get(key).copied()
    }
}

pub(crate) struct IconEngine {
    pub(crate) resolver: FileIconResolver,
    pub(crate) thumbnails: ThumbnailSourceResolver,
}

impl IconEngine {
    pub(crate) fn new() -> Self {
        Self {
            resolver: FileIconResolver::new(),
            thumbnails: ThumbnailSourceResolver::new(),
        }
    }
}
